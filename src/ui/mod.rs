//! The party-menu style window.

mod pages;
pub mod theme;
mod widgets;

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, pos2, Rect, RichText, Vec2};

use crate::core::{human_rate, AddChoice, Cmd, CoreHandle, Snapshot};
use crate::engine::ffi::TorrentPreview;
use crate::instance::Incoming;
use crate::settings::Settings;
use theme::{self as ff, LABEL, TEXT, TEXT_DIM};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Torrents,
    Add,
    Rss,
    Speed,
    Network,
    Privacy,
    Seeding,
    Config,
    Log,
}

const MENU: &[(Page, &str, bool)] = &[
    (Page::Torrents, "Torrents", false),
    (Page::Add, "Add", false),
    (Page::Rss, "RSS", false),
    (Page::Speed, "Speed", false),
    (Page::Network, "Network", false),
    (Page::Privacy, "Privacy", false),
    (Page::Seeding, "Seeding", false),
    (Page::Config, "Config", false),
    (Page::Log, "Log", true),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Downloading,
    Seeding,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Queue,
    Name,
    Health,
    DownSpeed,
    Ratio,
    Added,
}

pub struct AddState {
    pub magnet: String,
    pub preview: Option<(Vec<u8>, TorrentPreview, Vec<bool>)>,
    pub category: String,
    pub choice: AddChoice,
    pub error: Option<String>,
}

pub struct App {
    pub core: CoreHandle,
    core_join: Option<std::thread::JoinHandle<()>>,
    pub settings: Settings,
    dirty_since: Option<Instant>,
    pub page: Page,
    menu_touched: bool,
    pub snap: Snapshot,
    pub filter: Filter,
    pub sort: Sort,
    pub popup: Option<String>,
    pub popup_tab: u8,
    pub confirm_remove: Option<(String, String)>,
    pub add: AddState,
    incoming: Receiver<Incoming>,
    pub rss_rule: usize,
    pub new_feed: (String, String),
    pub tracker_input: String,
    pub adapters: Vec<crate::netif::Adapter>,
    pub adapters_at: Option<Instant>,
    pub notice: Option<(String, Instant)>,
    pub start_hidden: bool,
    frames: u64,
    pub download_dir: Arc<Mutex<String>>,
    #[cfg(windows)]
    tray: Option<crate::tray::Tray>,
    pub restore_pending: bool,
    pub updater: Arc<crate::updater::Updater>,
    pub manual_update_check: bool,
    quitting_for_update: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        core: CoreHandle,
        core_join: std::thread::JoinHandle<()>,
        settings: Settings,
        incoming: Receiver<Incoming>,
        start_hidden: bool,
        startup_notice: Option<String>,
    ) -> Self {
        configure_fonts(&cc.egui_ctx);
        ff::apply_style(&cc.egui_ctx);
        let download_dir = Arc::new(Mutex::new(settings.download_dir.clone()));

        #[cfg(windows)]
        let tray = crate::tray::Tray::new(core.clone(), cc.egui_ctx.clone(), Arc::clone(&download_dir));

        let choice = AddChoice {
            paused: settings.start_paused,
            sequential: settings.default_sequential,
            first_last: settings.default_first_last,
            skip_check: false,
        };
        Self {
            core,
            core_join: Some(core_join),
            settings,
            dirty_since: None,
            page: Page::Torrents,
            menu_touched: false,
            snap: Snapshot::default(),
            filter: Filter::All,
            sort: Sort::Queue,
            popup: None,
            popup_tab: 0,
            confirm_remove: None,
            add: AddState { magnet: String::new(), preview: None, category: String::new(), choice, error: None },
            incoming,
            rss_rule: 0,
            new_feed: (String::new(), String::new()),
            tracker_input: String::new(),
            adapters: Vec::new(),
            adapters_at: None,
            notice: startup_notice.map(|n| (n, Instant::now())),
            start_hidden,
            frames: 0,
            download_dir,
            #[cfg(windows)]
            tray,
            restore_pending: false,
            updater: {
                let u = Arc::new(crate::updater::Updater::default());
                crate::updater::check_in_background(Arc::clone(&u), cc.egui_ctx.clone());
                u
            },
            manual_update_check: false,
            quitting_for_update: false,
        }
    }

    /// Mark settings as edited; they're sent to the core once typing stops.
    pub fn changed(&mut self) {
        self.dirty_since = Some(Instant::now());
    }

    fn flush_settings(&mut self, force: bool) {
        if let Some(t) = self.dirty_since {
            if force || t.elapsed() > Duration::from_millis(400) {
                self.dirty_since = None;
                if let Ok(mut d) = self.download_dir.lock() {
                    *d = self.settings.download_dir.clone();
                }
                self.core.send(Cmd::ApplySettings(Box::new(self.settings.clone())));
            }
        }
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    pub fn refresh_adapters(&mut self, force: bool) {
        if force || self.adapters_at.map(|t| t.elapsed() > Duration::from_secs(4)).unwrap_or(true) {
            self.adapters = crate::netif::list();
            self.adapters_at = Some(Instant::now());
        }
    }

    fn handle_incoming(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.incoming.try_recv() {
            match msg {
                Incoming::Show => show(ctx),
                Incoming::Open(arg) => {
                    show(ctx);
                    self.open_argument(&arg);
                }
            }
        }
    }

    /// A magnet link or .torrent path from the command line or the browser.
    pub fn open_argument(&mut self, arg: &str) {
        let arg = arg.trim();
        if arg.starts_with("magnet:") {
            self.add.magnet = arg.to_string();
            self.add.preview = None;
            self.page = Page::Add;
            self.menu_touched = true;
        } else if arg.to_lowercase().ends_with(".torrent") {
            match std::fs::read(arg) {
                Ok(data) => self.preview_bytes(data),
                Err(e) => self.notify(format!("Couldn't open {arg}: {e}")),
            }
        }
    }

    pub fn preview_bytes(&mut self, data: Vec<u8>) {
        match crate::engine::ffi::parse_torrent(&data) {
            Ok(p) => {
                let n = p.files.len();
                self.add.preview = Some((data, p, vec![true; n]));
                self.add.error = None;
                self.page = Page::Add;
                self.menu_touched = true;
            }
            Err(e) => self.notify(e.what().to_string()),
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let paths: Vec<_> = dropped
            .iter()
            .map(|f| f.path().to_path_buf())
            .filter(|p| p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("torrent")) == Some(true))
            .collect();
        match paths.len() {
            0 => self.notify("Only .torrent files can be dropped here."),
            1 => {
                if let Ok(data) = std::fs::read(&paths[0]) {
                    self.preview_bytes(data);
                }
            }
            n => {
                let choice = self.add.choice;
                for p in paths {
                    if let Ok(data) = std::fs::read(&p) {
                        self.core.send(Cmd::AddTorrent { data, category: String::new(), choice, priorities: None });
                    }
                }
                self.notify(format!("Adding {n} torrents…"));
            }
        }
    }

    fn status_line(&self) -> String {
        let s = &self.snap;
        if let Some(e) = &s.engine_error {
            return e.clone();
        }
        if s.kill_hold {
            return s.kill_reason.clone();
        }
        if s.user_paused {
            return "Everything is paused. Resume from Torrents or the tray.".into();
        }
        let downloading = s.torrents.iter().filter(|t| t.info.state == 2 && !t.info.paused).count();
        let seeding = s.torrents.iter().filter(|t| t.info.state >= 3 && !t.info.paused).count();
        let mut parts = Vec::new();
        if !self.settings.adapter.is_empty() {
            let ip = s.adapter_ip.clone().unwrap_or_default();
            parts.push(format!("Bound to {} ({ip})", self.settings.adapter));
        }
        if self.settings.kill_switch {
            parts.push("Kill switch armed".into());
        }
        if s.listen_port > 0 {
            parts.push(format!("Port {}", s.listen_port));
        }
        parts.push(format!("{downloading} downloading, {seeding} seeding"));
        if s.turtle_active {
            parts.push("Turtle mode".into());
        }
        parts.join(" · ")
    }

    fn layout(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let gap = 12.0;
        let side_w = 210.0_f32.min(full.width() * 0.3);
        let bar_h = 48.0;
        let advanced = self.settings.advanced_mode;
        let items: Vec<_> = MENU.iter().filter(|m| advanced || !m.2).collect();

        let main = Rect::from_min_max(full.min, pos2(full.right() - side_w - gap, full.bottom() - bar_h - gap));
        let menu_h = items.len() as f32 * (ff::MENU_ITEM_HEIGHT + 6.0) + ff::WINDOW_PADDING as f32 * 2.0;
        let menu = Rect::from_min_size(pos2(main.right() + gap, full.top()), Vec2::new(side_w, menu_h));
        let info_h = 160.0;
        let info = Rect::from_min_max(pos2(menu.left(), main.bottom() - info_h), pos2(full.right(), main.bottom()));
        let bar = Rect::from_min_max(pos2(full.left(), full.bottom() - bar_h), full.max);

        ff::window_at(ui, menu, "menu", |ui| {
            for (page, label, _) in &items {
                let r = ff::menu_item(ui, label, self.menu_touched && self.page == *page, true);
                if r.clicked() {
                    self.menu_touched = true;
                    self.page = *page;
                }
            }
        });
        if !advanced && self.page == Page::Log {
            self.page = Page::Torrents;
        }

        let secs = self.snap.started.elapsed().as_secs();
        let time = format!("{:02}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60);
        let held = self.snap.kill_hold || self.snap.user_paused;
        let (dl, ul) = if held { (human_rate(0), human_rate(0)) } else { (human_rate(self.snap.dl_rate), human_rate(self.snap.ul_rate)) };
        let mut mode_clicked = false;
        ff::window_at(ui, info, "info", |ui| {
            info_row(ui, "Time", &time);
            info_row(ui, "DL", &dl);
            info_row(ui, "UL", &ul);
            ui.add_space(4.0);
            let label = if advanced { "MODE: ADVANCED" } else { "MODE: SIMPLE" };
            mode_clicked = ui
                .add_sized([ui.available_width(), 26.0], egui::Button::new(RichText::new(label).color(LABEL).strong()))
                .on_hover_text("Simple shows the settings most people need. Advanced shows everything, plus the Log.")
                .clicked();
        });
        if mode_clicked {
            self.settings.advanced_mode = !advanced;
            self.changed();
        }

        let status = self.status_line();
        let warn = self.snap.kill_hold || self.snap.engine_error.is_some();
        ff::window_at(ui, bar, "status", |ui| {
            ui.add_space(-4.0);
            ui.horizontal_centered(|ui| {
                ff::title(ui, &status, 16.0, if warn { ff::WARN } else { TEXT });
            });
        });

        ff::window_at(ui, main, "main", |ui| match self.page {
            Page::Torrents => self.page_torrents(ui),
            Page::Add => self.page_add(ui),
            Page::Rss => self.page_rss(ui),
            Page::Speed => self.page_speed(ui),
            Page::Network => self.page_network(ui),
            Page::Privacy => self.page_privacy(ui),
            Page::Seeding => self.page_seeding(ui),
            Page::Config => self.page_config(ui),
            Page::Log => self.page_log(ui),
        });

        self.update_prompt(ui.ctx());
        self.files_popup(ui.ctx(), main);
        self.remove_dialog(ui.ctx());
        self.toasts(ui.ctx(), pos2(main.right() - 14.0, main.top() + 14.0));
    }

    fn toasts(&mut self, ctx: &egui::Context, anchor: egui::Pos2) {
        let mut msgs = Vec::new();
        if let Some((t, at)) = &self.notice {
            if at.elapsed() < Duration::from_secs(5) {
                msgs.push(t.clone());
            }
        }
        if let Some((t, at)) = &self.snap.toast {
            if at.elapsed() < Duration::from_secs(5) {
                msgs.push(t.clone());
            }
        }
        let mut y = anchor.y;
        for (i, m) in msgs.iter().enumerate() {
            let area = egui::Area::new(egui::Id::new(("toast", i)))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::RIGHT_TOP)
                .fixed_pos(pos2(anchor.x, y))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.set_max_width(360.0);
                    ff::window(ui, |ui| {
                        ui.set_width(320.0);
                        ui.add(egui::Label::new(RichText::new(m).color(TEXT).strong()).wrap());
                    })
                });
            y += area.response.rect.height() + 10.0;
        }
    }
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ff::stat_label(ui, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ff::title(ui, value, 17.0, TEXT);
        });
    });
}

fn show(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

impl eframe::App for App {
    /// Runs every frame, and also while the window is hidden in the tray.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.frames == 2 && self.start_hidden {
            #[cfg(windows)]
            if self.tray.is_some() {
                crate::tray::hide_window(ctx);
            }
        }
        self.snap = self.core.snapshot();
        if self.dirty_since.is_none() && self.snap_settings_differ() {
            self.settings = crate::settings::Settings::load();
        }
        self.handle_incoming(ctx);

        #[cfg(windows)]
        if let Some(t) = self.tray.as_mut() {
            t.update(&self.snap, self.settings.rss_enabled, self.settings.kill_switch);
        }

        // Close button: hide to tray unless quitting.
        if ctx.input(|i| i.viewport().close_requested()) {
            #[cfg(windows)]
            {
                let quitting = crate::tray::QUIT.load(std::sync::atomic::Ordering::SeqCst);
                if !quitting && self.settings.close_to_tray && self.tray.is_some() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    crate::tray::hide_window(ctx);
                }
            }
        }
        self.watch_updater(ctx);
        self.flush_settings(false);
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_drops(&ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ff::BACKDROP).inner_margin(egui::Margin::same(14)))
            .show(ui, |ui| self.layout(ui));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_settings(true);
        self.core.send(Cmd::Shutdown);
        if let Some(j) = self.core_join.take() {
            let _ = j.join();
        }
    }
}

impl App {
    /// Quit once the update is staged; report manual checks.
    fn watch_updater(&mut self, ctx: &egui::Context) {
        use crate::updater::State;
        match self.updater.state() {
            State::Restarting if !self.quitting_for_update => {
                self.quitting_for_update = true;
                #[cfg(windows)]
                crate::tray::QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            State::Idle if self.manual_update_check => {
                self.manual_update_check = false;
                let err = self.updater.last_error.lock().ok().and_then(|e| e.clone());
                match err {
                    Some(e) => self.notify(format!("Couldn't check for updates: {e}")),
                    None => self.notify(format!("You're on the latest version (v{}).", crate::updater::APP_VERSION)),
                }
            }
            _ => {}
        }
    }

    /// Shown only when a newer version exists (or one is being installed).
    fn update_prompt(&mut self, ctx: &egui::Context) {
        use crate::updater::State;
        use std::sync::atomic::Ordering::SeqCst;
        let state = self.updater.state();
        let show = match &state {
            State::Available(_) => !self.updater.dismissed.load(SeqCst),
            State::Downloading(_) | State::Restarting | State::Failed(_) => true,
            _ => false,
        };
        if !show {
            return;
        }
        if matches!(state, State::Available(_)) {
            self.manual_update_check = false;
        }
        let mut install = None;
        let mut later = false;
        egui::Modal::new(egui::Id::new("update_prompt"))
            .frame(egui::Frame::NONE)
            .backdrop_color(egui::Color32::from_black_alpha(150))
            .show(ctx, |ui| {
                ff::window(ui, |ui| {
                    ui.set_width(460.0);
                    match &state {
                        State::Available(r) => {
                            ff::title(ui, "Update available", 24.0, TEXT);
                            ff::divider(ui);
                            ui.horizontal(|ui| {
                                ff::stat_label(ui, "CURRENT");
                                ui.label(RichText::new(format!("v{}", crate::updater::APP_VERSION)).strong());
                                ui.add_space(16.0);
                                ff::stat_label(ui, "NEW");
                                ui.label(RichText::new(format!("v{}", r.version)).strong().color(ff::GOOD));
                            });
                            if !r.notes.trim().is_empty() {
                                ui.add_space(6.0);
                                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                    ui.add(egui::Label::new(RichText::new(r.notes.trim()).color(TEXT_DIM)).wrap());
                                });
                            }
                            ui.add_space(6.0);
                            ui.label(dim(if crate::updater::is_installed() {
                                "Lifestream saves your torrents, closes, installs the update and reopens."
                            } else {
                                "Lifestream saves your torrents, swaps in the new version and reopens."
                            }));
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("Update now").color(ff::GOOD).strong()).clicked() {
                                    install = Some(r.clone());
                                }
                                if ui.button("Later").clicked() {
                                    later = true;
                                }
                                if !r.page_url.is_empty() {
                                    ui.hyperlink_to("What's new", &r.page_url);
                                }
                            });
                        }
                        State::Downloading(r) => {
                            ff::title(ui, &format!("Downloading v{}", r.version), 22.0, TEXT);
                            let done = self.updater.downloaded.load(SeqCst);
                            let total = self.updater.total.load(SeqCst).max(1);
                            ff::stat_bar(ui, done as f32 / total as f32, 420.0, ff::LABEL, egui::Color32::WHITE);
                            ui.label(dim(&format!(
                                "{} of {}",
                                crate::core::human_bytes(done as i64),
                                crate::core::human_bytes(total as i64)
                            )));
                        }
                        State::Restarting => {
                            ff::title(ui, "Installing update…", 22.0, TEXT);
                            ui.label(dim("Saving your torrents. Lifestream will reopen in a moment."));
                        }
                        State::Failed(e) => {
                            ff::title(ui, "Update didn't work", 22.0, ff::BAD);
                            ui.add(egui::Label::new(RichText::new(e).color(TEXT)).wrap());
                            ui.label(dim("Nothing was changed. You can try again later or grab it from the releases page."));
                            if ui.button("OK").clicked() {
                                later = true;
                            }
                        }
                        _ => {}
                    }
                });
            });
        if let Some(r) = install {
            crate::updater::install_in_background(Arc::clone(&self.updater), r, ctx.clone());
        }
        if later {
            self.updater.dismissed.store(true, SeqCst);
            if matches!(state, State::Failed(_)) {
                crate::updater::reset(&self.updater);
            }
        }
    }

    /// The core changes a few settings itself (tray toggles, VPN port).
    fn snap_settings_differ(&mut self) -> bool {
        // Cheap check: re-read only when the file changed.
        static LAST: Mutex<Option<std::time::SystemTime>> = Mutex::new(None);
        let modified = std::fs::metadata(Settings::path()).and_then(|m| m.modified()).ok();
        let mut last = LAST.lock().unwrap();
        if modified != *last {
            let first = last.is_none();
            *last = modified;
            return !first;
        }
        false
    }
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!("../../resources/Inter-Regular.ttf"))),
    );
    fonts.font_data.insert(
        "Nunito".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!("../../resources/Nunito-700.ttf"))),
    );
    if let Some(p) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        p.insert(0, "Inter".to_owned());
        p.insert(0, "Nunito".to_owned());
    }
    if let Some(m) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        m.push("Inter".to_owned());
    }
    ctx.set_fonts(fonts);
}

pub fn open_path(path: &str) {
    if path.trim().is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(path);
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

pub fn dim(text: &str) -> RichText {
    RichText::new(text).color(TEXT_DIM)
}
