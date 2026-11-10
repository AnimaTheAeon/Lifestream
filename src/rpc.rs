//! Remote control that speaks Transmission's RPC protocol, so Turks (and
//! Radarr, Sonarr, Prowlarr…) can use Lifestream as a download client.
//!
//! Supported: session-get, session-stats, torrent-add, torrent-get,
//! torrent-set (labels), torrent-start, torrent-stop, torrent-remove,
//! torrent-verify, queue-move-top.

use std::sync::mpsc;
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::applog;
use crate::core::{Cmd, CoreHandle, TorrentView};
use crate::settings::Settings;

const SESSION_HEADER: &str = "X-Transmission-Session-Id";

pub fn start(core: CoreHandle, settings: &Settings) {
    if !settings.rpc_enabled {
        return;
    }
    let host = if settings.rpc_allow_lan { "0.0.0.0" } else { "127.0.0.1" };
    let addr = format!("{host}:{}", settings.rpc_port);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            applog::warn(format!("Remote control couldn't start on {addr}: {e}. Is another torrent client using that port?"));
            return;
        }
    };
    applog::info(format!("Remote control listening on http://{addr}/transmission/rpc"));
    let session = format!("{:032x}", rand::random::<u128>());
    let (user, pass) = (settings.rpc_username.clone(), settings.rpc_password.clone());
    std::thread::Builder::new()
        .name("rpc".into())
        .spawn(move || {
            for req in server.incoming_requests() {
                let core = core.clone();
                let (session, user, pass) = (session.clone(), user.clone(), pass.clone());
                std::thread::spawn(move || handle(core, req, &session, &user, &pass));
            }
        })
        .ok();
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn reply(req: Request, status: u16, body: Value, session: &str) {
    let resp = Response::from_data(serde_json::to_vec(&body).unwrap_or_default())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=UTF-8"))
        .with_header(header(SESSION_HEADER, session));
    let _ = req.respond(resp);
}

fn authorized(req: &Request, user: &str, pass: &str) -> bool {
    if user.is_empty() && pass.is_empty() {
        return true;
    }
    let Some(h) = req.headers().iter().find(|h| h.field.equiv("Authorization")) else { return false };
    let v = h.value.as_str();
    let Some(b64) = v.strip_prefix("Basic ") else { return false };
    let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else { return false };
    String::from_utf8_lossy(&raw) == format!("{user}:{pass}")
}

fn handle(core: CoreHandle, mut req: Request, session: &str, user: &str, pass: &str) {
    // Without a username and password, only apps on this PC may connect, even
    // when "allow other computers" is on.
    let local = req.remote_addr().map(|a| a.ip().is_loopback()).unwrap_or(false);
    if !local && user.is_empty() && pass.is_empty() {
        let _ = req.respond(Response::from_string("Set a username and password in Lifestream (Config → Remote control) to allow other devices.").with_status_code(403));
        return;
    }
    let path = req.url().split('?').next().unwrap_or("").trim_end_matches('/').to_string();
    if !path.ends_with("/rpc") {
        let _ = req.respond(Response::from_string("Lifestream remote control: POST to /transmission/rpc").with_status_code(404));
        return;
    }
    if !authorized(&req, user, pass) {
        let resp = Response::from_string("Unauthorized").with_status_code(401).with_header(header("WWW-Authenticate", "Basic realm=\"Lifestream\""));
        let _ = req.respond(resp);
        return;
    }
    let sent = req.headers().iter().find(|h| h.field.equiv(SESSION_HEADER)).map(|h| h.value.to_string()).unwrap_or_default();
    if sent != session {
        let resp = Response::from_string("<h1>409: Conflict</h1><p>Get the session id from the header and try again.</p>")
            .with_status_code(409)
            .with_header(header(SESSION_HEADER, session));
        let _ = req.respond(resp);
        return;
    }
    if *req.method() != Method::Post {
        return reply(req, 405, json!({"result": "method not allowed"}), session);
    }
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let args = v.get("arguments").cloned().unwrap_or(json!({}));
    let tag = v.get("tag").cloned();
    let result = run(&core, method, &args);
    let mut out = match result {
        Ok(a) => json!({"result": "success", "arguments": a}),
        Err(e) => json!({"result": e, "arguments": {}}),
    };
    if let Some(t) = tag {
        out["tag"] = t;
    }
    reply(req, 200, out, session);
}

/// The hash other apps know a torrent by: its v1 (SHA-1) info hash. Only pure
/// v2 torrents fall back to the engine's hash.
fn public_hash(t: &TorrentView) -> &str {
    if t.info.hash_v1.is_empty() { &t.info.hash } else { &t.info.hash_v1 }
}

/// v1 info hash (lowercase hex) from a magnet link, hex or base32 form.
pub fn magnet_hash(uri: &str) -> Option<String> {
    let query = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        if !k.eq_ignore_ascii_case("xt") || v.len() < 9 || !v[..9].eq_ignore_ascii_case("urn:btih:") {
            return None;
        }
        let h = &v[9..];
        if h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(h.to_ascii_lowercase());
        }
        if h.len() != 32 {
            return None;
        }
        let (mut bits, mut nbits, mut out) = (0u64, 0u32, Vec::new());
        for c in h.chars() {
            let v = match c.to_ascii_uppercase() {
                u @ 'A'..='Z' => u as u64 - 'A' as u64,
                d @ '2'..='7' => d as u64 - '2' as u64 + 26,
                _ => return None,
            };
            bits = (bits << 5) | v;
            nbits += 5;
            if nbits >= 8 {
                nbits -= 8;
                out.push(((bits >> nbits) & 0xff) as u8);
            }
        }
        (out.len() == 20).then(|| out.iter().map(|b| format!("{b:02x}")).collect())
    })
}

/// Torrents named by "ids": numbers (list position), hashes, or all.
fn select<'a>(torrents: &'a [TorrentView], args: &Value) -> Vec<&'a TorrentView> {
    let Some(ids) = args.get("ids") else { return torrents.iter().collect() };
    let wanted: Vec<Value> = match ids {
        Value::Array(a) => a.clone(),
        Value::String(s) if s == "recently-active" => return torrents.iter().collect(),
        other => vec![other.clone()],
    };
    torrents
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            wanted.iter().any(|w| match w {
                Value::Number(n) => n.as_u64() == Some(*i as u64 + 1),
                Value::String(s) => s.eq_ignore_ascii_case(public_hash(t)) || s.eq_ignore_ascii_case(&t.info.hash),
                _ => false,
            })
        })
        .map(|(_, t)| t)
        .collect()
}

fn status_code(t: &TorrentView) -> i64 {
    // Transmission: 0 stopped, 1 check wait, 2 checking, 3 download wait, 4 downloading, 5 seed wait, 6 seeding
    if t.info.paused && !t.info.auto_managed {
        return 0;
    }
    match t.info.state {
        0 => 2,
        1 | 2 => {
            if t.info.paused {
                3
            } else {
                4
            }
        }
        _ => {
            if t.info.paused {
                0
            } else {
                6
            }
        }
    }
}

fn torrent_json(i: usize, t: &TorrentView) -> Value {
    let left = (t.info.total_wanted - t.info.total_done).max(0);
    let done = left == 0 && t.info.has_metadata;
    let labels: Vec<String> = if t.category.is_empty() { vec![] } else { vec![t.category.clone()] };
    json!({
        "id": i + 1,
        "hashString": public_hash(t),
        "name": t.info.name,
        "downloadDir": t.info.save_path,
        "totalSize": t.info.total_wanted,
        "sizeWhenDone": t.info.total_wanted,
        "leftUntilDone": left,
        "haveValid": t.info.total_done,
        "downloadedEver": t.info.all_time_download,
        "uploadedEver": t.info.all_time_upload,
        "percentDone": t.info.progress,
        "rateDownload": t.info.dl_rate,
        "rateUpload": t.info.ul_rate,
        "eta": if done { -1 } else { t.eta_secs.unwrap_or(-1) },
        "status": status_code(t),
        "error": if t.info.error.is_empty() { 0 } else { 3 },
        "errorString": t.info.error,
        "isFinished": done && (t.seed_goal_met || (t.info.paused && !t.info.auto_managed)),
        "isPrivate": t.info.is_private,
        "uploadRatio": t.ratio,
        "seedRatioLimit": 0,
        "seedRatioMode": 0,
        "secondsSeeding": t.info.seeding_secs,
        "secondsDownloading": (t.info.active_secs - t.info.seeding_secs).max(0),
        "addedDate": t.info.added_time,
        "doneDate": t.info.completed_time,
        "peersConnected": t.info.num_peers,
        "queuePosition": t.info.queue_position,
        "labels": labels,
        "magnetLink": "",
    })
}

fn run(core: &CoreHandle, method: &str, args: &Value) -> Result<Value, String> {
    match method {
        "session-get" => Ok(json!({
            "version": format!("4.0.5 (Lifestream {})", crate::updater::APP_VERSION),
            "rpc-version": 17,
            "rpc-version-minimum": 14,
            "download-dir": crate::settings::Settings::load().download_dir,
        })),
        "session-stats" => {
            let s = core.snapshot();
            let active = s.torrents.iter().filter(|t| t.info.dl_rate > 0 || t.info.ul_rate > 0).count();
            Ok(json!({"activeTorrentCount": active, "torrentCount": s.torrents.len(), "pausedTorrentCount": s.torrents.iter().filter(|t| t.info.paused).count(), "downloadSpeed": s.dl_rate, "uploadSpeed": s.ul_rate}))
        }
        "torrent-get" => {
            let s = core.snapshot();
            let list: Vec<Value> = {
                let picked = select(&s.torrents, args);
                s.torrents.iter().enumerate().filter(|(_, t)| picked.iter().any(|p| p.info.hash == t.info.hash)).map(|(i, t)| torrent_json(i, t)).collect()
            };
            // Only the requested fields, like Transmission.
            let list = match args.get("fields").and_then(|f| f.as_array()) {
                Some(fields) => list
                    .into_iter()
                    .map(|t| {
                        let mut o = serde_json::Map::new();
                        for f in fields.iter().filter_map(|f| f.as_str()) {
                            if let Some(v) = t.get(f) {
                                o.insert(f.to_string(), v.clone());
                            }
                        }
                        Value::Object(o)
                    })
                    .collect(),
                None => list,
            };
            Ok(json!({"torrents": list}))
        }
        "torrent-add" => {
            let category = args.get("labels").and_then(|l| l.as_array()).and_then(|a| a.first()).and_then(|l| l.as_str()).unwrap_or("").to_string();
            let save_path = args.get("download-dir").and_then(|d| d.as_str()).map(String::from).filter(|d| !d.trim().is_empty());
            let paused = args.get("paused").and_then(|p| p.as_bool()).unwrap_or(false);
            let (magnet, data) = if let Some(meta) = args.get("metainfo").and_then(|m| m.as_str()) {
                let bytes = base64::engine::general_purpose::STANDARD.decode(meta.trim()).map_err(|_| "invalid or corrupt torrent file".to_string())?;
                (None, Some(bytes))
            } else if let Some(f) = args.get("filename").and_then(|f| f.as_str()) {
                if f.trim_start().starts_with("magnet:") {
                    (Some(f.trim().to_string()), None)
                } else if f.starts_with("http://") || f.starts_with("https://") {
                    let bytes = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(45))
                        .user_agent("Lifestream")
                        .build()
                        .and_then(|c| c.get(f).send())
                        .and_then(|r| r.error_for_status())
                        .and_then(|r| r.bytes())
                        .map_err(|e| format!("couldn't download the torrent: {e}"))?;
                    (None, Some(bytes.to_vec()))
                } else {
                    return Err("filename must be a magnet link or an http(s) URL".into());
                }
            } else {
                return Err("no filename or metainfo given".into());
            };
            let (tx, rx) = mpsc::channel();
            core.send(Cmd::RemoteAdd { magnet, data, category, save_path, paused, reply: tx });
            let (hash, duplicate) = rx.recv_timeout(Duration::from_secs(30)).map_err(|_| "the torrent engine didn't answer".to_string())??;
            // Already there? Transmission reports that as a duplicate, with the same shape.
            let name = core.snapshot().torrents.iter().find(|t| public_hash(t).eq_ignore_ascii_case(&hash)).map(|t| t.info.name.clone()).unwrap_or_default();
            let key = if duplicate { "torrent-duplicate" } else { "torrent-added" };
            Ok(json!({ key: {"hashString": hash, "name": name, "id": 0} }))
        }
        "torrent-set" => {
            if let Some(labels) = args.get("labels").and_then(|l| l.as_array()) {
                let cat = labels.first().and_then(|l| l.as_str()).unwrap_or("").to_string();
                let s = core.snapshot();
                for t in select(&s.torrents, args) {
                    core.send(Cmd::SetCategory { hash: t.info.hash.clone(), category: cat.clone() });
                }
            }
            Ok(json!({}))
        }
        "torrent-start" | "torrent-start-now" => {
            let s = core.snapshot();
            for t in select(&s.torrents, args) {
                core.send(Cmd::Resume(t.info.hash.clone()));
            }
            Ok(json!({}))
        }
        "torrent-stop" => {
            let s = core.snapshot();
            for t in select(&s.torrents, args) {
                core.send(Cmd::Pause(t.info.hash.clone()));
            }
            Ok(json!({}))
        }
        "torrent-remove" => {
            let delete = args.get("delete-local-data").and_then(|d| d.as_bool()).unwrap_or(false);
            let s = core.snapshot();
            for t in select(&s.torrents, args) {
                applog::info(format!("Remote control removed {}{}", t.info.name, if delete { " and its files" } else { "" }));
                core.send(Cmd::Remove { hash: t.info.hash.clone(), delete_files: delete });
            }
            Ok(json!({}))
        }
        // Sonarr/Midgar use this for "recent episodes first".
        "queue-move-top" => {
            let s = core.snapshot();
            for t in select(&s.torrents, args) {
                core.send(Cmd::QueueTop(t.info.hash.clone()));
            }
            Ok(json!({}))
        }
        "torrent-verify" => {
            let s = core.snapshot();
            for t in select(&s.torrents, args) {
                core.send(Cmd::Recheck(t.info.hash.clone()));
            }
            Ok(json!({}))
        }
        // Accepted so clients don't fail; Lifestream manages these itself.
        "queue-move-up" | "queue-move-down" | "queue-move-bottom" | "session-set" | "torrent-reannounce" => Ok(json!({})),
        other => Err(format!("method name not recognized: {other}")),
    }
}

/// End-to-end check that drives Lifestream exactly like Midgar's (Sonarr's)
/// Transmission client does. Starts the real engine, so it's opt-in:
/// `cargo test -- --ignored remote_control_round_trip`
#[cfg(test)]
mod e2e {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    use base64::Engine as _;
    use serde_json::{json, Value};

    fn bencode_torrent(name: &str, length: usize) -> Vec<u8> {
        // One file, one piece of zeros.
        let piece_len = 16384usize;
        let pieces = length.div_ceil(piece_len);
        let mut hashes = Vec::new();
        for i in 0..pieces {
            use sha1_smol::Sha1;
            let size = (length - i * piece_len).min(piece_len);
            hashes.extend_from_slice(&Sha1::from(vec![0u8; size]).digest().bytes());
        }
        let mut info = Vec::new();
        info.extend_from_slice(format!("d6:lengthi{length}e4:name{}:{name}12:piece lengthi{piece_len}e6:pieces{}:", name.len(), hashes.len()).as_bytes());
        info.extend_from_slice(&hashes);
        info.extend_from_slice(b"e");
        let mut t = Vec::new();
        let announce = "http://127.0.0.1:1/announce";
        t.extend_from_slice(format!("d8:announce{}:{announce}4:info", announce.len()).as_bytes());
        t.extend_from_slice(&info);
        t.extend_from_slice(b"e");
        t
    }

    struct Client {
        port: u16,
        auth: String,
        session: Option<String>,
    }

    impl Client {
        fn raw(&self, method: &str, body: &str, session: Option<&str>) -> (u16, Vec<(String, String)>, String) {
            let mut s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(40))).unwrap();
            let mut req = format!("{method} /transmission/rpc HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAuthorization: Basic {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", self.auth, body.len());
            if let Some(id) = session {
                req.push_str(&format!("X-Transmission-Session-Id: {id}\r\n"));
            }
            req.push_str("\r\n");
            req.push_str(body);
            s.write_all(req.as_bytes()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            let (head, body) = out.split_once("\r\n\r\n").unwrap();
            let mut lines = head.lines();
            let status: u16 = lines.next().unwrap().split_whitespace().nth(1).unwrap().parse().unwrap();
            let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
            (status, headers, body.to_string())
        }

        /// Same handshake as Sonarr's TransmissionProxy.AuthenticateClient.
        fn call(&mut self, method: &str, args: Value) -> Value {
            if self.session.is_none() {
                let (status, headers, _) = self.raw("GET", "", None);
                assert_eq!(status, 409, "first request should ask for a session id");
                self.session = headers.into_iter().find(|(k, _)| k.eq_ignore_ascii_case("X-Transmission-Session-Id")).map(|(_, v)| v);
            }
            let body = json!({ "method": method, "arguments": args }).to_string();
            let (status, _, text) = self.raw("POST", &body, self.session.as_deref());
            assert_eq!(status, 200, "{method}: {text}");
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["result"], "success", "{method}: {text}");
            v["arguments"].clone()
        }
    }

    #[test]
    #[ignore]
    fn remote_control_round_trip() {
        let tmp = std::env::temp_dir().join(format!("lifestream-rpc-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", tmp.join("config"));

        let port = 39091;
        let settings = crate::settings::Settings {
            download_dir: tmp.join("downloads").display().to_string(),
            kill_switch: false,
            dht: false,
            lsd: false,
            natpmp: false,
            upnp: false,
            rpc_enabled: true,
            rpc_port: port,
            rpc_username: "midgar".into(),
            rpc_password: "mako".into(),
            ..Default::default()
        };
        settings.save();
        let (core, _join) = crate::core::start(settings.clone());
        crate::rpc::start(core.clone(), &settings);

        // Wait for the server.
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(started.elapsed() < Duration::from_secs(20), "server never came up");
            std::thread::sleep(Duration::from_millis(100));
        }

        // Wrong password is refused.
        let bad = Client { port, auth: base64::engine::general_purpose::STANDARD.encode("midgar:nope"), session: None };
        assert_eq!(bad.raw("GET", "", None).0, 401);

        let mut c = Client { port, auth: base64::engine::general_purpose::STANDARD.encode("midgar:mako"), session: None };

        // Sonarr's version check (needs >= 2.40; labels need >= 4.0).
        let session = c.call("session-get", json!({}));
        assert!(session["version"].as_str().unwrap().starts_with("4."));
        assert!(session["download-dir"].as_str().unwrap().ends_with("downloads"));

        // Add a .torrent like Sonarr does (metainfo + labels).
        let torrent = bencode_torrent("Some.Show.S01E01.mkv", 40000);
        let added = c.call(
            "torrent-add",
            json!({ "metainfo": base64::engine::general_purpose::STANDARD.encode(&torrent), "paused": false, "labels": ["tv-midgar"] }),
        );
        let hash = added["torrent-added"]["hashString"].as_str().expect("torrent-added").to_string();
        assert_eq!(hash.len(), 40);

        // Adding again is a duplicate, not an error.
        let again = c.call("torrent-add", json!({ "metainfo": base64::engine::general_purpose::STANDARD.encode(&torrent), "labels": ["tv-midgar"] }));
        assert_eq!(again["torrent-duplicate"]["hashString"], hash.as_str());

        // Magnet links too.
        let magnet = c.call(
            "torrent-add",
            json!({ "filename": "magnet:?xt=urn:btih:c12fe1c06bba254a9dc9f519b335aa7c1367a88a&dn=Other.Show.S02E03", "labels": ["tv-midgar"] }),
        );
        assert_eq!(magnet["torrent-added"]["hashString"], "c12fe1c06bba254a9dc9f519b335aa7c1367a88a");

        // It shows up in torrent-get with the fields Sonarr asks for.
        let fields = json!(["id", "hashString", "name", "downloadDir", "totalSize", "leftUntilDone", "isFinished", "eta", "status",
            "secondsDownloading", "secondsSeeding", "errorString", "uploadedEver", "downloadedEver", "seedRatioLimit", "seedRatioMode",
            "seedIdleLimit", "seedIdleMode", "fileCount", "file-count", "labels"]);
        let started = Instant::now();
        let t = loop {
            let got = c.call("torrent-get", json!({ "fields": fields }));
            if let Some(t) = got["torrents"].as_array().unwrap().iter().find(|t| t["hashString"] == hash.as_str()) {
                break t.clone();
            }
            assert!(started.elapsed() < Duration::from_secs(15), "torrent never appeared");
            std::thread::sleep(Duration::from_millis(300));
        };
        assert_eq!(t["name"], "Some.Show.S01E01.mkv");
        assert_eq!(t["labels"], json!(["tv-midgar"]));
        assert_eq!(t["totalSize"], 40000);
        // A label with no matching Lifestream category lands in the download folder.
        assert!(t["downloadDir"].as_str().unwrap().replace('\\', "/").ends_with("downloads"));
        assert!(t.get("rateDownload").is_none(), "only asked-for fields come back");

        // Relabel (Sonarr's "post-import category"), move to top, then remove.
        c.call("torrent-set", json!({ "ids": [hash], "labels": ["tv-imported"] }));
        c.call("queue-move-top", json!({ "ids": [hash] }));
        let started = Instant::now();
        loop {
            let got = c.call("torrent-get", json!({ "ids": [hash], "fields": ["labels"] }));
            if got["torrents"][0]["labels"] == json!(["tv-imported"]) {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "label never changed");
            std::thread::sleep(Duration::from_millis(300));
        }
        c.call("torrent-remove", json!({ "ids": [hash, "c12fe1c06bba254a9dc9f519b335aa7c1367a88a"], "delete-local-data": true }));
        let started = Instant::now();
        loop {
            let got = c.call("torrent-get", json!({ "fields": ["hashString"] }));
            if got["torrents"].as_array().unwrap().is_empty() {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(15), "torrents never went away");
            std::thread::sleep(Duration::from_millis(300));
        }

        core.send(crate::core::Cmd::Shutdown);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod magnet_tests {
    #[test]
    fn hex_and_base32() {
        let hex = "magnet:?xt=urn:btih:C12FE1C06BBA254A9DC9F519B335AA7C1367A88A&dn=x";
        let b32 = "magnet:?dn=x&xt=urn:btih:YEX6DQDLXISUVHOJ6UM3GNNKPQJWPKEK";
        assert_eq!(super::magnet_hash(hex).as_deref(), Some("c12fe1c06bba254a9dc9f519b335aa7c1367a88a"));
        assert_eq!(super::magnet_hash(b32).as_deref(), Some("c12fe1c06bba254a9dc9f519b335aa7c1367a88a"));
        assert_eq!(super::magnet_hash("magnet:?dn=nothing"), None);
    }
}
