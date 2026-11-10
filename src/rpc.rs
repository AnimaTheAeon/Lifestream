//! Remote control that speaks Transmission's RPC protocol, so Turks (and
//! Radarr, Sonarr, Prowlarr…) can use Lifestream as a download client.
//!
//! Supported: session-get, session-stats, torrent-add, torrent-get,
//! torrent-set (labels), torrent-start, torrent-stop, torrent-remove.

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
                Value::String(s) => s.eq_ignore_ascii_case(&t.info.hash),
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
        "hashString": t.info.hash,
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
            let hash = rx.recv_timeout(Duration::from_secs(30)).map_err(|_| "the torrent engine didn't answer".to_string())??;
            // Already there? Transmission reports that as a duplicate, with the same shape.
            let name = core.snapshot().torrents.iter().find(|t| t.info.hash == hash).map(|t| t.info.name.clone()).unwrap_or_default();
            Ok(json!({"torrent-added": {"hashString": hash, "name": name, "id": 0}}))
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
        other => Err(format!("method name not recognized: {other}")),
    }
}
