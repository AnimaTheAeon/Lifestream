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
