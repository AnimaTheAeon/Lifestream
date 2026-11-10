#include "lifestream/cpp/lt_bridge.h"
#include "lifestream/src/engine/bridge.rs.h"

#include <chrono>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <stdexcept>
#include <thread>
#include <vector>

#include <libtorrent/add_torrent_params.hpp>
#include <libtorrent/alert_types.hpp>
#include <libtorrent/announce_entry.hpp>
#include <libtorrent/extensions/smart_ban.hpp>
#include <libtorrent/extensions/ut_metadata.hpp>
#include <libtorrent/extensions/ut_pex.hpp>
#include <libtorrent/ip_filter.hpp>
#include <libtorrent/magnet_uri.hpp>
#include <libtorrent/peer_info.hpp>
#include <libtorrent/read_resume_data.hpp>
#include <libtorrent/session_params.hpp>
#include <libtorrent/settings_pack.hpp>
#include <libtorrent/torrent_info.hpp>
#include <libtorrent/torrent_status.hpp>
#include <libtorrent/write_resume_data.hpp>

namespace fs = std::filesystem;

namespace ls {

namespace {

std::string hex_of(const lt::torrent_handle &h) {
  std::ostringstream ss;
  ss << h.info_hashes().get_best();
  return ss.str();
}

std::string hex_of(const lt::info_hash_t &ih) {
  std::ostringstream ss;
  ss << ih.get_best();
  return ss.str();
}

lt::settings_pack make_pack(const LtSettings &s) {
  lt::settings_pack p;
  p.set_int(lt::settings_pack::alert_mask,
            lt::alert_category::error | lt::alert_category::status |
                lt::alert_category::storage | lt::alert_category::port_mapping |
                lt::alert_category::tracker);
  p.set_int(lt::settings_pack::download_rate_limit, s.download_limit);
  p.set_int(lt::settings_pack::upload_rate_limit, s.upload_limit);
  p.set_int(lt::settings_pack::connections_limit, s.connections_limit);
  p.set_int(lt::settings_pack::unchoke_slots_limit, s.unchoke_slots);
  p.set_int(lt::settings_pack::active_downloads, s.active_downloads);
  p.set_int(lt::settings_pack::active_seeds, s.active_seeds);
  p.set_int(lt::settings_pack::active_limit,
            std::max(s.active_limit, s.active_downloads + s.active_seeds));
  p.set_str(lt::settings_pack::listen_interfaces, std::string(s.listen_interfaces));
  p.set_str(lt::settings_pack::outgoing_interfaces, std::string(s.outgoing_interfaces));
  p.set_bool(lt::settings_pack::enable_dht, s.enable_dht);
  p.set_bool(lt::settings_pack::enable_lsd, s.enable_lsd);
  p.set_bool(lt::settings_pack::enable_upnp, s.enable_upnp);
  p.set_bool(lt::settings_pack::enable_natpmp, s.enable_natpmp);
  int policy = s.encryption == 2   ? lt::settings_pack::pe_forced
               : s.encryption == 1 ? lt::settings_pack::pe_enabled
                                   : lt::settings_pack::pe_disabled;
  p.set_int(lt::settings_pack::out_enc_policy, policy);
  p.set_int(lt::settings_pack::in_enc_policy, policy);
  p.set_int(lt::settings_pack::allowed_enc_level, lt::settings_pack::pe_both);
  p.set_bool(lt::settings_pack::prefer_rc4, s.encryption == 2);
  p.set_bool(lt::settings_pack::anonymous_mode, s.anonymous_mode);
  p.set_str(lt::settings_pack::user_agent, std::string(s.user_agent));
  p.set_str(lt::settings_pack::peer_fingerprint, std::string(s.peer_fingerprint));

  int ptype = lt::settings_pack::none;
  bool has_auth = !s.proxy_user.empty();
  if (s.proxy_type == 1)
    ptype = has_auth ? lt::settings_pack::socks5_pw : lt::settings_pack::socks5;
  else if (s.proxy_type == 2)
    ptype = has_auth ? lt::settings_pack::http_pw : lt::settings_pack::http;
  p.set_int(lt::settings_pack::proxy_type, ptype);
  p.set_str(lt::settings_pack::proxy_hostname, std::string(s.proxy_host));
  p.set_int(lt::settings_pack::proxy_port, s.proxy_port);
  p.set_str(lt::settings_pack::proxy_username, std::string(s.proxy_user));
  p.set_str(lt::settings_pack::proxy_password, std::string(s.proxy_pass));
  p.set_bool(lt::settings_pack::proxy_peer_connections, s.proxy_peers);
  p.set_bool(lt::settings_pack::proxy_tracker_connections, s.proxy_trackers);
  p.set_bool(lt::settings_pack::proxy_hostnames, true);
  return p;
}

void fill_add(lt::add_torrent_params &atp, const AddOptions &o) {
  atp.save_path = std::string(o.save_path);
  if (o.paused) {
    atp.flags |= lt::torrent_flags::paused;
    atp.flags &= ~lt::torrent_flags::auto_managed;
  } else {
    atp.flags &= ~lt::torrent_flags::paused;
    atp.flags |= lt::torrent_flags::auto_managed;
  }
  if (o.sequential) atp.flags |= lt::torrent_flags::sequential_download;
  if (o.seed_mode) atp.flags |= lt::torrent_flags::seed_mode;
  if (o.max_connections > 0) atp.max_connections = o.max_connections;
  if (o.max_uploads > 0) atp.max_uploads = o.max_uploads;
  if (o.use_priorities) {
    atp.file_priorities.clear();
    for (auto p : o.file_priorities) atp.file_priorities.push_back(lt::download_priority_t(p));
  }
}

rust::String rs(const std::string &s) { return rust::String::lossy(s); }

bool read_file(const fs::path &p, std::vector<char> &out) {
  std::ifstream f(p, std::ios::binary);
  if (!f) return false;
  out.assign(std::istreambuf_iterator<char>(f), std::istreambuf_iterator<char>());
  return true;
}

void write_file_atomic(const fs::path &p, const std::vector<char> &buf) {
  fs::path tmp = p;
  tmp += ".tmp";
  {
    std::ofstream f(tmp, std::ios::binary | std::ios::trunc);
    if (!f) return;
    f.write(buf.data(), (std::streamsize)buf.size());
  }
  std::error_code ec;
  fs::rename(tmp, p, ec);
  if (ec) {
    fs::remove(p, ec);
    fs::rename(tmp, p, ec);
  }
}

TorrentPreview preview_from(const lt::torrent_info &ti) {
  TorrentPreview pv;
  pv.name = rs(ti.name());
  pv.total_size = ti.total_size();
  pv.is_private = ti.priv();
  pv.hash = rs(hex_of(ti.info_hashes()));
  pv.comment = rs(ti.comment());
  pv.has_metadata = true;
  const auto &fst = ti.files();
  for (auto i : fst.file_range()) {
    if (fst.pad_file_at(i)) continue;
    FileEntry fe;
    fe.index = static_cast<int>(i);
    fe.path = rs(fst.file_path(i));
    fe.size = fst.file_size(i);
    fe.done = 0;
    fe.priority = 4;
    pv.files.push_back(std::move(fe));
  }
  return pv;
}

} // namespace

Engine::Engine(const LtSettings &s, bool enable_pex, rust::Str state_dir, bool start_paused)
    : state_dir_(std::string(state_dir)) {
  std::error_code fec;
  fs::create_directories(fs::path(state_dir_) / "resume", fec);

  lt::session_params params(make_pack(s), std::vector<std::shared_ptr<lt::plugin>>{});
  std::vector<char> st;
  if (read_file(fs::path(state_dir_) / "session.state", st) && !st.empty()) {
    try {
      lt::session_params old =
          lt::read_session_params(st, lt::session_handle::save_dht_state);
      params.dht_state = std::move(old.dht_state);
    } catch (...) {
    }
  }
  ses_ = std::make_unique<lt::session>(
      std::move(params), start_paused ? lt::session_handle::paused : lt::session_flags_t{});
  ses_->add_extension(&lt::create_ut_metadata_plugin);
  ses_->add_extension(&lt::create_smart_ban_plugin);
  if (enable_pex) ses_->add_extension(&lt::create_ut_pex_plugin);

  // Reload every torrent from its resume file.
  for (auto &entry : fs::directory_iterator(fs::path(state_dir_) / "resume", fec)) {
    if (entry.path().extension() != ".fastresume") continue;
    std::vector<char> buf;
    if (!read_file(entry.path(), buf)) continue;
    lt::error_code ec;
    lt::add_torrent_params atp = lt::read_resume_data(buf, ec);
    if (ec) continue;
    lt::torrent_handle h = ses_->add_torrent(std::move(atp), ec);
    if (!ec) register_handle(h);
  }
}

Engine::~Engine() {
  try {
    save_all_and_wait(4000);
  } catch (...) {
  }
}

void Engine::register_handle(const lt::torrent_handle &h) { handles_[hex_of(h)] = h; }

std::string Engine::resume_path(const std::string &hash) const {
  return (fs::path(state_dir_) / "resume" / (hash + ".fastresume")).string();
}

lt::torrent_handle Engine::find(rust::Str hash) const {
  auto it = handles_.find(std::string(hash));
  if (it == handles_.end()) return lt::torrent_handle();
  return it->second;
}

void Engine::apply_settings(const LtSettings &s) { ses_->apply_settings(make_pack(s)); }

rust::String Engine::add_magnet(rust::Str uri, const AddOptions &opts) {
  lt::error_code ec;
  lt::add_torrent_params atp = lt::parse_magnet_uri(std::string(uri), ec);
  if (ec) throw std::runtime_error("That magnet link doesn't look right: " + ec.message());
  std::string hash = hex_of(atp.info_hashes);
  if (handles_.count(hash)) throw std::runtime_error("That torrent is already in your list.");
  fill_add(atp, opts);
  lt::torrent_handle h = ses_->add_torrent(std::move(atp), ec);
  if (ec) throw std::runtime_error(ec.message());
  register_handle(h);
  if (opts.first_last) first_last_pending_.insert(hash);
  h.save_resume_data(lt::torrent_handle::save_info_dict);
  pending_saves_++;
  return rs(hash);
}

rust::String Engine::add_torrent_bytes(rust::Slice<const uint8_t> data, const AddOptions &opts) {
  lt::error_code ec;
  auto ti = std::make_shared<lt::torrent_info>(
      lt::span<char const>(reinterpret_cast<const char *>(data.data()), (std::ptrdiff_t)data.size()),
      ec, lt::from_span);
  if (ec) throw std::runtime_error("That .torrent file couldn't be read: " + ec.message());
  std::string hash = hex_of(ti->info_hashes());
  if (handles_.count(hash)) throw std::runtime_error("That torrent is already in your list.");
  lt::add_torrent_params atp;
  atp.ti = ti;
  fill_add(atp, opts);
  lt::torrent_handle h = ses_->add_torrent(std::move(atp), ec);
  if (ec) throw std::runtime_error(ec.message());
  register_handle(h);
  if (opts.first_last) apply_first_last(h);
  h.save_resume_data(lt::torrent_handle::save_info_dict);
  pending_saves_++;
  return rs(hash);
}

rust::Vec<TorrentInfo> Engine::torrents() const {
  rust::Vec<TorrentInfo> out;
  for (const auto &kv : handles_) {
    const auto &h = kv.second;
    if (!h.is_valid()) continue;
    lt::torrent_status s = h.status();
    TorrentInfo t;
    t.hash = rs(kv.first);
    t.name = rs(s.name.empty() ? kv.first : s.name);
    t.save_path = rs(s.save_path);
    switch (s.state) {
    case lt::torrent_status::checking_files:
    case lt::torrent_status::checking_resume_data: t.state = 0; break;
    case lt::torrent_status::downloading_metadata: t.state = 1; break;
    case lt::torrent_status::downloading: t.state = 2; break;
    case lt::torrent_status::finished: t.state = 3; break;
    case lt::torrent_status::seeding: t.state = 4; break;
    default: t.state = 2;
    }
    t.paused = bool(s.flags & lt::torrent_flags::paused);
    t.auto_managed = bool(s.flags & lt::torrent_flags::auto_managed);
    t.sequential = bool(s.flags & lt::torrent_flags::sequential_download);
    t.super_seeding = bool(s.flags & lt::torrent_flags::super_seeding);
    t.progress = s.progress;
    t.total_wanted = s.total_wanted;
    t.total_done = s.total_wanted_done;
    t.dl_rate = s.download_payload_rate;
    t.ul_rate = s.upload_payload_rate;
    t.num_seeds = s.num_seeds;
    t.num_peers = s.num_peers - s.num_seeds;
    t.swarm_seeds = std::max(s.num_complete, s.list_seeds);
    t.swarm_peers = std::max(s.num_incomplete, s.list_peers - s.list_seeds);
    t.all_time_upload = s.all_time_upload;
    t.all_time_download = s.all_time_download;
    t.seeding_secs = (int64_t)s.seeding_duration.count();
    t.active_secs = (int64_t)s.active_duration.count();
    t.added_time = (int64_t)s.added_time;
    t.completed_time = (int64_t)s.completed_time;
    t.distributed_copies = s.distributed_copies;
    t.has_metadata = s.has_metadata;
    t.queue_position = static_cast<int>(s.queue_position);
    t.error = rs(s.errc ? s.errc.message() : std::string());
    t.is_private = false;
    t.num_files = 0;
    if (s.has_metadata) {
      auto ti = h.torrent_file();
      if (ti) {
        t.is_private = ti->priv();
        t.num_files = ti->num_files();
      }
    }
    t.moving = s.moving_storage;
    out.push_back(std::move(t));
  }
  return out;
}

rust::Vec<FileEntry> Engine::files(rust::Str hash) const {
  rust::Vec<FileEntry> out;
  auto h = find(hash);
  if (!h.is_valid()) return out;
  auto ti = h.torrent_file();
  if (!ti) return out;
  std::vector<std::int64_t> progress = h.file_progress(lt::torrent_handle::piece_granularity);
  auto prios = h.get_file_priorities();
  const auto &fst = ti->files();
  for (auto i : fst.file_range()) {
    if (fst.pad_file_at(i)) continue;
    int idx = static_cast<int>(i);
    FileEntry fe;
    fe.index = idx;
    fe.path = rs(fst.file_path(i));
    fe.size = fst.file_size(i);
    fe.done = idx < (int)progress.size() ? progress[idx] : 0;
    fe.priority = idx < (int)prios.size() ? static_cast<int>(static_cast<std::uint8_t>(prios[idx])) : 4;
    out.push_back(std::move(fe));
  }
  return out;
}

rust::Vec<rust::String> Engine::trackers(rust::Str hash) const {
  rust::Vec<rust::String> out;
  auto h = find(hash);
  if (!h.is_valid()) return out;
  for (const auto &t : h.trackers()) out.push_back(rs(t.url));
  return out;
}

void Engine::add_tracker(rust::Str hash, rust::Str url) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  lt::announce_entry ae{std::string(url)};
  h.add_tracker(ae);
}

void Engine::set_file_priority(rust::Str hash, int32_t index, int32_t prio) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  h.file_priority(lt::file_index_t(index), lt::download_priority_t(std::uint8_t(prio)));
}

void Engine::pause_torrent(rust::Str hash) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  h.unset_flags(lt::torrent_flags::auto_managed);
  h.pause(lt::torrent_handle::graceful_pause);
}

void Engine::resume_torrent(rust::Str hash) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  h.set_flags(lt::torrent_flags::auto_managed);
  h.resume();
}

void Engine::remove_torrent(rust::Str hash, bool delete_files) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  std::string key(hash);
  if (delete_files)
    ses_->remove_torrent(h, lt::session_handle::delete_files);
  else
    ses_->remove_torrent(h);
  handles_.erase(key);
  first_last_pending_.erase(key);
  std::error_code ec;
  fs::remove(resume_path(key), ec);
}

void Engine::recheck(rust::Str hash) {
  auto h = find(hash);
  if (h.is_valid()) h.force_recheck();
}

void Engine::set_sequential(rust::Str hash, bool on) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  if (on)
    h.set_flags(lt::torrent_flags::sequential_download);
  else
    h.unset_flags(lt::torrent_flags::sequential_download);
}

void Engine::set_super_seeding(rust::Str hash, bool on) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  if (on)
    h.set_flags(lt::torrent_flags::super_seeding);
  else
    h.unset_flags(lt::torrent_flags::super_seeding);
}

void Engine::apply_first_last(const lt::torrent_handle &h) {
  auto ti = h.torrent_file();
  if (!ti) return;
  const auto &fst = ti->files();
  for (auto i : fst.file_range()) {
    std::int64_t size = fst.file_size(i);
    if (size <= 0) continue;
    auto first = fst.map_file(i, 0, 1).piece;
    auto last = fst.map_file(i, size - 1, 1).piece;
    h.piece_priority(first, lt::top_priority);
    h.piece_priority(last, lt::top_priority);
  }
}

void Engine::set_first_last(rust::Str hash) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  if (h.torrent_file())
    apply_first_last(h);
  else
    first_last_pending_.insert(std::string(hash));
}

void Engine::set_torrent_limits(rust::Str hash, int32_t max_conn, int32_t max_uploads) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  h.set_max_connections(max_conn > 0 ? max_conn : -1);
  h.set_max_uploads(max_uploads > 0 ? max_uploads : -1);
}

void Engine::move_storage(rust::Str hash, rust::Str path) {
  auto h = find(hash);
  if (!h.is_valid()) return;
  std::error_code ec;
  fs::create_directories(std::string(path), ec);
  h.move_storage(std::string(path));
}

void Engine::queue_top(rust::Str hash) {
  auto h = find(hash);
  if (h.is_valid()) h.queue_position_top();
}

rust::String Engine::magnet_link(rust::Str hash) const {
  auto h = find(hash);
  if (!h.is_valid()) return rust::String();
  return rs(lt::make_magnet_uri(h));
}

void Engine::pause_session() { ses_->pause(); }
void Engine::resume_session() { ses_->resume(); }
bool Engine::session_paused() const { return ses_->is_paused(); }

EngineTotals Engine::totals() const {
  EngineTotals t;
  t.listen_port = listen_port_;
  t.external_port = external_port_;
  t.listen_error = rs(listen_error_);
  t.dht_nodes = 0;
  return t;
}

bool Engine::check_incoming() const {
  for (const auto &kv : handles_) {
    if (!kv.second.is_valid()) continue;
    std::vector<lt::peer_info> peers;
    kv.second.get_peer_info(peers);
    for (const auto &p : peers)
      if (!(p.flags & lt::peer_info::local_connection)) return true;
  }
  return false;
}

void Engine::write_resume(const lt::add_torrent_params &p) {
  std::vector<char> buf = lt::write_resume_data_buf(p);
  std::string hash = hex_of(p.info_hashes);
  if (!handles_.count(hash)) return; // removed meanwhile
  write_file_atomic(resume_path(hash), buf);
}

void Engine::save_session_state() {
  std::vector<char> buf =
      lt::write_session_params_buf(ses_->session_state(lt::session_handle::save_dht_state),
                                   lt::session_handle::save_dht_state);
  write_file_atomic(fs::path(state_dir_) / "session.state", buf);
}

rust::Vec<rust::String> Engine::pop_alerts() {
  rust::Vec<rust::String> out;
  std::vector<lt::alert *> alerts;
  ses_->pop_alerts(&alerts);
  for (lt::alert *a : alerts) {
    if (auto *r = lt::alert_cast<lt::save_resume_data_alert>(a)) {
      write_resume(r->params);
      if (pending_saves_ > 0) pending_saves_--;
      continue;
    }
    if (lt::alert_cast<lt::save_resume_data_failed_alert>(a)) {
      if (pending_saves_ > 0) pending_saves_--;
      continue;
    }
    if (auto *m = lt::alert_cast<lt::metadata_received_alert>(a)) {
      std::string hash = hex_of(m->handle);
      if (first_last_pending_.count(hash)) {
        apply_first_last(m->handle);
        first_last_pending_.erase(hash);
      }
      m->handle.save_resume_data(lt::torrent_handle::save_info_dict);
      pending_saves_++;
      out.push_back(rs("METADATA|" + hash + "|" + m->message()));
      continue;
    }
    if (auto *f = lt::alert_cast<lt::torrent_finished_alert>(a)) {
      std::string hash = hex_of(f->handle);
      f->handle.save_resume_data(lt::torrent_handle::save_info_dict);
      pending_saves_++;
      out.push_back(rs("FINISHED|" + hash + "|" + f->message()));
      continue;
    }
    if (auto *ls = lt::alert_cast<lt::listen_succeeded_alert>(a)) {
      if (ls->socket_type == lt::socket_type_t::tcp) {
        listen_port_ = ls->port;
        listen_error_.clear();
      }
      out.push_back(rs("LOG||" + a->message()));
      continue;
    }
    if (auto *lf = lt::alert_cast<lt::listen_failed_alert>(a)) {
      listen_error_ = lf->message();
      out.push_back(rs("ERROR||" + a->message()));
      continue;
    }
    if (auto *pm = lt::alert_cast<lt::portmap_alert>(a)) {
      external_port_ = pm->external_port;
      out.push_back(rs("PORTMAP||" + std::to_string(pm->external_port) + "|" + a->message()));
      continue;
    }
    if (auto *pe = lt::alert_cast<lt::portmap_error_alert>(a)) {
      out.push_back(rs("LOG||" + pe->message()));
      continue;
    }
    if (auto *mv = lt::alert_cast<lt::storage_moved_alert>(a)) {
      mv->handle.save_resume_data(lt::torrent_handle::save_info_dict);
      pending_saves_++;
      out.push_back(rs("MOVED|" + hex_of(mv->handle) + "|" + a->message()));
      continue;
    }
    if (auto *te = lt::alert_cast<lt::torrent_error_alert>(a)) {
      out.push_back(rs("ERROR|" + hex_of(te->handle) + "|" + a->message()));
      continue;
    }
    if (a->category() & lt::alert_category::error) {
      // Tracker hiccups are common and noisy; keep them as plain log lines.
      out.push_back(rs("LOG||" + a->message()));
      continue;
    }
    if (lt::alert_cast<lt::torrent_paused_alert>(a) || lt::alert_cast<lt::torrent_resumed_alert>(a) ||
        lt::alert_cast<lt::torrent_checked_alert>(a)) {
      out.push_back(rs("LOG||" + a->message()));
    }
  }
  return out;
}

void Engine::request_resume_save(bool all) {
  for (const auto &kv : handles_) {
    if (!kv.second.is_valid()) continue;
    if (!all && !kv.second.need_save_resume_data()) continue;
    kv.second.save_resume_data(lt::torrent_handle::save_info_dict);
    pending_saves_++;
  }
  save_session_state();
}

void Engine::save_all_and_wait(int32_t wait_ms) {
  ses_->pause();
  request_resume_save(true);
  auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(wait_ms);
  while (pending_saves_ > 0 && std::chrono::steady_clock::now() < deadline) {
    ses_->wait_for_alert(std::chrono::milliseconds(200));
    pop_alerts();
  }
}

void Engine::set_ip_filter(const rust::Vec<IpRange> &ranges) {
  lt::ip_filter f;
  for (const auto &r : ranges) {
    lt::error_code e1, e2;
    auto a = lt::make_address(std::string(r.first), e1);
    auto b = lt::make_address(std::string(r.last), e2);
    if (e1 || e2 || a.is_v4() != b.is_v4()) continue;
    try {
      f.add_rule(a, b, lt::ip_filter::blocked);
    } catch (...) {
    }
  }
  ses_->set_ip_filter(f);
}

std::unique_ptr<Engine> new_engine(const LtSettings &s, bool enable_pex, rust::Str state_dir,
                                   bool start_paused) {
  return std::make_unique<Engine>(s, enable_pex, state_dir, start_paused);
}

TorrentPreview parse_torrent(rust::Slice<const uint8_t> data) {
  lt::error_code ec;
  lt::torrent_info ti(
      lt::span<char const>(reinterpret_cast<const char *>(data.data()), (std::ptrdiff_t)data.size()),
      ec, lt::from_span);
  if (ec) throw std::runtime_error("That .torrent file couldn't be read: " + ec.message());
  return preview_from(ti);
}

} // namespace ls
