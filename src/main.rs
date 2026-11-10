#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod applog;
mod backup;
mod blocklist;
mod core;
mod engine;
mod instance;
mod netif;
mod rss;
mod settings;
#[cfg(windows)]
mod tray;
mod ui;
mod updater;
#[cfg(windows)]
mod winint;

use eframe::egui;

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

    let icon = image::load_from_memory(include_bytes!("../resources/icon_256.png"))
        .ok()
        .map(|i| {
            let i = i.to_rgba8();
            let (w, h) = i.dimensions();
            egui::IconData { rgba: i.into_raw(), width: w, height: h }
        });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Lifestream")
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
            Ok(Box::new(ui::App::new(cc, core, join, settings, rx, start_hidden, restored)))
        }),
    )
}
