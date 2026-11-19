//! The MC360 look's files for the web page (pictures, swirl video, sounds, font and engine),
//! served from /mc360/. Byte ranges are honoured so the swirl video can loop and seek.

use tiny_http::{Header, Request, Response};

const FILES: &[(&str, &[u8], &str)] = &[
    ("blades-carbon.png", include_bytes!("../web/mc360/blades-carbon.png"), "image/png"),
    ("blades-default.png", include_bytes!("../web/mc360/blades-default.png"), "image/png"),
    ("blades-glass.png", include_bytes!("../web/mc360/blades-glass.png"), "image/png"),
    ("highlights.png", include_bytes!("../web/mc360/highlights.png"), "image/png"),
    ("icons.png", include_bytes!("../web/mc360/icons.png"), "image/png"),
    ("mc360.css", include_bytes!("../web/mc360/mc360.css"), "text/css; charset=UTF-8"),
    ("mc360.js", include_bytes!("../web/mc360/mc360.js"), "text/javascript; charset=UTF-8"),
    ("mc360.ttf", include_bytes!("../web/mc360/mc360.ttf"), "font/ttf"),
    ("notify-ring.webp", include_bytes!("../web/mc360/notify-ring.webp"), "image/webp"),
    ("shine.webp", include_bytes!("../web/mc360/shine.webp"), "image/webp"),
    ("snd/back.mp3", include_bytes!("../web/mc360/snd/back.mp3"), "audio/mpeg"),
    ("snd/bladeslide_1.mp3", include_bytes!("../web/mc360/snd/bladeslide_1.mp3"), "audio/mpeg"),
    ("snd/bladeslide_2.mp3", include_bytes!("../web/mc360/snd/bladeslide_2.mp3"), "audio/mpeg"),
    ("snd/bladeslide_3.mp3", include_bytes!("../web/mc360/snd/bladeslide_3.mp3"), "audio/mpeg"),
    ("snd/bladeslide_4.mp3", include_bytes!("../web/mc360/snd/bladeslide_4.mp3"), "audio/mpeg"),
    ("snd/cursor.mp3", include_bytes!("../web/mc360/snd/cursor.mp3"), "audio/mpeg"),
    ("snd/guide_close.mp3", include_bytes!("../web/mc360/snd/guide_close.mp3"), "audio/mpeg"),
    ("snd/guide_open.mp3", include_bytes!("../web/mc360/snd/guide_open.mp3"), "audio/mpeg"),
    ("snd/notification.mp3", include_bytes!("../web/mc360/snd/notification.mp3"), "audio/mpeg"),
    ("snd/select.mp3", include_bytes!("../web/mc360/snd/select.mp3"), "audio/mpeg"),
    ("snd/startup.mp3", include_bytes!("../web/mc360/snd/startup.mp3"), "audio/mpeg"),
    ("snd/tier2_enter.mp3", include_bytes!("../web/mc360/snd/tier2_enter.mp3"), "audio/mpeg"),
    ("snd/tier2_exit.mp3", include_bytes!("../web/mc360/snd/tier2_exit.mp3"), "audio/mpeg"),
    ("snd/tier3_enter.mp3", include_bytes!("../web/mc360/snd/tier3_enter.mp3"), "audio/mpeg"),
    ("snd/tier3_exit.mp3", include_bytes!("../web/mc360/snd/tier3_exit.mp3"), "audio/mpeg"),
    ("swirl-carbon.jpg", include_bytes!("../web/mc360/swirl-carbon.jpg"), "image/jpeg"),
    ("swirl-carbon.mp4", include_bytes!("../web/mc360/swirl-carbon.mp4"), "video/mp4"),
    ("swirl-default.jpg", include_bytes!("../web/mc360/swirl-default.jpg"), "image/jpeg"),
    ("swirl-default.mp4", include_bytes!("../web/mc360/swirl-default.mp4"), "video/mp4"),
    ("swirl-glass.jpg", include_bytes!("../web/mc360/swirl-glass.jpg"), "image/jpeg"),
    ("swirl-glass.mp4", include_bytes!("../web/mc360/swirl-glass.mp4"), "video/mp4"),
    ("ui.png", include_bytes!("../web/mc360/ui.png"), "image/png"),
];

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

/// Answers `/mc360/<file>`; false when there's no such file.
pub fn serve(req: Request, path: &str) -> Result<(), Request> {
    let name = path.trim_start_matches("/mc360/");
    let Some((_, data, kind)) = FILES.iter().find(|f| f.0 == name) else { return Err(req) };
    let range = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .and_then(|h| h.value.as_str().strip_prefix("bytes=").map(str::to_string));
    let len = data.len();
    if let Some(r) = range {
        let (a, b) = r.split_once('-').unwrap_or(("", ""));
        let (start, end) = match (a.trim().parse::<usize>().ok(), b.trim().parse::<usize>().ok()) {
            (Some(s), Some(e)) => (s, e.min(len.saturating_sub(1))),
            (Some(s), None) => (s, len.saturating_sub(1)),
            (None, Some(n)) => (len.saturating_sub(n), len.saturating_sub(1)),
            _ => (0, len.saturating_sub(1)),
        };
        if start >= len || start > end {
            let _ = req.respond(Response::empty(416).with_header(header("Content-Range", &format!("bytes */{len}"))));
            return Ok(());
        }
        let resp = Response::from_data(&data[start..=end])
            .with_status_code(206)
            .with_header(header("Content-Type", kind))
            .with_header(header("Accept-Ranges", "bytes"))
            .with_header(header("Content-Range", &format!("bytes {start}-{end}/{len}")))
            .with_header(header("Cache-Control", "max-age=86400"));
        let _ = req.respond(resp);
        return Ok(());
    }
    let resp = Response::from_data(*data)
        .with_header(header("Content-Type", kind))
        .with_header(header("Accept-Ranges", "bytes"))
        .with_header(header("Cache-Control", "max-age=86400"));
    let _ = req.respond(resp);
    Ok(())
}
