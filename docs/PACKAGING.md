# Build and redistribute SpoolCut packages

Build from a clean, committed tree using the root CMake project.
[`VERSION`](../VERSION) supplies the desktop and CLI versions, Windows
executable metadata and archive names. The
[desktop workflow](../.github/workflows/desktop.yml) contains the
production build commands and smoke tests. The
[core workflow](../.github/workflows/core.yml) tests export and the
pinned reference separately.

| Platform | Build environment | Binary | Corresponding source |
| --- | --- | --- | --- |
| Fedora 44 x86-64 | Debian Trixie x86-64 with the matching Debian source repositories enabled | `SpoolCut-0.3.0-Fedora-x86_64.AppImage` | `SpoolCut-0.3.0-Linux-sources.tar.zst` |
| Windows 11 x86-64 | MSYS2 UCRT64 using matching installed and repository package versions | `SpoolCut-0.3.0-Windows-x86_64.zip` | `SpoolCut-0.3.0-Windows-sources.tar.zst` |

Run CMake with `-DBUILD_TESTING=OFF` to build the production executables.
On Linux add `qt6-xdgdesktopportal-platformtheme`, `squashfs-tools`,
`xz-utils`, `zstd` and Debian `deb-src` entries to the normal build
prerequisites. Run the package scripts from the repository root.

```sh
python3 packaging/linux/package-linux.py build/spoolcut-desktop dist-linux
```

On Windows use the UCRT64 build dependencies in the
[Windows guide](WINDOWS.md), plus `gnupg`, `tar` and `zstd` in MSYS2.

```bash
python packaging/windows/package.py "$(cygpath -m "$MINGW_PREFIX")" build/spoolcut-desktop.exe dist-windows
```

The job for each platform runs the relocated desktop and CLI against AVC
and HEVC Main 10 footage with AAC. It also checks the bundle version,
source revision and checksums. For an AppImage the host supplies the
kernel, glibc and graphics libraries, including Wayland on KDE. On Windows
the OS supplies its system libraries and the ZIP carries the UCRT64 DLL
dependencies. Extract the complete ZIP before moving or running it.

## Corresponding source and notices

- Both source archives include the tracked SpoolCut tree and its SHA. The
  distribution recipes are in `packaging/` and the GitHub Actions workflow.
  The archives include the configuration of the FFmpeg build. Packaging
  rejects FFmpeg built with `--enable-nonfree`.
- Linux uses the exact source package and version identified by the Debian
  package database. The archive contains the matching Debian sources with
  packaging metadata and patches. The AppImage contains their copyright
  notices. For the pinned AppImage runtime, the archive includes the
  upstream build recipe, libfuse and squashfuse sources, and exact Alpine
  revisions, upstream archives and patches for musl, zlib, zstd and
  mimalloc. The pinned [upstream build log](https://github.com/AppImage/type2-runtime/actions/runs/39320578745/job/118293253809)
  records the linked package versions.
- Windows lists the exact MSYS2 binary packages and SHA-256 digests in
  `MSYS2-packages.tsv`. The companion archive includes corresponding
  upstream source, recipes and patches from the MSYS2 source packages.
  The package script verifies the source archive signatures using the
  MSYS2 keyring. Third-party notices are under `share/licenses`.
- Qt and libmpv are dynamically linked. The AppImage can be extracted to
  inspect or replace its libraries. The Windows ZIP exposes them as DLLs.
  See [Qt's open-source redistribution guidance](https://www.qt.io/development/open-source-lgpl-obligations).

Publish each binary alongside its corresponding source archive and both
checksums. Keep those files available together for redistribution under
the terms of [GPLv3](../LICENSE) and the component licenses.
