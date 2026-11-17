# Lifestream for Edge

Send magnet links to Lifestream from Microsoft Edge (also works in Chrome),
or TV shows through Midgar so they get filed into your TV folders.

- **Click a magnet link** on any page and it goes straight to Lifestream
  (can be turned off in the settings).
- **Right-click any magnet or .torrent link** → Send to Lifestream, Send to
  Lifestream, paused, or Send to Midgar (TV shows).
- **TV shows through Midgar** (optional): with "TV shows to Midgar" picked in
  the settings, names like `Show.S08`, `S08E03` or `Season 8` go to Midgar
  instead. Midgar checks that it wants them, sends them to Lifestream itself,
  and files the episodes into the show's folder when they finish. If Midgar
  turns one down, the notice says why in plain words (with Midgar's own
  wording underneath) and offers **Send to Lifestream anyway**.
- **Toolbar panel**: Lifestream's status, a box to paste a magnet, and your
  recent sends with Retry.
- A small notice in the corner of the page says whether it was sent, was
  already in Lifestream, or couldn't be sent.
- Three looks: FFVII menu, Xbox and Blades.

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
   click **Test Midgar**, and pick **TV shows to Midgar, everything else to
   Lifestream**. The show must already be added in Midgar.

To update, replace the folder's files and click **Reload** on the extension in
`edge://extensions`.

It talks to Lifestream's Transmission-style remote control (`/transmission/rpc`),
so Lifestream needs a username and password set for other devices to connect.
The login is kept in Edge's extension storage on that PC.

Midgar is reached through its API (`/api/v3/release/push`, with the API key).
Midgar sends the download to whichever download client it uses, so set
Lifestream up as Midgar's download client first.
