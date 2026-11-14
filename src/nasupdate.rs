#![cfg_attr(feature = "gui", allow(dead_code))]
//! Updates for the headless (web page) build.
//!
//! In Docker the start script sets LIFESTREAM_UPDATE_DIR (a folder in the
//! /config volume). Installing an update downloads the newest Synology
//! package, unpacks its app into `<dir>/next`, saves progress and exits with
//! RESTART_CODE. The start script then switches to the new version and starts
//! it again, so updates survive the container being recreated.
//!
//! As a regular Synology package it only reports the update; the page links
//! to the package for Package Center.

use crate::updater::{self, Release, APP_VERSION};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Exit code that tells the container's start script "start me again".
pub const RESTART_CODE: i32 = 75;
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);

static RESTART: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Status> =
    Mutex::new(Status { latest: None, checked: None, error: None, busy: None, checking: false, phase: "", done: 0, total: 0, target: None });

struct Status {
    latest: Option<Release>,
    checked: Option<Instant>,
    error: Option<String>,
    /// What the updater is doing right now, for the page.
    busy: Option<String>,
    checking: bool,
    /// "download", "unpack" or "restart" while installing, for the progress bar.
    phase: &'static str,
    done: u64,
    total: u64,
    target: Option<String>,
}

fn with<T>(f: impl FnOnce(&mut Status) -> T) -> T {
    let mut s = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut s)
}

pub fn restart_requested() -> bool {
    RESTART.load(Ordering::SeqCst)
}

fn update_dir() -> Option<PathBuf> {
    std::env::var("LIFESTREAM_UPDATE_DIR").ok().filter(|d| !d.trim().is_empty()).map(PathBuf::from)
}

/// True when Lifestream can install updates by itself (Docker).
pub fn can_install() -> bool {
    update_dir().is_some()
}

/// What the page shows about updates.
pub fn status_json(auto: bool) -> Value {
    with(|s| {
        let latest = s.latest.as_ref();
        json!({
            "current": APP_VERSION,
            "version": latest.map(|r| r.version.clone()),
            "page": latest.map(|r| r.page_url.clone()),
            "spk": latest.and_then(|r| r.nas.as_ref().map(|a| a.url.clone())),
            "installer": latest.and_then(|r| r.installer.as_ref().map(|a| a.url.clone())),
            "canInstall": can_install(),
            "auto": auto,
            "busy": s.busy.clone(),
            "phase": s.phase,
            "done": s.done,
            "total": s.total,
            "target": s.target.clone(),
            "checking": s.checking,
            "error": s.error.clone(),
            "checkedMinsAgo": s.checked.map(|t| t.elapsed().as_secs() / 60),
        })
    })
}

/// Asks GitHub for a newer release. Blocking.
fn check() -> Option<Release> {
    if with(|s| std::mem::replace(&mut s.checking, true)) {
        return None; // another check is running
    }
    let result = updater::fetch_latest();
    with(|s| {
        s.checking = false;
        s.checked = Some(Instant::now());
        match result {
            Ok(r) => {
                s.error = None;
                s.latest = r;
            }
            Err(e) => s.error = Some(format!("Couldn't check for updates: {e}")),
        }
        s.latest.clone()
    })
}

/// "Check now" from the page.
pub fn check_in_background() {
    std::thread::spawn(|| match check() {
        Some(r) => crate::applog::info(format!("Lifestream {} is available.", r.version)),
        None => crate::applog::info(format!("Update check: Lifestream {APP_VERSION} is the newest.")),
    });
}

/// "Update now" from the page.
pub fn install_in_background() -> Result<(), String> {
    if !can_install() {
        return Err("This copy can't update itself. Download the new package and install it in Package Center.".into());
    }
    let Some(release) = with(|s| s.latest.clone()) else {
        return Err("No update found yet. Press Check now first.".into());
    };
    if with(|s| s.busy.is_some()) {
        return Err("An update is already being installed.".into());
    }
    std::thread::spawn(move || install(&release));
    Ok(())
}

/// Checks on start (after a minute) and every 6 hours, installing by itself
/// when automatic updates are on.
pub fn start_background() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(60));
        loop {
            if let Some(r) = check() {
                let auto = crate::settings::Settings::load().auto_update;
                // Don't retry a version that already failed to start here.
                let failed = update_dir()
                    .and_then(|d| std::fs::read_to_string(d.join("failed").join("VERSION")).ok())
                    .is_some_and(|v| v.trim() == r.version);
                if failed {
                    crate::applog::warn(format!("Lifestream {} is available, but it didn't start last time, so it won't install by itself.", r.version));
                } else if auto && can_install() {
                    crate::applog::info(format!("Lifestream {} is available. Installing it automatically.", r.version));
                    install(&r);
                } else {
                    crate::applog::info(format!("Lifestream {} is available.", r.version));
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

fn install(release: &Release) {
    if with(|s| s.busy.is_some()) {
        return;
    }
    with(|s| {
        s.busy = Some(format!("Downloading {}…", release.version));
        s.error = None;
        s.phase = "download";
        s.done = 0;
        s.total = release.nas.as_ref().map(|a| a.size).unwrap_or(0);
        s.target = Some(release.version.clone());
    });
    match stage(release) {
        Ok(()) => {
            crate::applog::info(format!("Lifestream {} is ready. Saving progress and restarting into it.", release.version));
            with(|s| {
                s.busy = Some(format!("Restarting into {}…", release.version));
                s.phase = "restart";
            });
            RESTART.store(true, Ordering::SeqCst);
        }
        Err(e) => {
            crate::applog::error(format!("Update failed: {e} Lifestream keeps running the current version."));
            with(|s| {
                s.busy = None;
                s.phase = "";
                s.target = None;
                s.error = Some(format!("Update failed: {e}"));
            });
        }
    }
}

/// Downloads the package and leaves the new app in `<dir>/next`.
fn stage(release: &Release) -> Result<(), String> {
    let dir = update_dir().ok_or("Updates aren't set up here.")?;
    let asset = release.nas.as_ref().ok_or("This release has no Synology package.")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;

    let mut resp = updater::client()?.get(&asset.url).send().map_err(|e| format!("Download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status().as_u16()));
    }
    if let Some(len) = resp.content_length() {
        with(|s| s.total = len);
    }
    let mut bytes = Vec::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        let got = bytes.len() as u64;
        with(|s| s.done = got);
    }
    if let Some(expected) = &asset.sha256 {
        let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if &actual != expected {
            return Err("The download was corrupted (checksum mismatch). Nothing was changed.".into());
        }
    }

    with(|s| {
        s.busy = Some(format!("Unpacking {}…", release.version));
        s.phase = "unpack";
    });
    let work = dir.join("staging");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let spk = work.join("package.spk");
    std::fs::write(&spk, &bytes).map_err(|e| format!("Couldn't save the download: {e}"))?;
    tar(&["-xf", &spk.to_string_lossy(), "-C", &work.to_string_lossy(), "package.tgz"])?;
    tar(&["-xzf", &work.join("package.tgz").to_string_lossy(), "-C", &work.to_string_lossy(), "app"])?;
    let app = work.join("app");
    if !app.join("lifestream").is_file() {
        return Err("The package didn't contain the Lifestream program.".into());
    }
    std::fs::write(app.join("VERSION"), &release.version).map_err(|e| e.to_string())?;

    let next = dir.join("next");
    let _ = std::fs::remove_dir_all(&next);
    std::fs::rename(&app, &next).map_err(|e| format!("Couldn't put the update in place: {e}"))?;
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

fn tar(args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new("tar").args(args).output().map_err(|e| format!("Couldn't run tar: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Couldn't unpack the package: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}
