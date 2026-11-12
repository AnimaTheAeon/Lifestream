//! Checks GitHub Releases on launch and installs newer versions.
//!
//! Installed copies (next to Inno Setup's unins000.exe) update by running the
//! new installer; the portable exe replaces itself, like Jenova.

#[cfg(feature = "gui")]
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
#[cfg(feature = "gui")]
use std::sync::atomic::Ordering;
#[cfg(feature = "gui")]
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

#[cfg(feature = "gui")]
use sha2::{Digest, Sha256};

const RELEASES_API: &str = "https://api.github.com/repos/AnimaTheAeon/Lifestream/releases/latest";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AFTER_UPDATE_ARG: &str = "--after-update";

#[derive(Clone, Debug)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    pub installer: Option<Asset>,
    pub portable: Option<Asset>,
    /// The Synology package.
    pub nas: Option<Asset>,
}

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    Available(Release),
    Downloading(Release),
    /// Ready: the app should quit now (the installer or new exe takes over).
    Restarting,
    Failed(String),
}

#[derive(Default)]
pub struct Updater {
    state: Mutex<State>,
    pub downloaded: AtomicU64,
    pub total: AtomicU64,
    pub dismissed: std::sync::atomic::AtomicBool,
    /// Why the last check didn't get an answer (only shown for manual checks).
    pub last_error: Mutex<Option<String>>,
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn set(&self, s: State) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }
}

pub fn reset(updater: &Updater) {
    updater.set(State::Idle);
}

/// True when this copy was installed by the setup program.
pub fn is_installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("unins000.exe").exists()))
        .unwrap_or(false)
}

/// Quiet check: only an available update changes anything the user sees.
#[cfg(feature = "gui")]
pub fn check_in_background(updater: Arc<Updater>, ctx: eframe::egui::Context) {
    updater.set(State::Checking);
    std::thread::spawn(move || {
        match fetch_latest() {
            Ok(Some(r)) => {
                crate::applog::info(format!("Update available: v{}", r.version));
                updater.set(State::Available(r));
            }
            Ok(None) => {
                *updater.last_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
                updater.set(State::Idle);
            }
            Err(e) => {
                crate::applog::info(format!("Update check skipped: {e}"));
                *updater.last_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                updater.set(State::Idle);
            }
        }
        ctx.request_repaint();
    });
}

pub(crate) fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .user_agent("lifestream-updater")
        .build()
        .map_err(|e| e.to_string())
}

pub fn fetch_latest() -> Result<Option<Release>, String> {
    let url = std::env::var("LIFESTREAM_UPDATE_URL").unwrap_or_else(|_| RELEASES_API.into());
    let resp = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("couldn't reach GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub returned HTTP {}", resp.status().as_u16()));
    }
    let json: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    Ok(parse_release(&json).filter(|r| is_newer(&r.version, APP_VERSION)))
}

pub fn parse_release(json: &serde_json::Value) -> Option<Release> {
    let text = |k: &str| json.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let version = extract_version(&text("tag_name")).or_else(|| extract_version(&text("name")))?;
    let assets: Vec<Asset> = json
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some(Asset {
                        name: a.get("name")?.as_str()?.to_string(),
                        url: a.get("browser_download_url")?.as_str()?.to_string(),
                        size: a.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                        sha256: a
                            .get("digest")
                            .and_then(|d| d.as_str())
                            .and_then(|d| d.strip_prefix("sha256:"))
                            .map(|d| d.to_ascii_lowercase()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let lower = |a: &&Asset| a.name.to_ascii_lowercase();
    Some(Release {
        version,
        notes: text("body"),
        page_url: text("html_url"),
        installer: assets.iter().find(|a| {
            let n = lower(a);
            n.starts_with("lifestream-setup") && n.ends_with(".exe")
        }).cloned(),
        portable: assets.iter().find(|a| lower(a) == "lifestream.exe").cloned(),
        nas: assets.iter().find(|a| lower(a).ends_with(".spk")).cloned(),
    })
}

pub fn extract_version(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for start in 0..b.len() {
        if !b[start].is_ascii_digit() {
            continue;
        }
        if start > 0 && b[start - 1].is_ascii_alphanumeric() && !matches!(b[start - 1], b'v' | b'V') {
            continue;
        }
        let mut end = start;
        while end < b.len() && (b[end].is_ascii_digit() || b[end] == b'.') {
            end += 1;
        }
        let v = text[start..end].trim_end_matches('.');
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    None
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches(['v', 'V']).split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parse(candidate), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(feature = "gui")]
pub fn install_in_background(updater: Arc<Updater>, release: Release, ctx: eframe::egui::Context) {
    updater.set(State::Downloading(release.clone()));
    updater.downloaded.store(0, Ordering::SeqCst);
    updater.total.store(0, Ordering::SeqCst);
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            if is_installed() {
                let asset = release.installer.clone().ok_or("This release has no installer to download.")?;
                let bytes = download(&asset, &updater, &ctx)?;
                let path = std::env::temp_dir().join(&asset.name);
                std::fs::write(&path, bytes).map_err(|e| format!("Couldn't save the installer: {e}"))?;
                run_installer_after_exit(&path)?;
            } else {
                let asset = release.portable.clone().ok_or("This release has no lifestream.exe to download.")?;
                let bytes = download(&asset, &updater, &ctx)?;
                if bytes.len() < 1024 || &bytes[..2] != b"MZ" {
                    return Err("The download isn't a Windows program. Nothing was changed.".into());
                }
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                replace_exe(&exe, &bytes)?;
                let _ = std::process::Command::new(&exe).arg(AFTER_UPDATE_ARG).spawn();
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                crate::applog::info(format!("Updating to v{}", release.version));
                updater.set(State::Restarting);
            }
            Err(e) => {
                crate::applog::error(format!("Update failed: {e}"));
                updater.set(State::Failed(e));
            }
        }
        ctx.request_repaint();
    });
}

#[cfg(feature = "gui")]
fn download(asset: &Asset, updater: &Updater, ctx: &eframe::egui::Context) -> Result<Vec<u8>, String> {
    let mut resp = client()?.get(&asset.url).send().map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(asset.size);
    updater.total.store(total, Ordering::SeqCst);
    let mut bytes = Vec::with_capacity(total as usize);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        updater.downloaded.store(bytes.len() as u64, Ordering::SeqCst);
        ctx.request_repaint();
    }
    if let Some(expected) = &asset.sha256 {
        let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if &actual != expected {
            return Err("The download was corrupted (checksum mismatch). Nothing was changed.".into());
        }
    }
    Ok(bytes)
}

/// Starts the installer once this process has fully exited (so torrent
/// progress is saved first). It shows a progress bar, then reopens Lifestream.
fn run_installer_after_exit(setup: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let setup = setup.to_string_lossy().replace('\'', "''");
        let script = format!(
            "Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue; \
             Start-Process -FilePath '{setup}' -ArgumentList '/SILENT','/SUPPRESSMSGBOXES','/NORESTART','/NOCANCEL'",
            pid = std::process::id()
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Couldn't start the installer: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = setup;
        Err("Installer updates only work on Windows.".into())
    }
}

fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// Windows lets a running exe be renamed but not overwritten.
fn replace_exe(exe: &Path, new_bytes: &[u8]) -> Result<(), String> {
    let staged = exe.with_extension("new");
    std::fs::write(&staged, new_bytes).map_err(|e| format!("Couldn't save the update: {e}"))?;
    let old = old_path(exe);
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(exe, &old) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("Windows wouldn't let Lifestream replace itself ({e}). Download it from the release page instead."));
    }
    if let Err(e) = std::fs::rename(&staged, exe) {
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&staged);
        return Err(format!("Couldn't install the update: {e}"));
    }
    Ok(())
}

/// Removes the previous exe left behind by a portable update.
pub fn cleanup_old_exe() {
    let Ok(exe) = std::env::current_exe() else { return };
    let old = old_path(&exe);
    if old.exists() {
        std::thread::spawn(move || {
            for _ in 0..30 {
                if std::fs::remove_file(&old).is_ok() || !old.exists() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(extract_version("v0.1.2").as_deref(), Some("0.1.2"));
        assert_eq!(extract_version("Lifestream v1").as_deref(), Some("1"));
        assert!(is_newer("0.1.2", "0.1.1"));
        assert!(is_newer("0.2", "0.1.9"));
        assert!(!is_newer("0.1.1", "0.1.1"));
        assert!(!is_newer("0.1.0", "0.1.1"));
    }

    #[test]
    fn picks_both_assets() {
        let json = serde_json::json!({
            "tag_name": "v0.2.0", "name": "Lifestream v0.2.0", "body": "notes", "html_url": "u",
            "assets": [
                {"name": "lifestream.exe", "size": 1, "browser_download_url": "a", "digest": "sha256:AB"},
                {"name": "Lifestream-Setup-0.2.0.exe", "size": 2, "browser_download_url": "b"}
            ]
        });
        let r = parse_release(&json).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.portable.unwrap().sha256.as_deref(), Some("ab"));
        assert_eq!(r.installer.unwrap().url, "b");
    }
}
