//! System tray icon with quick toggles (Windows).

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::core::{Cmd, CoreHandle};

pub static QUIT: AtomicBool = AtomicBool::new(false);

pub fn show_window(ctx: &eframe::egui::Context) {
    use eframe::egui::ViewportCommand;
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}

pub fn hide_window(ctx: &eframe::egui::Context) {
    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Visible(false));
}

pub struct Tray {
    icon: TrayIcon,
    pause: MenuItem,
    turtle: CheckMenuItem,
    rss: CheckMenuItem,
    kill: MenuItem,
    status: MenuItem,
    last_tip: String,
}

struct Ids {
    pause: MenuId,
    turtle: MenuId,
    rss: MenuId,
    clipboard: MenuId,
    folder: MenuId,
    show: MenuId,
    quit: MenuId,
}

impl Tray {
    pub fn new(core: CoreHandle, ctx: eframe::egui::Context, download_dir: Arc<std::sync::Mutex<String>>) -> Option<Self> {
        let status = MenuItem::new("Lifestream", false, None);
        let pause = MenuItem::new("Pause all", true, None);
        let turtle = CheckMenuItem::new("Turtle mode", true, false, None);
        let rss = CheckMenuItem::new("RSS auto-download", true, true, None);
        let kill = MenuItem::new("Kill switch: armed", false, None);
        let clipboard = MenuItem::new("Add magnet from clipboard", true, None);
        let folder = MenuItem::new("Open download folder", true, None);
        let show = MenuItem::new("Show window", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        let (s1, s2, s3) = (PredefinedMenuItem::separator(), PredefinedMenuItem::separator(), PredefinedMenuItem::separator());
        let items: [&dyn IsMenuItem; 12] =
            [&status, &s1, &pause, &turtle, &rss, &kill, &s2, &clipboard, &folder, &show, &s3, &quit];
        menu.append_items(&items).ok()?;

        let ids = Ids {
            pause: pause.id().clone(),
            turtle: turtle.id().clone(),
            rss: rss.id().clone(),
            clipboard: clipboard.id().clone(),
            folder: folder.id().clone(),
            show: show.id().clone(),
            quit: quit.id().clone(),
        };

        let img = image::load_from_memory(include_bytes!("../resources/icon_64.png")).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let icon = tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Lifestream")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        let ctx2 = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_window(&ctx2);
            }
        }));

        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let snap = core.snapshot();
            if e.id == ids.pause {
                core.send(if snap.user_paused { Cmd::ResumeAll } else { Cmd::PauseAll });
            } else if e.id == ids.turtle {
                core.send(Cmd::SetTurtle(!snap.turtle_active));
            } else if e.id == ids.rss {
                let on = crate::settings::Settings::load().rss_enabled;
                core.send(Cmd::SetRss(!on));
            } else if e.id == ids.clipboard {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                    let text = text.trim().to_string();
                    if text.starts_with("magnet:") {
                        core.send(Cmd::AddFromUrl { url: text, category: String::new() });
                    }
                }
            } else if e.id == ids.folder {
                let dir = download_dir.lock().map(|d| d.clone()).unwrap_or_default();
                crate::ui::open_path(&dir);
            } else if e.id == ids.show {
                show_window(&ctx);
            } else if e.id == ids.quit {
                QUIT.store(true, Ordering::SeqCst);
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }));

        Some(Self { icon: tray, pause, turtle, rss, kill, status, last_tip: String::new() })
    }

    /// Keep labels and the tooltip in step with the app.
    pub fn update(&mut self, snap: &crate::core::Snapshot, rss_on: bool, kill_on: bool, protection: &str) {
        let tip = format!(
            "Lifestream · VPN: {}\nDown {}  ·  Up {}",
            protection.to_lowercase(),
            crate::core::human_rate(snap.dl_rate),
            crate::core::human_rate(snap.ul_rate),
        );
        if tip != self.last_tip {
            let _ = self.icon.set_tooltip(Some(&tip));
            self.status.set_text(format!(
                "Down {}  ·  Up {}",
                crate::core::human_rate(snap.dl_rate),
                crate::core::human_rate(snap.ul_rate)
            ));
            self.last_tip = tip;
        }
        self.pause.set_text(if snap.user_paused { "Resume all" } else { "Pause all" });
        self.turtle.set_checked(snap.turtle_active);
        self.rss.set_checked(rss_on);
        self.kill.set_text(if !kill_on {
            "Kill switch: off"
        } else if snap.kill_hold {
            "Kill switch: tripped (paused)"
        } else {
            "Kill switch: armed"
        });
    }
}
