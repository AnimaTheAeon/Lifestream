//! User settings, saved as JSON in the data folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "Lifestream";

/// %APPDATA%\Lifestream on Windows; ~/.config/Lifestream elsewhere.
pub fn data_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join(APP_NAME);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn engine_dir() -> PathBuf {
    let d = data_dir().join("engine");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn default_download_dir() -> String {
    dirs::download_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"))
        .join(APP_NAME)
        .display()
        .to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Category {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Feed {
    pub name: String,
    pub url: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RssRule {
    pub name: String,
    pub enabled: bool,
    pub must_contain: String,
    pub must_not_contain: String,
    pub episodes: String,
    pub use_regex: bool,
    pub category: String,
    pub skip_duplicates: bool,
    /// Empty = every feed.
    pub feeds: Vec<String>,
}

impl Default for RssRule {
    fn default() -> Self {
        Self {
            name: "New rule".into(),
            enabled: true,
            must_contain: String::new(),
            must_not_contain: String::new(),
            episodes: String::new(),
            use_regex: false,
            category: String::new(),
            skip_duplicates: true,
            feeds: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SeedAction {
    Pause,
    Remove,
    RemoveAndDelete,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SeederDisplay {
    Both,
    Connected,
    Swarm,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    // Mode
    pub advanced_mode: bool,
    /// "ff" (FFVII menu), "green" (Box of X) or "blades" (Blades).
    pub theme: String,

    // Folders
    pub download_dir: String,
    pub categories: Vec<Category>,
    pub incomplete_enabled: bool,
    pub incomplete_dir: String,
    /// The folder picked the last time a torrent was added.
    pub last_save_dir: String,

    // Adding
    pub start_paused: bool,
    pub default_sequential: bool,
    pub default_first_last: bool,

    // Speed (KB/s, 0 = unlimited)
    pub dl_limit_kb: i32,
    pub ul_limit_kb: i32,
    pub turtle_manual: bool,
    pub turtle_schedule: bool,
    pub turtle_dl_kb: i32,
    pub turtle_ul_kb: i32,
    pub turtle_from: String,
    pub turtle_to: String,
    /// Monday first.
    pub turtle_days: [bool; 7],
    pub max_peers_global: i32,
    pub max_peers_torrent: i32,
    pub upload_slots_torrent: i32,
    pub upload_slots_global: i32,
    pub active_downloads: i32,
    pub active_seeds: i32,

    // Network
    pub adapter: String,
    pub bind_only: bool,
    pub kill_switch: bool,
    pub disable_ipv6: bool,
    pub listen_port: u16,
    pub follow_vpn_port: bool,
    pub natpmp: bool,
    pub upnp: bool,

    // Privacy
    pub encryption: i32,
    pub anonymous_mode: bool,
    pub dht: bool,
    pub pex: bool,
    pub lsd: bool,
    pub private_honor_seed: bool,
    pub private_min_seed_hours: f32,
    pub private_real_name: bool,
    pub proxy_type: i32,
    pub proxy_host: String,
    pub proxy_port: i32,
    pub proxy_user: String,
    pub proxy_pass: String,
    pub proxy_peers: bool,
    pub proxy_trackers: bool,
    pub blocklist_url: String,
    pub blocklist_auto: bool,
    pub blocklist_last_update: i64,

    // Seeding
    pub ratio_limit: f32,
    pub seed_hours: f32,
    pub seed_action: SeedAction,
    pub seeder_display: SeederDisplay,
    pub health_bar: bool,

    // RSS
    pub rss_enabled: bool,
    pub rss_interval_min: u32,
    pub feeds: Vec<Feed>,
    pub rules: Vec<RssRule>,

    // App
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    /// Windows pop-up notifications for finished downloads and VPN changes.
    pub system_notifications: bool,
    /// No page transitions, click glows or sliding rows.
    pub reduce_motion: bool,
    pub prevent_sleep: bool,
    pub auto_backup: bool,
    pub backup_dir: String,
    pub backup_keep: u32,
    pub last_backup: i64,

    // Remote control (Transmission-compatible RPC, used by Turks)
    pub rpc_enabled: bool,
    pub rpc_port: u16,
    pub rpc_allow_lan: bool,
    pub rpc_username: String,
    pub rpc_password: String,
    /// Docker: install new versions by itself (checked every 6 hours).
    pub auto_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let dl = default_download_dir();
        Self {
            advanced_mode: false,
            theme: "ff".into(),
            download_dir: dl.clone(),
            categories: Vec::new(),
            incomplete_enabled: false,
            incomplete_dir: PathBuf::from(&dl).join("Incomplete").display().to_string(),
            last_save_dir: String::new(),
            start_paused: false,
            default_sequential: false,
            default_first_last: false,
            dl_limit_kb: 0,
            ul_limit_kb: 0,
            turtle_manual: false,
            turtle_schedule: false,
            turtle_dl_kb: 1500,
            turtle_ul_kb: 300,
            turtle_from: "18:00".into(),
            turtle_to: "23:30".into(),
            turtle_days: [true; 7],
            max_peers_global: 500,
            max_peers_torrent: 80,
            upload_slots_torrent: 8,
            upload_slots_global: 20,
            active_downloads: 3,
            active_seeds: 5,
            adapter: String::new(),
            bind_only: true,
            kill_switch: true,
            disable_ipv6: true,
            listen_port: rand::random_range(49160..65000),
            follow_vpn_port: true,
            natpmp: true,
            upnp: false,
            encryption: 1,
            anonymous_mode: true,
            dht: true,
            pex: true,
            lsd: false,
            private_honor_seed: true,
            private_min_seed_hours: 72.0,
            private_real_name: true,
            proxy_type: 0,
            proxy_host: String::new(),
            proxy_port: 1080,
            proxy_user: String::new(),
            proxy_pass: String::new(),
            proxy_peers: true,
            proxy_trackers: true,
            blocklist_url: String::new(),
            blocklist_auto: true,
            blocklist_last_update: 0,
            ratio_limit: 2.0,
            seed_hours: 0.0,
            seed_action: SeedAction::Pause,
            seeder_display: SeederDisplay::Both,
            health_bar: true,
            rss_enabled: true,
            rss_interval_min: 15,
            feeds: Vec::new(),
            rules: Vec::new(),
            start_with_windows: false,
            close_to_tray: true,
            system_notifications: true,
            reduce_motion: false,
            prevent_sleep: true,
            auto_backup: true,
            backup_dir: data_dir().join("backups").display().to_string(),
            backup_keep: 4,
            last_backup: 0,
            rpc_enabled: true,
            rpc_port: 9091,
            rpc_allow_lan: false,
            rpc_username: String::new(),
            rpc_password: String::new(),
            auto_update: true,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        data_dir().join("settings.json")
    }

    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => {
                let mut s: Self = serde_json::from_str(&text).unwrap_or_else(|e| {
                    crate::applog::warn(format!("Settings file couldn't be read ({e}); using defaults."));
                    Self::default()
                });
                // "Only use this adapter" and the kill switch are one switch now.
                s.bind_only = s.kill_switch;
                s
            }
            Err(_) => {
                let s = Self::default();
                s.save();
                s
            }
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        let tmp = path.with_extension("json.tmp");
        if let Ok(text) = serde_json::to_string_pretty(self) {
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Where a torrent in `category` ends up.
    pub fn path_for_category(&self, category: &str) -> String {
        self.categories
            .iter()
            .find(|c| c.name == category && !c.path.trim().is_empty())
            .map(|c| c.path.clone())
            .unwrap_or_else(|| self.download_dir.clone())
    }
}
