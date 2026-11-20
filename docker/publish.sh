#!/bin/sh
# Publish a local package directory as a mirror-ready repo.
#
# Usage:
#   ./docker/publish.sh <packages-dir> <output-dir>
#
# <packages-dir> should contain *.mpk files. The script generates
# index.toml (name, version, sha256, url) into <output-dir>/index.toml
# and copies the .mpk files into <output-dir>/pkgs/.
# Point docker-compose at <output-dir> (default: ./publish).

set -eu

SRC="${1:?usage: publish.sh <packages-dir> <output-dir>}"
OUT="${2:?usage: publish.sh <packages-dir> <output-dir>}"

mkdir -p "$OUT/pkgs"
cp "$SRC"/*.mpk "$OUT/pkgs/"

{
  echo 'name = "local-mirror"'
  echo 'packages = ['
  for mpk in "$OUT"/pkgs/*.mpk; do
    base=$(basename "$mpk" .mpk)
    # Expect name-version.mpk naming.
    name=${base%-*}
    version=${base##*-}
    sha=$(sha256sum "$mpk" | cut -d' ' -f1)
    cat <<EOF

[[packages]]
name = "$name"
version = "$version"
sha256 = "$sha"
url = "pkgs/$(basename "$mpk")"
EOF
  done
  echo ']'
} > "$OUT/index.toml"

echo "published $(ls "$OUT/pkgs" | wc -l) package(s) to $OUT"
echo "serve it with: docker compose -f docker/docker-compose.yml up -d"
