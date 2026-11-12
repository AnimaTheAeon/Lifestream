# Lifestream for Docker (headless; controlled from its web page on port 9091).
# Building downloads the newest Synology release from GitHub and runs it.
FROM debian:bookworm-slim

ARG VERSION=latest
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl tini \
 && rm -rf /var/lib/apt/lists/* \
 && set -e; \
    if [ "$VERSION" = "latest" ]; then \
      VERSION="$(curl -fsSL https://api.github.com/repos/AnimaTheAeon/Lifestream/releases/latest | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n1)"; \
    fi; \
    echo "Lifestream $VERSION"; \
    mkdir -p /tmp/spk /opt/lifestream; \
    curl -fsSL -o /tmp/l.spk "https://github.com/AnimaTheAeon/Lifestream/releases/download/v${VERSION}/Lifestream-${VERSION}-x86_64.spk"; \
    tar -xf /tmp/l.spk -C /tmp/spk package.tgz; \
    tar -xzf /tmp/spk/package.tgz -C /opt/lifestream app; \
    rm -rf /tmp/spk /tmp/l.spk

COPY entrypoint.sh /entrypoint.sh
RUN chmod 755 /entrypoint.sh

ENV HOME=/config XDG_CONFIG_HOME=/config
VOLUME ["/config"]
EXPOSE 9091
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
