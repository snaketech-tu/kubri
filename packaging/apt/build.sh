#!/usr/bin/env bash
# Builds Kubri's apt repository from .deb files, with the app-store data (AppStream) that lets
# software centers like the COSMIC Store, GNOME Software and KDE Discover show Kubri.
# packaging/apt/build.sh DEBS OUT URL
#   DEBS  a folder with the .deb files to offer, for amd64 and arm64
#   OUT   where the repository goes; it is served at URL
# Signs the repository with the secret key in gpg's keyring, or with the one in $APT_SIGNING_KEY
# (armored), and writes its public half to OUT/kubri.gpg.
# Needs apt-ftparchive (apt-utils), appstreamcli with its compose module (appstream-compose)
# and gpg.
set -euo pipefail

debs=$(cd "$1" && pwd) out=$2 url=${3%/}
suite=stable component=main archs=(amd64 arm64)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ -n "${APT_SIGNING_KEY:-}" ]; then
  export GNUPGHOME=$work/gnupg
  mkdir -m 700 "$GNUPGHOME"
  gpg --batch --quiet --import <<<"$APT_SIGNING_KEY"
fi
key=$(gpg --batch --list-secret-keys --with-colons | awk -F: '$1 == "fpr" { print $10; exit }')
[ -n "$key" ] || { echo "no secret key to sign the repository with" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out/pool/$component/k/kubri"
cp "$debs"/*.deb "$out/pool/$component/k/kubri/"
out=$(cd "$out" && pwd)

cd "$out"
for arch in "${archs[@]}"; do
  dir=dists/$suite/$component/binary-$arch
  mkdir -p "$dir"
  apt-ftparchive --arch "$arch" packages pool > "$dir/Packages"
  gzip -9nk "$dir/Packages"
  xz -k "$dir/Packages"
done

# The app-store data, made from the metainfo, desktop file and icons inside the newest package of
# each architecture.
dep11=dists/$suite/$component/dep11
mkdir -p "$dep11"
for arch in "${archs[@]}"; do
  deb=$(printf '%s\n' pool/$component/k/kubri/*_"$arch".deb | sort -V | tail -n 1)
  mkdir -p "$work/$arch/root"
  dpkg-deb -x "$deb" "$work/$arch/root"
  # The screenshots are copied into the repository, with thumbnails in the sizes stores show.
  appstreamcli compose --print-report=on-error --origin=kubri --result-root="$work/$arch" \
    --data-dir="$work/$arch/data" --icons-dir="$work/$arch/icons" --hints-dir="$work/$arch/hints" \
    --media-dir="$out/media" --media-baseurl="$url/media" "$work/$arch/root" > /dev/null
  # Packages from before 1.10.1 have no metainfo: the repository then works without the entry.
  zcat "$work/$arch/data/kubri.xml.gz" > "$work/$arch/kubri.xml"
  if ! grep -q '<component' "$work/$arch/kubri.xml"; then
    echo "::warning::$deb has no app-store entry, so software centers will not list Kubri for $arch"
    continue
  fi
  appstreamcli convert --format=yaml "$work/$arch/kubri.xml" "$work/$arch/kubri.yml"
  grep -q '^ID: io.github.snaketech_tu.kubri$' "$work/$arch/kubri.yml" ||
    { echo "no app-store entry for Kubri in $deb" >&2; exit 1; }
  # The header names the origin, which software centers file the icons under, and each entry
  # names the package that installs it.
  sed -e "s/^Version: '1.0'$/&\nOrigin: kubri\nPriority: 0/" -e 's/^ID: .*/&\nPackage: kubri/' \
    "$work/$arch/kubri.yml" > "$dep11/Components-$arch.yml"
  gzip -9nk "$dep11/Components-$arch.yml"
done
for size in "$work/amd64/icons"/*/; do
  [ -d "$size" ] || continue
  size=${size%/}
  tarball=$out/$dep11/icons-$(basename "$size").tar
  (cd "$size" && tar --owner=0 --group=0 -cf "$tarball" -- *.png)
  gzip -9nk "$tarball"
done
rmdir "$dep11" 2> /dev/null || true

apt-ftparchive \
  -o "APT::FTPArchive::Release::Origin=Kubri" \
  -o "APT::FTPArchive::Release::Label=Kubri" \
  -o "APT::FTPArchive::Release::Suite=$suite" \
  -o "APT::FTPArchive::Release::Codename=$suite" \
  -o "APT::FTPArchive::Release::Architectures=${archs[*]}" \
  -o "APT::FTPArchive::Release::Components=$component" \
  -o "APT::FTPArchive::Release::Description=Kubri, a Kubernetes GUI" \
  release "dists/$suite" > Release
mv Release "dists/$suite/Release"
gpg --batch --yes --local-user "$key" --clearsign -o "dists/$suite/InRelease" "dists/$suite/Release"
gpg --batch --yes --local-user "$key" --armor --detach-sign -o "dists/$suite/Release.gpg" "dists/$suite/Release"
gpg --batch --yes --export "$key" > kubri.gpg

# What people add to /etc/apt/sources.list.d/.
cat > kubri.sources <<SOURCES
Types: deb
URIs: $url
Suites: $suite
Components: $component
Signed-By: /usr/share/keyrings/kubri.gpg
SOURCES
echo "built the apt repository in $out"
