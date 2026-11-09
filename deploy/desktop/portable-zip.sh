#!/usr/bin/env bash
# Assemble the Windows portable zip: the desktop shell, the `lkb` sidecar and
# the bundled web resources exactly as `tauri build` lays them out next to the
# executable. Unzip and run; no installer, no registry writes.
#
#   deploy/desktop/portable-zip.sh <release-dir> <out-dir> <version>
#
# <release-dir> is desktop/src-tauri/target/release (or
# desktop/src-tauri/target/<triple>/release on cross builds).
set -euo pipefail

if [[ $# -ne 3 ]]; then
  sed -n '2,9p' "$0"
  exit 2
fi

release_dir="$(cd "$1" && pwd)"
out_dir="$(mkdir -p "$2" && cd "$2" && pwd)"
version="$3"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

app_exe="$(find "$release_dir" -maxdepth 1 -type f -name "lkb-desktop*.exe" -print -quit)"
[[ -n "$app_exe" ]] || { echo "missing lkb-desktop*.exe in $release_dir" >&2; exit 1; }
[[ -f "$release_dir/lkb.exe" ]] || { echo "missing lkb.exe in $release_dir (run scripts/build-sidecar first)" >&2; exit 1; }
[[ -f "$release_dir/resources/web/index.html" ]] || { echo "missing resources/web in $release_dir (run scripts/build-sidecar first)" >&2; exit 1; }

name="lkb"
zip_name="lkb-solo-v${version}-windows-x64-portable"
staging="$(mktemp -d)"
trap 'rm -rf "$staging"' EXIT

mkdir -p "$staging/$name/resources"
cp "$app_exe" "$staging/$name/lkb-desktop.exe"
cp "$release_dir/lkb.exe" "$staging/$name/lkb.exe"
cp -R "$release_dir/resources/web" "$staging/$name/resources/web"
# windows-gnu builds load WebView2 dynamically; MSVC builds link it statically
# and simply do not have the file.
if [[ -f "$release_dir/WebView2Loader.dll" ]]; then
  cp "$release_dir/WebView2Loader.dll" "$staging/$name/"
fi
for f in LICENSE LICENSE-MIT LICENSE-APACHE LICENSING.md README.md; do
  if [[ -f "$repo_root/$f" ]]; then
    cp "$repo_root/$f" "$staging/$name/"
  fi
done

(cd "$staging" && zip -qr "$out_dir/$zip_name.zip" "$name")
echo "portable -> $out_dir/$zip_name.zip"
