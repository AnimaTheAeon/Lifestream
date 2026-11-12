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
  by default), so [Turks](https://github.com/AnimaTheAeon/Turks),
  [Midgar](https://github.com/AnimaTheAeon/Midgar) and
  Radarr/Sonarr/Prowlarr can use Lifestream as their download client
  (Settings → Remote control). In those apps pick **Transmission**, port 9091,
  URL base `/transmission/`. Other devices (like a NAS) need a username and
  password set.

**App**
- Simple / Advanced mode: Simple shows what most people need
- Every setting has a Shinra-logo help icon: hover it to read what it does
- Tray icon with quick toggles: pause all, turtle, RSS, kill switch status,
  add magnet from clipboard
- Opens magnet links and .torrent files (Settings → Make default)
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
Lifestream updates itself: it checks for a new release each time it opens.

- **Lifestream-x.y.z-x86_64.spk**: Synology version (DSM 7.2.1+, x86_64
  models such as the DS923+). It runs Lifestream in Container Manager inside
  **Barrier**'s network (the VPN container `nas-vpn`), so every byte goes
  through the VPN, and Lifestream can't run at all while Barrier is stopped.
  1. In Barrier's compose file, add `"9091:9091"` under `ports:` and rebuild
     Barrier (Lifestream's page is published through Barrier).
  2. Package Center → **Manual Install** → the `.spk`. It installs Container
     Manager if needed and builds the project **lifestream** (a minute or two).
  3. Open **Lifestream** from the DSM main menu (or port 9091). The first
     visit asks for a username and password.

  Shared folders are at their DSM paths inside it (`/volume1/Downloads/...`),
  so Midgar and Turks see the same paths. Lifestream updates itself (Settings
  → Updates). If you rebuild or recreate Barrier, then in Container Manager
  choose Project → lifestream → Action → **Build** so Lifestream rejoins it.
  Coming from the hand-made Container Manager project (`docker/`): delete that
  project first (keep its folder). The package brings over its settings and
  torrents from `/volume1/docker/lifestream/config` on first start.

The desktop app serves the same web page on its remote-control port, so you
can check on downloads from your phone too.

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

Headless (Synology) build: `cargo build --release --no-default-features`
(no GUI libraries needed); `synology/build-spk.sh` packages it. CI builds it on
Ubuntu 22.04 with libtorrent bundled.
