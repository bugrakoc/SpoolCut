# SpoolCut

<img src="assets/io.github.bugrakoc.SpoolCut.png" alt="SpoolCut icon" width="96" height="96">

SpoolCut is a free, open-source desktop editor for trimming and joining
compatible video. It copies the compressed video and audio from your
recordings, so export avoids re-encoding time and generation loss.
SpoolCut focuses on a single, gapless timeline with keyframe-aligned cuts.
Effects, transitions and multitrack compositing are outside that workflow.

> **Help test SpoolCut 0.4.x.** A five-minute run with two short clips helps
> us check more hardware and recording formats. Successful and partial
> results are welcome. See the [testing guide](docs/TESTING.md) and
> [compatibility matrix](docs/COMPATIBILITY.md).

## Download and run

Get an AppImage for Fedora 44 x86-64 or a portable ZIP for Windows 11
x86-64 from [GitHub Releases](https://github.com/bugrakoc/SpoolCut/releases).
Use the 0.4.x candidate for public testing when its packages are available.
The packages include Qt, libmpv, FFmpeg and FFprobe. Other Linux
distributions are welcome to test the AppImage or build from source.

- **Fedora.** Make the AppImage executable and open it. It adds a SpoolCut
  application launcher on first run. If you move it, run it from the new
  location to update the launcher. Use `--appimage-extract-and-run` if
  FUSE is unavailable.
- **Windows.** Extract the ZIP and open `SpoolCut/bin/SpoolCut.exe`. Keep
  the contents of `bin` together. See the [Windows guide](docs/WINDOWS.md).

Linux defaults to OpenGL with a software fallback. To force software,
run your downloaded AppImage with `SPOOLCUT_QT_PREVIEW=software`. For
example, with the 0.4.0 candidate:

```sh
SPOOLCUT_QT_PREVIEW=software ./SpoolCut-0.4.0-rc.1-Fedora-x86_64.AppImage
```

Windows defaults to software preview. The [Windows guide](docs/WINDOWS.md#portable-package)
shows how to opt into OpenGL. Both modes decode footage on the CPU.
The preview tooltip and **Help → System Information** show the active
renderer after you play a clip.

## Edit and export

1. Import MP4 or MOV files. Drag a bin item to the timeline or double-click
   it to append it. Drag clip bodies to reorder them and drag their edges to
   trim. SpoolCut keeps the timeline gapless.
2. Click the ruler to jump to a safe keyframe or drag the ruler to scrub by
   frame. Scroll to pan, use **−**, **+**, or Ctrl+scroll to zoom, and click
   **Fit** to show the whole sequence.
3. Press **S** to split at a safe boundary near the playhead. Use **Space**
   to play and pause, and **←** or **→** to jump between safe keyframes.
   Select a clip to cut or copy it. Paste inserts at the boundary nearest
   the playhead. Use Ctrl+Z to undo and Ctrl+Shift+Z to redo.
4. Save your sequence as a SpoolCut JSON project. If a source moves, select
   it in the bin and choose **Relink source**. SpoolCut checks the existing
   cuts against the replacement before restoring playback.
5. Choose **Export Matroska** to write a new MKV. The status bar shows
   planning, copying, joining and verification. **Cancel operation** stops
   the active export or media inspection.

### Supported footage

- MP4 or MOV with one H.264 (`avc1`) or HEVC (`hvc1`) video track and one
  AAC-LC audio track. Clips in a sequence must share stream properties and
  decoder configuration. Extra tracks such as subtitles and timecode are
  omitted. Other video tags, additional audio tracks and stream side data
  such as a display rotation matrix are unsupported.
- Video cuts use closed IDRs that partition presentation order. The out
  point is exclusive. Source end is usable when the final displayed frame
  and audio range pass validation. Discarded, corrupt or otherwise unsafe
  packets can disqualify a cut.
- Export copies packets and keeps complete AAC frames inside each video
  range. Audio may differ briefly at joins when cut points fall between AAC
  frames. MKV packet timestamps round to milliseconds.
- SpoolCut verifies the staged output before publication and preserves
  existing destinations. Allow space for the staged clips and joined file.
  Linux requires hard-link support in the destination folder. Windows uses
  a same-volume rename with replacement disabled.

## Command line

Use `spoolcut` from a CMake build, launch the AppImage with `--cli` on
Fedora, or run `SpoolCut/bin/spoolcut-cli.exe` on Windows.

```sh
spoolcut inspect camera-a.mp4
spoolcut inspect camera-b.mp4
spoolcut plan timeline.spoolcut.json
spoolcut export timeline.spoolcut.json output.mkv
```

A project stores source paths and ordered clip boundaries in each source's
video time base. Relative paths resolve from the project directory. Use
the integer values from `inspect` as cut boundaries. The PTS values below
are examples.

```json
{
  "sources": {"a": "camera-a.mp4", "b": "camera-b.mp4"},
  "clips": [
    {"source": "a", "in_pts": 0, "out_pts": 8192},
    {"source": "b", "in_pts": 8192, "out_pts": 16384}
  ]
}
```

`plan` reports the effective timeline in microseconds. Press Ctrl+C to
cancel. SpoolCut looks for FFmpeg and ffprobe alongside the executable and
then on PATH. Set `SPOOLCUT_FFMPEG` and `SPOOLCUT_FFPROBE` to use specific
executables.

## Build and test

Use CMake 3.25+, C++20 GCC or Clang, Qt 6.8+, Ninja or another CMake
generator, pkg-config and libmpv API 2.3+. Provide FFmpeg and ffprobe with
H.264 and HEVC. Tests also require Python 3 and the x264, x265 and AAC
encoders. Example Debian development packages are `g++ cmake ninja-build
pkg-config qt6-base-dev qt6-wayland libmpv-dev ffmpeg python3`.
For Fedora use `gcc-c++ cmake ninja-build pkgconf-pkg-config qt6-qtbase-devel
qt6-qtwayland mpv-devel python3` with codec-enabled media tools.

```sh
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
./build/spoolcut-desktop
ctest --test-dir build --output-on-failure
```

Set `-DSPOOLCUT_BUILD_DESKTOP=OFF` for a CLI-only build with Qt Core.
Run the desktop tests under a graphical session, Xvfb, or Weston. Use
`-DSPOOLCUT_RUST_REFERENCE=/path/to/reference/spoolcut` to compare output
with the pinned [Rust/GTK reference](https://github.com/bugrakoc/SpoolCut/tree/39cb4eecef8914f187871aac2187f8f023d14e0d).
See the [migration evidence](docs/CPP_QT_STATUS.md) for automated and
maintainer results.

## Reports and privacy

Share successful, partial or unsuccessful compatibility results through
the [testing guide](docs/TESTING.md#submit-a-compatibility-report).
File reproducible problems in [GitHub Issues](https://github.com/bugrakoc/SpoolCut/issues/new/choose).
The [contributor guide](CONTRIBUTING.md) explains what helps a report.

SpoolCut has no automatic telemetry. **Help → System Information** reads
local diagnostic details you can copy and share voluntarily. See the
[privacy practices](docs/PRIVACY.md).

## License and sources

SpoolCut is distributed under [GPLv3](LICENSE). The matching Linux and
Windows source archives include the exact SpoolCut revision, third-party
package inventories and corresponding source and build recipes. Binary
packages include notices and a source revision. Download the matching
archive from the same release and keep it available when redistributing
a binary. See the [packaging guide](docs/PACKAGING.md).
