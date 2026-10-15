#!/usr/bin/env python3
# RWildFire - a fractal flame editor written in Rust
# Copyright (C) 2026 Christian Nielebock
# SPDX-License-Identifier: LGPL-3.0-or-later
"""Publishes a release on GitHub.

    python3 tools/release/release.py            # tag v<version> and push it: GitHub Actions
                                                # (.github/workflows/release.yml) builds the
                                                # archives and creates the release
    python3 tools/release/release.py --local    # build here and upload with the gh CLI
    python3 tools/release/release.py --dry-run  # only show what would happen

The version is the one in Cargo.toml; bump it (all crates) and commit before releasing. The
working tree must be clean and main pushed, so the release matches the published sources.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def sh(*cmd, capture=False, dry=False):
    print("+", " ".join(cmd), flush=True)
    if dry:
        return ""
    r = subprocess.run(cmd, cwd=ROOT, check=True, text=True, capture_output=capture)
    return r.stdout.strip() if capture else ""


def out(*cmd):
    return subprocess.run(cmd, cwd=ROOT, check=True, text=True, capture_output=True).stdout.strip()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--local", action="store_true", help="build here and upload with gh instead of GitHub Actions")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    text = (ROOT / "Cargo.toml").read_text()
    version = re.search(r'\[workspace\.package\][^\[]*?version\s*=\s*"([^"]+)"', text, re.S).group(1)
    tag = f"v{version}"
    problems = []
    if out("git", "status", "--porcelain"):
        problems.append("the working tree has uncommitted changes")
    branch = out("git", "rev-parse", "--abbrev-ref", "HEAD")
    sh("git", "fetch", "origin", "--tags", dry=args.dry_run)
    if out("git", "rev-parse", "HEAD") != out("git", "rev-parse", f"origin/{branch}"):
        problems.append(f"{branch} differs from origin/{branch} (push first)")
    if out("git", "tag", "--list", tag):
        problems.append(f"tag {tag} exists already (bump the version in Cargo.toml)")
    for c in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        v = re.search(r'^version\s*=\s*"([^"]+)"', c.read_text(), re.M)
        if v and v.group(1) != version:
            problems.append(f"{c.relative_to(ROOT)} has version {v.group(1)}, Cargo.toml {version}")
    if problems:
        sys.exit("not released:\n- " + "\n- ".join(problems))

    print(f"releasing RWildFire {version} ({tag})")
    if args.local:
        sh(sys.executable, "tools/release/package.py", dry=args.dry_run)
        notes = (ROOT / "tools/release/notes.md").read_text().replace("{version}", version)
        notes_file = ROOT / "dist" / "notes.md"
        if not args.dry_run:
            notes_file.write_text(notes)
        archives = [str(p) for p in sorted((ROOT / "dist").glob(f"RWildFire-{version}-*.zip"))]
        sh("git", "tag", "-a", tag, "-m", f"RWildFire {version}", dry=args.dry_run)
        sh("git", "push", "origin", tag, dry=args.dry_run)
        sh("gh", "release", "create", tag, *archives, "--title", f"RWildFire {version}", "--notes-file", str(notes_file), "--generate-notes", dry=args.dry_run)
    else:
        sh("git", "tag", "-a", tag, "-m", f"RWildFire {version}", dry=args.dry_run)
        sh("git", "push", "origin", tag, dry=args.dry_run)
        print("GitHub Actions builds and publishes the release: https://github.com/Ravetracer/RWildFire/actions")


if __name__ == "__main__":
    main()
