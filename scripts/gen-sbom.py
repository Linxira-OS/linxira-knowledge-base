#!/usr/bin/env python3
"""Generate a source-level SPDX 2.3 JSON SBOM for a release.

The SBOM covers the locked dependency set of the tagged commit:

* `Cargo.lock`            -> crates.io packages (`pkg:cargo/<name>@<version>`)
* `web/package-lock.json` -> npm packages (`pkg:npm/<name>@<version>`)

It does not scan the compiled binary; licenses are reported as NOASSERTION
because the lockfiles do not carry license metadata.

Usage:
    scripts/gen-sbom.py --version 0.2.0 --tag v0.2.0 [--commit SHA] --out out.spdx.json
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sys
from pathlib import Path

try:
    import tomllib  # Python >= 3.11
except ModuleNotFoundError:  # pragma: no cover - older local interpreters
    sys.stderr.write("error: Python 3.11+ is required (tomllib)\n")
    raise SystemExit(2) from None

REPO = "Linxira-OS/linxira-knowledge-base"
ROOT = Path(".")


def sanitize(value: str) -> str:
    """SPDX IDs allow [A-Za-z0-9.-] plus '-'."""
    cleaned = "".join(ch if (ch.isalnum() or ch in ".-") else "-" for ch in value)
    return cleaned.strip("-") or "unknown"


def package(
    *,
    spdx_id: str,
    name: str,
    version: str,
    purl: str | None,
    checksum: str | None = None,
    download_location: str = "NOASSERTION",
) -> dict:
    entry: dict = {
        "name": name,
        "SPDXID": spdx_id,
        "versionInfo": version,
        "downloadLocation": download_location,
        "filesAnalyzed": False,
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": "NOASSERTION",
        "copyrightText": "NOASSERTION",
    }
    if checksum:
        entry["checksums"] = [{"algorithm": "SHA256", "checksumValue": checksum}]
    if purl:
        entry["externalRefs"] = [
            {
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": purl,
            }
        ]
    return entry


def cargo_packages() -> list[dict]:
    lock = ROOT / "Cargo.lock"
    if not lock.is_file():
        raise SystemExit(f"error: {lock} not found (run from the repository root)")
    data = tomllib.loads(lock.read_text(encoding="utf-8"))
    out = []
    for entry in data.get("package", []):
        name, version = entry["name"], entry["version"]
        out.append(
            package(
                spdx_id=f"SPDXRef-Package-cargo-{sanitize(name)}-{sanitize(version)}",
                name=name,
                version=version,
                purl=f"pkg:cargo/{name}@{version}",
                checksum=entry.get("checksum"),
                download_location=f"https://crates.io/api/v1/crates/{name}/{version}/download",
            )
        )
    return out


def npm_packages() -> list[dict]:
    lock = ROOT / "web" / "package-lock.json"
    if not lock.is_file():
        raise SystemExit(f"error: {lock} not found (run from the repository root)")
    data = json.loads(lock.read_text(encoding="utf-8"))
    seen: set[tuple[str, str]] = set()
    out = []
    for path, meta in data.get("packages", {}).items():
        if not path or meta.get("link"):
            continue
        if "node_modules/" not in path:
            continue  # workspace paths other than the root: not shipped dependencies
        name = meta.get("name") or path.rsplit("node_modules/", 1)[1]
        version = meta.get("version")
        if not version:
            continue
        key = (name, version)
        if key in seen:
            continue
        seen.add(key)
        encoded = name.replace("@", "%40")
        out.append(
            package(
                spdx_id=f"SPDXRef-Package-npm-{sanitize(name)}-{sanitize(version)}",
                name=name,
                version=version,
                purl=f"pkg:npm/{encoded}@{version}",
                download_location=meta.get("resolved", "NOASSERTION"),
            )
        )
    return out


def created_timestamp(explicit: str | None) -> str:
    if explicit:
        try:
            return dt.datetime.fromisoformat(explicit).astimezone(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        except ValueError as err:
            raise SystemExit(f"error: --created must be ISO 8601, got '{explicit}': {err}") from None
    epoch = os.environ.get("SOURCE_DATE_EPOCH")
    moment = (
        dt.datetime.fromtimestamp(int(epoch), tz=dt.timezone.utc)
        if epoch
        else dt.datetime.now(tz=dt.timezone.utc)
    )
    return moment.strftime("%Y-%m-%dT%H:%M:%SZ")


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate an SPDX 2.3 JSON SBOM")
    parser.add_argument("--version", required=True, help="release version without the leading v, e.g. 0.2.0")
    parser.add_argument("--tag", help="release tag, e.g. v0.2.0 (default: v<version>)")
    parser.add_argument("--commit", default=os.environ.get("GITHUB_SHA", "unknown"), help="commit SHA")
    parser.add_argument("--created", help="SPDX creation timestamp (default: SOURCE_DATE_EPOCH or now)")
    parser.add_argument("--out", required=True, help="output path, e.g. sbom.spdx.json")
    args = parser.parse_args()

    version = args.version
    tag = args.tag or f"v{version}"

    app = package(
        spdx_id="SPDXRef-Package-lkb",
        name="lkb",
        version=version,
        purl=f"pkg:cargo/lkb@{version}",
        download_location=f"https://github.com/{REPO}",
    )
    web_app = package(
        spdx_id="SPDXRef-Package-lkb-web",
        name="lkb-web",
        version=version,
        purl=f"pkg:npm/lkb-web@{version}",
        download_location=f"https://github.com/{REPO}",
    )

    packages = [app, web_app, *cargo_packages(), *npm_packages()]
    packages.sort(key=lambda pkg: pkg["SPDXID"])

    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"linxira-knowledge-base-{version}",
        "documentNamespace": f"https://github.com/{REPO}/sbom/{tag}/{args.commit}",
        "creationInfo": {
            "created": created_timestamp(args.created),
            "creators": ["Tool: scripts/gen-sbom.py", "Organization: Linxira-OS"],
        },
        "documentDescribes": [app["SPDXID"], web_app["SPDXID"]],
        "packages": packages,
        "relationships": [
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": app["SPDXID"],
            },
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": web_app["SPDXID"],
            },
        ],
    }

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(document, indent=2, sort_keys=False) + "\n", encoding="utf-8")
    sys.stderr.write(f"wrote {out}: {len(packages)} packages ({tag} @ {args.commit})\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
