#!/usr/bin/env bash
# Writes the latest.json the desktop app's updater reads, from the signatures of a release's
# update packages: packaging/latest-json.sh 1.2.0 sigs > latest.json
#
# sigs holds the .sig files the Release workflow attached, named like the package they sign.
# Each package is listed as {os}-{arch}-{format}, which the updater looks for first; the
# AppImage, the setup.exe and the macOS app also stand for {os}-{arch}, for older updaters.
set -euo pipefail

version=$1
dir=$2
base="https://github.com/snaketech-tu/kubri/releases/download/v$version"

arch_of() {
  case $1 in
    *aarch64* | *arm64*) echo aarch64 ;;
    *x86_64* | *amd64* | *x64*) echo x86_64 ;;
    *) return 1 ;;
  esac
}

platforms='{}'
shopt -s nullglob
for sig in "$dir"/*.sig; do
  package=$(basename "$sig" .sig)
  arch=$(arch_of "$package") || { echo "No architecture in $package, skipped" >&2; continue; }
  case $package in
    *.app.tar.gz) keys="darwin-$arch-app darwin-$arch" ;;
    *.AppImage) keys="linux-$arch-appimage linux-$arch" ;;
    *.deb) keys="linux-$arch-deb" ;;
    *.rpm) keys="linux-$arch-rpm" ;;
    *-setup.exe) keys="windows-$arch-nsis windows-$arch" ;;
    *.msi) keys="windows-$arch-msi" ;;
    *) echo "Not an update package: $package, skipped" >&2; continue ;;
  esac
  for key in $keys; do
    platforms=$(jq --arg key "$key" --arg url "$base/$package" --rawfile signature "$sig" \
      '.[$key] = {signature: ($signature | rtrimstr("\n")), url: $url}' <<<"$platforms")
  done
done

if [ "$platforms" = '{}' ]; then
  echo "No signatures in $dir" >&2
  exit 1
fi

jq -n --arg version "$version" --argjson platforms "$platforms" \
  --arg notes "https://github.com/snaketech-tu/kubri/releases/tag/v$version" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{version: $version, notes: $notes, pub_date: $date, platforms: $platforms}'
