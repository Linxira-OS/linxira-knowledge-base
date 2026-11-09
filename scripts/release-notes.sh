#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Extract the release notes for a tag from CHANGELOG.md, plus a footer with the
# repository / issue tracker / security advisory links and the asset map.
#
# Usage:
#   scripts/release-notes.sh v0.2.1 [--out notes.md]
#
# - The tag must look like v<major>.<minor>.<patch>.
# - CHANGELOG.md (override with CHANGELOG_FILE) must contain a `## [<version>]`
#   section; a missing section is a hard error, never an empty release body.
# - Notes go to stdout by default, or to --out FILE. Diagnostics go to stderr.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/release-notes.sh v<major>.<minor>.<patch> [--out FILE]

environment:
  CHANGELOG_FILE   changelog path (default: CHANGELOG.md)
  GITHUB_REPOSITORY  owner/repo, used for links (default: Linxira-OS/linxira-knowledge-base)
EOF
}

tag=""
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --out)
            [ $# -ge 2 ] || { echo "error: --out needs a value" >&2; exit 2; }
            out="$2"; shift 2 ;;
        -*)
            echo "error: unknown option: $1" >&2; usage; exit 2 ;;
        *)
            if [ -n "$tag" ]; then
                echo "error: unexpected extra argument: $1" >&2; usage; exit 2
            fi
            tag="$1"; shift ;;
    esac
done

if [ -z "$tag" ]; then
    echo "error: missing tag argument" >&2
    usage
    exit 2
fi

case "$tag" in
    v[0-9]*\.[0-9]*\.[0-9]*) : ;;
    *) echo "error: tag '$tag' must look like v<major>.<minor>.<patch>" >&2; exit 1 ;;
esac
version="${tag#v}"

changelog="${CHANGELOG_FILE:-CHANGELOG.md}"
[ -f "$changelog" ] || { echo "error: $changelog not found (run from the repository root)" >&2; exit 1; }

repo="${GITHUB_REPOSITORY:-Linxira-OS/linxira-knowledge-base}"

# The section runs from `## [<version>]` to the next level-2 heading.
body="$(awk -v version="$version" '
    index($0, "## [" version "]") == 1 { found = 1; print; next }
    found && /^## / { exit }
    found { print }
' "$changelog")"

content_lines="$(printf '%s\n' "$body" | grep -c '[^[:space:]]' || true)"
if [ -z "${body//[[:space:]]/}" ] || [ "$content_lines" -le 1 ]; then
    echo "error: no non-empty '## [$version]' section found in $changelog" >&2
    exit 1
fi

# Image reference (lowercase owner, as GHCR requires) computed without bash 4
# `${var,,}` so the script also runs on macOS' bash 3.2.
image="ghcr.io/$(printf '%s' "$repo" | tr '[:upper:]' '[:lower:]'):${version}"

footer="$(cat <<EOF

---

## 发布资产 · Release assets

- 个人用户版（solo CLI）：\`lkb-solo-v${version}-x86_64-unknown-linux-gnu.tar.gz\`、\`lkb-solo-v${version}-x86_64-pc-windows-msvc.zip\`，内含 LICENSE、LICENSE-MIT、LICENSE-APACHE、LICENSING.md、README、CHANGELOG 与 \`deploy/solo/\` 示例配置。
- 桌面版（Windows / Linux）：\`lkb-solo-v${version}-windows-x64-setup.exe\`（NSIS 安装包）、\`lkb-solo-v${version}-windows-x64-portable.zip\`、\`lkb-solo-v${version}-linux-amd64.deb\`、\`lkb-solo-v${version}-linux-amd64.AppImage\`、\`lkb-solo-v${version}-linux-amd64.tar.gz\`（便携包）。**macOS 不发布**。
- 企业部署版（enterprise）：\`${image}\`（另推 \`${version%.*}\` 与 \`latest\`）与 \`linxira-knowledge-base-enterprise-v${version}.tar.gz\`（compose + nginx + 示例配置 + web/dist）。
- Web SPA：\`lkb-web-v${version}.tar.gz\`；\`SHA256SUMS\`；SPDX SBOM：\`linxira-knowledge-base-v${version}.spdx.json\`。
- 同一二进制同时支持 SQLite 与 PostgreSQL（配置切换），**不存在**单独的 SQLite 版 / PostgreSQL 版构建，也没有 debug 包或源码包（源码用 GitHub 自动生成的 source zip）；所有安装包均未签名（Windows 可能出现 SmartScreen 提示）。

校验（SHA-256）与构建来源证明（provenance attestation）：

\`\`\`bash
sha256sum -c SHA256SUMS
gh attestation verify lkb-solo-v${version}-x86_64-unknown-linux-gnu.tar.gz --repo ${repo}
\`\`\`

---

- 仓库：https://github.com/${repo}
- 使用文档：https://github.com/${repo}/tree/main/docs
- 问题反馈：https://github.com/${repo}/issues
- 安全漏洞（私密上报）：https://github.com/${repo}/security/advisories/new
- 版本规则、升级与回滚：https://github.com/${repo}/blob/main/document/release-process.md
EOF
)"

notes="$body
$footer"

if [ -n "$out" ]; then
    printf '%s\n' "$notes" > "$out"
    echo "wrote $out ($(printf '%s\n' "$notes" | wc -l | tr -d ' ') lines, version $version)" >&2
else
    printf '%s\n' "$notes"
fi
