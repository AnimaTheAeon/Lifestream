//! The web page's own sign-in. Browsers get a sign-in page in the look picked
//! in Lifestream (instead of the browser's plain pop-up), and stay signed in
//! with a cookie. Other apps (Midgar, Turks, Materia…) keep using the
//! Transmission RPC with a username and password, as before.

use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Request, Response};

use crate::settings::{data_dir, Settings};

const PAGE: &str = include_str!("../web/login.html");
const COOKIE: &str = "lifestream_session";
/// "Remember me": 30 days. Otherwise until the browser closes (kept 12 hours here).
const REMEMBER_SECS: u64 = 30 * 24 * 3600;
const SHORT_SECS: u64 = 12 * 3600;

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    /// SHA-256 of the cookie value (the cookie itself is never stored).
    token: String,
    /// SHA-256 of the username and password it was made with: changing them signs everyone out.
    cred: String,
    expires: u64,
}

static SESSIONS: Mutex<Option<Vec<Entry>>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn path() -> std::path::PathBuf {
    data_dir().join("web-sessions.json")
}

fn with_sessions<T>(f: impl FnOnce(&mut Vec<Entry>) -> T) -> T {
    let mut guard = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    let list = guard.get_or_insert_with(|| {
        std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    });
    let t = now();
    list.retain(|e| e.expires > t);
    f(list)
}

fn save(list: &[Entry]) {
    if let Ok(b) = serde_json::to_vec(list) {
        let tmp = path().with_extension("tmp");
        if std::fs::write(&tmp, b).is_ok() {
            let _ = std::fs::rename(tmp, path());
        }
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn cookie_value(req: &Request) -> Option<String> {
    let h = req.headers().iter().find(|h| h.field.equiv("Cookie"))?;
    h.value.as_str().split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k == COOKIE).then(|| v.to_string())
    })
}

/// True when the browser carries a valid sign-in cookie for the current username and password.
pub fn signed_in(req: &Request, user: &str, pass: &str) -> bool {
    let Some(c) = cookie_value(req) else { return false };
    let (token, cred) = (sha(&c), sha(&format!("{user}:{pass}")));
    with_sessions(|list| list.iter().any(|e| e.token == token && e.cred == cred))
}

/// Files the sign-in page itself needs, which load before signing in.
pub fn is_public(path: &str) -> bool {
    matches!(path, "/icon.png" | "/fonts/nunito-700.ttf" | "/fonts/nunito-800.ttf" | "/fonts/orbitron-800.ttf")
}

/// Where the page lives; anything else that isn't signed in gets a plain 401.
pub fn is_page(path: &str) -> bool {
    matches!(path, "" | "/web" | "/transmission/web")
}

pub fn send_page(req: Request) {
    let theme = Settings::load().theme;
    let theme = if theme == "green" || theme == "blades" || theme == "glass" { theme } else { "ff".to_string() };
    let resp = Response::from_string(PAGE.replace("_THEME_", &theme))
        .with_header(header("Content-Type", "text/html; charset=UTF-8"))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Frame-Options", "DENY"));
    let _ = req.respond(resp);
}

fn redirect(req: Request, to: &str, cookie: Option<String>) {
    let mut resp = Response::empty(303).with_header(header("Location", to)).with_header(header("Cache-Control", "no-store"));
    if let Some(c) = cookie {
        resp = resp.with_header(header("Set-Cookie", &c));
    }
    let _ = req.respond(resp);
}

fn form_value(body: &str, key: &str) -> Option<String> {
    body.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// POST /lifestream/login from the sign-in page.
pub fn login(mut req: Request, user: &str, pass: &str) {
    let mut body = String::new();
    let _ = req.as_reader().take(16 * 1024).read_to_string(&mut body);
    let (u, p) = (form_value(&body, "username").unwrap_or_default(), form_value(&body, "password").unwrap_or_default());
    // Compare hashes so the time taken doesn't hint at how much was right.
    if user.is_empty() || sha(&format!("{u}:{p}")) != sha(&format!("{user}:{pass}")) {
        crate::applog::warn("Web page: a sign-in with the wrong username or password".to_string());
        std::thread::sleep(Duration::from_millis(800)); // slow down guessing
        return redirect(req, "/?loginFailed=true", None);
    }
    let remember = form_value(&body, "rememberMe").is_some();
    let token = format!("{:032x}{:032x}", rand::random::<u128>(), rand::random::<u128>());
    let entry = Entry {
        token: sha(&token),
        cred: sha(&format!("{user}:{pass}")),
        expires: now() + if remember { REMEMBER_SECS } else { SHORT_SECS },
    };
    with_sessions(|list| {
        list.push(entry);
        save(list);
    });
    let mut cookie = format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict");
    if remember {
        cookie.push_str(&format!("; Max-Age={REMEMBER_SECS}"));
    }
    redirect(req, "/", Some(cookie));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_values_decode() {
        let body = "username=mid+gar&password=p%40ss%26word%25&rememberMe=on";
        assert_eq!(form_value(body, "username").as_deref(), Some("mid gar"));
        assert_eq!(form_value(body, "password").as_deref(), Some("p@ss&word%"));
        assert!(form_value(body, "rememberMe").is_some());
        assert!(form_value("username=a&password=b", "rememberMe").is_none());
        assert_eq!(decode("100%"), "100%");
    }

    #[test]
    fn page_has_theme_slot_and_form() {
        assert!(PAGE.contains("'_THEME_'"));
        assert!(PAGE.contains("action=\"/lifestream/login\""));
        assert!(PAGE.contains("name=\"username\"") && PAGE.contains("name=\"password\""));
    }
}
