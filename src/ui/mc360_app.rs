//! The MC360 look for Lifestream: one skin blade per page, each with a menu
//! of that page's parts and actions. Picking an entry opens the page on the
//! blade (drawn exactly as in the other looks) and does what the entry says.

use super::mc360::{Blade, Event, Info, Item, Mc360, Tint};
use super::theme as ff;
use super::{App, Filter, Page, MENU};
use crate::core::Cmd;
use eframe::egui;

// Entry numbers: page * 100 + what to do there.
const OPEN: u32 = 0;
const T_ALL: u32 = 1;
const T_DOWNLOADING: u32 = 2;
const T_FINISHED: u32 = 3;
const T_PAUSED: u32 = 4;
const T_ADD: u32 = 10;
const T_REMOVE_ALL: u32 = 11;
const R_CHECK: u32 = 1;
const S_UPDATES: u32 = 1;

const LOOKS: [ff::Theme; 5] = [ff::Theme::FfMenu, ff::Theme::GreenDash, ff::Theme::Blades, ff::Theme::Glass, ff::Theme::Mc360];

fn page_no(p: Page) -> u32 {
    match p {
        Page::Torrents => 0,
        Page::Rss => 1,
        Page::Connection => 2,
        Page::Settings => 3,
        Page::Log => 4,
    }
}

fn id(p: Page, what: u32) -> u32 {
    page_no(p) * 100 + what
}

impl App {
    fn mc360_blades(&self) -> Vec<(Page, Blade)> {
        let advanced = self.settings.advanced_mode;
        MENU.iter()
            .filter(|m| advanced || !m.2)
            .map(|(page, _, _)| {
                let (key, tint, items) = match page {
                    Page::Torrents => (
                        "torrents",
                        Tint::Green,
                        vec![
                            Item::new(id(*page, T_ALL), "All torrents", "list", "Every torrent, with pause, resume, move and remove."),
                            Item::new(id(*page, T_DOWNLOADING), "Downloading", "down", "Only torrents that are still downloading."),
                            Item::new(id(*page, T_FINISHED), "Finished", "up", "Torrents that are done, seeding or stopped."),
                            Item::new(id(*page, T_PAUSED), "Paused", "clock", "Torrents you paused."),
                            Item::new(id(*page, T_ADD), "Add torrent…", "plus", "Paste a magnet or pick a file."),
                            Item::new(id(*page, T_REMOVE_ALL), "Remove all, keep files…", "trash", "One confirm, files stay."),
                        ],
                    ),
                    Page::Rss => (
                        "rss",
                        Tint::Orange,
                        vec![
                            Item::new(id(*page, OPEN), "Feeds and rules", "rss", "Feeds are checked on a timer; rules pick what gets downloaded."),
                            Item::new(id(*page, R_CHECK), "Check feeds now", "refresh", "Look at every feed right away."),
                        ],
                    ),
                    Page::Connection => (
                        "connection",
                        Tint::Blue,
                        vec![Item::new(id(*page, OPEN), "VPN and network", "shield", "Kill switch, adapter, ports and speed limits.")],
                    ),
                    Page::Settings => (
                        "settings",
                        Tint::Purple,
                        vec![
                            Item::new(id(*page, OPEN), "All settings", "gear", "Folders, seeding, look, backups and more."),
                            Item::new(id(*page, S_UPDATES), "Check for updates", "refresh", "Look for a newer Lifestream now."),
                        ],
                    ),
                    Page::Log => ("log", Tint::Grey, vec![Item::new(id(*page, OPEN), "Log", "doc", "What Lifestream has been doing.")]),
                };
                (*page, Blade { key: key.into(), tint, items })
            })
            .collect()
    }

    /// The entry that matches what the page shows right now.
    fn mc360_current(&self, page: Page) -> u32 {
        match page {
            Page::Torrents => id(
                page,
                match self.filter {
                    Filter::All => T_ALL,
                    Filter::Downloading => T_DOWNLOADING,
                    Filter::Finished => T_FINISHED,
                    Filter::Paused => T_PAUSED,
                },
            ),
            _ => id(page, OPEN),
        }
    }

    fn mc360_check_updates(&mut self, ctx: &egui::Context) {
        self.manual_update_check = true;
        self.updater.dismissed.store(false, std::sync::atomic::Ordering::SeqCst);
        crate::updater::check_in_background(std::sync::Arc::clone(&self.updater), ctx.clone());
    }

    pub(crate) fn mc360_layout(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.settings.advanced_mode && self.page == Page::Log {
            self.page = Page::Torrents;
        }
        let defs = self.mc360_blades();
        let blades: Vec<Blade> = defs.iter().map(|d| d.1.clone()).collect();
        let mut mc = self.mc.take().unwrap_or_else(|| {
            let mut m = Mc360::new(Some(crate::settings::data_dir().join("mc360.txt")));
            let b = defs.iter().position(|d| d.0 == self.page).unwrap_or(0);
            m.home(b);
            m
        });
        // The page changed somewhere else (the status chip, the tray, a link): follow it.
        let cur_b = defs.iter().position(|d| d.0 == self.page).unwrap_or(0);
        if mc.tier() == 2 && mc.blade() != cur_b {
            let item = self.mc360_current(self.page);
            mc.show_page(cur_b, item, &blades[cur_b].items);
        }

        // Esc and the mouse's back button close a dialog or the files pop-up first.
        let typing = ctx.egui_wants_keyboard_input();
        let esc = !typing && ui.input(|i| i.key_pressed(egui::Key::Escape));
        let mback = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1));
        let mut swallowed = false;
        if esc || mback {
            let mut used = true;
            if self.confirm_remove_all {
                self.confirm_remove_all = false;
            } else if self.confirm_remove.is_some() {
                self.confirm_remove = None;
            } else if self.popup.is_some() && !self.add_open {
                self.popup = None;
                self.core.send(Cmd::WatchFiles(None));
            } else {
                used = false;
            }
            if used {
                swallowed = true;
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
        }
        let prot = self.protection();
        let held = self.snap.kill_hold || self.snap.user_paused;
        let (dl, ul) = if held { (0, 0) } else { (self.snap.dl_rate, self.snap.ul_rate) };
        let downloading = self.snap.torrents.iter().filter(|t| t.info.state == 2 && !t.info.paused).count();
        let card = vec![
            prot.1.clone(),
            format!("↓ {} · ↑ {}", crate::core::human_rate(dl), crate::core::human_rate(ul)),
            format!("{} torrents · {downloading} downloading", self.snap.torrents.len()),
        ];
        let look_names: Vec<&str> = LOOKS.iter().map(|t| t.label()).collect();
        let ver = crate::updater::APP_VERSION.to_string();
        let info = Info {
            app: "Lifestream",
            ver: &ver,
            plat: "Windows",
            icon: "swirl",
            card,
            looks: &look_names,
            look: 4,
            popup_open: swallowed || self.confirm_remove.is_some() || self.confirm_remove_all || self.add_open || self.popup.is_some() || self.picking.is_some(),
            app_calm: self.settings.reduce_motion,
            checking: self.manual_update_check,
        };
        let mut before_back = || false;
        let events = mc.ui(ui, &blades, &info, &mut before_back, &mut |child| {
            ff::set_hide_headings(true);
            self.render_page_body(child);
            ff::set_hide_headings(false);
        });
        for ev in events {
            match ev {
                Event::Open(b, item) => {
                    let page = defs[b].0;
                    self.page = page;
                    let what = item % 100;
                    match (page, what) {
                        (Page::Torrents, T_ALL) => self.filter = Filter::All,
                        (Page::Torrents, T_DOWNLOADING) => self.filter = Filter::Downloading,
                        (Page::Torrents, T_FINISHED) => self.filter = Filter::Finished,
                        (Page::Torrents, T_PAUSED) => self.filter = Filter::Paused,
                        (Page::Torrents, T_ADD) => self.add_open = true,
                        (Page::Torrents, T_REMOVE_ALL) => self.confirm_remove_all = true,
                        (Page::Rss, R_CHECK) => self.core.send(Cmd::RssCheckNow),
                        (Page::Settings, S_UPDATES) => self.mc360_check_updates(&ctx),
                        _ => {}
                    }
                }
                Event::Home(_) => {}
                Event::SetLook(i) => {
                    let theme = LOOKS[i.min(LOOKS.len() - 1)];
                    ff::set_theme(theme);
                    ff::apply_style(&ctx);
                    self.settings.theme = theme.key().to_string();
                    self.changed();
                }
                Event::CheckUpdates => self.mc360_check_updates(&ctx),
            }
        }
        self.mc = Some(mc);
        let full = ui.max_rect();
        self.files_popup(&ctx, full.shrink(40.0));
        self.add_dialog(&ctx);
        self.remove_dialog(&ctx);
        self.toasts(&ctx, egui::pos2(full.right() - 40.0, full.top() + 90.0));
    }
}
