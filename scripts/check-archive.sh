#!/usr/bin/env bash
# Verify the file manifest of a release archive before it is published.
#
# Usage: scripts/check-archive.sh <archive> <solo|web|enterprise>
#
# Checks that
#   - the archive is readable, has exactly one top-level directory,
#   - the files required for that archive kind are present,
#   - no workspace junk (target/, node_modules/, .git/, __pycache__/, …) leaked in.
# Exits non-zero with a specific message on the first problem.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/check-archive.sh <archive.tar.gz|archive.zip> <solo|web|enterprise>
EOF
}

archive="${1:-}"
kind="${2:-}"
if [ -z "$archive" ] || [ -z "$kind" ]; then
    usage
    exit 2
fi
if [ ! -f "$archive" ]; then
    echo "error: archive not found: $archive" >&2
    exit 1
fi
case "$kind" in
    solo|web|enterprise) : ;;
    *) echo "error: unknown kind '$kind' (expected solo, web or enterprise)" >&2; exit 2 ;;
esac

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

case "$archive" in
    *.zip)
        command -v unzip >/dev/null || { echo "error: unzip not available" >&2; exit 1; }
        unzip -Z1 "$archive" > "$tmp"
        ;;
    *.tar.gz|*.tgz)
        tar -tzf "$archive" > "$tmp"
        ;;
    *)
        echo "error: unsupported archive type: $archive (expected .tar.gz, .tgz or .zip)" >&2
        exit 2
        ;;
esac

entries="$(grep -v '/$' "$tmp" | grep -v '^$' || true)"
if [ -z "$entries" ]; then
    echo "error: archive contains no files: $archive" >&2
    exit 1
fi

roots="$(printf '%s\n' "$entries" | awk -F/ 'NF>1 {print $1}' | sort -u)"
root_count="$(printf '%s\n' "$roots" | grep -c . || true)"
if [ "$root_count" -ne 1 ]; then
    echo "error: expected exactly one top-level directory in $archive, found: $(printf '%s' "$roots" | tr '\n' ' ')" >&2
    exit 1
fi
root="$(printf '%s\n' "$roots" | head -n 1)"

has() { printf '%s\n' "$entries" | grep -qx "$root/$1"; }
require() {
    if ! has "$1"; then
        echo "error: $archive is missing $root/$1" >&2
        exit 1
    fi
}

case "$kind" in
    solo)
        if ! has lkb && ! has lkb.exe; then
            echo "error: $archive has neither $root/lkb nor $root/lkb.exe" >&2
            exit 1
        fi
        require LICENSE
        require README.md
        require CHANGELOG.md
        require deploy/solo/lkb.toml.example
        require deploy/solo/README.md
        ;;
    web)
        require index.html
        ;;
    enterprise)
        require deploy/compose.yaml
        require deploy/nginx.conf
        require deploy/lkb.enterprise.toml.example
        require deploy/.env.example
        require deploy/README.md
        require web/dist/index.html
        require LICENSE
        ;;
esac

forbidden='(^|/)(target|node_modules|\.git|__pycache__|\.venv)(/|$)'
if printf '%s\n' "$entries" | grep -Eq "$forbidden"; then
    echo "error: $archive contains workspace/build junk:" >&2
    printf '%s\n' "$entries" | grep -E "$forbidden" | head -n 10 >&2
    exit 1
fi

count="$(printf '%s\n' "$entries" | grep -c . || true)"
echo "OK $archive: kind=$kind root=$root files=$count"
