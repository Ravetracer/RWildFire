#!/usr/bin/env python3
# RWildFire - a fractal flame editor written in Rust
# Copyright (C) 2026 Christian Nielebock
#
# SPDX-License-Identifier: LGPL-3.0-or-later
"""Downloads Intel Open Image Denoise (OIDN) and keeps the parts RWildFire loads at runtime.

RWildFire does not link OIDN; it looks for the library next to the executable (`lib/` on
Linux and macOS, the executable's folder on Windows) and, during development, in `oidn/` of
the repository. This script fills such a folder from the official release package (verified
against its SHA-256) with the core library, the CPU device and the GPU devices for NVIDIA
(CUDA) and AMD (HIP) — Apple GPUs (Metal) on macOS — plus oneTBB and the license files.
Intel GPU (SYCL) support is left out.

    python3 tools/fetch_oidn.py                     # into ./oidn for this platform
    python3 tools/fetch_oidn.py --dest dist/rwf     # next to a packaged app
    python3 tools/fetch_oidn.py --platform windows  # another platform's files
"""

import argparse
import hashlib
import io
import platform
import re
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

VERSION = "2.5.1"
BASE = f"https://github.com/RenderKit/oidn/releases/download/v{VERSION}/"

PACKAGES = {
    "linux": (f"oidn-{VERSION}.x86_64.linux.tar.gz", "743c3e2aff8c220d5d70fe6cb970fb3d36f2702d2693c61d1d148e404cf37cd6"),
    "windows": (f"oidn-{VERSION}.x64.windows.zip", "f11f91bc072a5e3a564515724cb72ab8fcfbc445c84a84c197ff9b16cc01396f"),
    "macos-arm64": (f"oidn-{VERSION}.arm64.macos.tar.gz", "98e0aca8e7ab69e9f4f0191582a500fd8b0d9085d68662f18ab6469e934a6efd"),
    "macos-x86_64": (f"oidn-{VERSION}.x86_64.macos.tar.gz", "a4341639005a33ce0d944bd7c771ce34de1465be7eb3189b7c3b5b0064429832"),
}

# Files to keep (regular expressions on the file name) and where they go below --dest.
KEEP = {
    "linux": (r"lib(OpenImageDenoise(_core|_device_(cpu|cuda|hip))?|tbb)\.so(\.[0-9.]+)?$", "lib"),
    "windows": (r"(OpenImageDenoise(_core|_device_(cpu|cuda|hip))?|tbb12)\.dll$", ""),
    "macos-arm64": (r"lib(OpenImageDenoise(_core|_device_(cpu|metal))?|tbb)(\.[0-9.]+)?\.dylib$", "lib"),
    "macos-x86_64": (r"lib(OpenImageDenoise(_core|_device_(cpu|metal))?|tbb)(\.[0-9.]+)?\.dylib$", "lib"),
}
DOCS = ("LICENSE.txt", "third-party-programs.txt", "third-party-programs-oneTBB.txt")


def this_platform():
    s = platform.system()
    if s == "Linux":
        return "linux"
    if s == "Windows":
        return "windows"
    if s == "Darwin":
        return "macos-arm64" if platform.machine() in ("arm64", "aarch64") else "macos-x86_64"
    sys.exit(f"no OIDN package for {s}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--platform", choices=sorted(PACKAGES), default=None)
    ap.add_argument("--dest", default=str(Path(__file__).resolve().parent.parent / "oidn"))
    args = ap.parse_args()
    plat = args.platform or this_platform()
    name, sha = PACKAGES[plat]
    dest = Path(args.dest)

    print(f"downloading {BASE + name}")
    data = urllib.request.urlopen(BASE + name).read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != sha:
        sys.exit(f"SHA-256 mismatch: got {digest}, expected {sha}")

    pattern, sub = KEEP[plat]
    lib_dir = dest / sub
    lib_dir.mkdir(parents=True, exist_ok=True)
    doc_dir = dest / "oidn-license"
    doc_dir.mkdir(parents=True, exist_ok=True)
    kept = 0
    if name.endswith(".zip"):
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            for info in z.infolist():
                fname = info.filename.rsplit("/", 1)[-1]
                if re.fullmatch(pattern, fname):
                    (lib_dir / fname).write_bytes(z.read(info))
                    kept += 1
                elif fname in DOCS:
                    (doc_dir / fname).write_bytes(z.read(info))
    else:
        with tarfile.open(fileobj=io.BytesIO(data)) as t:
            links = []
            for m in t.getmembers():
                fname = m.name.rsplit("/", 1)[-1]
                if re.fullmatch(pattern, fname):
                    if m.issym():
                        links.append((fname, m.linkname))
                    elif m.isfile():
                        out = lib_dir / fname
                        out.write_bytes(t.extractfile(m).read())
                        out.chmod(0o755)
                        kept += 1
                elif fname in DOCS and m.isfile():
                    (doc_dir / fname).write_bytes(t.extractfile(m).read())
            for fname, target in links:
                # Only plain sibling links (e.g. libtbb.so.12 -> libtbb.so.12.19).
                if "/" in target:
                    continue
                link = lib_dir / fname
                if link.is_symlink() or link.exists():
                    link.unlink()
                link.symlink_to(target)
    total = sum(p.stat().st_size for p in lib_dir.iterdir() if p.is_file() and not p.is_symlink())
    print(f"{kept} libraries ({total / 1e6:.0f} MB) in {lib_dir}, licenses in {doc_dir}")


if __name__ == "__main__":
    main()
