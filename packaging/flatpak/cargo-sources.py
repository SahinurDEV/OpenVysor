#!/usr/bin/env python3
"""Writes cargo-sources.json, the crates Flatpak downloads before an offline build.

Run from the repository root after Cargo.lock changes:

    python3 packaging/flatpak/cargo-sources.py

Only crates.io packages are supported, which is all OpenVysor uses. Each crate is
checked against the SHA-256 that Cargo.lock already records.
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "native" / "Cargo.lock"
OUTPUT = Path(__file__).resolve().parent / "cargo-sources.json"
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"

CONFIG = f"""[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "{VENDOR}"
"""


def main() -> int:
    packages = tomllib.loads(LOCK.read_text(encoding="utf-8"))["package"]
    sources = []
    for package in packages:
        source = package.get("source")
        if source is None:
            continue  # OpenVysor itself
        if source != REGISTRY:
            print(f"unsupported source for {package['name']}: {source}", file=sys.stderr)
            return 1
        name, version, checksum = package["name"], package["version"], package["checksum"]
        folder = f"{VENDOR}/{name}-{version}"
        sources.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": checksum,
            "dest": folder,
        })
        sources.append({
            "type": "inline",
            "contents": json.dumps({"package": checksum, "files": {}}),
            "dest": folder,
            "dest-filename": ".cargo-checksum.json",
        })
    sources.append({"type": "inline", "contents": CONFIG, "dest": "cargo", "dest-filename": "config.toml"})
    OUTPUT.write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
    print(f"{OUTPUT.name}: {len(sources) // 2} crates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
