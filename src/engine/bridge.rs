//! Bridge to the libtorrent session (see cpp/lt_bridge.cpp).

#[cxx::bridge(namespace = "ls")]
pub mod ffi {
    #[derive(Clone, Debug, Default)]
    pub struct LtSettings {
        pub download_limit: i32,
        pub upload_limit: i32,
        pub connections_limit: i32,
        pub unchoke_slots: i32,
        pub active_downloads: i32,
        pub active_seeds: i32,
        pub active_limit: i32,
        pub listen_interfaces: String,
        pub outgoing_interfaces: String,
        pub enable_dht: bool,
        pub enable_lsd: bool,
        pub enable_upnp: bool,
        pub enable_natpmp: bool,
        /// 0 allow plain only, 1 prefer, 2 require
        pub encryption: i32,
        pub anonymous_mode: bool,
        pub user_agent: String,
        pub peer_fingerprint: String,
        /// 0 none, 1 socks5, 2 http
        pub proxy_type: i32,
        pub proxy_host: String,
        pub proxy_port: i32,
        pub proxy_user: String,
        pub proxy_pass: String,
        pub proxy_peers: bool,
        pub proxy_trackers: bool,
    }

    #[derive(Clone, Debug, Default)]
    pub struct AddOptions {
        pub save_path: String,
        pub paused: bool,
        pub sequential: bool,
        pub first_last: bool,
        pub seed_mode: bool,
        pub max_connections: i32,
        pub max_uploads: i32,
        pub use_priorities: bool,
        pub file_priorities: Vec<u8>,
    }

    #[derive(Clone, Debug, Default)]
    pub struct TorrentInfo {
        pub hash: String,
        pub name: String,
        pub save_path: String,
        /// 0 checking, 1 metadata, 2 downloading, 3 finished, 4 seeding
        pub state: i32,
        pub paused: bool,
        pub auto_managed: bool,
        pub sequential: bool,
        pub super_seeding: bool,
        pub progress: f32,
        pub total_wanted: i64,
        pub total_done: i64,
        pub dl_rate: i32,
        pub ul_rate: i32,
        pub num_seeds: i32,
        pub num_peers: i32,
        pub swarm_seeds: i32,
        pub swarm_peers: i32,
        pub all_time_upload: i64,
        pub all_time_download: i64,
        pub seeding_secs: i64,
        pub active_secs: i64,
        pub added_time: i64,
        pub completed_time: i64,
        pub distributed_copies: f32,
        pub has_metadata: bool,
        pub queue_position: i32,
        pub error: String,
        pub is_private: bool,
        pub num_files: i32,
        pub moving: bool,
    }

    #[derive(Clone, Debug, Default)]
    pub struct FileEntry {
        pub index: i32,
        pub path: String,
        pub size: i64,
        pub done: i64,
        pub priority: i32,
    }

    #[derive(Clone, Debug, Default)]
    pub struct TorrentPreview {
        pub name: String,
        pub total_size: i64,
        pub is_private: bool,
        pub hash: String,
        pub comment: String,
        pub has_metadata: bool,
        pub files: Vec<FileEntry>,
    }

    #[derive(Clone, Debug, Default)]
    pub struct IpRange {
        pub first: String,
        pub last: String,
    }

    #[derive(Clone, Debug, Default)]
    pub struct EngineTotals {
        pub listen_port: i32,
        pub external_port: i32,
        pub listen_error: String,
        pub dht_nodes: i32,
    }

    unsafe extern "C++" {
        include!("lifestream/cpp/lt_bridge.h");

        type Engine;

        fn new_engine(
            s: &LtSettings,
            enable_pex: bool,
            state_dir: &str,
            start_paused: bool,
        ) -> Result<UniquePtr<Engine>>;
        fn parse_torrent(data: &[u8]) -> Result<TorrentPreview>;

        fn apply_settings(self: Pin<&mut Engine>, s: &LtSettings);
        fn add_magnet(self: Pin<&mut Engine>, uri: &str, opts: &AddOptions) -> Result<String>;
        fn add_torrent_bytes(self: Pin<&mut Engine>, data: &[u8], opts: &AddOptions) -> Result<String>;
        fn torrents(self: &Engine) -> Vec<TorrentInfo>;
        fn files(self: &Engine, hash: &str) -> Vec<FileEntry>;
        fn trackers(self: &Engine, hash: &str) -> Vec<String>;
        fn add_tracker(self: Pin<&mut Engine>, hash: &str, url: &str);
        fn set_file_priority(self: Pin<&mut Engine>, hash: &str, index: i32, prio: i32);
        fn pause_torrent(self: Pin<&mut Engine>, hash: &str);
        fn resume_torrent(self: Pin<&mut Engine>, hash: &str);
        fn remove_torrent(self: Pin<&mut Engine>, hash: &str, delete_files: bool);
        fn recheck(self: Pin<&mut Engine>, hash: &str);
        fn set_sequential(self: Pin<&mut Engine>, hash: &str, on: bool);
        fn set_super_seeding(self: Pin<&mut Engine>, hash: &str, on: bool);
        fn set_first_last(self: Pin<&mut Engine>, hash: &str);
        fn set_torrent_limits(self: Pin<&mut Engine>, hash: &str, max_conn: i32, max_uploads: i32);
        fn move_storage(self: Pin<&mut Engine>, hash: &str, path: &str);
        fn queue_top(self: Pin<&mut Engine>, hash: &str);
        fn magnet_link(self: &Engine, hash: &str) -> String;
        fn pause_session(self: Pin<&mut Engine>);
        fn resume_session(self: Pin<&mut Engine>);
        fn session_paused(self: &Engine) -> bool;
        fn totals(self: &Engine) -> EngineTotals;
        fn check_incoming(self: &Engine) -> bool;
        fn pop_alerts(self: Pin<&mut Engine>) -> Vec<String>;
        fn request_resume_save(self: Pin<&mut Engine>, all: bool);
        fn save_all_and_wait(self: Pin<&mut Engine>, wait_ms: i32);
        fn set_ip_filter(self: Pin<&mut Engine>, ranges: &Vec<IpRange>);
    }
}

// The engine is only ever touched from the core worker thread, and
// libtorrent's session is itself thread safe.
unsafe impl Send for ffi::Engine {}
