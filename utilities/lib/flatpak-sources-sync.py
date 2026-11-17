#!/usr/bin/env python3
"""Add the crates in Cargo.lock that the Flatpak's offline crate list (cargo-sources.json)
is missing, in the same form as its other entries (the crate archive and its checksum file).
Run it after adding a dependency anywhere in the workspace: the Flatpak build fetches the
whole lockfile offline. Prints what it added."""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCK = (ROOT / "Cargo.lock").read_text()
LIST = ROOT / "apps/rivet-files/packaging/flatpak/cargo-sources.json"

entries = json.loads(LIST.read_text())
pattern = r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"\nsource = "registry\+https://github.com/rust-lang/crates.io-index"\nchecksum = "([0-9a-f]+)"'
packages = re.findall(pattern, LOCK)
known = {e["dest"] for e in entries if e.get("type") == "archive"}
config = [e for e in entries if e.get("dest-filename") == "config"]
crates = [e for e in entries if e.get("dest-filename") != "config"]
added = []
for name, version, sha in packages:
    dest = f"cargo/vendor/{name}-{version}"
    if dest in known:
        continue
    crates.append({"type": "archive", "archive-type": "tar-gzip", "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate", "sha256": sha, "dest": dest})
    crates.append({"type": "inline", "contents": json.dumps({"package": sha, "files": {}}), "dest": dest, "dest-filename": ".cargo-checksum.json"})
    added.append(f"{name} {version}")
# Sorted by destination, each archive before its checksum file.
crates.sort(key=lambda e: (e["dest"], 0 if e["type"] == "archive" else 1))
LIST.write_text(json.dumps(crates + config, indent=4) + "\n")
print(f"added {len(added)}: " + ", ".join(added) if added else "nothing missing")
