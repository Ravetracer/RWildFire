## Downloads

| System | File | Start |
|--------|------|-------|
| Linux (x86-64, glibc 2.35+, e.g. Ubuntu 22.04, Debian 12, Fedora 36 and later) | `RWildFire-{version}-linux-x86_64.zip` | unpack, then double-click `RWildFire-{version}-x86_64.AppImage` |
| Windows 10/11 (64 bit) | `RWildFire-{version}-windows-x86_64.zip` | unpack, then double-click `rwf-app.exe` |

Nothing needs to be installed. Each archive contains the editor, the command line program,
the AI denoiser (Intel Open Image Denoise), the user manual (`RWildFire-Manual-{version}.pdf`)
and a short `README.txt`.

- **Windows**: the program is not signed, so SmartScreen may warn about an unknown publisher:
  click *More info*, then *Run anyway*.
- **Linux**: if the AppImage does not start from the file manager, make it executable
  (`chmod +x RWildFire-{version}-x86_64.AppImage`).
- A graphics card with Vulkan (Linux, Windows) or DirectX 12 (Windows) makes rendering fast;
  without one RWildFire falls back to its CPU renderer.
- Video export (MP4, MOV, WebM) needs [ffmpeg](https://ffmpeg.org) in the `PATH`.
