# Lifestream for Edge

Send magnet links to Lifestream from Microsoft Edge (also works in Chrome).

- **Click a magnet link** on any page and it goes straight to Lifestream
  (can be turned off in the settings).
- **Right-click any magnet or .torrent link** → Send to Lifestream, or Send to
  Lifestream, paused.
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

To update, replace the folder's files and click **Reload** on the extension in
`edge://extensions`.

It talks to Lifestream's Transmission-style remote control (`/transmission/rpc`),
so Lifestream needs a username and password set for other devices to connect.
The login is kept in Edge's extension storage on that PC.
