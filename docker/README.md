# Lifestream in Docker, through NAS VPN

Lifestream runs inside NAS VPN's network, so it can only reach the internet
through ExpressVPN. If the VPN drops, NAS VPN's kill switch cuts it off, and
Lifestream's own kill switch (locked to the tunnel, `tun0`) pauses everything.

## Before you start
- NAS VPN is running and connected, and its **kill switch is on** (NAS VPN › Settings).
- Uninstall the Lifestream package in Package Center if it's there (it would
  fight over port 9091).

## Step 1. Open port 9091 on NAS VPN
Lifestream's web page has to be published by NAS VPN, because Lifestream uses
its network. In `/volume1/docker/nas-vpn/docker-compose.yml`, add one line
under `ports:`

```yaml
    ports:
      - "8085:8085"
      - "9091:9091"     # Lifestream
```

Then in **Container Manager › Project › nas-vpn › Action › Build** (or Stop
then Start) so the new port takes effect.

## Step 2. Put this folder on the NAS
Upload this `lifestream` folder to your `docker` shared folder, so you have
`/volume1/docker/lifestream/docker-compose.yml`.

If your downloads live somewhere other than `/volume1/downloads`, change that
line in `docker-compose.yml` first.

## Step 3. Create the project
1. **Container Manager › Project › Create**
2. Name `lifestream`, path `/volume1/docker/lifestream`
3. **Use existing docker-compose.yml** › Next › Next › Done.
   The first start takes a minute or two (it downloads the newest Lifestream).

## Step 4. Check it
1. Open `http://<your-NAS-IP>:9091`, choose a username and password.
2. **Connection** page: the adapter is `tun0` and the badge is green. The leak
   check should show the same ExpressVPN address NAS VPN shows.
3. In Midgar and Turks, the Transmission download client is: this NAS,
   port 9091, URL base `/transmission/`, and that username and password.
   Inside Midgar/Turks, the download folder is `/volume1/downloads/Lifestream`.

## Good to know
- **Start order:** NAS VPN first, then Lifestream. If NAS VPN is ever rebuilt
  or recreated (not just restarted), restart the lifestream project after.
  Until then it has no internet: safe, but nothing moves.
- **Updating Lifestream:** it updates itself. Every 6 hours it checks GitHub
  and installs new versions (Settings › Updates, where you can also press
  Check now / Update now, or turn automatic updates off). Updates are kept in
  `config/app-update`, so they survive the container being recreated. If an
  update won't start, it goes back to the version the image was built with.
  Rebuilding (Project › lifestream › Action › Build) still works and is only
  needed to pick up changes to this folder's files.
- **Other apps** go through the VPN the same way: `network_mode:
  "container:nas-vpn"`, and their web port moves onto NAS VPN's `ports:` list.
