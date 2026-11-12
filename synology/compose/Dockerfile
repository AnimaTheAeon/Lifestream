# Built on the NAS by Container Manager when the package installs or updates.
# Uses the Lifestream program that came inside the package (../app).
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*
COPY app /opt/lifestream/app
COPY compose/entrypoint.sh /entrypoint.sh
RUN chmod 755 /entrypoint.sh /opt/lifestream/app/lifestream
ENV HOME=/config XDG_CONFIG_HOME=/config LIFESTREAM_UPDATE_DIR=/config/app-update
EXPOSE 9091
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
