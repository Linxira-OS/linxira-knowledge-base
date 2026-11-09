#!/usr/bin/env bash
# Assemble the Linux portable tar.gz: the desktop shell, the `lkb` sidecar and
# the bundled web resources exactly as `tauri build` lays them out next to the
# executable. Unpack and run; no installer, no root.
#
#   deploy/desktop/portable-tar.sh <release-dir> <out-dir> <version>
#
# <release-dir> is desktop/src-tauri/target/release (or
# desktop/src-tauri/target/<triple>/release on cross builds).
set -euo pipefail

if [[ $# -ne 3 ]]; then
  sed -n '2,10p' "$0"
  exit 2
fi

release_dir="$(cd "$1" && pwd)"
out_dir="$(mkdir -p "$2" && cd "$2" && pwd)"
version="$3"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

app_bin="$(find "$release_dir" -maxdepth 1 -type f -name "lkb-desktop" -print -quit)"
[[ -n "$app_bin" ]] || { echo "missing lkb-desktop in $release_dir" >&2; exit 1; }
[[ -f "$release_dir/lkb" ]] || { echo "missing lkb in $release_dir (run scripts/build-sidecar first)" >&2; exit 1; }
[[ -f "$release_dir/resources/web/index.html" ]] || { echo "missing resources/web in $release_dir (run scripts/build-sidecar first)" >&2; exit 1; }

name="lkb"
tar_name="lkb-solo-v${version}-linux-amd64"
staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

mkdir -p "$staging/$name/resources"
cp "$app_bin" "$staging/$name/lkb-desktop"
cp "$release_dir/lkb" "$staging/$name/lkb"
cp -R "$release_dir/resources/web" "$staging/$name/resources/web"
chmod +x "$staging/$name/lkb-desktop" "$staging/$name/lkb"
if [[ -f "$repo_root/LICENSE" ]]; then
  cp "$repo_root/LICENSE" "$staging/$name/"
fi
if [[ -f "$repo_root/README.md" ]]; then
  cp "$repo_root/README.md" "$staging/$name/"
fi

tar --owner=0 --group=0 -czf "$out_dir/$tar_name.tar.gz" -C "$staging" "$name"
echo "portable -> $out_dir/$tar_name.tar.gz"
