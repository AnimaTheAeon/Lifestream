#pragma once
#include <map>
#include <memory>
#include <set>
#include <string>

#include "rust/cxx.h"

#include <libtorrent/session.hpp>
#include <libtorrent/torrent_handle.hpp>

namespace ls {

struct LtSettings;
struct AddOptions;
struct TorrentInfo;
struct FileEntry;
struct TorrentPreview;
struct IpRange;
struct EngineTotals;

class Engine {
public:
  Engine(const LtSettings &s, bool enable_pex, rust::Str state_dir, bool start_paused);
  ~Engine();

  void apply_settings(const LtSettings &s);
  rust::String add_magnet(rust::Str uri, const AddOptions &opts);
  rust::String add_torrent_bytes(rust::Slice<const uint8_t> data, const AddOptions &opts);

  rust::Vec<TorrentInfo> torrents() const;
  rust::Vec<FileEntry> files(rust::Str hash) const;
  rust::Vec<rust::String> trackers(rust::Str hash) const;
  void add_tracker(rust::Str hash, rust::Str url);

  void set_file_priority(rust::Str hash, int32_t index, int32_t prio);
  void pause_torrent(rust::Str hash);
  void resume_torrent(rust::Str hash);
  void stop_torrent(rust::Str hash);
  void force_start(rust::Str hash);
  void remove_torrent(rust::Str hash, bool delete_files);
  void recheck(rust::Str hash);
  void set_sequential(rust::Str hash, bool on);
  void set_super_seeding(rust::Str hash, bool on);
  void set_first_last(rust::Str hash);
  void set_torrent_limits(rust::Str hash, int32_t max_conn, int32_t max_uploads);
  void move_storage(rust::Str hash, rust::Str path);
  void queue_top(rust::Str hash);
  void queue_set(rust::Str hash, int32_t pos);
  rust::String magnet_link(rust::Str hash) const;

  void pause_session();
  void resume_session();
  bool session_paused() const;

  EngineTotals totals() const;
  bool check_incoming() const;

  rust::Vec<rust::String> pop_alerts();
  void request_resume_save(bool all);
  void save_all_and_wait(int32_t wait_ms);
  void set_ip_filter(const rust::Vec<IpRange> &ranges);

private:
  lt::torrent_handle find(rust::Str hash) const;
  void register_handle(const lt::torrent_handle &h);
  void write_resume(const lt::add_torrent_params &p);
  void save_session_state();
  void save_queue_order();
  void apply_first_last(const lt::torrent_handle &h);
  std::string resume_path(const std::string &hash) const;

  std::unique_ptr<lt::session> ses_;
  std::map<std::string, lt::torrent_handle> handles_;
  std::set<std::string> first_last_pending_;
  std::string state_dir_;
  int pending_saves_ = 0;
  int listen_port_ = 0;
  int external_port_ = 0;
  std::string listen_error_;
};

std::unique_ptr<Engine> new_engine(const LtSettings &s, bool enable_pex, rust::Str state_dir, bool start_paused);
TorrentPreview parse_torrent(rust::Slice<const uint8_t> data);

} // namespace ls
