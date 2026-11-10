# Lifestream

A torrent client for Windows with a classic JRPG party-menu look, built for
using torrents through a VPN. Sibling app to
[Jenova](https://github.com/AnimaTheAeon/Jenova).

Built on [libtorrent](https://www.libtorrent.org/) (the engine behind
qBittorrent and Deluge) with a Rust + egui interface.

## Features

**Torrents**
- Magnet links and .torrent files; drag and drop onto the window
- Asks where to save each download and remembers the last folder you picked
- Click a torrent's square icon for its files: skip files, set priority
- Seeders vs. leechers shown as connected (whole swarm), plus a swarm health bar
- Download in order, first-and-last-pieces-first, super-seeding, recheck, queue
- Categories with their own folders (point TV/Movies at Jenova's watch folders)

**VPN and privacy**
- Bind to a network adapter: torrent traffic can't fall back to your normal connection
- Kill switch, on by default: if the adapter drops, everything pauses (and
  moves to loopback) until it's back; also holds downloads if Lifestream
  starts before your VPN
- Port forwarding from your VPN (NAT-PMP, e.g. ProtonVPN), incoming-peer check, leak check
- Encryption (allow / prefer / require), anonymous mode, IPv6 off
- Private tracker mode: DHT/PEX/LSD off for private torrents, minimum seed
  time honored, real client name reported where trackers require it
- SOCKS5/HTTP proxy, IP blocklists (P2P/DAT, gzip) with weekly updates

**Automation**
- RSS feeds with rules: must / must-not contain, episode ranges
  (`S02E05-`), regex, skip duplicates, per-rule category
- Turtle mode (alternate limits) on demand or on a schedule
- Stop seeding at a ratio or time, then pause / remove / remove + delete
- Remote control that speaks Transmission's RPC (port 9091, this PC only
  by default), so [Turks](https://github.com/AnimaTheAeon/Turks) and
  Radarr/Sonarr/Prowlarr can use Lifestream as their download client
  (Config → Remote control)

**App**
- Simple / Advanced mode: Simple shows what most people need
- Every setting has a Shinra-logo help icon: hover it to read what it does
- Tray icon with quick toggles: pause all, turtle, RSS, kill switch status,
  add magnet from clipboard
- Opens magnet links and .torrent files (Config → Make default)
- Start with Windows, close to tray, prevent sleep while downloading
- Backup and restore of settings, RSS rules and the torrent list (with
  progress), plus automatic weekly backups

Data and logs live in `%APPDATA%\Lifestream`.

## Download

Each [release](https://github.com/AnimaTheAeon/Lifestream/releases) has two files:

- **Lifestream-Setup-x.y.z.exe**: installer. Adds Start menu and optional
  desktop shortcuts, can set Lifestream as the app for magnet links and
  .torrent files, and shows up in Windows' installed apps for uninstalling.
  No admin rights needed.
- **lifestream.exe**: portable. Run it from anywhere; nothing is installed.

Both keep settings in the same place, so you can switch between them.

## Building

Windows builds run on GitHub Actions (`.github/workflows/build-windows.yml`):
vcpkg builds libtorrent, `cargo build --release` makes `lifestream.exe`, and
Inno Setup (`installer/lifestream.iss`) wraps it into the installer.
Bumping `version` in `Cargo.toml` and pushing to `main` publishes a release.

Locally on Windows:

```powershell
vcpkg install libtorrent:x64-windows-static-md
$env:VCPKG_ROOT = "C:\path\to\vcpkg"
cargo build --release
```

On Linux: `sudo apt install libtorrent-rasterbar-dev libgtk-3-dev` then `cargo build`.
