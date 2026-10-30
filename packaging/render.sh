#!/usr/bin/env bash
# Fills the packaging templates with a release's version and checksums.
# packaging/render.sh VERSION ASSETS OUT
#   ASSETS  a folder with the release's .dmg, .deb and -setup.exe files
#   OUT     where the filled files go: homebrew/, winget/, aur/ and flatpak/
set -euo pipefail

version=$1 assets=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)

sum() {
  local file="$assets/$1"
  [ -f "$file" ] || { echo "missing $1 in $assets" >&2; exit 1; }
  sha256sum "$file" | cut -d' ' -f1
}

declare -A values=(
  [VERSION]=$version
  [DATE]=$(date -u +%Y-%m-%d)
  [SHA256_DMG_AARCH64]=$(sum "Kubri_${version}_aarch64.dmg")
  [SHA256_DMG_X64]=$(sum "Kubri_${version}_x64.dmg")
  [SHA256_DEB_AMD64]=$(sum "Kubri_${version}_amd64.deb")
  [SHA256_DEB_ARM64]=$(sum "Kubri_${version}_arm64.deb")
  [SHA256_EXE_X64]=$(sum "Kubri_${version}_x64-setup.exe")
)

for dir in homebrew winget aur flatpak; do
  mkdir -p "$out/$dir"
  for template in "$here/$dir"/*; do
    text=$(<"$template")
    for key in "${!values[@]}"; do text=${text//@$key@/${values[$key]}}; done
    if grep -q '@[A-Z0-9_]*@' <<<"$text"; then
      echo "unfilled placeholder in $template" >&2
      exit 1
    fi
    printf '%s\n' "$text" > "$out/$dir/$(basename "$template")"
  done
done
echo "filled the packaging for $version in $out"
