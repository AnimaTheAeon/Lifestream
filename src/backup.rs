//! Export / restore settings, RSS rules and the torrent list in one file.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::settings::{data_dir, Settings};

pub const EXT: &str = "lsbackup";

fn pending_path() -> PathBuf {
    data_dir().join("restore_pending.zip")
}

fn files_to_save() -> Vec<(String, PathBuf)> {
    let dir = data_dir();
    let mut out = Vec::new();
    for name in ["settings.json", "torrents.json", "rss_history.json", "blocklist.cache"] {
        let p = dir.join(name);
        if p.exists() {
            out.push((name.to_string(), p));
        }
    }
    let engine = dir.join("engine");
    let state = engine.join("session.state");
    if state.exists() {
        out.push(("engine/session.state".into(), state));
    }
    if let Ok(entries) = std::fs::read_dir(engine.join("resume")) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("fastresume") {
                let name = format!("engine/resume/{}", p.file_name().unwrap().to_string_lossy());
                out.push((name, p));
            }
        }
    }
    out
}

pub fn export(to: &Path) -> Result<usize, String> {
    let file = std::fs::File::create(to).map_err(|e| format!("couldn't create the file: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let files = files_to_save();
    for (name, path) in &files {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        zip.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(files.len())
}

/// Checks the backup and queues it; it's unpacked on the next start.
pub fn stage_restore(from: &Path) -> Result<(), String> {
    let data = std::fs::read(from).map_err(|e| format!("couldn't read the backup: {e}"))?;
    let zip = zip::ZipArchive::new(std::io::Cursor::new(&data)).map_err(|_| "that isn't a Lifestream backup".to_string())?;
    if !zip.file_names().any(|n| n.map(|n| n == "settings.json").unwrap_or(false)) {
        return Err("that backup has no settings in it".into());
    }
    std::fs::write(pending_path(), &data).map_err(|e| e.to_string())
}

/// Called at startup, before anything is loaded.
pub fn apply_pending_restore() -> Option<String> {
    let path = pending_path();
    let data = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let mut zip = match zip::ZipArchive::new(std::io::Cursor::new(data)) {
        Ok(z) => z,
        Err(e) => return Some(format!("Backup couldn't be restored: {e}")),
    };
    let dir = data_dir();
    // Replace the torrent list wholesale so removed torrents don't linger.
    let _ = std::fs::remove_dir_all(dir.join("engine").join("resume"));
    let mut count = 0;
    for i in 0..zip.len() {
        let Ok(mut f) = zip.by_index(i) else { continue };
        let Some(name) = f.enclosed_name() else { continue };
        let out = dir.join(name);
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_ok() && std::fs::write(&out, buf).is_ok() {
            count += 1;
        }
    }
    Some(format!("Restored {count} files from backup."))
}

pub fn auto_backup_if_due(settings: &mut Settings) {
    let now = chrono::Utc::now().timestamp();
    if now - settings.last_backup < 7 * 86400 {
        return;
    }
    let dir = PathBuf::from(&settings.backup_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let name = format!("Lifestream-{}.{EXT}", chrono::Local::now().format("%Y-%m-%d"));
    match export(&dir.join(name)) {
        Ok(_) => {
            settings.last_backup = now;
            settings.save();
            crate::applog::info("Weekly backup saved.");
            prune(&dir, settings.backup_keep.max(1) as usize);
        }
        Err(e) => crate::applog::warn(format!("Weekly backup failed: {e}")),
    }
}

fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some(EXT)
                && p.file_name().map(|n| n.to_string_lossy().starts_with("Lifestream-")).unwrap_or(false)
        })
        .collect();
    backups.sort();
    while backups.len() > keep {
        let _ = std::fs::remove_file(backups.remove(0));
    }
}
