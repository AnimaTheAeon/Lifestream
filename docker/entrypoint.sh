#!/bin/sh
# Runs Lifestream. When Docker stops the container it asks Lifestream to save
# download progress first (Lifestream watches for a stop-request file).
#
# Updates: Lifestream unpacks a new version into $UPD/next and exits with
# code 75. This script then switches to it and starts it again. If an update
# won't start, it goes back to the version built into the image.
STOP=/config/Lifestream/stop-request
BUILT=/opt/lifestream/app
UPD=${LIFESTREAM_UPDATE_DIR:-/config/app-update}
mkdir -p /config/Lifestream "$UPD"

version() { cat "$1/VERSION" 2>/dev/null || echo 0; }
# True when version $1 is newer than version $2.
newer() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" = "$1" ]; }

pick() {
	if [ -x "$UPD/next/lifestream" ]; then
		rm -rf "$UPD/previous"
		[ -d "$UPD/current" ] && mv "$UPD/current" "$UPD/previous"
		mv "$UPD/next" "$UPD/current"
	fi
	rm -rf "$UPD/next"
	if [ -x "$UPD/current/lifestream" ] && newer "$(version "$UPD/current")" "$(version "$BUILT")"; then
		DIR="$UPD/current"
	else
		DIR="$BUILT"
	fi
}

STOPPING=0
PID=
stop() {
	STOPPING=1
	touch "$STOP"
	i=0
	while [ $i -lt 40 ] && [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; do
		sleep 0.25
		i=$((i + 1))
	done
	[ -n "$PID" ] && kill -TERM "$PID" 2>/dev/null
}
trap stop TERM INT

while :; do
	pick
	echo "Starting Lifestream $(version "$DIR")"
	rm -f "$STOP"
	cd "$DIR" || exit 1
	STARTED=$(date +%s)
	./lifestream &
	PID=$!
	wait "$PID"
	CODE=$?
	# wait returns early when a signal arrives; wait again for the real exit.
	while kill -0 "$PID" 2>/dev/null; do wait "$PID"; CODE=$?; done
	[ "$STOPPING" = 1 ] && exit 0
	if [ "$CODE" = 75 ]; then
		echo "Lifestream: switching to the update"
		continue
	fi
	# An update that dies right away is set aside; the built-in version runs instead.
	if [ "$DIR" = "$UPD/current" ] && [ $(( $(date +%s) - STARTED )) -lt 30 ]; then
		echo "Lifestream $(version "$DIR") stopped right after starting (code $CODE). Going back to $(version "$BUILT")."
		rm -rf "$UPD/failed"
		mv "$UPD/current" "$UPD/failed"
		continue
	fi
	exit "$CODE"
done
