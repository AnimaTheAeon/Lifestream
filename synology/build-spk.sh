#!/bin/sh
# Assemble Lifestream-<version>-x86_64.spk from a headless build.
# The package runs Lifestream in Container Manager, inside Barrier's network.
# Usage: synology/build-spk.sh <app folder> <version> [outdir]
#   <app folder>  holds the `lifestream` binary and a lib/ folder with its
#                 bundled shared libraries
set -eu
APP="$1"
VERSION="$2"
OUT="${3:-dist}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/pkg/app" "$WORK/pkg/ui" "$WORK/pkg/compose"
cp -R "$APP/." "$WORK/pkg/app/"
chmod 755 "$WORK/pkg/app/lifestream"
echo "$VERSION" > "$WORK/pkg/app/VERSION"
cp -R "$HERE/ui/." "$WORK/pkg/ui/"
# Container Manager builds and runs Lifestream from these (conf/resource).
cp "$HERE/compose/compose.yaml" "$HERE/compose/Dockerfile" "$WORK/pkg/compose/"
cp "$HERE/../docker/entrypoint.sh" "$WORK/pkg/compose/entrypoint.sh"
(cd "$WORK/pkg" && tar --owner=0 --group=0 -czf "$WORK/package.tgz" app ui compose)

mkdir -p "$WORK/spk"
sed "s/@VERSION@/$VERSION/" "$HERE/INFO.in" > "$WORK/spk/INFO"
echo "extractsize=\"$(du -sk "$WORK/pkg" | cut -f1)\"" >> "$WORK/spk/INFO"
cp "$WORK/package.tgz" "$WORK/spk/"
cp -R "$HERE/scripts" "$HERE/conf" "$WORK/spk/"
cp "$HERE/PACKAGE_ICON.PNG" "$HERE/PACKAGE_ICON_256.PNG" "$WORK/spk/"
chmod 755 "$WORK/spk/scripts/"*

mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
NAME="Lifestream-$VERSION-x86_64.spk"
(cd "$WORK/spk" && tar --owner=0 --group=0 -cf "$OUT/$NAME" INFO package.tgz scripts conf PACKAGE_ICON.PNG PACKAGE_ICON_256.PNG)
echo "$OUT/$NAME"
