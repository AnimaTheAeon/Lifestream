//! The background worker that owns the torrent engine.
//!
//! The UI never touches the engine directly: it sends [`Cmd`]s and reads the
//! latest [`Snapshot`]. Everything that must keep running while the window is
//! hidden (kill switch, turtle schedule, seeding rules, RSS, watch folder)
//! lives here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::applog;
use crate::engine::ffi::{self, AddOptions, Engine, FileEntry, LtSettings, TorrentInfo};
use crate::rss::{FeedItem, RssHistory};
use crate::settings::{SeedAction, Settings};

pub enum Cmd {
    ApplySettings(Box<Settings>),
    AddMagnet { uri: String, category: String, choice: AddChoice },
    AddTorrent { data: Vec<u8>, category: String, choice: AddChoice, priorities: Option<Vec<u8>> },
    AddFromUrl { url: String, category: String },
    Pause(String),
    Resume(String),
    Remove { hash: String, delete_files: bool },
    Recheck(String),
    Sequential(String, bool),
    SuperSeed(String, bool),
    FirstLast(String),
    QueueTop(String),
    FilePriority { hash: String, index: i32, priority: i32 },
    AddTracker(String, String),
    WatchFiles(Option<String>),
    PauseAll,
    ResumeAll,
    SetTurtle(bool),
    #[cfg_attr(not(windows), allow(dead_code))]
    SetRss(bool),
    CopyMagnet(String),
    RssCheckNow,
    UpdateBlocklist,
    LeakCheck,
    SaveNow,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AddChoice {
    pub paused: bool,
    pub sequential: bool,
    pub first_last: bool,
    pub skip_check: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TorrentView {
    pub info: TorrentInfo,
    pub category: String,
    pub ratio: f64,
    pub eta_secs: Option<i64>,
    pub seed_goal_met: bool,
}

#[derive(Clone, Debug)]
pub struct FeedStatus {
    pub checked: String,
    pub result: Result<usize, String>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub started: Instant,
    pub torrents: Vec<TorrentView>,
    pub dl_rate: i64,
    pub ul_rate: i64,
    pub files_hash: Option<String>,
    pub files: Vec<FileEntry>,
    pub trackers: Vec<String>,
    pub user_paused: bool,
    pub kill_hold: bool,
    pub kill_reason: String,
    pub adapter_ip: Option<String>,
    pub listen_port: i32,
    pub external_port: i32,
    pub listen_error: String,
    pub incoming_seen: bool,
    pub turtle_active: bool,
    pub public_ip: Option<Result<String, String>>,
    pub leak_checking: bool,
    pub rss_items: Vec<FeedItem>,
    pub rss_status: HashMap<String, FeedStatus>,
    pub rss_checking: bool,
    pub blocklist_count: usize,
    pub blocklist_status: String,
    pub restart_needed: bool,
    pub engine_error: Option<String>,
    pub anonymous_effective: bool,
    pub toast: Option<(String, Instant)>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            torrents: Vec::new(),
            dl_rate: 0,
            ul_rate: 0,
            files_hash: None,
            files: Vec::new(),
            trackers: Vec::new(),
            user_paused: false,
            kill_hold: false,
            kill_reason: String::new(),
            adapter_ip: None,
            listen_port: 0,
            external_port: 0,
            listen_error: String::new(),
            incoming_seen: false,
            turtle_active: false,
            public_ip: None,
            leak_checking: false,
            rss_items: Vec::new(),
            rss_status: HashMap::new(),
            rss_checking: false,
            blocklist_count: 0,
            blocklist_status: String::new(),
            restart_needed: false,
            engine_error: None,
            anonymous_effective: true,
            toast: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Meta {
    category: String,
    /// Where finished files move to when "keep unfinished separate" is on.
    final_path: Option<String>,
    /// The seeding goal already fired; don't fire again if the user resumes.
    seed_rule_done: bool,
}

fn meta_path() -> PathBuf {
    crate::settings::data_dir().join("torrents.json")
}

fn load_meta() -> HashMap<String, Meta> {
    std::fs::read_to_string(meta_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct CoreHandle {
    pub tx: Sender<Cmd>,
    pub snap: Arc<Mutex<Snapshot>>,
}

impl CoreHandle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
    pub fn snapshot(&self) -> Snapshot {
        self.snap.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

pub fn start(settings: Settings) -> (CoreHandle, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let snap = Arc::new(Mutex::new(Snapshot::default()));
    let handle = CoreHandle { tx: tx.clone(), snap: Arc::clone(&snap) };
    let join = std::thread::Builder::new()
        .name("lifestream-core".into())
        .spawn(move || Core::new(settings, rx, tx, snap).run())
        .expect("couldn't start the core thread");
    (handle, join)
}

/// Results from helper threads.
enum Bg {
    Rss(String, Result<Vec<FeedItem>, String>),
    RssDone,
    Blocklist(Result<Vec<ffi::IpRange>, String>),
    PublicIp(Result<String, String>),
    Download { url: String, category: String, data: Result<Vec<u8>, String> },
}

struct Core {
    settings: Settings,
    rx: Receiver<Cmd>,
    tx: Sender<Cmd>,
    bg_tx: Sender<Bg>,
    bg_rx: Receiver<Bg>,
    snap: Arc<Mutex<Snapshot>>,
    engine: Option<cxx::UniquePtr<Engine>>,
    meta: HashMap<String, Meta>,
    pex_at_start: bool,

    user_paused: bool,
    kill_hold: bool,
    kill_reason: String,
    applied: Option<LtSettings>,
    adapter_ip: Option<String>,
    turtle_active: bool,
    watch_files: Option<String>,
    incoming_seen: bool,
    last_external_port: i32,

    rss_history: RssHistory,
    rss_items: Vec<FeedItem>,
    rss_status: HashMap<String, FeedStatus>,
    rss_last_check: Option<Instant>,
    rss_checking: bool,
    blocklist_count: usize,
    blocklist_status: String,
    public_ip: Option<Result<String, String>>,
    leak_checking: bool,
    toast: Option<(String, Instant)>,
    started: Instant,
}

impl Core {
    fn new(settings: Settings, rx: Receiver<Cmd>, tx: Sender<Cmd>, snap: Arc<Mutex<Snapshot>>) -> Self {
        let (bg_tx, bg_rx) = mpsc::channel();
        Self {
            pex_at_start: settings.pex,
            settings,
            rx,
            tx,
            bg_tx,
            bg_rx,
            snap,
            engine: None,
            meta: load_meta(),
            user_paused: false,
            kill_hold: false,
            kill_reason: String::new(),
            applied: None,
            adapter_ip: None,
            turtle_active: false,
            watch_files: None,
            incoming_seen: false,
            last_external_port: 0,
            rss_history: RssHistory::load(),
            rss_items: Vec::new(),
            rss_status: HashMap::new(),
            rss_last_check: None,
            rss_checking: false,
            blocklist_count: 0,
            blocklist_status: String::new(),
            public_ip: None,
            leak_checking: false,
            toast: None,
            started: Instant::now(),
        }
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn engine(&mut self) -> Option<std::pin::Pin<&mut Engine>> {
        self.engine.as_mut().and_then(|e| e.as_mut())
    }

    // ---------------------------------------------------------------- settings

    /// The adapter state decides whether we may touch the network at all.
    fn evaluate_adapter(&mut self) {
        let s = &self.settings;
        let mut hold = false;
        let mut reason = String::new();
        let mut ip = None;
        if s.adapter.is_empty() {
            if s.kill_switch {
                hold = true;
                reason = "Pick your VPN adapter on the Network page. Downloads wait until you do, because the kill switch is on.".into();
            }
        } else {
            match crate::netif::find(&s.adapter) {
                Some(a) => {
                    ip = a
                        .ipv4
                        .map(|v| v.to_string())
                        .or_else(|| if s.disable_ipv6 { None } else { a.ipv6.map(|v| v.to_string()) });
                    if ip.is_none() && s.kill_switch {
                        hold = true;
                        reason = format!("{} has no usable address. Everything is paused until it does.", s.adapter);
                    }
                }
                None => {
                    if s.kill_switch {
                        hold = true;
                        reason = format!("{} is down. Everything is paused until it comes back.", s.adapter);
                    }
                }
            }
        }
        if hold != self.kill_hold {
            if hold {
                applog::warn(format!("Kill switch: {reason}"));
                self.toast("Kill switch tripped: downloads paused");
            } else if self.kill_hold {
                applog::info("Kill switch: adapter is back, resuming.");
                self.toast("VPN is back: resuming");
            }
        }
        self.kill_hold = hold;
        self.kill_reason = reason;
        self.adapter_ip = ip;
    }

    fn in_turtle_window(&self) -> bool {
        let s = &self.settings;
        if !s.turtle_schedule {
            return false;
        }
        let now = Local::now();
        let day = now.weekday().num_days_from_monday() as usize;
        let parse = |t: &str| NaiveTime::parse_from_str(t.trim(), "%H:%M").ok();
        let (Some(from), Some(to)) = (parse(&s.turtle_from), parse(&s.turtle_to)) else {
            return false;
        };
        let t = now.time();
        if from <= to {
            s.turtle_days[day] && t >= from && t < to
        } else {
            // Overnight window, e.g. 22:00-06:00: belongs to the day it started.
            let yesterday = (day + 6) % 7;
            (s.turtle_days[day] && t >= from) || (s.turtle_days[yesterday] && t < to)
        }
    }

    fn any_private_active(&self) -> bool {
        self.snap
            .lock()
            .map(|s| s.torrents.iter().any(|t| t.info.is_private && !t.info.paused))
            .unwrap_or(false)
    }

    fn build_lt(&self) -> LtSettings {
        let s = &self.settings;
        let kb = |v: i32| if v <= 0 { 0 } else { v.saturating_mul(1024) };
        let (dl, ul) = if self.turtle_active {
            (kb(s.turtle_dl_kb), kb(s.turtle_ul_kb))
        } else {
            (kb(s.dl_limit_kb), kb(s.ul_limit_kb))
        };
        let port = s.listen_port;
        let (listen, outgoing) = if self.kill_hold {
            // Nowhere to go: even DHT can't leak out of loopback.
            (format!("127.0.0.1:{port}"), "127.0.0.1".to_string())
        } else if s.bind_only && self.adapter_ip.is_some() {
            let ip = self.adapter_ip.clone().unwrap();
            let listen = if ip.contains(':') { format!("[{ip}]:{port}") } else { format!("{ip}:{port}") };
            (listen, ip)
        } else if s.disable_ipv6 {
            (format!("0.0.0.0:{port}"), String::new())
        } else {
            (format!("0.0.0.0:{port},[::]:{port}"), String::new())
        };
        let anonymous = s.anonymous_mode && !(s.private_real_name && self.any_private_active());
        let version = env!("CARGO_PKG_VERSION");
        LtSettings {
            download_limit: dl,
            upload_limit: ul,
            connections_limit: s.max_peers_global.max(10),
            unchoke_slots: s.upload_slots_global.max(1),
            active_downloads: s.active_downloads.max(1),
            active_seeds: s.active_seeds.max(0),
            active_limit: (s.active_downloads + s.active_seeds).max(2),
            listen_interfaces: listen,
            outgoing_interfaces: outgoing,
            enable_dht: s.dht && !self.kill_hold,
            enable_lsd: s.lsd && !self.kill_hold,
            enable_upnp: s.upnp && !self.kill_hold,
            enable_natpmp: s.natpmp && !self.kill_hold,
            encryption: s.encryption,
            anonymous_mode: anonymous,
            user_agent: format!("Lifestream/{version}"),
            peer_fingerprint: "-LS0100-".into(),
            proxy_type: s.proxy_type,
            proxy_host: s.proxy_host.clone(),
            proxy_port: s.proxy_port,
            proxy_user: s.proxy_user.clone(),
            proxy_pass: s.proxy_pass.clone(),
            proxy_peers: s.proxy_peers,
            proxy_trackers: s.proxy_trackers,
        }
    }

    fn lt_eq(a: &LtSettings, b: &LtSettings) -> bool {
        format!("{a:?}") == format!("{b:?}")
    }

    /// Push the current state into the engine if anything changed.
    fn sync_engine(&mut self) {
        let want = self.build_lt();
        let changed = self.applied.as_ref().map(|a| !Self::lt_eq(a, &want)).unwrap_or(true);
        let should_pause = self.user_paused || self.kill_hold;
        let anon = want.anonymous_mode;
        if let Some(mut e) = self.engine() {
            if should_pause && !e.session_paused() {
                e.as_mut().pause_session();
            }
            if changed {
                e.as_mut().apply_settings(&want);
            }
            if !should_pause && e.session_paused() {
                e.as_mut().resume_session();
            }
        }
        if changed {
            self.applied = Some(want);
        }
        if let Ok(mut s) = self.snap.lock() {
            s.anonymous_effective = anon;
        }
    }

    fn apply_settings(&mut self, new: Settings) {
        let blocklist_changed = new.blocklist_url != self.settings.blocklist_url;
        let per_torrent_changed = new.max_peers_torrent != self.settings.max_peers_torrent
            || new.upload_slots_torrent != self.settings.upload_slots_torrent;
        self.settings = new;
        self.settings.save();
        self.evaluate_adapter();
        self.turtle_active = self.settings.turtle_manual || self.in_turtle_window();
        self.sync_engine();
        if per_torrent_changed {
            let hashes: Vec<String> = self.current_hashes();
            let (c, u) = (self.settings.max_peers_torrent, self.settings.upload_slots_torrent);
            if let Some(mut e) = self.engine() {
                for h in &hashes {
                    e.as_mut().set_torrent_limits(h, c, u);
                }
            }
        }
        if blocklist_changed {
            if self.settings.blocklist_url.trim().is_empty() {
                crate::blocklist::clear_cache();
                self.blocklist_count = 0;
                self.blocklist_status = "No blocklist.".into();
                if let Some(e) = self.engine() {
                    e.set_ip_filter(&Vec::new());
                }
            } else {
                self.update_blocklist();
            }
        }
        #[cfg(windows)]
        crate::winint::set_autostart(self.settings.start_with_windows);
    }

    fn current_hashes(&self) -> Vec<String> {
        self.snap
            .lock()
            .map(|s| s.torrents.iter().map(|t| t.info.hash.clone()).collect())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- adding

    fn add_options(&self, category: &str, choice: AddChoice) -> (AddOptions, Option<String>) {
        let final_path = self.settings.path_for_category(category);
        let (save_path, final_dest) = if self.settings.incomplete_enabled
            && !self.settings.incomplete_dir.trim().is_empty()
            && !choice.skip_check
        {
            (self.settings.incomplete_dir.clone(), Some(final_path))
        } else {
            (final_path, None)
        };
        let _ = std::fs::create_dir_all(&save_path);
        (
            AddOptions {
                save_path,
                paused: choice.paused,
                sequential: choice.sequential,
                first_last: choice.first_last,
                seed_mode: choice.skip_check,
                max_connections: self.settings.max_peers_torrent,
                max_uploads: self.settings.upload_slots_torrent,
                use_priorities: false,
                file_priorities: Vec::new(),
            },
            final_dest,
        )
    }

    fn default_choice(&self) -> AddChoice {
        AddChoice {
            paused: self.settings.start_paused,
            sequential: self.settings.default_sequential,
            first_last: self.settings.default_first_last,
            skip_check: false,
        }
    }

    fn remember(&mut self, hash: &str, category: &str, final_path: Option<String>) {
        self.meta.insert(
            hash.to_string(),
            Meta { category: category.to_string(), final_path, seed_rule_done: false },
        );
        self.save_meta();
    }

    fn save_meta(&self) {
        if let Ok(t) = serde_json::to_string_pretty(&self.meta) {
            let _ = std::fs::write(meta_path(), t);
        }
    }

    fn add_magnet(&mut self, uri: &str, category: &str, choice: AddChoice) -> Result<String, String> {
        let (opts, final_path) = self.add_options(category, choice);
        let e = self.engine().ok_or("The torrent engine isn't running.")?;
        let hash = e.add_magnet(uri.trim(), &opts).map_err(|e| e.what().to_string())?;
        self.remember(&hash, category, final_path);
        Ok(hash)
    }

    fn add_bytes(
        &mut self,
        data: &[u8],
        category: &str,
        choice: AddChoice,
        priorities: Option<Vec<u8>>,
    ) -> Result<String, String> {
        let (mut opts, final_path) = self.add_options(category, choice);
        if let Some(p) = priorities {
            opts.use_priorities = true;
            opts.file_priorities = p;
        }
        let e = self.engine().ok_or("The torrent engine isn't running.")?;
        let hash = e.add_torrent_bytes(data, &opts).map_err(|e| e.what().to_string())?;
        self.remember(&hash, category, final_path);
        Ok(hash)
    }

    fn report_add(&mut self, what: &str, r: Result<String, String>) {
        match r {
            Ok(_) => {
                applog::info(format!("Added {what}"));
                self.toast(format!("Added {what}"));
            }
            Err(e) => {
                applog::error(format!("Couldn't add {what}: {e}"));
                self.toast(format!("Couldn't add: {e}"));
            }
        }
    }

    fn download_torrent_url(&self, url: String, category: String) {
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            let data = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(45))
                .user_agent("Mozilla/5.0")
                .build()
                .and_then(|c| c.get(&url).send())
                .and_then(|r| r.error_for_status())
                .and_then(|r| r.bytes())
                .map(|b| b.to_vec())
                .map_err(|e| e.to_string());
            let _ = tx.send(Bg::Download { url, category, data });
        });
    }

    // ---------------------------------------------------------------- commands

    fn handle(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::ApplySettings(s) => self.apply_settings(*s),
            Cmd::AddMagnet { uri, category, choice } => {
                let r = self.add_magnet(&uri, &category, choice);
                self.report_add("magnet link", r);
            }
            Cmd::AddTorrent { data, category, choice, priorities } => {
                let r = self.add_bytes(&data, &category, choice, priorities);
                self.report_add("torrent", r);
            }
            Cmd::AddFromUrl { url, category } => {
                if url.trim_start().starts_with("magnet:") {
                    let c = self.default_choice();
                    let r = self.add_magnet(&url, &category, c);
                    self.report_add("magnet link", r);
                } else {
                    self.download_torrent_url(url, category);
                }
            }
            Cmd::Pause(h) => {
                if let Some(e) = self.engine() {
                    e.pause_torrent(&h)
                }
            }
            Cmd::Resume(h) => {
                if let Some(e) = self.engine() {
                    e.resume_torrent(&h)
                }
            }
            Cmd::Remove { hash, delete_files } => {
                if let Some(e) = self.engine() {
                    e.remove_torrent(&hash, delete_files);
                }
                self.meta.remove(&hash);
                self.save_meta();
                applog::info(if delete_files { "Removed torrent and deleted its files." } else { "Removed torrent (files kept)." });
            }
            Cmd::Recheck(h) => {
                if let Some(e) = self.engine() {
                    e.recheck(&h)
                }
            }
            Cmd::Sequential(h, on) => {
                if let Some(e) = self.engine() {
                    e.set_sequential(&h, on)
                }
            }
            Cmd::SuperSeed(h, on) => {
                if let Some(e) = self.engine() {
                    e.set_super_seeding(&h, on)
                }
            }
            Cmd::FirstLast(h) => {
                if let Some(e) = self.engine() {
                    e.set_first_last(&h)
                }
            }
            Cmd::QueueTop(h) => {
                if let Some(e) = self.engine() {
                    e.queue_top(&h)
                }
            }
            Cmd::FilePriority { hash, index, priority } => {
                if let Some(e) = self.engine() {
                    e.set_file_priority(&hash, index, priority)
                }
            }
            Cmd::AddTracker(h, url) => {
                if let Some(e) = self.engine() {
                    e.add_tracker(&h, &url)
                }
            }
            Cmd::WatchFiles(h) => self.watch_files = h,
            Cmd::PauseAll => {
                self.user_paused = true;
                self.sync_engine();
                applog::info("Paused everything.");
            }
            Cmd::ResumeAll => {
                self.user_paused = false;
                self.sync_engine();
                applog::info("Resumed everything.");
            }
            Cmd::SetTurtle(on) => {
                self.settings.turtle_manual = on;
                self.settings.save();
                self.turtle_active = on || self.in_turtle_window();
                self.sync_engine();
            }
            Cmd::SetRss(on) => {
                self.settings.rss_enabled = on;
                self.settings.save();
            }
            Cmd::RssCheckNow => self.check_rss(true),
            Cmd::CopyMagnet(h) => {
                if let Some(e) = self.engine.as_ref() {
                    let link = e.magnet_link(&h);
                    if let Ok(mut c) = arboard::Clipboard::new() {
                        let _ = c.set_text(link);
                    }
                }
                self.toast("Magnet link copied");
            }
            Cmd::UpdateBlocklist => self.update_blocklist(),
            Cmd::LeakCheck => self.leak_check(),
            Cmd::SaveNow => {
                if let Some(e) = self.engine() {
                    e.request_resume_save(true)
                }
            }
            Cmd::Shutdown => return false,
        }
        true
    }

    // ---------------------------------------------------------------- helpers

    fn leak_check(&mut self) {
        if self.leak_checking {
            return;
        }
        self.leak_checking = true;
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            let r = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .and_then(|c| c.get("https://api.ipify.org").send())
                .and_then(|r| r.text())
                .map(|t| t.trim().to_string())
                .map_err(|e| e.to_string());
            let _ = tx.send(Bg::PublicIp(r));
        });
    }

    fn update_blocklist(&mut self) {
        let url = self.settings.blocklist_url.trim().to_string();
        if url.is_empty() {
            return;
        }
        self.blocklist_status = "Downloading…".into();
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Bg::Blocklist(crate::blocklist::download(&url)));
        });
    }

    fn check_rss(&mut self, forced: bool) {
        if self.rss_checking {
            return;
        }
        let due = match self.rss_last_check {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(self.settings.rss_interval_min.max(5) as u64 * 60),
        };
        if !forced && (!due || !self.settings.rss_enabled) {
            return;
        }
        let feeds: Vec<_> = self.settings.feeds.iter().filter(|f| f.enabled).cloned().collect();
        self.rss_last_check = Some(Instant::now());
        if feeds.is_empty() {
            return;
        }
        self.rss_checking = true;
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            for feed in feeds {
                let r = crate::rss::fetch(&feed);
                let _ = tx.send(Bg::Rss(feed.name.clone(), r));
            }
            let _ = tx.send(Bg::RssDone);
        });
    }

    fn run_rss_rules(&mut self) {
        if !self.settings.rss_enabled {
            return;
        }
        let rules: Vec<_> = self.settings.rules.iter().filter(|r| r.enabled).cloned().collect();
        let items = self.rss_items.clone();
        let mut changed = false;
        for item in &items {
            if self.rss_history.seen.contains(&item.guid) {
                continue;
            }
            for rule in &rules {
                match crate::rss::rule_matches(rule, item) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(e) => {
                        applog::warn(format!("RSS rule \"{}\": {e}", rule.name));
                        continue;
                    }
                }
                if rule.skip_duplicates {
                    if let Some(key) = crate::rss::episode_key(&item.title) {
                        let set = self.rss_history.episodes.entry(rule.name.clone()).or_default();
                        if !set.insert(key) {
                            applog::info(format!("RSS: skipped duplicate {}", item.title));
                            break;
                        }
                    }
                }
                applog::info(format!("RSS rule \"{}\" grabbed {}", rule.name, item.title));
                let _ = self.tx.send(Cmd::AddFromUrl { url: item.link.clone(), category: rule.category.clone() });
                break;
            }
            self.rss_history.seen.insert(item.guid.clone());
            changed = true;
        }
        if changed {
            self.rss_history.save();
        }
    }

    fn handle_bg(&mut self, bg: Bg) {
        match bg {
            Bg::Rss(name, r) => {
                let checked = Local::now().format("%H:%M").to_string();
                match r {
                    Ok(items) => {
                        let n = items.len();
                        self.rss_items.retain(|i| i.feed != name);
                        self.rss_items.extend(items);
                        self.rss_status.insert(name, FeedStatus { checked, result: Ok(n) });
                    }
                    Err(e) => {
                        applog::warn(format!("RSS feed \"{name}\": {e}"));
                        self.rss_status.insert(name, FeedStatus { checked, result: Err(e) });
                    }
                }
            }
            Bg::RssDone => {
                self.rss_checking = false;
                self.run_rss_rules();
            }
            Bg::Blocklist(r) => match r {
                Ok(ranges) => {
                    self.blocklist_count = ranges.len();
                    self.blocklist_status = format!("{} ranges loaded.", ranges.len());
                    self.settings.blocklist_last_update = chrono::Utc::now().timestamp();
                    self.settings.save();
                    if let Some(e) = self.engine() {
                        e.set_ip_filter(&ranges);
                    }
                    applog::info(format!("Blocklist updated: {} ranges", ranges.len()));
                }
                Err(e) => {
                    self.blocklist_status = format!("Update failed: {e}");
                    applog::warn(format!("Blocklist: {e}"));
                }
            },
            Bg::PublicIp(r) => {
                self.leak_checking = false;
                self.public_ip = Some(r);
            }
            Bg::Download { url, category, data } => match data {
                Ok(bytes) => {
                    let c = self.default_choice();
                    let r = self.add_bytes(&bytes, &category, c, None);
                    self.report_add("torrent from feed", r);
                }
                Err(e) => applog::error(format!("Couldn't download {url}: {e}")),
            },
        }
    }

    fn handle_alert(&mut self, line: &str) {
        let mut parts = line.splitn(3, '|');
        let kind = parts.next().unwrap_or("");
        let hash = parts.next().unwrap_or("").to_string();
        let msg = parts.next().unwrap_or("").to_string();
        match kind {
            "FINISHED" => {
                // Already-complete torrents report "finished" again when they load.
                if self.started.elapsed() > Duration::from_secs(20) {
                    applog::info(format!("Finished: {msg}"));
                    self.toast("Download finished");
                }
                let dest = self.meta.get(&hash).and_then(|m| m.final_path.clone());
                if let Some(dest) = dest {
                    if let Some(e) = self.engine() {
                        e.move_storage(&hash, &dest);
                    }
                    if let Some(m) = self.meta.get_mut(&hash) {
                        m.final_path = None;
                    }
                    self.save_meta();
                }
            }
            "PORTMAP" => {
                let port: i32 = msg.split('|').next().and_then(|p| p.parse().ok()).unwrap_or(0);
                applog::info(msg.split('|').nth(1).unwrap_or("").to_string());
                if self.settings.follow_vpn_port
                    && port > 0
                    && port != self.settings.listen_port as i32
                    && port != self.last_external_port
                {
                    self.last_external_port = port;
                    applog::info(format!("VPN forwarded port {port}; listening on it now."));
                    self.settings.listen_port = port as u16;
                    self.settings.save();
                    self.sync_engine();
                }
            }
            "ERROR" => applog::error(msg),
            "METADATA" | "MOVED" | "LOG" => applog::info(msg),
            _ => {}
        }
    }

    fn scan_watch_folder(&mut self) {
        if !self.settings.watch_enabled || self.settings.watch_dir.trim().is_empty() {
            return;
        }
        let dir = PathBuf::from(&self.settings.watch_dir);
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("torrent")) != Some(true) {
                continue;
            }
            // Skip files that are still being written.
            if let Ok(m) = entry.metadata() {
                if m.modified().ok().and_then(|t| t.elapsed().ok()).map(|d| d < Duration::from_secs(2)).unwrap_or(false) {
                    continue;
                }
            }
            let Ok(data) = std::fs::read(&path) else { continue };
            let c = self.default_choice();
            let cat = String::new();
            let r = self.add_bytes(&data, &cat, c, None);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.report_add(&name, r);
            done_with_watched(&path, self.settings.watch_delete);
        }
    }

    fn apply_seed_rules(&mut self, torrents: &[TorrentView]) {
        let s = self.settings.clone();
        let mut fired = Vec::new();
        for t in torrents {
            if !t.seed_goal_met || t.info.paused {
                continue;
            }
            if self.meta.get(&t.info.hash).map(|m| m.seed_rule_done).unwrap_or(false) {
                continue;
            }
            fired.push((t.info.hash.clone(), t.info.name.clone()));
        }
        for (hash, name) in fired {
            let meta = self.meta.entry(hash.clone()).or_default();
            meta.seed_rule_done = true;
            match s.seed_action {
                SeedAction::Pause => {
                    applog::info(format!("Seeding goal reached, paused: {name}"));
                    if let Some(e) = self.engine() {
                        e.pause_torrent(&hash);
                    }
                }
                SeedAction::Remove | SeedAction::RemoveAndDelete => {
                    let del = s.seed_action == SeedAction::RemoveAndDelete;
                    applog::info(format!("Seeding goal reached, removed: {name}"));
                    if let Some(e) = self.engine() {
                        e.remove_torrent(&hash, del);
                    }
                    self.meta.remove(&hash);
                }
            }
            self.save_meta();
        }
    }

    fn seed_goal_met(&self, t: &TorrentInfo, ratio: f64) -> bool {
        let s = &self.settings;
        if t.state != 4 && t.state != 3 {
            return false;
        }
        if t.is_private && s.private_honor_seed && (t.seeding_secs as f32) < s.private_min_seed_hours * 3600.0 {
            return false;
        }
        (s.ratio_limit > 0.0 && ratio >= s.ratio_limit as f64)
            || (s.seed_hours > 0.0 && t.seeding_secs as f32 >= s.seed_hours * 3600.0)
    }

    fn refresh_snapshot(&mut self) {
        let Some(e) = self.engine.as_ref() else { return };
        let infos = e.torrents();
        let totals = e.totals();
        let (files, trackers) = match &self.watch_files {
            Some(h) => (e.files(h), e.trackers(h)),
            None => (Vec::new(), Vec::new()),
        };
        let mut views = Vec::with_capacity(infos.len());
        let (mut dl, mut ul) = (0i64, 0i64);
        for info in infos {
            dl += info.dl_rate as i64;
            ul += info.ul_rate as i64;
            let base = info.all_time_download.max(info.total_done).max(1);
            let ratio = info.all_time_upload as f64 / base as f64;
            let left = (info.total_wanted - info.total_done).max(0);
            let eta = if info.state == 2 && info.dl_rate > 0 { Some(left / info.dl_rate as i64) } else { None };
            let met = self.seed_goal_met(&info, ratio);
            let category = self.meta.get(&info.hash).map(|m| m.category.clone()).unwrap_or_default();
            views.push(TorrentView { info, category, ratio, eta_secs: eta, seed_goal_met: met });
        }
        views.sort_by_key(|v| (v.info.queue_position < 0, v.info.queue_position, v.info.added_time));
        self.apply_seed_rules(&views);

        if let Ok(mut s) = self.snap.lock() {
            s.started = self.started;
            s.torrents = views;
            s.dl_rate = dl;
            s.ul_rate = ul;
            s.files_hash = self.watch_files.clone();
            s.files = files;
            s.trackers = trackers;
            s.user_paused = self.user_paused;
            s.kill_hold = self.kill_hold;
            s.kill_reason = self.kill_reason.clone();
            s.adapter_ip = self.adapter_ip.clone();
            s.listen_port = totals.listen_port;
            s.external_port = totals.external_port;
            s.listen_error = totals.listen_error.clone();
            s.incoming_seen = self.incoming_seen;
            s.turtle_active = self.turtle_active;
            s.public_ip = self.public_ip.clone();
            s.leak_checking = self.leak_checking;
            s.rss_items = self.rss_items.clone();
            s.rss_status = self.rss_status.clone();
            s.rss_checking = self.rss_checking;
            s.blocklist_count = self.blocklist_count;
            s.blocklist_status = self.blocklist_status.clone();
            s.restart_needed = self.settings.pex != self.pex_at_start;
            s.toast = self.toast.clone();
        }
    }

    fn set_awake(&self) {
        #[cfg(windows)]
        {
            let busy = self.settings.prevent_sleep
                && self.snap.lock().map(|s| s.torrents.iter().any(|t| t.info.state == 2 && !t.info.paused)).unwrap_or(false);
            crate::winint::keep_awake(busy);
        }
    }

    // ---------------------------------------------------------------- main loop

    fn run(mut self) {
        applog::info(format!("Lifestream {} starting.", env!("CARGO_PKG_VERSION")));
        self.evaluate_adapter();
        self.turtle_active = self.settings.turtle_manual || self.in_turtle_window();
        let lt = self.build_lt();
        let state_dir = crate::settings::engine_dir().display().to_string();
        let start_paused = self.kill_hold;
        match ffi::new_engine(&lt, self.settings.pex, &state_dir, start_paused) {
            Ok(e) => {
                self.engine = Some(e);
                self.applied = Some(lt);
                applog::info("Torrent engine started.");
            }
            Err(e) => {
                let msg = format!("The torrent engine couldn't start: {}", e.what());
                applog::error(&msg);
                if let Ok(mut s) = self.snap.lock() {
                    s.engine_error = Some(msg);
                }
            }
        }
        if self.kill_hold {
            applog::warn(format!("Kill switch: {}", self.kill_reason));
        }
        let cached = crate::blocklist::load_cached();
        if !cached.is_empty() {
            self.blocklist_count = cached.len();
            self.blocklist_status = format!("{} ranges loaded.", cached.len());
            if let Some(e) = self.engine() {
                e.set_ip_filter(&cached);
            }
        } else if !self.settings.blocklist_url.trim().is_empty() {
            self.update_blocklist();
        }
        self.sync_engine();

        let mut last_adapter = Instant::now();
        let mut last_slow = Instant::now() - Duration::from_secs(60);
        let mut last_resume_save = Instant::now();
        let mut last_incoming = Instant::now() - Duration::from_secs(60);
        loop {
            // Commands from the UI, briefly blocking so the loop isn't hot.
            match self.rx.recv_timeout(Duration::from_millis(250)) {
                Ok(cmd) => {
                    if !self.handle(cmd) {
                        break;
                    }
                    while let Ok(cmd) = self.rx.try_recv() {
                        if !self.handle(cmd) {
                            return self.shutdown();
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            while let Ok(bg) = self.bg_rx.try_recv() {
                self.handle_bg(bg);
            }
            let alerts = match self.engine() {
                Some(e) => e.pop_alerts(),
                None => Vec::new(),
            };
            for a in alerts {
                self.handle_alert(&a);
            }

            // The kill switch checks the adapter twice a second.
            if last_adapter.elapsed() >= Duration::from_millis(500) {
                last_adapter = Instant::now();
                self.evaluate_adapter();
                self.sync_engine();
            }
            self.refresh_snapshot();

            if last_slow.elapsed() >= Duration::from_secs(5) {
                last_slow = Instant::now();
                let turtle = self.settings.turtle_manual || self.in_turtle_window();
                if turtle != self.turtle_active {
                    self.turtle_active = turtle;
                    applog::info(if turtle { "Turtle mode on." } else { "Turtle mode off." });
                    self.sync_engine();
                }
                self.scan_watch_folder();
                self.check_rss(false);
                self.set_awake();
                if self.settings.blocklist_auto
                    && !self.settings.blocklist_url.trim().is_empty()
                    && chrono::Utc::now().timestamp() - self.settings.blocklist_last_update > 7 * 86400
                    && self.blocklist_status != "Downloading…"
                {
                    self.update_blocklist();
                }
                if self.settings.auto_backup {
                    crate::backup::auto_backup_if_due(&mut self.settings);
                }
                if self.toast.as_ref().map(|t| t.1.elapsed() > Duration::from_secs(5)).unwrap_or(false) {
                    self.toast = None;
                }
            }
            if last_incoming.elapsed() >= Duration::from_secs(15) {
                last_incoming = Instant::now();
                if !self.incoming_seen {
                    self.incoming_seen = self.engine.as_ref().map(|e| e.check_incoming()).unwrap_or(false);
                }
            }
            if last_resume_save.elapsed() >= Duration::from_secs(120) {
                last_resume_save = Instant::now();
                if let Some(e) = self.engine() {
                    e.request_resume_save(false);
                }
            }
        }
        self.shutdown();
    }

    fn shutdown(mut self) {
        applog::info("Saving progress and shutting down…");
        if let Some(e) = self.engine() {
            e.save_all_and_wait(5000);
        }
        self.engine = None;
        self.rss_history.save();
        self.save_meta();
        #[cfg(windows)]
        crate::winint::keep_awake(false);
    }
}

fn done_with_watched(path: &Path, delete: bool) {
    if delete {
        let _ = std::fs::remove_file(path);
    } else {
        let mut added = path.as_os_str().to_owned();
        added.push(".added");
        let _ = std::fs::rename(path, PathBuf::from(added));
    }
}

/// Shown in the UI before the user has a real list.
pub fn human_bytes(n: i64) -> String {
    let n = n.max(0) as f64;
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", v as i64, UNITS[i])
    } else if v >= 100.0 {
        format!("{:.0} {}", v, UNITS[i])
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

pub fn human_rate(n: i64) -> String {
    format!("{}/s", human_bytes(n))
}

pub fn human_eta(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs / 60) % 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs / 3600) % 24)
    }
}
