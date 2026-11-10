//! Every menu page.

use eframe::egui::{self, Align, Color32, Layout, RichText};

use super::theme::{self as ff, BAD, GOOD, LABEL, TEXT, TEXT_DIM, WARN};
use super::widgets::{self as w, field, page_title, rate, row, section, toggle};
use super::{dim, open_path, App, Filter, Page, Sort};
use crate::core::{human_bytes, human_eta, human_rate, Cmd, TorrentView};
use crate::settings::{Category, Feed, RssRule, SeedAction, SeederDisplay};

const BLUE: (Color32, Color32) = (Color32::from_rgb(30, 110, 230), Color32::from_rgb(140, 230, 255));
const GREEN: (Color32, Color32) = (Color32::from_rgb(30, 154, 70), Color32::from_rgb(160, 240, 176));
const GREY: (Color32, Color32) = (Color32::from_rgb(90, 94, 120), Color32::from_rgb(164, 168, 192));
const GOLD: (Color32, Color32) = (Color32::from_rgb(200, 150, 20), Color32::from_rgb(255, 236, 140));

fn status_of(t: &TorrentView, held: bool) -> (&'static str, Color32) {
    let i = &t.info;
    if held && !i.paused {
        return ("HELD", WARN());
    }
    if !i.error.is_empty() {
        return ("ERROR", BAD());
    }
    if i.moving {
        return ("MOVING", WARN());
    }
    if i.paused {
        return if i.auto_managed { ("QUEUED", TEXT_DIM()) } else { ("PAUSED", WARN()) };
    }
    match i.state {
        0 => ("CHECKING", WARN()),
        1 => ("FINDING INFO", LABEL()),
        2 => ("DOWNLOADING", GOOD()),
        _ => ("SEEDING", GOOD()),
    }
}

impl App {
    // ===================================================================== Torrents

    pub(super) fn page_torrents(&mut self, ui: &mut egui::Ui) {
        let paused_all = self.snap.user_paused;
        ui.horizontal(|ui| {
            ff::title(ui, "Torrents", 26.0, TEXT());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Add torrent").clicked() {
                    self.page = Page::Add;
                    self.menu_touched = true;
                }
                let turtle = self.snap.turtle_active;
                if ui
                    .button(RichText::new(if turtle { "TURTLE: ON" } else { "TURTLE: OFF" }).color(if turtle { WARN() } else { TEXT() }).strong())
                    .on_hover_text("Switch to your slower turtle limits (set on the Speed page).")
                    .clicked()
                {
                    self.core.send(Cmd::SetTurtle(!turtle));
                }
                let (label, color) = if paused_all { ("ALL: PAUSED", WARN()) } else { ("ALL: RUNNING", GOOD()) };
                if ui
                    .button(RichText::new(label).color(color).strong())
                    .on_hover_text("Pause or resume every torrent at once.")
                    .clicked()
                {
                    self.core.send(if paused_all { Cmd::ResumeAll } else { Cmd::PauseAll });
                }
            });
        });
        ui.label(dim("Your party of downloads. Click a torrent's icon to see its files."));

        if self.snap.kill_hold {
            ui.add_space(4.0);
            ff::window(ui, |ui| {
                ui.set_width(ui.available_width());
                ff::title(ui, "Kill switch is holding everything", 16.0, WARN());
                ui.add(egui::Label::new(RichText::new(&self.snap.kill_reason).color(TEXT())).wrap());
                if ui.button("Go to Network").clicked() {
                    self.page = Page::Network;
                    self.menu_touched = true;
                }
            });
        }

        let ts = &self.snap.torrents;
        let n_dl = ts.iter().filter(|t| t.info.state <= 2 && !t.info.paused).count();
        let n_seed = ts.iter().filter(|t| t.info.state >= 3 && !t.info.paused).count();
        let n_paused = ts.iter().filter(|t| t.info.paused).count();
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (f, label) in [
                (Filter::All, format!("All {}", ts.len())),
                (Filter::Downloading, format!("Downloading {n_dl}")),
                (Filter::Seeding, format!("Seeding {n_seed}")),
                (Filter::Paused, format!("Paused {n_paused}")),
            ] {
                if w::pill(ui, &label, self.filter == f).clicked() {
                    self.filter = f;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("sort")
                    .selected_text(sort_name(self.sort))
                    .show_ui(ui, |ui| {
                        for s in [Sort::Queue, Sort::Name, Sort::Health, Sort::DownSpeed, Sort::Ratio, Sort::Added] {
                            ui.selectable_value(&mut self.sort, s, sort_name(s));
                        }
                    });
                ff::stat_label(ui, "SORT");
            });
        });
        ff::divider(ui);

        let mut list: Vec<TorrentView> = self
            .snap
            .torrents
            .iter()
            .filter(|t| match self.filter {
                Filter::All => true,
                Filter::Downloading => t.info.state <= 2 && !t.info.paused,
                Filter::Seeding => t.info.state >= 3 && !t.info.paused,
                Filter::Paused => t.info.paused,
            })
            .cloned()
            .collect();
        match self.sort {
            Sort::Queue => {}
            Sort::Name => list.sort_by_key(|t| t.info.name.to_lowercase()),
            Sort::Health => list.sort_by(|a, b| {
                let r = |t: &TorrentView| t.info.swarm_seeds as f32 / (t.info.swarm_peers.max(0) as f32 + 1.0);
                r(a).partial_cmp(&r(b)).unwrap_or(std::cmp::Ordering::Equal)
            }),
            Sort::DownSpeed => list.sort_by_key(|t| -t.info.dl_rate),
            Sort::Ratio => list.sort_by(|a, b| b.ratio.partial_cmp(&a.ratio).unwrap_or(std::cmp::Ordering::Equal)),
            Sort::Added => list.sort_by_key(|t| -t.info.added_time),
        }

        egui::ScrollArea::vertical()
            .id_salt("torrents")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if list.is_empty() {
                    ui.add_space(30.0);
                    ui.vertical_centered(|ui| {
                        ff::title(ui, if self.snap.torrents.is_empty() { "No torrents yet" } else { "Nothing here" }, 20.0, TEXT_DIM());
                        ui.label(dim("Add a magnet link or a .torrent file, or drop one onto this window."));
                    });
                }
                for t in &list {
                    self.torrent_row(ui, t);
                }
            });
    }

    fn torrent_row(&mut self, ui: &mut egui::Ui, t: &TorrentView) {
        let i = &t.info;
        let held = self.snap.kill_hold || self.snap.user_paused;
        let (status, color) = status_of(t, held);
        let letter: String = i.name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default();
        let active = !i.paused && i.error.is_empty();
        let advanced = self.settings.advanced_mode;
        ui.add_space(6.0);
        let row = ui.horizontal(|ui| {
            let e = ff::emblem(ui, &letter, 58.0, active).on_hover_text("Show files");
            if e.clicked() {
                self.popup = Some(i.hash.clone());
                self.popup_tab = 0;
                self.core.send(Cmd::WatchFiles(Some(i.hash.clone())));
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let max_name = (ui.available_width() - 330.0).max(120.0);
                    ui.scope(|ui| {
                        ui.set_max_width(max_name);
                        ui.add(egui::Label::new(RichText::new(&i.name).color(TEXT()).size(18.0).strong()).truncate())
                            .on_hover_text(&i.name);
                    });
                    ui.add_space(8.0);
                    ff::title(ui, status, 13.0, color);
                    if i.is_private {
                        ff::tag(ui, "PRIVATE", WARN()).on_hover_text(
                            "From a private tracker: DHT, peer exchange and local discovery are off for it, and it only talks to its own tracker.",
                        );
                    }
                    if !t.category.is_empty() {
                        ff::tag(ui, &t.category.to_uppercase(), LABEL());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.menu_button("More", |ui| self.torrent_menu(ui, t));
                        let (label, tip) = if i.paused { ("Resume", "Start this torrent") } else { ("Pause", "Pause this torrent") };
                        if ui.small_button(label).on_hover_text(tip).clicked() {
                            self.core.send(if i.paused { Cmd::Resume(i.hash.clone()) } else { Cmd::Pause(i.hash.clone()) });
                        }
                        ui.label(dim(&human_bytes(i.total_wanted)));
                    });
                });
                ui.horizontal(|ui| {
                    let done = i.state >= 3;
                    ff::stat_label(ui, if done { "UP" } else { "DL" });
                    let colors = if i.paused { GREY } else if done { GREEN } else { BLUE };
                    let bw = (ui.available_width() - 330.0).clamp(80.0, 320.0);
                    ff::stat_bar(ui, i.progress, bw, colors.0, colors.1);
                    ff::title(ui, &format!("{:.0}%", i.progress * 100.0), 14.0, TEXT());
                    ui.add_space(6.0);
                    let disp = self.settings.seeder_display;
                    let fmt = |c: i32, s: i32| match disp {
                        SeederDisplay::Both => format!("{c} ({})", s.max(0)),
                        SeederDisplay::Connected => c.to_string(),
                        SeederDisplay::Swarm => s.max(0).to_string(),
                    };
                    ff::stat_label(ui, "S").on_hover_text("Seeders: connected (in the whole swarm)");
                    ui.label(RichText::new(fmt(i.num_seeds, i.swarm_seeds)).strong());
                    ff::stat_label(ui, "L").on_hover_text("Leechers: connected (in the whole swarm)");
                    ui.label(RichText::new(fmt(i.num_peers, i.swarm_peers)).strong());
                    ff::stat_label(ui, "RATIO").on_hover_text("Uploaded ÷ downloaded");
                    ui.label(RichText::new(format!("{:.2}", t.ratio)).strong());
                });
                ui.horizontal(|ui| {
                    let (dr, ur) = if held { (0, 0) } else { (i.dl_rate as i64, i.ul_rate as i64) };
                    let mut bits = vec![format!("Down {}", human_rate(dr)), format!("Up {}", human_rate(ur))];
                    if let Some(eta) = t.eta_secs {
                        bits.push(format!("ETA {}", human_eta(eta)));
                    }
                    if i.sequential {
                        bits.push("In order".into());
                    }
                    if !i.error.is_empty() {
                        bits.push(i.error.clone());
                    }
                    ui.label(dim(&bits.join("   ")));
                    if self.settings.health_bar && i.has_metadata {
                        ui.add_space(8.0);
                        let copies = i.distributed_copies.max(0.0);
                        ff::stat_label(ui, "HEALTH");
                        ff::stat_bar(ui, (copies / 5.0).min(1.0), 70.0, GOLD.0, GOLD.1).on_hover_text(format!(
                            "{copies:.1} full copies among connected peers. Under 1 means the torrent may never finish."
                        ));
                    }
                    if advanced && t.seed_goal_met {
                        ff::tag(ui, "GOAL MET", GOOD());
                    }
                });
            });
        });
        row.response.context_menu(|ui| self.torrent_menu(ui, t));
        ui.add_space(4.0);
        ff::divider(ui);
    }

    fn torrent_menu(&mut self, ui: &mut egui::Ui, t: &TorrentView) {
        let i = &t.info;
        let h = i.hash.clone();
        let adv = self.settings.advanced_mode;
        if ui.button("Show files").clicked() {
            self.popup = Some(h.clone());
            self.popup_tab = 0;
            self.core.send(Cmd::WatchFiles(Some(h.clone())));
            ui.close();
        }
        if ui.button("Open folder").clicked() {
            open_path(&i.save_path);
            ui.close();
        }
        if ui.button("Copy magnet link").clicked() {
            self.core.send(Cmd::CopyMagnet(h.clone()));
            ui.close();
        }
        ui.separator();
        if ui
            .button(if i.sequential { "Download in order: ON" } else { "Download in order: OFF" })
            .on_hover_text("Grabs pieces start to finish so you can watch a video while it downloads. A bit worse for the swarm.")
            .clicked()
        {
            self.core.send(Cmd::Sequential(h.clone(), !i.sequential));
            ui.close();
        }
        if ui
            .button("First and last pieces first")
            .on_hover_text("Video players often need the end of the file to start playing. This grabs it early.")
            .clicked()
        {
            self.core.send(Cmd::FirstLast(h.clone()));
            ui.close();
        }
        if ui.button("Move to top of queue").clicked() {
            self.core.send(Cmd::QueueTop(h.clone()));
            ui.close();
        }
        if adv {
            if ui
                .button(if i.super_seeding { "Super-seeding: ON" } else { "Super-seeding: OFF" })
                .on_hover_text("For when you're the first seeder: sends each peer a different piece so copies spread faster.")
                .clicked()
            {
                self.core.send(Cmd::SuperSeed(h.clone(), !i.super_seeding));
                ui.close();
            }
            if ui.button("Check files again").on_hover_text("Re-verifies everything on disk.").clicked() {
                self.core.send(Cmd::Recheck(h.clone()));
                ui.close();
            }
        }
        ui.separator();
        if ui.button(RichText::new("Remove…").color(BAD())).clicked() {
            self.confirm_remove = Some((h, i.name.clone()));
            ui.close();
        }
    }

    pub(super) fn remove_dialog(&mut self, ctx: &egui::Context) {
        let Some((hash, name)) = self.confirm_remove.clone() else { return };
        let mut close = false;
        egui::Area::new(egui::Id::new("remove_dialog"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ff::window(ui, |ui| {
                    ui.set_width(420.0);
                    ff::title(ui, "Remove torrent?", 20.0, TEXT());
                    ui.add(egui::Label::new(RichText::new(&name).color(TEXT_DIM())).wrap());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                        if ui.button("Remove, keep files").clicked() {
                            self.core.send(Cmd::Remove { hash: hash.clone(), delete_files: false });
                            close = true;
                        }
                        if ui.button(RichText::new("Remove and delete files").color(BAD())).clicked() {
                            self.core.send(Cmd::Remove { hash: hash.clone(), delete_files: true });
                            close = true;
                        }
                    });
                });
            });
        if close {
            self.confirm_remove = None;
            if self.popup.as_deref() == Some(hash.as_str()) {
                self.popup = None;
            }
        }
    }

    pub(super) fn files_popup(&mut self, ctx: &egui::Context, main: egui::Rect) {
        let Some(hash) = self.popup.clone() else { return };
        let Some(t) = self.snap.torrents.iter().find(|t| t.info.hash == hash).cloned() else {
            self.popup = None;
            self.core.send(Cmd::WatchFiles(None));
            return;
        };
        let files = if self.snap.files_hash.as_deref() == Some(hash.as_str()) { self.snap.files.clone() } else { Vec::new() };
        let mut close = false;
        let adv = self.settings.advanced_mode;
        egui::Area::new(egui::Id::new("files_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(main.min + egui::vec2(84.0, 84.0))
            .show(ctx, |ui| {
                ff::window(ui, |ui| {
                    let width = (main.width() - 140.0).clamp(360.0, 620.0);
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.scope(|ui| {
                            ui.set_max_width(width - 90.0);
                            ui.add(egui::Label::new(RichText::new(&t.info.name).color(TEXT()).size(18.0).strong()).truncate());
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                    });
                    if adv {
                        ui.horizontal(|ui| {
                            for (i, name) in ["Files", "Trackers", "Info"].iter().enumerate() {
                                if w::pill(ui, name, self.popup_tab == i as u8).clicked() {
                                    self.popup_tab = i as u8;
                                }
                            }
                        });
                    }
                    ff::divider(ui);
                    match if adv { self.popup_tab } else { 0 } {
                        0 => self.popup_files(ui, &t, &files),
                        1 => self.popup_trackers(ui, &t),
                        _ => popup_info(ui, &t),
                    }
                });
            });
        if close {
            self.popup = None;
            self.core.send(Cmd::WatchFiles(None));
        }
    }

    fn popup_files(&mut self, ui: &mut egui::Ui, t: &TorrentView, files: &[crate::engine::ffi::FileEntry]) {
        if !t.info.has_metadata {
            ui.label(dim("Still finding the torrent's details from peers. Files show up once that's done."));
            return;
        }
        ui.label(dim(&format!(
            "{} file{} · {} · uncheck to skip, click the priority to change it",
            files.len(),
            if files.len() == 1 { "" } else { "s" },
            human_bytes(t.info.total_wanted)
        )));
        egui::ScrollArea::vertical().id_salt("popup_files").max_height(340.0).show(ui, |ui| {
            for f in files.iter().take(2000) {
                ui.horizontal(|ui| {
                    let mut wanted = f.priority > 0;
                    if ui.checkbox(&mut wanted, "").changed() {
                        self.core.send(Cmd::FilePriority { hash: t.info.hash.clone(), index: f.index, priority: if wanted { 4 } else { 0 } });
                    }
                    let name_w = (ui.available_width() - 250.0).max(100.0);
                    ui.scope(|ui| {
                        ui.set_max_width(name_w);
                        ui.add(egui::Label::new(RichText::new(&f.path).color(if wanted { TEXT() } else { TEXT_DIM() })).truncate())
                            .on_hover_text(&f.path);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (label, next) = match f.priority {
                            0 => ("Skip", 4),
                            1..=3 => ("Low", 4),
                            4..=5 => ("Normal", 7),
                            _ => ("High", 1),
                        };
                        if ui.small_button(label).on_hover_text("Click to cycle Low → Normal → High").clicked() && f.priority > 0 {
                            self.core.send(Cmd::FilePriority { hash: t.info.hash.clone(), index: f.index, priority: next });
                        }
                        let pct = if f.size > 0 { f.done as f32 / f.size as f32 } else { 1.0 };
                        ui.label(dim(&format!("{:.0}%", pct * 100.0)));
                        ff::stat_bar(ui, pct, 70.0, BLUE.0, BLUE.1);
                        ui.label(dim(&human_bytes(f.size)));
                    });
                });
            }
            if files.len() > 2000 {
                ui.label(dim(&format!("…and {} more", files.len() - 2000)));
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Open folder").clicked() {
                open_path(&t.info.save_path);
            }
        });
    }

    fn popup_trackers(&mut self, ui: &mut egui::Ui, t: &TorrentView) {
        let trackers = self.snap.trackers.clone();
        if trackers.is_empty() {
            ui.label(dim("No trackers. This torrent finds peers through DHT and peer exchange."));
        }
        egui::ScrollArea::vertical().id_salt("popup_trackers").max_height(260.0).show(ui, |ui| {
            for tr in &trackers {
                ui.label(RichText::new(tr).color(TEXT()).monospace());
            }
        });
        if t.info.is_private {
            ui.label(dim("Private torrents can't take extra trackers."));
            return;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.tracker_input).hint_text("udp://tracker.example:1337/announce").desired_width(320.0));
            if ui.button("Add tracker").on_hover_text("Extra trackers can help slow torrents find more peers.").clicked() {
                for line in self.tracker_input.split_whitespace() {
                    self.core.send(Cmd::AddTracker(t.info.hash.clone(), line.to_string()));
                }
                self.tracker_input.clear();
            }
        });
    }

    // ===================================================================== Add

    pub(super) fn page_add(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Add", "Paste a magnet link or open a .torrent file. You can also drop .torrent files anywhere on this window.");
        egui::ScrollArea::vertical().id_salt("add").auto_shrink([false, false]).show(ui, |ui| {
            if self.add.preview.is_none() {
                field(ui, "MAGNET LINK", "Starts with magnet:?xt=", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.add.magnet)
                                .hint_text("magnet:?xt=urn:btih:…")
                                .desired_width((ui.available_width() - 80.0).max(200.0)),
                        );
                        if ui.button("Paste").clicked() {
                            if let Ok(t) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                                self.add.magnet = t.trim().to_string();
                            }
                        }
                    });
                });
                ui.add_space(10.0);
                ff::window(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ff::title(ui, "Drop .torrent files here", 18.0, TEXT());
                        ui.label(dim("or"));
                        if ui.button("Choose .torrent file").clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("Torrent", &["torrent"]).pick_file() {
                                match std::fs::read(&p) {
                                    Ok(d) => self.preview_bytes(d),
                                    Err(e) => self.add.error = Some(e.to_string()),
                                }
                            }
                        }
                    });
                });
            } else if let Some((_, pv, wanted)) = self.add.preview.as_mut() {
                ui.horizontal(|ui| {
                    ff::title(ui, &pv.name, 19.0, TEXT());
                    if pv.is_private {
                        ff::tag(ui, "PRIVATE", WARN());
                    }
                });
                let picked: i64 = pv.files.iter().zip(wanted.iter()).filter(|(_, w)| **w).map(|(f, _)| f.size).sum();
                ui.label(dim(&format!("{} files · {} selected of {}", pv.files.len(), human_bytes(picked), human_bytes(pv.total_size))));
                ui.horizontal(|ui| {
                    if ui.small_button("Select all").clicked() {
                        wanted.iter_mut().for_each(|w| *w = true);
                    }
                    if ui.small_button("Select none").clicked() {
                        wanted.iter_mut().for_each(|w| *w = false);
                    }
                });
                egui::ScrollArea::vertical().id_salt("add_files").max_height(220.0).show(ui, |ui| {
                    for (f, w) in pv.files.iter().zip(wanted.iter_mut()).take(3000) {
                        ui.horizontal(|ui| {
                            ui.checkbox(w, RichText::new(&f.path).color(TEXT()));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(dim(&human_bytes(f.size))));
                        });
                    }
                });
            }

            section(ui, "Where it goes");
            let cats: Vec<String> = self.settings.categories.iter().map(|c| c.name.clone()).collect();
            row(ui, "Category", "Each category can have its own folder (set them up in Config). When you click Add, Lifestream asks where to save it, starting in the category's folder or the last folder you picked.", |ui| {
                egui::ComboBox::from_id_salt("add_cat")
                    .selected_text(if self.add.category.is_empty() { "None".to_string() } else { self.add.category.clone() })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.add.category, String::new(), "None");
                        for c in &cats {
                            ui.selectable_value(&mut self.add.category, c.clone(), c);
                        }
                    });
            });

            section(ui, "Options");
            row(ui, "Start right away", "Off = add it paused so you can start it later.", |ui| {
                let mut on = !self.add.choice.paused;
                if toggle(ui, &mut on) {
                    self.add.choice.paused = !on;
                }
            });
            row(ui, "Download in order", "Lets you start watching a video before it's done. Slightly worse for the swarm.", |ui| {
                toggle(ui, &mut self.add.choice.sequential);
            });
            row(ui, "First and last pieces first", "Helps video players start sooner.", |ui| {
                toggle(ui, &mut self.add.choice.first_last);
            });
            if self.settings.advanced_mode {
                row(ui, "Skip hash check", "Only for files you already have complete and trust. Lifestream will assume they're right.", |ui| {
                    toggle(ui, &mut self.add.choice.skip_check);
                });
            }

            if let Some(e) = &self.add.error {
                ui.colored_label(BAD(), e);
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let has_preview = self.add.preview.is_some();
                let can_add = has_preview || self.add.magnet.trim().starts_with("magnet:");
                let add_clicked = ui
                    .add_enabled(can_add, egui::Button::new(RichText::new("Add to party…").color(GOOD()).strong()))
                    .clicked();
                let picked = if add_clicked { self.ask_save_folder(&self.add.category.clone()) } else { None };
                if let Some(save_path) = picked {
                    let save_path = Some(save_path);
                    let category = self.add.category.clone();
                    let choice = self.add.choice;
                    if let Some((data, pv, wanted)) = self.add.preview.take() {
                        let priorities = if wanted.iter().all(|w| *w) {
                            None
                        } else {
                            // Indexed by the torrent's real file index (padding files are hidden).
                            let len = pv.files.iter().map(|f| f.index as usize + 1).max().unwrap_or(0);
                            let mut p = vec![4u8; len];
                            for (f, w) in pv.files.iter().zip(wanted.iter()) {
                                if !*w {
                                    p[f.index as usize] = 0;
                                }
                            }
                            Some(p)
                        };
                        self.core.send(Cmd::AddTorrent { data, category, choice, priorities, save_path });
                    } else {
                        self.core.send(Cmd::AddMagnet { uri: self.add.magnet.trim().to_string(), category, choice, save_path });
                        self.add.magnet.clear();
                    }
                    self.add.error = None;
                    self.page = Page::Torrents;
                }
                if (has_preview || !self.add.magnet.is_empty()) && ui.button("Cancel").clicked() {
                    self.add.preview = None;
                    self.add.magnet.clear();
                    self.add.error = None;
                }
                if !can_add && !self.add.magnet.is_empty() {
                    ui.colored_label(WARN(), "Magnet links start with magnet:");
                }
            });
        });
    }

    // ===================================================================== RSS

    pub(super) fn page_rss(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ff::title(ui, "RSS", 26.0, TEXT());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let checking = self.snap.rss_checking;
                if ui.add_enabled(!checking, egui::Button::new(if checking { "Checking…" } else { "Check now" })).clicked() {
                    self.core.send(Cmd::RssCheckNow);
                }
                let mut on = self.settings.rss_enabled;
                if w::toggle_labeled(ui, &mut on, "AUTO-DOWNLOAD: ON", "AUTO-DOWNLOAD: OFF") {
                    self.settings.rss_enabled = on;
                    self.changed();
                }
            });
        });
        ui.label(dim("Feeds are checked on a timer. Anything that matches a rule is added for you."));
        ff::divider(ui);
        let adv = self.settings.advanced_mode;
        egui::ScrollArea::vertical().id_salt("rss").auto_shrink([false, false]).show(ui, |ui| {
            section(ui, "Feeds");
            let mut remove = None;
            let mut changed = false;
            for (i, feed) in self.settings.feeds.iter_mut().enumerate() {
                let status = self.snap.rss_status.get(&feed.name);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&feed.name).color(TEXT()).strong());
                        ui.label(RichText::new(&feed.url).color(TEXT_DIM()).size(12.0).monospace());
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                        changed |= toggle(ui, &mut feed.enabled);
                        match status {
                            Some(s) => match &s.result {
                                Ok(n) => ff::title(ui, &format!("OK · {n} items · {}", s.checked), 13.0, GOOD()),
                                Err(e) => ff::title(ui, &format!("FAILED · {}", s.checked), 13.0, BAD()).on_hover_text(e),
                            },
                            None => ff::title(ui, "Not checked yet", 13.0, TEXT_DIM()),
                        };
                    });
                });
                ff::divider(ui);
            }
            if let Some(i) = remove {
                self.settings.feeds.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_feed.0).hint_text("Name").desired_width(140.0));
                ui.add(egui::TextEdit::singleline(&mut self.new_feed.1).hint_text("https://…/rss").desired_width((ui.available_width() - 100.0).max(160.0)));
                let ok = !self.new_feed.1.trim().is_empty();
                if ui.add_enabled(ok, egui::Button::new("Add feed")).clicked() {
                    let name = if self.new_feed.0.trim().is_empty() { format!("Feed {}", self.settings.feeds.len() + 1) } else { self.new_feed.0.trim().to_string() };
                    self.settings.feeds.push(Feed { name, url: self.new_feed.1.trim().to_string(), enabled: true });
                    self.new_feed = (String::new(), String::new());
                    changed = true;
                    self.core.send(Cmd::RssCheckNow);
                }
            });
            ui.add_space(4.0);
            row(ui, "Check every", "How often feeds are read, in minutes.", |ui| {
                changed |= ui.add(egui::DragValue::new(&mut self.settings.rss_interval_min).range(5..=1440).suffix(" min")).changed();
            });

            w::section_help(ui, "Rules", "A rule says which feed items to download, like \"Show Name 1080p\" from season 2 on. Anything it matches is added automatically and saved to the rule's category.");
            ui.horizontal_wrapped(|ui| {
                for (i, r) in self.settings.rules.iter().enumerate() {
                    if w::pill(ui, &r.name, self.rss_rule == i).clicked() {
                        self.rss_rule = i;
                    }
                }
                if ui.button("New rule").clicked() {
                    self.settings.rules.push(RssRule::default());
                    self.rss_rule = self.settings.rules.len() - 1;
                    changed = true;
                }
            });
            let cats: Vec<String> = self.settings.categories.iter().map(|c| c.name.clone()).collect();
            let feed_names: Vec<String> = self.settings.feeds.iter().map(|f| f.name.clone()).collect();
            let mut delete_rule = false;
            if let Some(rule) = self.settings.rules.get_mut(self.rss_rule) {
                ui.add_space(6.0);
                ui.columns(2, |cols| {
                    changed |= field(&mut cols[0], "NAME", "", |ui| ui.text_edit_singleline(&mut rule.name)).changed();
                    changed |= field(&mut cols[1], "SAVE AS", "Category for anything this rule grabs.", |ui| {
                        let mut c = false;
                        egui::ComboBox::from_id_salt("rule_cat")
                            .selected_text(if rule.category.is_empty() { "None".to_string() } else { rule.category.clone() })
                            .show_ui(ui, |ui| {
                                c |= ui.selectable_value(&mut rule.category, String::new(), "None").changed();
                                for n in &cats {
                                    c |= ui.selectable_value(&mut rule.category, n.clone(), n).changed();
                                }
                            });
                        c
                    });
                    changed |= field(
                        &mut cols[0],
                        "MUST CONTAIN",
                        if rule.use_regex { "A regular expression the title must match." } else { "Every word must appear in the title (any order)." },
                        |ui| ui.text_edit_singleline(&mut rule.must_contain),
                    )
                    .changed();
                    changed |= field(
                        &mut cols[1],
                        "MUST NOT CONTAIN",
                        if rule.use_regex { "A regular expression the title must not match." } else { "Comma-separated. Any of these in the title skips it." },
                        |ui| ui.text_edit_singleline(&mut rule.must_not_contain),
                    )
                    .changed();
                    changed |= field(
                        &mut cols[0],
                        "EPISODES",
                        "Leave empty for everything. Examples: S02 · S02E05- (from E05 on) · S01E01-S01E10 · S03E02, S03E07",
                        |ui| ui.text_edit_singleline(&mut rule.episodes),
                    )
                    .changed();
                });
                row(ui, "Rule on", "", |ui| changed |= toggle(ui, &mut rule.enabled));
                row(ui, "Skip duplicates", "Only grab each episode once, even if a second release shows up.", |ui| {
                    changed |= toggle(ui, &mut rule.skip_duplicates)
                });
                if adv {
                    row(ui, "Use regular expressions", "For precise matching. Case doesn't matter.", |ui| changed |= toggle(ui, &mut rule.use_regex));
                    row(ui, "Only these feeds", "None checked = every feed.", |ui| {
                        for f in feed_names.iter().rev() {
                            let mut on = rule.feeds.contains(f);
                            if ui.checkbox(&mut on, f).changed() {
                                if on {
                                    rule.feeds.push(f.clone());
                                } else {
                                    rule.feeds.retain(|x| x != f);
                                }
                                changed = true;
                            }
                        }
                    });
                }
                let matches: Vec<&crate::rss::FeedItem> = self
                    .snap
                    .rss_items
                    .iter()
                    .filter(|i| crate::rss::rule_matches(rule, i).unwrap_or(false))
                    .collect();
                let err = self.snap.rss_items.first().and_then(|i| crate::rss::rule_matches(rule, i).err());
                ui.horizontal(|ui| {
                    if let Some(e) = &err {
                        ui.colored_label(BAD(), e);
                    } else {
                        ff::title(ui, &format!("Matches right now: {}", matches.len()), 14.0, LABEL());
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(RichText::new("Delete rule").color(BAD())).clicked() {
                            delete_rule = true;
                        }
                    });
                });
                for m in matches.iter().take(12) {
                    ui.label(RichText::new(format!("• {}", m.title)).color(TEXT_DIM()).size(12.5));
                }
            }
            if delete_rule {
                self.settings.rules.remove(self.rss_rule);
                self.rss_rule = self.rss_rule.saturating_sub(1);
                changed = true;
            }

            if adv && !self.snap.rss_items.is_empty() {
                section(ui, "Latest feed items");
                let items: Vec<_> = self.snap.rss_items.iter().take(40).cloned().collect();
                for it in items {
                    ui.horizontal(|ui| {
                        if ui.small_button("Get").on_hover_text("Download this item now").clicked() {
                            self.core.send(Cmd::AddFromUrl { url: it.link.clone(), category: String::new() });
                        }
                        ui.label(RichText::new(&it.title).color(TEXT()).size(13.0));
                        ui.label(dim(&it.feed));
                    });
                }
            }
            if changed {
                self.changed();
            }
        });
    }

    // ===================================================================== Speed

    pub(super) fn page_speed(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Speed", "Bandwidth limits and how many connections to keep open. 0 means no limit.");
        let adv = self.settings.advanced_mode;
        let s = &mut self.settings;
        let mut changed = false;
        egui::ScrollArea::vertical().id_salt("speed").auto_shrink([false, false]).show(ui, |ui| {
            row(ui, "Download limit", "Total for all torrents.", |ui| changed |= rate(ui, &mut s.dl_limit_kb).changed());
            row(ui, "Upload limit", "Total for all torrents. Leaving some upload free keeps your browsing snappy.", |ui| {
                changed |= rate(ui, &mut s.ul_limit_kb).changed()
            });

            section(ui, "Turtle mode");
            row(ui, "Turtle mode now", "Switch to the slower limits below right away. Also in the tray menu.", |ui| {
                changed |= toggle(ui, &mut s.turtle_manual)
            });
            row(ui, "Turtle download", "", |ui| changed |= rate(ui, &mut s.turtle_dl_kb).changed());
            row(ui, "Turtle upload", "", |ui| changed |= rate(ui, &mut s.turtle_ul_kb).changed());
            row(ui, "Use turtle limits on a schedule", "Slows things down while you game or stream.", |ui| {
                changed |= toggle(ui, &mut s.turtle_schedule)
            });
            if s.turtle_schedule {
                row(ui, "From / to", "24-hour time, like 18:00. Overnight works too (22:00 to 06:00).", |ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut s.turtle_to).desired_width(60.0)).changed();
                    ui.label("to");
                    changed |= ui.add(egui::TextEdit::singleline(&mut s.turtle_from).desired_width(60.0)).changed();
                });
                row(ui, "Days", "", |ui| {
                    for (i, d) in ["Sun", "Sat", "Fri", "Thu", "Wed", "Tue", "Mon"].iter().enumerate() {
                        let idx = 6 - i;
                        changed |= ui.checkbox(&mut s.turtle_days[idx], *d).changed();
                    }
                });
            }

            section(ui, "Queue");
            row(ui, "Downloads at once", "Others wait their turn.", |ui| {
                changed |= ui.add(egui::DragValue::new(&mut s.active_downloads).range(1..=50)).changed()
            });
            row(ui, "Seeds at once", "Finished torrents that keep sharing at the same time.", |ui| {
                changed |= ui.add(egui::DragValue::new(&mut s.active_seeds).range(0..=100)).changed()
            });
            if adv {
                section(ui, "Connections");
                row(ui, "Max peers (all torrents)", "More isn't always faster; home routers can choke on too many.", |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut s.max_peers_global).range(20..=5000)).changed()
                });
                row(ui, "Max peers per torrent", "", |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut s.max_peers_torrent).range(5..=1000)).changed()
                });
                row(ui, "Upload slots per torrent", "How many peers each torrent uploads to at once.", |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut s.upload_slots_torrent).range(1..=200)).changed()
                });
                row(ui, "Upload slots (all torrents)", "", |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut s.upload_slots_global).range(1..=500)).changed()
                });
            }
        });
        if changed {
            self.changed();
        }
    }

    // ===================================================================== Network

    pub(super) fn page_network(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Network", "Which adapter torrents use, and what happens if your VPN drops.");
        let (prot, title, detail) = self.protection();
        ui.horizontal(|ui| {
            ff::badge(ui, prot, 30.0);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                let color = match prot {
                    ff::Badge::Good => GOOD(),
                    ff::Badge::Warn => WARN(),
                    ff::Badge::Bad => BAD(),
                };
                ff::title(ui, &title, 18.0, color);
                ui.add(egui::Label::new(dim(&detail)).wrap());
            });
        });
        ff::divider(ui);
        self.refresh_adapters(false);
        let adv = self.settings.advanced_mode;
        let adapters = self.adapters.clone();
        let mut changed = false;
        let mut refresh = false;
        egui::ScrollArea::vertical().id_salt("net").auto_shrink([false, false]).show(ui, |ui| {
            let s = &mut self.settings;
            row(ui, "Network adapter", "Pick your VPN here. Lifestream binds to it so torrent traffic can't use your normal connection.", |ui| {
                if ui.small_button("Refresh").clicked() {
                    refresh = true;
                }
                let current = if s.adapter.is_empty() {
                    "Any adapter".to_string()
                } else {
                    adapters.iter().find(|a| a.name == s.adapter).map(|a| a.label()).unwrap_or_else(|| format!("{} (not connected)", s.adapter))
                };
                egui::ComboBox::from_id_salt("adapter").width(240.0).selected_text(current).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut s.adapter, String::new(), "Any adapter (not recommended)").changed();
                    for a in &adapters {
                        changed |= ui.selectable_value(&mut s.adapter, a.name.clone(), a.label()).changed();
                    }
                });
            });
            if s.adapter.is_empty() && s.kill_switch {
                ui.colored_label(WARN(), "The kill switch needs an adapter to watch. Until you pick one, downloads stay paused.");
            }
            row(ui, "Only use this adapter", "Torrent traffic can never fall back to your normal connection.", |ui| {
                changed |= toggle(ui, &mut s.bind_only)
            });
            w::row_tagged(
                ui,
                "Kill switch",
                Some(("ON BY DEFAULT", GOOD())),
                "If the adapter goes down, pause everything and don't resume until it's back. Also holds downloads if Lifestream starts before your VPN connects.",
                |ui| changed |= w::toggle_labeled(ui, &mut s.kill_switch, "ARMED", "OFF"),
            );
            if adv {
                row(ui, "Disable IPv6", "Many VPNs only tunnel IPv4, so IPv6 can leak your real address.", |ui| {
                    changed |= toggle(ui, &mut s.disable_ipv6)
                });
            }

            section(ui, "Port");
            row(ui, "Listening port", "Peers connect to you on this port. A forwarded, open port means faster downloads.", |ui| {
                if ui.button("Random").clicked() {
                    s.listen_port = rand::random_range(49160..65000);
                    changed = true;
                }
                changed |= ui.add(egui::DragValue::new(&mut s.listen_port).range(1024..=65535)).changed();
            });
            row(ui, "Get port from VPN (NAT-PMP)", "For VPNs with port forwarding, like ProtonVPN. Lifestream switches to the port your VPN gives it.", |ui| {
                let c = toggle(ui, &mut s.natpmp);
                if c {
                    s.follow_vpn_port = s.natpmp;
                }
                changed |= c;
            });
            if adv {
                row(ui, "UPnP on home router", "Leave off while on a VPN: it opens a port on your real connection.", |ui| {
                    changed |= toggle(ui, &mut s.upnp)
                });
            }
            let sn = &self.snap;
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ff::stat_label(ui, "LISTENING");
                if sn.listen_port > 0 {
                    ff::title(ui, &sn.listen_port.to_string(), 15.0, GOOD());
                } else {
                    ff::title(ui, "—", 15.0, TEXT_DIM());
                }
                if sn.external_port > 0 {
                    ff::stat_label(ui, "VPN PORT");
                    ff::title(ui, &sn.external_port.to_string(), 15.0, GOOD());
                }
                ff::stat_label(ui, "INCOMING PEERS");
                let (t, c) = if sn.incoming_seen { ("SEEN · PORT IS OPEN", GOOD()) } else { ("NONE YET", WARN()) };
                ff::title(ui, t, 15.0, c).on_hover_text("Once a peer connects to you, your port is reachable. It can take a few minutes with active torrents.");
            });
            if !sn.listen_error.is_empty() {
                ui.colored_label(BAD(), &sn.listen_error);
            }

            w::section_help(ui, "Leak check", "Asks a lookup site where your traffic comes out: once over your normal connection, and once from the adapter torrents use, exactly the way the torrent engine connects. Runs on its own when Lifestream opens, when the VPN reconnects, and every 30 minutes.");
            let adapter_name = self.settings.adapter.clone();
            let (prot, prot_title, _) = self.protection();
            let summary = match sn.leak.as_ref().and_then(|l| l.torrent.as_ref()) {
                Some(Ok(t)) if !t.org.is_empty() => format!("Torrents exit at {} ({})", t.ip, t.org),
                Some(Ok(t)) => format!("Torrents exit at {}", t.ip),
                Some(Err(_)) => "No internet through this adapter".to_string(),
                None if sn.leak_checking => "Checking…".to_string(),
                None => "Not checked yet".to_string(),
            };
            if w::disclosure(ui, self.leak_open, |ui| {
                ff::badge(ui, prot, 18.0);
                let color = match prot {
                    ff::Badge::Good => GOOD(),
                    ff::Badge::Warn => WARN(),
                    ff::Badge::Bad => BAD(),
                };
                ff::title(ui, &prot_title, 15.0, color);
                ui.label(dim(&summary));
            }) {
                self.leak_open = !self.leak_open;
            }
            if self.leak_open {
                ui.add_space(4.0);
                if ui.add_enabled(!sn.leak_checking, egui::Button::new(if sn.leak_checking { "Checking…" } else { "Run leak check" })).clicked() {
                    self.core.send(Cmd::LeakCheck);
                }
                ui.add_space(4.0);
                if let Some(r) = &sn.leak {
                    leak_report(ui, r, &adapter_name);
                }
            }
            let lower = adapter_name.to_lowercase();
            if ["tailscale", "zerotier", "hamachi", "radmin"].iter().any(|n| lower.contains(n)) {
                ui.add_space(4.0);
                ui.colored_label(
                    WARN(),
                    "This adapter is a private-network tool, not an internet VPN. Unless it's set to use an exit node, torrents bound to it can't reach peers at all.",
                );
            }
        });
        if refresh {
            self.refresh_adapters(true);
        }
        if changed {
            self.changed();
        }
    }

    // ===================================================================== Privacy

    pub(super) fn page_privacy(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Privacy", "Settings that make your connections harder to see or fingerprint.");
        let adv = self.settings.advanced_mode;
        let restart = self.snap.restart_needed;
        let anon_eff = self.snap.anonymous_effective;
        let bl_status = self.snap.blocklist_status.clone();
        let s = &mut self.settings;
        let mut changed = false;
        let mut update_bl = false;
        egui::ScrollArea::vertical().id_salt("privacy").auto_shrink([false, false]).show(ui, |ui| {
            row(ui, "Encryption", "Hides torrent traffic from your ISP's throttling. It doesn't hide your IP; that's the VPN's job. Require can mean fewer peers.", |ui| {
                let names = ["Allow (plain only)", "Prefer", "Require"];
                egui::ComboBox::from_id_salt("enc").selected_text(names[s.encryption.clamp(0, 2) as usize]).show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        changed |= ui.selectable_value(&mut s.encryption, i as i32, *n).changed();
                    }
                });
            });
            row(ui, "Anonymous mode", "Hides the app's name and version from peers and trackers.", |ui| changed |= toggle(ui, &mut s.anonymous_mode));
            if s.anonymous_mode && !anon_eff {
                ui.label(dim("Paused right now because a private torrent is active (see Private trackers)."));
            }

            w::section_help(ui, "Private trackers", "Torrents marked private automatically skip DHT, peer exchange and local discovery, and only talk to their own tracker. They get a PRIVATE tag in your list.");
            row(ui, "Honor tracker seeding rules", "Private torrents ignore your stop-seeding goal until they've seeded for the minimum time below.", |ui| {
                changed |= toggle(ui, &mut s.private_honor_seed)
            });
            if s.private_honor_seed {
                row(ui, "Minimum seed time", "Most private trackers ask for 72 hours or more. Check your tracker's rules.", |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut s.private_min_seed_hours).range(0.0..=2000.0).suffix(" h")).changed()
                });
            }
            row(ui, "Report real client name on private trackers", "Many private trackers ban apps they don't recognize, so anonymous mode turns off while a private torrent is active.", |ui| {
                changed |= toggle(ui, &mut s.private_real_name)
            });

            if adv {
                section(ui, "Peer discovery");
                row(ui, "DHT", "Finds peers without a tracker. Needed for most magnet links.", |ui| changed |= toggle(ui, &mut s.dht));
                row(ui, "Peer exchange (PEX)", "Peers share other peers with you. Takes effect after a restart.", |ui| changed |= toggle(ui, &mut s.pex));
                if restart {
                    ui.colored_label(WARN(), "Restart Lifestream for the peer exchange change to apply.");
                }
                row(ui, "Local peer discovery", "Looks for peers on your home network. Keep off when using a VPN.", |ui| changed |= toggle(ui, &mut s.lsd));

                section(ui, "Proxy");
                row(ui, "Type", "Route connections through a proxy server, like a SOCKS5 proxy from your VPN provider.", |ui| {
                    let names = ["None", "SOCKS5", "HTTP"];
                    egui::ComboBox::from_id_salt("ptype").selected_text(names[s.proxy_type.clamp(0, 2) as usize]).show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            changed |= ui.selectable_value(&mut s.proxy_type, i as i32, *n).changed();
                        }
                    });
                });
                if s.proxy_type > 0 {
                    row(ui, "Host and port", "", |ui| {
                        changed |= ui.add(egui::DragValue::new(&mut s.proxy_port).range(1..=65535)).changed();
                        changed |= ui.add(egui::TextEdit::singleline(&mut s.proxy_host).hint_text("proxy.example.net").desired_width(170.0)).changed();
                    });
                    row(ui, "Username and password", "Leave empty if the proxy doesn't need them.", |ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut s.proxy_pass).password(true).hint_text("password").desired_width(110.0)).changed();
                        changed |= ui.add(egui::TextEdit::singleline(&mut s.proxy_user).hint_text("username").desired_width(110.0)).changed();
                    });
                    row(ui, "Send peer connections through proxy", "Without this, only tracker traffic uses the proxy.", |ui| changed |= toggle(ui, &mut s.proxy_peers));
                    row(ui, "Send tracker traffic through proxy", "", |ui| changed |= toggle(ui, &mut s.proxy_trackers));
                }

                section(ui, "Blocklist");
                row(ui, "Blocklist URL", "A list of IP ranges to refuse (P2P or DAT format, gzip is fine).", |ui| {
                    if ui.button("Update").clicked() {
                        update_bl = true;
                    }
                    changed |= ui.add(egui::TextEdit::singleline(&mut s.blocklist_url).hint_text("https://… (.p2p, .dat or .gz)").desired_width(230.0)).changed();
                });
                row(ui, "Update weekly", "", |ui| changed |= toggle(ui, &mut s.blocklist_auto));
                ui.label(dim(if bl_status.is_empty() { "No blocklist loaded." } else { &bl_status }));
            }
        });
        if changed {
            self.changed();
        }
        if update_bl {
            self.flush_settings_now();
            self.core.send(Cmd::UpdateBlocklist);
        }
    }

    fn flush_settings_now(&mut self) {
        self.dirty_since = Some(std::time::Instant::now() - std::time::Duration::from_secs(5));
        self.flush_settings(true);
    }

    // ===================================================================== Seeding

    pub(super) fn page_seeding(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Seeding", "When to stop sharing, and how seeders and leechers are shown.");
        let _adv = self.settings.advanced_mode;
        let s = &mut self.settings;
        let mut changed = false;
        egui::ScrollArea::vertical().id_salt("seed").auto_shrink([false, false]).show(ui, |ui| {
            row(ui, "Stop at ratio", "Ratio = uploaded ÷ downloaded. 2.0 means you've shared twice what you took. 0 = never stop.", |ui| {
                changed |= ui.add(egui::DragValue::new(&mut s.ratio_limit).range(0.0..=100.0).speed(0.05).max_decimals(2)).changed()
            });
            row(ui, "Or after seeding for", "Whichever comes first. 0 = no time limit.", |ui| {
                changed |= ui.add(egui::DragValue::new(&mut s.seed_hours).range(0.0..=10000.0).suffix(" h")).changed()
            });
            row(ui, "Then", "What happens when a torrent reaches its goal. If you resume it afterwards, it keeps seeding until you stop it.", |ui| {
                let label = |a: SeedAction| match a {
                    SeedAction::Pause => "Pause the torrent",
                    SeedAction::Remove => "Remove it (keep files)",
                    SeedAction::RemoveAndDelete => "Remove it and delete files",
                };
                egui::ComboBox::from_id_salt("then").selected_text(label(s.seed_action)).show_ui(ui, |ui| {
                    for a in [SeedAction::Pause, SeedAction::Remove, SeedAction::RemoveAndDelete] {
                        changed |= ui.selectable_value(&mut s.seed_action, a, label(a)).changed();
                    }
                });
            });

            section(ui, "Seeders vs. leechers");
            row(ui, "Show as", "S 42 (180) means connected to 42 of 180 seeders in the swarm. Tip: sort the Torrents list by \"Seeders vs. leechers\" to spot torrents that are hard to find.", |ui| {
                let label = |d: SeederDisplay| match d {
                    SeederDisplay::Both => "Connected (swarm)",
                    SeederDisplay::Connected => "Connected only",
                    SeederDisplay::Swarm => "Swarm only",
                };
                egui::ComboBox::from_id_salt("sdisp").selected_text(label(s.seeder_display)).show_ui(ui, |ui| {
                    for d in [SeederDisplay::Both, SeederDisplay::Connected, SeederDisplay::Swarm] {
                        changed |= ui.selectable_value(&mut s.seeder_display, d, label(d)).changed();
                    }
                });
            });
            row(ui, "Swarm health bar", "Gold bar under each torrent: how many full copies exist among your peers.", |ui| {
                changed |= toggle(ui, &mut s.health_bar)
            });
        });
        if changed {
            self.changed();
        }
    }

    // ===================================================================== Config

    pub(super) fn page_config(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Config", "Where files go and how the app starts.");
        let adv = self.settings.advanced_mode;
        let mut changed = false;
        let mut export = false;
        let mut restore = false;
        let mut theme_switched = false;
        let mut make_default = false;
        egui::ScrollArea::vertical().id_salt("config").auto_shrink([false, false]).show(ui, |ui| {
            let s = &mut self.settings;
            row(ui, "Mode", "Simple shows the settings most people need. Advanced shows everything, plus the Log.", |ui| {
                changed |= w::toggle_labeled(ui, &mut s.advanced_mode, "ADVANCED", "SIMPLE");
            });
            row(ui, "Theme", "Switches the whole app's look right away. FFVII menu: blue windows and the pointing hand. Box of X: glowing grid, jelly orb and slanted tabs.", |ui| {
                // Right-to-left row: list in reverse so they read FFVII, Green.
                for theme in [ff::Theme::GreenDash, ff::Theme::FfMenu] {
                    let selected = ff::theme() == theme;
                    let mut text = RichText::new(theme.label()).strong();
                    if selected {
                        text = text.color(ff::pal().on_accent);
                    }
                    let mut button = egui::Button::new(text);
                    if selected {
                        button = button.fill(ff::pal().accent);
                    }
                    if ui.add(button).clicked() && !selected {
                        ff::set_theme(theme);
                        ff::apply_style(ui.ctx());
                        s.theme = theme.key().to_string();
                        changed = true;
                        // Land on Config in the new layout (once; BACK still works).
                        theme_switched = true;
                    }
                }
            });
            section(ui, "Folders");
            w::label_help(ui, "Default download folder", "Where RSS downloads go when their rule has no category, and the first folder offered when you add a torrent. After that, Lifestream offers the last folder you picked.");
            ui.horizontal(|ui| changed |= w::folder_field(ui, &mut s.download_dir, (ui.available_width() - 90.0).max(200.0)));
            ui.add_space(6.0);

            w::label_help(ui, "Categories", "Pick a category when adding a torrent and its folder is offered as the save location. RSS rules save straight into their category's folder. Point Movies or TV at a folder Jenova watches and subtitles arrive on their own.");
            let mut remove = None;
            for (i, c) in s.categories.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(110.0)).changed();
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                    changed |= w::folder_field(ui, &mut c.path, (ui.available_width() - 90.0).max(150.0));
                });
            }
            if let Some(i) = remove {
                s.categories.remove(i);
                changed = true;
            }
            if ui.button("Add category").clicked() {
                s.categories.push(Category { name: format!("Category {}", s.categories.len() + 1), path: s.download_dir.clone() });
                changed = true;
            }

            ui.add_space(6.0);
            if adv {
                row(ui, "Keep unfinished files in a separate folder", "Finished downloads move to their real folder, so apps watching that folder (like Jenova) only see complete files.", |ui| {
                    changed |= toggle(ui, &mut s.incomplete_enabled)
                });
                if s.incomplete_enabled {
                    ui.horizontal(|ui| changed |= w::folder_field(ui, &mut s.incomplete_dir, (ui.available_width() - 90.0).max(200.0)));
                }
                section(ui, "Defaults for new torrents");
                row(ui, "Add paused", "", |ui| changed |= toggle(ui, &mut s.start_paused));
                row(ui, "Download in order", "", |ui| changed |= toggle(ui, &mut s.default_sequential));
                row(ui, "First and last pieces first", "", |ui| changed |= toggle(ui, &mut s.default_first_last));
            }

            section(ui, "App");
            if cfg!(windows) {
                row(ui, "Open magnet links and .torrent files with Lifestream", "Your browser sends magnet links straight here.", |ui| {
                    if ui.button("Make default").clicked() {
                        make_default = true;
                    }
                });
                row(ui, "Start with Windows", "Starts quietly in the tray.", |ui| changed |= toggle(ui, &mut s.start_with_windows));
                row(ui, "Close to tray", "The X button hides the window; downloads keep going. Quit from the tray menu.", |ui| {
                    changed |= toggle(ui, &mut s.close_to_tray)
                });
            }
            if adv {
                row(ui, "Prevent sleep while downloading", "Keeps the PC awake until downloads finish.", |ui| changed |= toggle(ui, &mut s.prevent_sleep));
            }

            section(ui, "Remote control");
            row(ui, "Let Turks send downloads here", "Turns on a Transmission-compatible remote control, so Turks (or Radarr, Sonarr, Prowlarr) can add torrents and watch their progress. In Turks, add a 'Transmission / Lifestream' download client. Changes apply after a restart.", |ui| {
                changed |= toggle(ui, &mut s.rpc_enabled)
            });
            if s.rpc_enabled {
                row(ui, "Port", "9091 is Transmission's usual port.", |ui| {
                    changed |= ui.add(egui::DragValue::new(&mut s.rpc_port).range(1024..=65535)).changed()
                });
                if adv {
                    row(ui, "Allow other computers", "Off: only apps on this PC can connect. Turn on if Turks runs on another machine, and set a username and password.", |ui| {
                        changed |= toggle(ui, &mut s.rpc_allow_lan)
                    });
                    row(ui, "Username", "Optional. Must match the download client settings in Turks.", |ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut s.rpc_username).desired_width(180.0)).changed()
                    });
                    row(ui, "Password", "", |ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut s.rpc_password).password(true).desired_width(180.0)).changed()
                    });
                }
            }

            section(ui, "Backup");
            row(ui, "Back up settings, RSS rules and torrent list", "One file. Includes download progress so nothing starts over after a restore.", |ui| {
                if ui.button("Restore").clicked() {
                    restore = true;
                }
                if ui.button("Export").clicked() {
                    export = true;
                }
            });
            row(ui, "Automatic weekly backup", "", |ui| changed |= toggle(ui, &mut s.auto_backup));
            if adv && s.auto_backup {
                ui.horizontal(|ui| changed |= w::folder_field(ui, &mut s.backup_dir, (ui.available_width() - 90.0).max(200.0)));
                row(ui, "Keep the last", "", |ui| changed |= ui.add(egui::DragValue::new(&mut s.backup_keep).range(1..=52).suffix(" backups")).changed());
            }
            row(ui, "Data and logs", &crate::settings::data_dir().display().to_string(), |ui| {
                if ui.button("Open folder").clicked() {
                    open_path(&crate::settings::data_dir().display().to_string());
                }
            });
            row(ui, &format!("Lifestream v{}", crate::updater::APP_VERSION), "Lifestream checks for updates each time it opens and only says something when there's a new version.", |ui| {
                if ui.button("Check for updates").clicked() {
                    self.manual_update_check = true;
                    self.updater.dismissed.store(false, std::sync::atomic::Ordering::SeqCst);
                    crate::updater::check_in_background(std::sync::Arc::clone(&self.updater), ui.ctx().clone());
                }
            });
        });
        if theme_switched && ff::is_green() {
            self.green_in_page = true;
        }
        if changed {
            self.changed();
        }
        if make_default {
            #[cfg(windows)]
            match crate::winint::register_handlers() {
                Ok(()) => self.notify("Magnet links and .torrent files now open in Lifestream."),
                Err(e) => self.notify(format!("Couldn't set that up: {e}")),
            }
        }
        if export {
            let name = format!("Lifestream-{}.{}", chrono::Local::now().format("%Y-%m-%d"), crate::backup::EXT);
            if let Some(p) = rfd::FileDialog::new().set_file_name(&name).add_filter("Lifestream backup", &[crate::backup::EXT]).save_file() {
                self.flush_settings_now();
                self.core.send(Cmd::SaveNow);
                std::thread::sleep(std::time::Duration::from_millis(600));
                match crate::backup::export(&p) {
                    Ok(n) => self.notify(format!("Backup saved ({n} files).")),
                    Err(e) => self.notify(format!("Backup failed: {e}")),
                }
            }
        }
        if restore {
            if let Some(p) = rfd::FileDialog::new().add_filter("Lifestream backup", &[crate::backup::EXT, "zip"]).pick_file() {
                match crate::backup::stage_restore(&p) {
                    Ok(()) => {
                        self.restore_pending = true;
                        self.notify("Backup ready. Restart Lifestream to load it.");
                    }
                    Err(e) => self.notify(format!("Couldn't restore: {e}")),
                }
            }
        }
    }

    // ===================================================================== Log

    pub(super) fn page_log(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ff::title(ui, "Log", 26.0, TEXT());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Clear").clicked() {
                    crate::applog::clear();
                }
                if ui.button("Open log file").clicked() {
                    open_path(&crate::settings::data_dir().display().to_string());
                }
            });
        });
        ui.label(dim("What Lifestream has been up to."));
        ff::divider(ui);
        egui::ScrollArea::vertical().id_salt("log").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            crate::applog::with_lines(|lines| {
                for l in lines {
                    let color = match l.level {
                        crate::applog::Level::Info => TEXT(),
                        crate::applog::Level::Warn => WARN(),
                        crate::applog::Level::Error => BAD(),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&l.time).color(TEXT_DIM()).monospace());
                        ui.label(RichText::new(&l.text).color(color));
                    });
                }
            });
        });
    }
}

fn popup_info(ui: &mut egui::Ui, t: &TorrentView) {
    let i = &t.info;
    let added = chrono::DateTime::from_timestamp(i.added_time, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let rows = [
        ("Saved in", i.save_path.clone()),
        ("Info hash", i.hash.clone()),
        ("Added", added),
        ("Downloaded (all time)", human_bytes(i.all_time_download)),
        ("Uploaded (all time)", human_bytes(i.all_time_upload)),
        ("Seeding time", human_eta(i.seeding_secs)),
        ("Full copies in swarm", format!("{:.2}", i.distributed_copies.max(0.0))),
        ("Queue position", if i.queue_position >= 0 { (i.queue_position + 1).to_string() } else { "—".into() }),
    ];
    egui::Grid::new("info_grid").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
        for (k, v) in rows {
            ff::stat_label(ui, k);
            ui.label(RichText::new(v).color(TEXT()));
            ui.end_row();
        }
    });
}

fn sort_name(s: Sort) -> &'static str {
    match s {
        Sort::Queue => "Queue order",
        Sort::Name => "Name",
        Sort::Health => "Seeders vs. leechers",
        Sort::DownSpeed => "Download speed",
        Sort::Ratio => "Ratio",
        Sort::Added => "Newest first",
    }
}

fn ip_line(ui: &mut egui::Ui, label: &str, r: &Result<crate::core::IpInfo, String>) {
    ui.horizontal_wrapped(|ui| {
        ff::stat_label(ui, label);
        match r {
            Ok(i) => {
                ff::title(ui, &i.ip, 15.0, TEXT());
                let mut bits = Vec::new();
                if !i.org.is_empty() {
                    bits.push(i.org.clone());
                }
                if !i.place.is_empty() {
                    bits.push(i.place.clone());
                }
                if !bits.is_empty() {
                    ui.label(RichText::new(bits.join(" · ")).color(TEXT_DIM()));
                }
            }
            Err(e) => {
                ff::title(ui, "NO CONNECTION", 15.0, BAD()).on_hover_text(e);
            }
        }
    });
}

/// Plain-language verdict for the two lookups.
fn leak_report(ui: &mut egui::Ui, r: &crate::core::LeakResult, adapter: &str) {
    ff::window(ui, |ui| {
        ui.set_width(ui.available_width());
        ip_line(ui, "EVERYTHING ELSE", &r.normal);
        match &r.torrent {
            None => {
                ui.add_space(4.0);
                ff::title(ui, "Torrents aren't bound to an adapter", 15.0, WARN());
                ui.add(egui::Label::new(dim(
                    "They use whatever route Windows picks, which is the address above. Pick your VPN adapter and turn on \"Only use this adapter\" so torrents can only go through it.",
                )).wrap());
            }
            Some(t) => {
                let via = r.bound_ip.clone().unwrap_or_default();
                ip_line(ui, &format!("TORRENTS (via {via})"), t);
                ui.add_space(6.0);
                match (t, &r.normal) {
                    (Err(_), _) => {
                        ff::title(ui, "Nothing gets out through this adapter", 15.0, BAD());
                        ui.add(egui::Label::new(dim(&format!(
                            "{adapter} can't reach the internet, so torrents can't connect either. That usually means it isn't a VPN tunnel (or the VPN is disconnected). Your real address is not being used for torrents."
                        ))).wrap());
                    }
                    (Ok(t), Ok(n)) if t.ip != n.ip => {
                        ff::title(ui, "Torrents go through your VPN", 15.0, GOOD());
                        ui.add(egui::Label::new(dim(&format!(
                            "Peers see {}{}. Your other apps use {} because your VPN only covers some traffic (split tunneling). That's fine: Lifestream is locked to the VPN.",
                            t.ip,
                            if t.org.is_empty() { String::new() } else { format!(" ({})", t.org) },
                            n.ip
                        ))).wrap());
                    }
                    (Ok(t), _) if crate::core::org_kind(&t.org) == crate::core::OrgKind::Vpn => {
                        ff::title(ui, "Everything goes through your VPN", 15.0, GOOD());
                        ui.add(egui::Label::new(dim(&format!(
                            "Peers see {} ({}), a VPN or datacenter address. Your whole PC is on the VPN, torrents included.",
                            t.ip, t.org
                        ))).wrap());
                    }
                    (Ok(t), _) if crate::core::org_kind(&t.org) == crate::core::OrgKind::HomeIsp => {
                        ff::title(ui, "Torrents are NOT going through a VPN", 15.0, BAD());
                        ui.add(egui::Label::new(dim(&format!(
                            "Peers see {} ({}), which looks like a home internet provider. {adapter} isn't tunnelling to the internet: pick your VPN's adapter, and make sure the VPN is connected.",
                            t.ip, t.org
                        ))).wrap());
                    }
                    (Ok(t), _) => {
                        ff::title(ui, "Same address on both routes", 15.0, WARN());
                        ui.add(egui::Label::new(dim(&format!(
                            "Peers see {}{}. If that's your VPN provider, everything on this PC goes through the VPN and you're covered. If it's your home internet provider, {adapter} isn't tunnelling to the internet: pick the right adapter.",
                            t.ip,
                            if t.org.is_empty() { String::new() } else { format!(" ({})", t.org) }
                        ))).wrap());
                    }
                }
            }
        }
    });
}
