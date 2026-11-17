//! Matching folders: when a torrent is added without a folder, look in the
//! folders the user listed for one whose name appears in the torrent's name.
//! "ubuntu-24.04.1-desktop-amd64.iso" lands in "/volume1/OS/Ubuntu".

use std::path::Path;

/// How deep to look under each listed folder (1 = its own subfolders).
const DEPTH: usize = 2;
/// Shorter folder names ("TV", "OS") match too much by accident.
const MIN_LEN: usize = 3;

/// Lowercase words separated by single spaces, padded with a space on each
/// side so a whole-word test is a plain `contains`.
fn words(s: &str) -> String {
    let mut out = String::from(" ");
    let mut last_space = true;
    for ch in s.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    if !last_space {
        out.push(' ');
    }
    out
}

fn skip(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('@') || name.starts_with('#')
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if skip(&name) {
            continue;
        }
        out.push((words(&name), path.to_string_lossy().to_string()));
        if depth > 1 {
            collect(&path, depth - 1, out);
        }
    }
}

/// The best folder for a torrent called `name`, if one matches. The longest
/// matching folder name wins, so "Linux Mint" beats "Linux".
pub fn find(roots: &[String], name: &str) -> Option<String> {
    let target = words(name);
    if target.trim().is_empty() {
        return None;
    }
    let mut folders = Vec::new();
    for root in roots.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
        collect(Path::new(root), DEPTH, &mut folders);
    }
    folders
        .into_iter()
        .filter(|(w, _)| w.trim().chars().count() >= MIN_LEN && target.contains(w.as_str()))
        .max_by_key(|(w, path)| (w.len(), std::cmp::Reverse(path.len())))
        .map(|(_, path)| path)
}

/// The display name inside a magnet link (`dn=`), if it has one.
pub fn magnet_name(uri: &str) -> Option<String> {
    let query = uri.split_once('?')?.1;
    let raw = query.split('&').find_map(|kv| kv.strip_prefix("dn="))?;
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    let name = String::from_utf8_lossy(&out).trim().to_string();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_whole_words_and_prefers_longest() {
        let tmp = std::env::temp_dir().join(format!("ls-match-{}", std::process::id()));
        for d in ["OS/Ubuntu", "OS/Linux", "OS/Linux/Linux Mint", "OS/Win", "Movies/@eaDir/Ubuntu Server"] {
            std::fs::create_dir_all(tmp.join(d)).unwrap();
        }
        let roots = vec![tmp.join("OS").display().to_string(), tmp.join("Movies").display().to_string()];
        let hit = |n: &str| find(&roots, n).map(|p| p.replace('\\', "/"));
        assert!(hit("ubuntu-24.04.1-desktop-amd64.iso").unwrap().ends_with("OS/Ubuntu"));
        assert!(hit("Linux.Mint.22.Cinnamon").unwrap().ends_with("Linux Mint"));
        assert!(hit("Arch Linux 2026").unwrap().ends_with("OS/Linux"));
        // "Win" is in "Windows" but not as a whole word.
        assert_eq!(hit("Windows 11 ISO"), None);
        assert_eq!(hit("Fedora"), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn reads_magnet_names() {
        assert_eq!(
            magnet_name("magnet:?xt=urn:btih:abc&dn=ubuntu-24.04%2Bdesktop+amd64.iso&tr=x").as_deref(),
            Some("ubuntu-24.04+desktop amd64.iso")
        );
        assert_eq!(magnet_name("magnet:?xt=urn:btih:abc"), None);
    }
}
