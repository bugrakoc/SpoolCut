# SpoolCut on Windows 11

The target is 64-bit Windows 11. Use the **UCRT64** shell that ships with MSYS2.
The MSYS2 Rust package reports the host as `x86_64-pc-windows-gnu`. It uses the
UCRT64 toolchain and native import libraries for GTK and libmpv. Build and test
with the packages from the same MSYS2 prefix. Mixing packages from another MSYS2
prefix or a rustup/MSVC toolchain can select incompatible libraries.

## Build and test

In UCRT64, update MSYS2 with `pacman -Syu`. If it asks you to close the shell,
reopen UCRT64 and run the update again. Then install the build dependencies.

```sh
pacman -S --needed git gnupg tar zstd \
  mingw-w64-ucrt-x86_64-{gcc,rust,pkgconf,gtk4,mpv,ffmpeg,python,adwaita-icon-theme,hicolor-icon-theme}
cargo test --locked --features desktop
SPOOLCUT_PREVIEW=software GSK_RENDERER=cairo GTK_A11Y=none \
  cargo test --locked --features desktop --bin spoolcut-desktop -- --ignored
cargo build --locked --release --features desktop
./target/release/spoolcut-desktop.exe
```

SpoolCut checks `SPOOLCUT_FFMPEG` and `SPOOLCUT_FFPROBE` first. Set each to a
full executable path to use a custom installation. It then looks beside the
running executable, in a sibling `bin` subdirectory, and on PATH. An invalid
explicit path results in an error.

The Windows desktop defaults to libmpv's software renderer. On systems where
GTK creates a WGL context, set `SPOOLCUT_PREVIEW=opengl` to try OpenGL rendering.
The editor falls back to software if GL setup or drawing fails. The tooltip
over the preview shows the active renderer and any OpenGL fallback reason.
OpenGL rendering and hardware video
decoding are separate. The preview currently uses CPU video decoding. To
require OpenGL during a smoke test, also set `SPOOLCUT_REQUIRE_OPENGL=1`
and run the ignored desktop test without the Cairo override. Linux defaults to
its EGL-based preview and accepts `SPOOLCUT_PREVIEW=software` for testing.

Projects use the same JSON format. Relative sources resolve from the project
directory. Files on a second drive have absolute paths. Use **Relink…** when
moving a source or opening a project on a system that cannot resolve its paths.
A preview can hold a source open on Windows. Close the project or the app to
release the file before moving it if Windows reports a sharing violation.

Exports stage in the destination directory. Windows publishes through a
same-volume Unicode rename without replacement or copy flags. This avoids
requiring hard links on Windows, including on exFAT. Real exFAT testing is
still pending. Linux publishes via a same-filesystem hard link. Either system
keeps an existing file if another application creates it before publication.

## Portable package and corresponding source

After the release build, run the following in UCRT64.

```sh
python packaging/windows/package.py "$(cygpath -m "$MINGW_PREFIX")" dist-windows
```

The package copies the executable, FFmpeg and ffprobe, their native DLL
dependencies, and the GTK resources. It records the installed package
versions, license IDs, binary hashes and source archive URLs. The companion
source archive contains SpoolCut's exact Git revision, its locked Cargo crate
sources, and signed MSYS2 source archives for the bundled components. The
MSYS2 archives include upstream sources and the matching build recipes and
patches. To use the vendored crates, extract the SpoolCut source tar alongside
`vendor` and copy `vendor-config.toml` to `.cargo/config.toml`.

CI builds the package on pull requests and version tags. Keep the companion
source archive available wherever the Windows binary is redistributed. GitHub
Actions artifacts expire, so copy both archives into any published release.

## Clean Windows 11 VM checklist

Download the Windows artifact from the latest passing run for the revision you
want to test. Open the [Windows pull request checks](https://github.com/bugrakoc/SpoolCut/pull/3/checks)
to find the run. GitHub wraps the portable ZIP and its SHA-256
checksum in an outer ZIP. Extract both layers into a directory with spaces
and a non-ASCII character, then double-click `SpoolCut/bin/SpoolCut.exe` on
a VM without MSYS2 or FFmpeg installed.

1. Check the editor, Explorer and taskbar icons, window buttons and native file
   pickers at 100%, 150% and 200% scaling. Check that the native title is followed
   by a project toolbar. Move the window between displays with different scales
   if available.
2. Import clips with spaces and Unicode in their filenames using both the file
   picker and a drop of multiple files from Explorer onto the media bin. Reorder,
   trim, split and scrub. Try Space, arrow keys, copy/paste and undo/redo. Play through
   joins with audio enabled and rapidly switch sources while paused.
3. Save and reopen the project. Close the app, move one source to a different
   drive, reopen and check the red **Unavailable · Relink source** label. Relink
   the file and check that its media details return.
4. Export H.264/AAC and HEVC/AAC sequences. Check that another player shows the
   intended joins and synchronized audio. Cancel an export and check that no
   output or staging folder remains. Repeat to an exFAT volume, including with
   a destination that already exists.
5. Open and close the editor several times. To try WGL, run
   `$env:SPOOLCUT_PREVIEW = 'opengl'` in PowerShell and then launch
   `& .\SpoolCut\bin\SpoolCut.exe` from that shell. Repeat playback and seeking,
   then record the full preview tooltip and whether the VM exposes accelerated
   graphics. If the VM falls back to software, WGL still needs validation on a
   Windows system with working OpenGL drivers.

On Fedora, repeat the preview, editing and export checks in a Wayland session.

### VM results for the first portable build, 25 November 2026

| Check | Result |
| --- | --- |
| Windows chrome and DPI | Icons, native controls and dialogs scaled. The editor stayed at 100% when Windows was set to 150%, then scaled at 200%. Multiple displays were unavailable. |
| Editing and preview | Unicode filenames, edits, shortcuts, playback through joins and paused source switches worked. Playback stuttered occasionally. |
| Persistence and relink | Save, reopen and relink after renaming a missing source worked. A second drive was unavailable. The missing-source label was easy to overlook. |
| Export | HEVC/AAC played as audio only in Windows Media Player Legacy and failed in Fedora players. VLC aborted. H.264/AAC playback, cancellation and exFAT checks remain pending. |
| Renderer | Software preview worked both via Explorer and with the PowerShell OpenGL override. Hardware WGL remains unverified. |
| Explorer drag-and-drop and title bar | Explorer drops did not import files. The native title and the project toolbar both showed the project name. |

The bundled GTK 4.24 Win32 backend uses whole-number scaling. The fractional
monitor scaling work first appeared in the GTK 4.25.0 development release.
Retest 150% and mixed-DPI displays once MSYS2 ships a stable build with
[GTK's Win32 change](https://gitlab.gnome.org/GNOME/gtk/-/merge_requests/10405).

The failed sample contained HEVC Main 10 video at 3840×2160 and 25 fps plus
AAC stereo. FFmpeg software-decoded all 870 frames and the audio without
errors, through both joins. Neither track was marked as default. New exports
mark the single video and audio tracks as defaults. Test playback again on
the affected systems to determine whether this resolves their failures.

For a failed export, keep the MKV and project JSON. On a system with FFmpeg,
these commands report the tracks and attempt to decode both streams.

```sh
ffprobe -v error -show_streams -show_format -of json SpoolCut.mkv
ffmpeg -hide_banner -v error -xerror -i SpoolCut.mkv -map 0:v:0 -map 0:a:0 -f null -
```
