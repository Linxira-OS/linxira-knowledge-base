#!/usr/bin/env bash
# Build the desktop inputs: the `lkb` sidecar and the web SPA.
#
#   scripts/build-sidecar.sh [--target <triple>] [--skip-web] [--skip-rust]
#
# Copies `target/<triple>/release/lkb[.exe]` to
# `desktop/src-tauri/binaries/lkb-<triple>[.exe]` (the name Tauri's
# `externalBin` expects) and `web/dist` to `desktop/src-tauri/resources/web`
# (bundled as a resource and served by `lkb serve` via LKB_WEB_DIR).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target=""
skip_web=0
skip_rust=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      target="${2:?--target needs a target triple}"
      shift 2
      ;;
    --skip-web) skip_web=1; shift ;;
    --skip-rust) skip_rust=1; shift ;;
    -h | --help)
      sed -n '2,10p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

triple="${target:-$(rustc -vV | sed -n 's/^host: //p')}"
exe_suffix=""
case "$triple" in *windows*) exe_suffix=".exe" ;; esac

if [[ $skip_web -eq 0 ]]; then
  (cd "$root/web" && npm ci --no-audit --no-fund && npm run build)
fi

if [[ $skip_rust -eq 0 ]]; then
  if [[ -n "$target" ]]; then
    (cd "$root" && cargo build --release --locked --target "$target")
    bin="$root/target/$target/release/lkb$exe_suffix"
  else
    (cd "$root" && cargo build --release --locked)
    bin="$root/target/release/lkb$exe_suffix"
  fi
  [[ -f "$bin" ]] || { echo "sidecar binary not found: $bin" >&2; exit 1; }
fi

sidecar_dir="$root/desktop/src-tauri/binaries"
web_dir="$root/desktop/src-tauri/resources/web"
mkdir -p "$sidecar_dir"
rm -rf "$web_dir"
mkdir -p "$web_dir"

if [[ $skip_rust -eq 0 ]]; then
  cp "$bin" "$sidecar_dir/lkb-${triple}${exe_suffix}"
  echo "sidecar -> $sidecar_dir/lkb-${triple}${exe_suffix}"
fi
cp -R "$root/web/dist/." "$web_dir/"
echo "web     -> $web_dir"
