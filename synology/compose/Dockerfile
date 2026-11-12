# The image inside the Lifestream package (built by synology/build-spk.sh).
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/*
COPY app /opt/lifestream/app
COPY entrypoint.sh /entrypoint.sh
RUN chmod 755 /entrypoint.sh /opt/lifestream/app/lifestream
ENV HOME=/config XDG_CONFIG_HOME=/config LIFESTREAM_UPDATE_DIR=/config/app-update
EXPOSE 9091
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
