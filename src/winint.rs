//! Windows integration: autostart, magnet/.torrent handling, keep awake.

#![cfg(windows)]

use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

fn exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

pub fn set_autostart(on: bool) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((run, _)) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
        if on {
            let _ = run.set_value("Lifestream", &format!("\"{}\" --tray", exe()));
        } else {
            let _ = run.delete_value("Lifestream");
        }
    }
}

/// Makes Lifestream open magnet links and .torrent files (per user, no admin).
pub fn register_handlers() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let exe = exe();
    let cmd = format!("\"{exe}\" \"%1\"");
    let icon = format!("\"{exe}\",0");
    let classes = hkcu.create_subkey(r"Software\Classes").map_err(|e| e.to_string())?.0;

    let (magnet, _) = classes.create_subkey("magnet").map_err(|e| e.to_string())?;
    magnet.set_value("", &"URL:Magnet link").map_err(|e| e.to_string())?;
    magnet.set_value("URL Protocol", &"").map_err(|e| e.to_string())?;
    let (mi, _) = magnet.create_subkey("DefaultIcon").map_err(|e| e.to_string())?;
    mi.set_value("", &icon).map_err(|e| e.to_string())?;
    let (mc, _) = magnet.create_subkey(r"shell\open\command").map_err(|e| e.to_string())?;
    mc.set_value("", &cmd).map_err(|e| e.to_string())?;

    let (prog, _) = classes.create_subkey("Lifestream.Torrent").map_err(|e| e.to_string())?;
    prog.set_value("", &"Torrent file").map_err(|e| e.to_string())?;
    let (pi, _) = prog.create_subkey("DefaultIcon").map_err(|e| e.to_string())?;
    pi.set_value("", &icon).map_err(|e| e.to_string())?;
    let (pc, _) = prog.create_subkey(r"shell\open\command").map_err(|e| e.to_string())?;
    pc.set_value("", &cmd).map_err(|e| e.to_string())?;
    let (ext, _) = classes.create_subkey(".torrent").map_err(|e| e.to_string())?;
    ext.set_value("", &"Lifestream.Torrent").map_err(|e| e.to_string())?;
    ext.set_value("Content Type", &"application/x-bittorrent").map_err(|e| e.to_string())?;

    // Tell Explorer associations changed.
    unsafe {
        use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    Ok(())
}

/// Who opens a link type: Ok(()) = Lifestream, Err(name) = another app
/// (or "nothing" when no app is set).
fn owner(user_choice_key: &str, fallback_progid: Option<String>) -> Result<(), String> {
    use winreg::enums::HKEY_CLASSES_ROOT;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    // A choice made in Windows Settings wins over plain registration.
    let chosen: Option<String> = hkcu.open_subkey(user_choice_key).ok().and_then(|k| k.get_value("ProgId").ok());
    let Some(progid) = chosen.or(fallback_progid) else { return Err("nothing".into()) };
    let cmd: String = hkcr
        .open_subkey(format!(r"{progid}\shell\open\command"))
        .and_then(|k| k.get_value(""))
        .unwrap_or_default();
    if !cmd.is_empty() && cmd.to_lowercase().contains(&exe().to_lowercase()) {
        return Ok(());
    }
    // A readable name for the other app, if it has one.
    let name: String = hkcr
        .open_subkey(format!(r"{progid}\Application"))
        .and_then(|k| k.get_value("ApplicationName"))
        .or_else(|_| hkcr.open_subkey(&progid).and_then(|k| k.get_value("")))
        .unwrap_or_default();
    let name = if name.trim().is_empty() || name.starts_with("URL:") { progid } else { name };
    Err(name)
}

/// Whether Lifestream opens magnet links and .torrent files.
/// Ok(()) = it does; Err(other app's name) = something else does.
pub fn handler_status() -> Result<(), String> {
    use winreg::enums::HKEY_CLASSES_ROOT;
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let magnet = owner(r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\magnet\UserChoice", Some("magnet".into()));
    let torrent_progid: Option<String> = hkcr.open_subkey(".torrent").and_then(|k| k.get_value("")).ok();
    let torrent = owner(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.torrent\UserChoice", torrent_progid);
    magnet.and(torrent)
}

pub fn keep_awake(on: bool) {
    use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
    unsafe {
        if on {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        } else {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

/// Named mutex the installer checks so it can close Lifestream before updating.
pub fn hold_install_mutex() {
    use windows::Win32::System::Threading::CreateMutexW;
    unsafe {
        // Leaked on purpose: it must live until the process exits.
        let _ = CreateMutexW(None, false, windows::core::w!("LifestreamSingleInstance"));
    }
}

const AUMID: &str = "AnimaTheAeon.Lifestream";

/// Registers Lifestream's name and icon for Windows notifications (per user).
pub fn register_notifications() {
    let icon = crate::settings::data_dir().join("notify_icon_v3.png");
    if !icon.exists() {
        let _ = std::fs::create_dir_all(crate::settings::data_dir());
        let _ = std::fs::write(&icon, include_bytes!("../resources/icon_64.png"));
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((k, _)) = hkcu.create_subkey(format!(r"Software\Classes\AppUserModelId\{AUMID}")) {
        let _ = k.set_value("DisplayName", &"Lifestream");
        let _ = k.set_value("IconUri", &icon.display().to_string());
    }
}

/// Shows a Windows notification (off the UI thread).
pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let mut t = tauri_winrt_notification::Toast::new(AUMID).title(&title);
        if !body.is_empty() {
            t = t.text1(&body);
        }
        if let Err(e) = t.show() {
            crate::applog::warn(format!("Notification failed: {e:?}"));
        }
    });
}

/// Once per new version: ask Explorer to redraw icons, so shortcuts and the
/// taskbar pick up a changed app icon without a sign-out.
pub fn refresh_icons_once() {
    let marker = crate::settings::data_dir().join("icons-refreshed");
    let version = env!("CARGO_PKG_VERSION");
    if std::fs::read_to_string(&marker).map(|v| v.trim() == version).unwrap_or(false) {
        return;
    }
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    // ie4uinit -show rebuilds the icon cache on Windows 10/11.
    let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let tool = std::path::Path::new(&sys).join("System32").join("ie4uinit.exe");
    if tool.exists() {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new(tool).arg("-show").creation_flags(0x0800_0000).spawn(); // CREATE_NO_WINDOW
    }
    let _ = std::fs::write(&marker, version);
}
