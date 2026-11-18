# Lifestream for Edge

Send magnet links to Lifestream from Microsoft Edge (also works in Chrome),
or TV shows through Midgar and movies through Turks, so they get filed into
your TV and Movies folders.

- **Click a magnet link** on any page and it goes where **Where links go**
  says (can be turned off in the settings).
- **Right-click any magnet or .torrent link** → Send to Lifestream, Send to
  Lifestream, paused, Send to Midgar (TV shows) or Send to Turks (movies).
- **Where links go** (settings): always Lifestream; automatic (names like
  `Show.S08`, `S08E03` or `Season 8` go to Midgar, names like
  `Movie.2019.1080p.BluRay` go to Turks, everything else to Lifestream); or
  ask each time (buttons for Midgar, Turks and Lifestream appear in the
  corner of the page, with the likely one marked, and nothing is sent until
  you pick). Midgar and Turks check that they want the link, send it to
  Lifestream themselves, and file it into the show's or movie's folder when
  it finishes. If one turns a link down, the notice says why in plain words
  (with the app's own wording underneath) and offers **Send to Lifestream
  anyway**.
- **Toolbar panel**: Lifestream's status, a box to paste a magnet with a
  Send to choice (Automatic, Midgar, Turks, Lifestream), and your recent
  sends with Retry.
- A small notice in the corner of the page says whether it was sent, was
  already in Lifestream, or couldn't be sent.
- Four looks: FFVII menu, Xbox, Blades and Dark Glass.

## Install

1. Copy this `edge-extension` folder somewhere you'll keep it. Edge loads it
   from there.
2. Open `edge://extensions`, turn on **Developer mode**, click **Load unpacked**
   and pick the folder.
3. In the settings page that opens, enter Lifestream's address (for example
   `http://192.168.1.127:9091`) and its remote-control username and password,
   then click **Test connection**.
4. For TV shows through Midgar: enter Midgar's address (for example
   `http://192.168.1.127:8990`) and its API key (Midgar › Settings › General),
   click **Test Midgar**. Do the same for Turks (for example
   `http://192.168.1.127:7878`, API key from Turks › Settings › General;
   Turks 0.5.12 or newer), then pick **Automatic** or **Ask me each time**
   under Where links go. The show or movie must already be added in Midgar
   or Turks.

To update, replace the folder's files and click **Reload** on the extension in
`edge://extensions`.

It talks to Lifestream's Transmission-style remote control (`/transmission/rpc`),
so Lifestream needs a username and password set for other devices to connect.
The login is kept in Edge's extension storage on that PC.

Midgar and Turks are reached through their APIs (`/api/v3/release/push`,
with the API key). They send the download to whichever download client they
use, so set Lifestream up as their download client first.
