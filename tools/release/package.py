#!/usr/bin/env python3
# RWildFire - a fractal flame editor written in Rust
# Copyright (C) 2026 Christian Nielebock
# SPDX-License-Identifier: LGPL-3.0-or-later
"""Builds the ready-to-run release archives:

    dist/RWildFire-<version>-linux-x86_64.zip     RWildFire-<version>-x86_64.AppImage + manual
    dist/RWildFire-<version>-windows-x86_64.zip   rwf-app.exe, rwf-cli.exe, DLLs + manual

Both hold the user manual PDF, a README and the license files. Linux users unpack and start
the AppImage (the editor; "cli" as first argument runs the command line program), Windows
users unpack and start rwf-app.exe. Intel Open Image Denoise (the AI denoiser) is included.

    python3 tools/release/package.py                  # both platforms
    python3 tools/release/package.py --only linux     # one platform
    python3 tools/release/package.py --jobs 8         # build jobs (default: half the cores)

Requirements on the build machine (Linux):
- Docker: the Linux binaries are built in Debian 12 (tools/release/linux.Dockerfile), so they
  need glibc 2.35 or newer instead of the build machine's.
- The Rust target x86_64-pc-windows-gnu and mingw-w64 (x86_64-w64-mingw32-gcc, -windres).
- What tools/build_manual.py needs (the manual is rebuilt for the current version).
- Internet access for the OIDN download (tools/fetch_oidn.py) and appimagetool (both SHA-256
  checked; appimagetool is cached in target/release-tools).
"""

import argparse
import hashlib
import os
import re
import shutil
import stat
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DIST = ROOT / "dist"
IMAGE = "rwildfire-linux-build"
WIN_TARGET = "x86_64-pc-windows-gnu"
APPIMAGETOOL = (
    "https://github.com/AppImage/appimagetool/releases/download/1.9.0/appimagetool-x86_64.AppImage",
    "46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1",
)


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    return re.search(r'\[workspace\.package\][^\[]*?version\s*=\s*"([^"]+)"', text, re.S).group(1)


def run(cmd, **kw):
    print("+", " ".join(str(c) for c in cmd), flush=True)
    subprocess.run(cmd, check=True, **kw)


def build_linux(jobs: int) -> Path:
    run(["docker", "build", "-q", "-t", IMAGE, "-f", str(ROOT / "tools/release/linux.Dockerfile"), str(ROOT / "tools/release")])
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    target = ROOT / "target" / "docker-linux"
    target.mkdir(parents=True, exist_ok=True)
    # Created here so Docker does not create them owned by root.
    for d in ("registry", "git"):
        (cargo_home / d).mkdir(parents=True, exist_ok=True)
    run([
        "docker", "run", "--rm", f"--cpus={jobs}",
        "--user", f"{os.getuid()}:{os.getgid()}",
        "-e", "CARGO_HOME=/cargo", "-e", "HOME=/tmp",
        "-v", f"{cargo_home / 'registry'}:/cargo/registry",
        "-v", f"{cargo_home / 'git'}:/cargo/git",
        "-v", f"{ROOT}:/src", "-v", f"{target}:/target", "-w", "/src",
        IMAGE,
        "cargo", "build", "--release", "--locked", "-j", str(jobs), "--target-dir", "/target", "-p", "rwf-app", "-p", "rwf-cli",
    ])
    return target / "release"


def build_windows(jobs: int) -> Path:
    env = dict(os.environ, RUSTFLAGS="-C target-feature=+crt-static")
    run(["cargo", "build", "--release", "--locked", "-j", str(jobs), "--target", WIN_TARGET, "-p", "rwf-app", "-p", "rwf-cli"], cwd=ROOT, env=env)
    return ROOT / "target" / WIN_TARGET / "release"


def readme(plat: str, ver: str) -> str:
    if plat == "linux":
        start = f"""Start
-----
Unpack the archive anywhere and start RWildFire-{ver}-x86_64.AppImage (double-click it in your
file manager, or run it in a terminal). Nothing needs to be installed. If your file manager
does not start it, make it executable first: chmod +x RWildFire-{ver}-x86_64.AppImage

Requirements: a 64-bit Linux with glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and
later) and FUSE (standard on desktop distributions; without it, start the AppImage with
--appimage-extract-and-run), a graphics driver with Vulkan support (without it RWildFire uses
its much slower CPU renderer), ALSA for sound (dancing flames) and a desktop with
xdg-desktop-portal for the file dialogs (standard on GNOME, KDE, Xfce and most others).

The AppImage contains the editor, the command line program and the AI denoiser (Intel Open
Image Denoise)."""
        cli = f"./RWildFire-{ver}-x86_64.AppImage cli"
    else:
        start = """Start
-----
Unpack the archive anywhere (right-click > Extract all) and double-click rwf-app.exe. Nothing
needs to be installed.

Windows SmartScreen may warn that the app is from an unknown publisher, because it is not
signed. Click "More info", then "Run anyway".

Requirements: Windows 10 or 11 (64 bit) and a graphics card with DirectX 12 or Vulkan support
(without one RWildFire uses its much slower CPU renderer).

The AI denoiser (Intel Open Image Denoise) is in the OpenImageDenoise*.dll and tbb12.dll files
next to the program. Keep them in the same folder as rwf-app.exe."""
        cli = "rwf-cli.exe"
    if plat == "linux":
        contents = f"RWildFire-{ver}-x86_64.AppImage   the editor; with \"cli\" as first argument the command line program"
    else:
        contents = "rwf-app.exe                   the editor\nrwf-cli.exe                   the command line program (rendering, batches, scripts, ...)"
    return f"""RWildFire {ver}
{'=' * (10 + len(ver))}

A fractal flame editor and renderer, a port of JWildfire (https://github.com/thargor6/JWildfire)
to Rust with GPU rendering.

{start}

Contents
--------
{contents}
RWildFire-Manual-{ver}.pdf    the user manual (also: Help > User manual (PDF) in the app)
LICENSE, COPYING, NOTICE      license (LGPL 3.0 or later) and third-party notices
oidn-license/                 license of Intel Open Image Denoise (Apache 2.0)

Optional: video export
----------------------
MP4, MOV and WebM videos are written by ffmpeg (https://ffmpeg.org). Install it and make sure
it is in the PATH; without it, animations can still be saved as PNG frames.

Command line
------------
Run {cli} without arguments for a summary of its commands; the manual describes them.

Settings
--------
Preferences, your gradients and scripts are stored in {'~/.config/rwildfire' if plat == 'linux' else '%APPDATA%\\rwildfire'}.
"""


def appimagetool() -> Path:
    tool = ROOT / "target" / "release-tools" / "appimagetool-x86_64.AppImage"
    if not tool.exists():
        url, sha = APPIMAGETOOL
        print(f"downloading {url}")
        data = urllib.request.urlopen(url).read()
        if hashlib.sha256(data).hexdigest() != sha:
            sys.exit("appimagetool: SHA-256 mismatch")
        tool.parent.mkdir(parents=True, exist_ok=True)
        tool.write_bytes(data)
        tool.chmod(0o755)
    return tool


def build_appimage(bin_dir: Path, ver: str, manual: Path, out: Path) -> Path:
    """AppDir: AppRun, desktop file and icon at the top, programs in usr/bin, OIDN in usr/lib,
    manual and licenses in usr/share/doc/rwildfire."""
    appdir = DIST / "AppDir"
    if appdir.exists():
        shutil.rmtree(appdir)
    (appdir / "usr" / "bin").mkdir(parents=True)
    for b in ("rwf-app", "rwf-cli"):
        shutil.copy2(bin_dir / b, appdir / "usr" / "bin" / b)
        run(["strip", str(appdir / "usr" / "bin" / b)])
    shutil.copy2(ROOT / "tools/release/AppRun", appdir / "AppRun")
    (appdir / "AppRun").chmod(0o755)
    shutil.copy2(ROOT / "tools/release/rwildfire.desktop", appdir / "rwildfire.desktop")
    shutil.copy2(ROOT / "crates/rwf-app/assets/icon.png", appdir / "rwildfire.png")
    run([sys.executable, str(ROOT / "tools/fetch_oidn.py"), "--platform", "linux", "--dest", str(appdir / "usr")])
    doc = appdir / "usr" / "share" / "doc" / "rwildfire"
    doc.mkdir(parents=True)
    shutil.move(str(appdir / "usr" / "oidn-license"), doc / "oidn-license")
    for f in ("LICENSE", "COPYING", "NOTICE"):
        shutil.copy2(ROOT / f, doc / f)
    shutil.copy2(manual, doc / manual.name)
    image = out / f"RWildFire-{ver}-x86_64.AppImage"
    env = dict(os.environ, ARCH="x86_64", APPIMAGE_EXTRACT_AND_RUN="1")
    run([str(appimagetool()), "--no-appstream", str(appdir), str(image)], env=env)
    shutil.copytree(doc / "oidn-license", out / "oidn-license")
    shutil.rmtree(appdir)
    return image


def package(plat: str, bin_dir: Path, ver: str, manual: Path) -> Path:
    name = f"RWildFire-{ver}-{plat}-x86_64"
    stage = DIST / name
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    if plat == "linux":
        build_appimage(bin_dir, ver, manual, stage)
    else:
        for b in ("rwf-app", "rwf-cli"):
            shutil.copy2(bin_dir / f"{b}.exe", stage / f"{b}.exe")
        run([sys.executable, str(ROOT / "tools/fetch_oidn.py"), "--platform", plat, "--dest", str(stage)])
    for f in ("LICENSE", "COPYING", "NOTICE"):
        shutil.copy2(ROOT / f, stage / f)
    shutil.copy2(manual, stage / manual.name)
    text = readme(plat, ver)
    (stage / "README.txt").write_text(text.replace("\n", "\r\n") if plat == "windows" else text, encoding="utf-8")

    archive = DIST / f"{name}.zip"
    archive.unlink(missing_ok=True)
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for p in sorted(stage.rglob("*")):
            if p.is_dir():
                continue
            info = zipfile.ZipInfo.from_file(p, f"{name}/{p.relative_to(stage).as_posix()}")
            mode = 0o755 if os.access(p, os.X_OK) or p.suffix in (".exe", ".AppImage") else 0o644
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            with open(p, "rb") as fh:
                z.writestr(info, fh.read(), compresslevel=9)
    print(f"{archive} ({archive.stat().st_size / 1e6:.0f} MB)")
    return archive


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", choices=["linux", "windows"])
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    args = ap.parse_args()
    ver = version()
    DIST.mkdir(exist_ok=True)
    run([sys.executable, str(ROOT / "tools/build_manual.py")])
    manual = ROOT / "docs" / "manual" / "build" / f"RWildFire-Manual-{ver}.pdf"
    out = []
    if args.only in (None, "linux"):
        out.append(package("linux", build_linux(args.jobs), ver, manual))
    if args.only in (None, "windows"):
        out.append(package("windows", build_windows(args.jobs), ver, manual))
    print("\n".join(str(p) for p in out))


if __name__ == "__main__":
    main()
