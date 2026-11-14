//! The web page: Lifestream's controls in a browser. This is how the Synology
//! version is used, and it also works on the desktop app (same port as remote
//! control). Torrent actions go through the Transmission RPC; this module only
//! serves the page plus a few Lifestream-specific endpoints.

use std::io::Read;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response};

use crate::core::{Cmd, CoreHandle};
use crate::settings::{Category, Settings};

const PAGE: &str = include_str!("../web/index.html");
const ICON: &[u8] = include_bytes!("../resources/icon_256.png");
const NUNITO_700: &[u8] = include_bytes!("../resources/Nunito-700.ttf");
const NUNITO_800: &[u8] = include_bytes!("../resources/Nunito-800.ttf");
const ORBITRON_800: &[u8] = include_bytes!("../resources/Orbitron-800.ttf");

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn send_json(req: Request, status: u16, body: Value) {
    let resp = Response::from_data(serde_json::to_vec(&body).unwrap_or_default())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=UTF-8"))
        .with_header(header("Cache-Control", "no-store"));
    let _ = req.respond(resp);
}

fn send_page(req: Request) {
    let resp = Response::from_string(PAGE)
        .with_header(header("Content-Type", "text/html; charset=UTF-8"))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Frame-Options", "DENY"));
    let _ = req.respond(resp);
}

fn read_json(req: &mut Request) -> Option<Value> {
    let mut body = String::new();
    req.as_reader().take(1024 * 1024).read_to_string(&mut body).ok()?;
    serde_json::from_str(&body).ok()
}

/// `setup_only`: no username/password yet and the visitor isn't on this
/// computer, so the only thing allowed is choosing them.
pub fn handle(core: &CoreHandle, mut req: Request, setup_only: bool) {
    let path = req.url().split('?').next().unwrap_or("").trim_end_matches('/').to_string();
    let method = req.method().clone();

    // Requests that change things must come from the page itself.
    if method != Method::Get && !same_origin(&req) {
        return send_json(req, 403, json!({ "error": "Cross-site request refused." }));
    }

    match (method, path.as_str()) {
        (Method::Get, "" | "/web" | "/transmission/web") => send_page(req),
        (Method::Get, "/icon.png" | "/fonts/nunito-700.ttf" | "/fonts/nunito-800.ttf" | "/fonts/orbitron-800.ttf") => {
            let (data, kind) = match path.as_str() {
                "/icon.png" => (ICON, "image/png"),
                "/fonts/nunito-700.ttf" => (NUNITO_700, "font/ttf"),
                "/fonts/orbitron-800.ttf" => (ORBITRON_800, "font/ttf"),
                _ => (NUNITO_800, "font/ttf"),
            };
            let resp = Response::from_data(data)
                .with_header(header("Content-Type", kind))
                .with_header(header("Cache-Control", "max-age=86400"));
            let _ = req.respond(resp);
        }
        (Method::Get, "/lifestream/api/state") => send_json(req, 200, state(core, setup_only)),
        (Method::Post, "/lifestream/api/setup") => {
            if !setup_only {
                return send_json(req, 409, json!({ "error": "A password is already set." }));
            }
            let body = read_json(&mut req).unwrap_or_default();
            let user = body["user"].as_str().unwrap_or("").trim().to_string();
            let pass = body["pass"].as_str().unwrap_or("").to_string();
            if user.is_empty() || pass.len() < 6 {
                return send_json(req, 400, json!({ "error": "Pick a username and a password of at least 6 characters." }));
            }
            let mut s = Settings::load();
            // Someone may have set one a moment ago in another tab.
            if !s.rpc_username.is_empty() || !s.rpc_password.is_empty() {
                return send_json(req, 409, json!({ "error": "A password is already set." }));
            }
            s.rpc_username = user;
            s.rpc_password = pass;
            crate::rpc::set_credentials(&s.rpc_username, &s.rpc_password);
            core.send(Cmd::ApplySettings(Box::new(s)));
            send_json(req, 200, json!({ "ok": true }))
        }
        _ if setup_only => send_json(req, 401, json!({ "error": "Set a username and password first." })),
        (Method::Get, "/lifestream/api/settings") => send_json(req, 200, settings_json(&Settings::load())),
        (Method::Put, "/lifestream/api/settings") => {
            let Some(body) = read_json(&mut req) else {
                return send_json(req, 400, json!({ "error": "Bad request." }));
            };
            match apply_settings(&body) {
                Ok(s) => {
                    crate::rpc::set_credentials(&s.rpc_username, &s.rpc_password);
                    core.send(Cmd::ApplySettings(Box::new(s.clone())));
                    send_json(req, 200, settings_json(&s))
                }
                Err(e) => send_json(req, 400, json!({ "error": e })),
            }
        }
        (Method::Post, "/lifestream/api/action") => {
            let body = read_json(&mut req).unwrap_or_default();
            match body["action"].as_str().unwrap_or("") {
                "update-check" => {
                    crate::nasupdate::check_in_background();
                    return send_json(req, 200, json!({ "ok": true }));
                }
                "update-install" => {
                    return match crate::nasupdate::install_in_background() {
                        Ok(()) => send_json(req, 200, json!({ "ok": true })),
                        Err(e) => send_json(req, 400, json!({ "error": e })),
                    };
                }
                _ => {}
            }
            let cmd = match body["action"].as_str().unwrap_or("") {
                "pause-all" => Some(Cmd::PauseAll),
                "resume-all" => Some(Cmd::ResumeAll),
                "turtle-on" => Some(Cmd::SetTurtle(true)),
                "turtle-off" => Some(Cmd::SetTurtle(false)),
                "leak-check" => Some(Cmd::LeakCheck),
                "rss-check" => Some(Cmd::RssCheckNow),
                "update-blocklist" => Some(Cmd::UpdateBlocklist),
                "recheck" => body["hash"].as_str().map(|h| Cmd::Recheck(h.to_string())),
                "queue-top" => body["hash"].as_str().map(|h| Cmd::QueueTop(h.to_string())),
                _ => None,
            };
            match cmd {
                Some(c) => {
                    core.send(c);
                    send_json(req, 200, json!({ "ok": true }))
                }
                None => send_json(req, 400, json!({ "error": "Unknown action." })),
            }
        }
        (Method::Get, "/lifestream/api/log") => {
            let lines: Vec<Value> = crate::applog::with_lines(|all| {
                all.iter()
                    .rev()
                    .take(300)
                    .map(|l| json!({ "time": l.time, "level": match l.level {
                        crate::applog::Level::Info => "info",
                        crate::applog::Level::Warn => "warn",
                        crate::applog::Level::Error => "error",
                    }, "text": l.text }))
                    .collect()
            });
            send_json(req, 200, json!({ "lines": lines }))
        }
        _ => {
            let _ = req.respond(Response::from_string("Not found").with_status_code(404));
        }
    }
}

fn same_origin(req: &Request) -> bool {
    let get = |name: &'static str| req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.to_string());
    // The page marks its own requests. Another site can't add this header:
    // the browser would ask first (a CORS preflight), and Lifestream never says yes.
    // This keeps working behind reverse proxies and odd host names, where
    // Origin and Host don't match.
    if get("X-Lifestream").is_some() || get("Sec-Fetch-Site").as_deref() == Some("same-origin") {
        return true;
    }
    let host = get("Host").unwrap_or_default();
    match get("Origin") {
        // Browsers send Origin on POST/PUT; it must be this server.
        Some(origin) => origin.split("://").nth(1).map(|o| o.eq_ignore_ascii_case(&host)).unwrap_or(false),
        // Scripts and apps (curl, Midgar) don't send one.
        None => true,
    }
}

fn state(core: &CoreHandle, setup_only: bool) -> Value {
    if setup_only {
        return json!({ "setup": true, "version": crate::updater::APP_VERSION });
    }
    let s = core.snapshot();
    let settings = Settings::load();
    let adapters: Vec<Value> = crate::netif::list()
        .into_iter()
        .map(|a| json!({ "name": a.name, "label": a.label(), "vpn": a.looks_like_vpn }))
        .collect();
    let leak = s.leak.as_ref().map(|l| {
        let side = |r: &Result<crate::core::IpInfo, String>| match r {
            Ok(i) => json!({ "ip": i.ip, "org": i.org, "place": i.place }),
            Err(e) => json!({ "error": e }),
        };
        json!({ "normal": side(&l.normal), "torrent": l.torrent.as_ref().map(side), "boundIp": l.bound_ip })
    });
    json!({
        "setup": false,
        "version": crate::updater::APP_VERSION,
        "headless": cfg!(not(feature = "gui")),
        "uptimeSecs": s.started.elapsed().as_secs(),
        "dlRate": s.dl_rate,
        "ulRate": s.ul_rate,
        "torrentCount": s.torrents.len(),
        "userPaused": s.user_paused,
        "theme": match settings.theme.as_str() { "green" => "green", "blades" => "blades", _ => "ff" },
        "advanced": settings.advanced_mode,
        "reduceMotion": settings.reduce_motion,
        "killHold": s.kill_hold,
        "killReason": s.kill_reason,
        "adapter": settings.adapter,
        "adapterIp": s.adapter_ip,
        "listenPort": s.listen_port,
        "externalPort": s.external_port,
        "listenError": s.listen_error,
        "incomingSeen": s.incoming_seen,
        "turtle": s.turtle_active,
        "engineError": s.engine_error,
        "restartNeeded": s.restart_needed,
        "blocklistCount": s.blocklist_count,
        "blocklistStatus": s.blocklist_status,
        "leak": leak,
        "leakChecking": s.leak_checking,
        "toast": s.toast.as_ref().map(|t| t.0.clone()),
        "categories": settings.categories.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "adapters": adapters,
        "update": crate::nasupdate::status_json(settings.auto_update),
    })
}

/// The settings the web page edits. The password is never sent back.
fn settings_json(s: &Settings) -> Value {
    json!({
        "download_dir": s.download_dir,
        "categories": s.categories.iter().map(|c| json!({ "name": c.name, "path": c.path })).collect::<Vec<_>>(),
        "incomplete_enabled": s.incomplete_enabled,
        "incomplete_dir": s.incomplete_dir,
        "start_paused": s.start_paused,
        "dl_limit_kb": s.dl_limit_kb,
        "ul_limit_kb": s.ul_limit_kb,
        "turtle_dl_kb": s.turtle_dl_kb,
        "turtle_ul_kb": s.turtle_ul_kb,
        "active_downloads": s.active_downloads,
        "active_seeds": s.active_seeds,
        "max_peers_global": s.max_peers_global,
        "adapter": s.adapter,
        "kill_switch": s.kill_switch,
        "listen_port": s.listen_port,
        "follow_vpn_port": s.follow_vpn_port,
        "natpmp": s.natpmp,
        "upnp": s.upnp,
        "encryption": s.encryption,
        "anonymous_mode": s.anonymous_mode,
        "dht": s.dht,
        "pex": s.pex,
        "lsd": s.lsd,
        "blocklist_url": s.blocklist_url,
        "ratio_limit": s.ratio_limit,
        "seed_hours": s.seed_hours,
        "seed_action": match s.seed_action {
            crate::settings::SeedAction::Pause => "pause",
            crate::settings::SeedAction::Remove => "remove",
            crate::settings::SeedAction::RemoveAndDelete => "remove-delete",
        },
        "rss_enabled": s.rss_enabled,
        "rss_interval_min": s.rss_interval_min,
        "rpc_username": s.rpc_username,
        "rpc_password_set": !s.rpc_password.is_empty(),
        "advanced_mode": s.advanced_mode,
        "auto_update": s.auto_update,
        "theme": s.theme,
        "reduce_motion": s.reduce_motion,
    })
}

fn apply_settings(b: &Value) -> Result<Settings, String> {
    let mut s = Settings::load();
    let str_of = |k: &str| b.get(k).and_then(Value::as_str).map(|v| v.trim().to_string());
    let bool_of = |k: &str| b.get(k).and_then(Value::as_bool);
    let int_of = |k: &str| b.get(k).and_then(Value::as_i64);
    let float_of = |k: &str| b.get(k).and_then(Value::as_f64);

    if let Some(v) = str_of("download_dir") {
        if v.is_empty() {
            return Err("The download folder can't be empty.".into());
        }
        s.download_dir = v;
    }
    if let Some(list) = b.get("categories").and_then(Value::as_array) {
        s.categories = list
            .iter()
            .filter_map(|c| {
                let name = c.get("name")?.as_str()?.trim().to_string();
                let path = c.get("path").and_then(Value::as_str).unwrap_or("").trim().to_string();
                (!name.is_empty()).then_some(Category { name, path })
            })
            .collect();
    }
    if let Some(v) = bool_of("incomplete_enabled") {
        s.incomplete_enabled = v;
    }
    if let Some(v) = str_of("incomplete_dir") {
        s.incomplete_dir = v;
    }
    if let Some(v) = bool_of("start_paused") {
        s.start_paused = v;
    }
    let clamp = |v: i64, lo: i64, hi: i64| v.clamp(lo, hi) as i32;
    if let Some(v) = int_of("dl_limit_kb") {
        s.dl_limit_kb = clamp(v, 0, 10_000_000);
    }
    if let Some(v) = int_of("ul_limit_kb") {
        s.ul_limit_kb = clamp(v, 0, 10_000_000);
    }
    if let Some(v) = int_of("turtle_dl_kb") {
        s.turtle_dl_kb = clamp(v, 0, 10_000_000);
    }
    if let Some(v) = int_of("turtle_ul_kb") {
        s.turtle_ul_kb = clamp(v, 0, 10_000_000);
    }
    if let Some(v) = int_of("active_downloads") {
        s.active_downloads = clamp(v, -1, 1000);
    }
    if let Some(v) = int_of("active_seeds") {
        s.active_seeds = clamp(v, -1, 1000);
    }
    if let Some(v) = int_of("max_peers_global") {
        s.max_peers_global = clamp(v, 10, 10_000);
    }
    if let Some(v) = str_of("theme") {
        s.theme = match v.as_str() { "green" => "green".into(), "blades" => "blades".into(), _ => "ff".into() };
    }
    if let Some(v) = str_of("adapter") {
        s.adapter = v;
    }
    if let Some(v) = bool_of("kill_switch") {
        s.kill_switch = v;
    }
    if let Some(v) = int_of("listen_port") {
        s.listen_port = v.clamp(1024, 65535) as u16;
    }
    for (k, field) in [
        ("follow_vpn_port", &mut s.follow_vpn_port as &mut bool),
        ("natpmp", &mut s.natpmp),
        ("upnp", &mut s.upnp),
        ("anonymous_mode", &mut s.anonymous_mode),
        ("dht", &mut s.dht),
        ("pex", &mut s.pex),
        ("lsd", &mut s.lsd),
        ("rss_enabled", &mut s.rss_enabled),
        ("advanced_mode", &mut s.advanced_mode),
        ("auto_update", &mut s.auto_update),
        ("reduce_motion", &mut s.reduce_motion),
    ] {
        if let Some(v) = b.get(k).and_then(Value::as_bool) {
            *field = v;
        }
    }
    if let Some(v) = int_of("encryption") {
        s.encryption = v.clamp(0, 2) as i32;
    }
    if let Some(v) = str_of("blocklist_url") {
        s.blocklist_url = v;
    }
    if let Some(v) = float_of("ratio_limit") {
        s.ratio_limit = v.clamp(0.0, 1000.0) as f32;
    }
    if let Some(v) = float_of("seed_hours") {
        s.seed_hours = v.clamp(0.0, 100_000.0) as f32;
    }
    if let Some(v) = str_of("seed_action") {
        s.seed_action = match v.as_str() {
            "remove" => crate::settings::SeedAction::Remove,
            "remove-delete" => crate::settings::SeedAction::RemoveAndDelete,
            _ => crate::settings::SeedAction::Pause,
        };
    }
    if let Some(v) = int_of("rss_interval_min") {
        s.rss_interval_min = v.clamp(5, 24 * 60) as u32;
    }
    if let Some(v) = str_of("rpc_username") {
        if v.is_empty() && cfg!(not(feature = "gui")) {
            return Err("The username can't be empty.".into());
        }
        s.rpc_username = v;
    }
    if let Some(v) = b.get("rpc_password").and_then(Value::as_str) {
        if !v.is_empty() {
            if v.len() < 6 {
                return Err("Use a password of at least 6 characters.".into());
            }
            s.rpc_password = v.to_string();
        }
    }
    Ok(s)
}
