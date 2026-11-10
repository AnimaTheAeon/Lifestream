#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// The headless (Synology) build leaves the desktop-only helpers unused.
#![cfg_attr(not(feature = "gui"), allow(dead_code))]

mod applog;
mod backup;
mod blocklist;
mod core;
mod engine;
mod instance;
mod netif;
mod rpc;
mod rss;
mod settings;
#[cfg(all(windows, feature = "gui"))]
mod tray;
#[cfg(feature = "gui")]
mod ui;
mod updater;
mod web;
#[cfg(windows)]
mod winint;

#[cfg(feature = "gui")]
use eframe::egui;

/// The Synology (headless) version: no window, controlled from the web page.
#[cfg(not(feature = "gui"))]
fn main() {
    use std::time::Duration;

    applog::info(format!("Lifestream {} (headless) starting.", updater::APP_VERSION));
    let restored = backup::apply_pending_restore();
    if let Some(msg) = &restored {
        applog::info(msg.clone());
    }
    let fresh = !settings::Settings::path().exists();
    let mut settings = settings::Settings::load();
    // First run on a Synology: start out in the usual shared downloads folder.
    if fresh && std::path::Path::new("/volume1/downloads").is_dir() {
        settings.download_dir = "/volume1/downloads/Lifestream".into();
        settings.incomplete_dir = "/volume1/downloads/Lifestream/Incomplete".into();
        settings.save();
    }
    // The web page is the only way in, so it must answer on the network.
    if !settings.rpc_enabled || !settings.rpc_allow_lan {
        settings.rpc_enabled = true;
        settings.rpc_allow_lan = true;
        settings.save();
    }
    let (core, join) = core::start(settings.clone());
    rpc::start(core.clone(), &settings);

    // DSM stops the package by creating this file (see the package's
    // start-stop-status script), so progress is saved before exiting.
    let stop_file = settings::data_dir().join("stop-request");
    let _ = std::fs::remove_file(&stop_file);
    while !join.is_finished() {
        if stop_file.exists() {
            let _ = std::fs::remove_file(&stop_file);
            core.send(core::Cmd::Shutdown);
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = join.join();
    applog::info("Lifestream stopped.");
}

#[cfg(feature = "gui")]
fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let start_hidden = args.iter().any(|a| a == "--tray");
    let after_update = args.iter().any(|a| a == updater::AFTER_UPDATE_ARG);
    let open_args: Vec<String> = args.iter().filter(|a| !a.starts_with("--")).cloned().collect();

    // Hand off to a running copy if there is one.
    let Some(listener) = instance::claim(&open_args, after_update) else {
        return Ok(());
    };

    updater::cleanup_old_exe();
    let restored = backup::apply_pending_restore();
    if let Some(msg) = &restored {
        applog::info(msg.clone());
    }
    #[cfg(windows)]
    winint::hold_install_mutex();
    #[cfg(windows)]
    winint::register_notifications();
    let mut settings = settings::Settings::load();
    // The installer leaves a note when "Start with Windows" was ticked there.
    let marker = settings::data_dir().join("installer-autostart");
    if marker.exists() {
        settings.start_with_windows = true;
        settings.save();
        let _ = std::fs::remove_file(&marker);
    }
    #[cfg(windows)]
    winint::set_autostart(settings.start_with_windows);

    let (core, join) = core::start(settings.clone());
    rpc::start(core.clone(), &settings);

    let icon = image::load_from_memory(include_bytes!("../resources/icon_256.png"))
        .ok()
        .map(|i| {
            let i = i.to_rgba8();
            let (w, h) = i.dimensions();
            egui::IconData { rgba: i.into_raw(), width: w, height: h }
        });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("Lifestream {}", updater::APP_VERSION))
        .with_inner_size([1180.0, 800.0])
        .with_min_inner_size([860.0, 600.0])
        .with_drag_and_drop(true);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };

    eframe::run_native(
        "Lifestream",
        options,
        Box::new(move |cc| {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            instance::serve(listener, tx.clone(), move || ctx.request_repaint());
            for a in open_args {
                let _ = tx.send(instance::Incoming::Open(a));
            }
            Ok(Box::new(ui::App::new(cc, core, join, settings, rx, tx, start_hidden, restored)))
        }),
    )
}
