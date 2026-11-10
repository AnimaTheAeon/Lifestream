//! IP blocklists in P2P (PeerGuardian) or DAT (eMule) format, optionally gzipped.

use std::io::Read;
use std::net::IpAddr;

use crate::engine::ffi::IpRange;

fn cache_path() -> std::path::PathBuf {
    crate::settings::data_dir().join("blocklist.cache")
}

pub fn parse(text: &str) -> Vec<IpRange> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        // P2P: "description:1.2.3.0-1.2.3.255"; DAT: "1.2.3.0 - 1.2.3.255 , 000 , description"
        let range = if let Some(pos) = line.rfind(':').filter(|_| !line.contains(',')) {
            &line[pos + 1..]
        } else {
            line.split(',').next().unwrap_or("")
        };
        let Some((a, b)) = range.split_once('-') else { continue };
        let a = strip_zeros(a.trim());
        let b = strip_zeros(b.trim());
        if a.parse::<IpAddr>().is_ok() && b.parse::<IpAddr>().is_ok() {
            out.push(IpRange { first: a, last: b });
        }
    }
    out
}

/// DAT files pad octets ("001.002.003.004"); IpAddr rejects that.
fn strip_zeros(ip: &str) -> String {
    if ip.contains(':') {
        return ip.to_string();
    }
    ip.split('.')
        .map(|o| {
            let t = o.trim_start_matches('0');
            if t.is_empty() { "0".to_string() } else { t.to_string() }
        })
        .collect::<Vec<_>>()
        .join(".")
}

pub fn download(url: &str) -> Result<Vec<IpRange>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("couldn't download the blocklist: {e}"))?
        .bytes()
        .map_err(|e| e.to_string())?;
    let text = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut s = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut s)
            .map_err(|e| format!("couldn't unpack the blocklist: {e}"))?;
        s
    } else if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec()))
            .map_err(|e| e.to_string())?;
        let mut s = String::new();
        zip.by_index(0)
            .map_err(|e| e.to_string())?
            .read_to_string(&mut s)
            .map_err(|e| e.to_string())?;
        s
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let ranges = parse(&text);
    if ranges.is_empty() {
        return Err("the file didn't contain any IP ranges".into());
    }
    let cache: String = ranges.iter().map(|r| format!("{}-{}\n", r.first, r.last)).collect();
    let _ = std::fs::write(cache_path(), cache);
    Ok(ranges)
}

pub fn load_cached() -> Vec<IpRange> {
    std::fs::read_to_string(cache_path())
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn clear_cache() {
    let _ = std::fs::remove_file(cache_path());
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats() {
        let r = super::parse("Bad people:1.2.3.0-1.2.3.255\n001.002.004.000 - 001.002.004.255 , 000 , x\n# c\n");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].first, "1.2.3.0");
        assert_eq!(r[1].first, "1.2.4.0");
        assert_eq!(r[1].last, "1.2.4.255");
    }
}
