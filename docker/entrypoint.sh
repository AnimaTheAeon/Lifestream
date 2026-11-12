#!/bin/sh
# Runs Lifestream and, when Docker stops the container, asks it to save
# download progress first (it watches for a stop-request file).
STOP=/config/Lifestream/stop-request
mkdir -p /config/Lifestream
rm -f "$STOP"
cd /opt/lifestream/app || exit 1
./lifestream &
PID=$!
stop() {
	touch "$STOP"
	i=0
	while [ $i -lt 40 ] && kill -0 "$PID" 2>/dev/null; do
		sleep 0.25
		i=$((i + 1))
	done
	kill -TERM "$PID" 2>/dev/null
}
trap stop TERM INT
wait "$PID"
# wait returns early when a signal arrives; wait again for the real exit.
while kill -0 "$PID" 2>/dev/null; do wait "$PID"; done
