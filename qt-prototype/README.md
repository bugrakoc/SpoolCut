# SpoolCut Qt desktop

The Qt Widgets migration build on `cpp-qt` uses the same lossless C++ core
as the CLI. It loads existing SpoolCut projects, imports and indexes local
MP4/MOV sources, and exports compatible edits to Matroska using stream copy.
The Rust reference remains available at `39cb4ee`.

The version appears in **Help → About SpoolCut**, the CLI's `--version`,
and the AppImage and ZIP filenames.

## Build

Use Qt 6.8 or newer, libmpv with API 2.3 or newer, GCC or Clang with C++20,
CMake, Ninja, pkg-config and FFmpeg/ffprobe. On Fedora use a libmpv and FFmpeg
build with H.264 and HEVC support. The tests also require x264 and x265.
On Windows use a single MSYS2 UCRT64 prefix for GCC, Qt, libmpv and FFmpeg.

```sh
cmake -S qt-prototype -B build-qt -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build-qt
ctest --test-dir build-qt -V
```

CTest generates H.264 and HEVC Main 10 input with AAC audio and opens the
widgets. Run it in a graphical session, under Xvfb, or under Weston. It uses
libmpv's null audio output to verify the decoded audio path in CI.

Launch `build-qt/spoolcut-qt` or `build-qt/spoolcut-qt.exe`. Linux tries OpenGL
by default. Set `SPOOLCUT_QT_PREVIEW=software` to force CPU rendering. Windows
starts with software rendering. Set `SPOOLCUT_QT_PREVIEW=opengl` to test OpenGL
explicitly. The status bar and preview tooltip show the selected renderer and
any fallback reason. Both renderers use CPU video decoding.

## Editing a project

- Use **Import media** or drop MP4/MOV files onto the window. Double-click a
  bin item to append it or drag it to a position on the timeline.
- Click the ruler to snap to a safe keyframe. Drag the ruler to scrub. Zoom out past Fit to see future space on the timeline.
  Drag a clip to reorder it or drag an edge to trim. Press **S** to split
  near the playhead at a safe point. **Space** toggles playback, and the
  left and right arrows jump to safe keyframes.
- Use **Open project** and **Save project** with the existing SpoolCut JSON
  files. Select an unavailable source and choose **Relink source** to locate
  a replacement. The affected clips stay visible on the timeline. Relinking
  validates existing cuts and restores timeline timing.
- Use **Export Matroska** to write a new MKV. SpoolCut rejects incompatible
  streams and existing destinations. Use **Cancel operation** during indexing
  or export.

## Windows portable ZIP

Extract the ZIP and run `SpoolCut Qt/bin/SpoolCutQt.exe`. The
package contains its own Qt platform plugin, libmpv and media tools. The
bundled CLI is at `SpoolCut Qt/bin/spoolcut.exe` and supports `inspect`,
`plan` and `export`. Keep the
matching source archive available alongside the binary if redistributing.

## Manual checks

1. Open two clips with different lengths. Include an H.264 and a Main 10 HEVC
   source with audible AAC audio. Try paths with spaces and non-ASCII characters.
   Check proportional widths, timeline zoom, fit and drag scrubbing.
2. Play through a join, pause, resume and rapidly scrub across the join in both
   directions. Resize the window and both splitters during playback. Clear all
   sources, open clips again and close and reopen the app several times.
3. Check the renderer tooltip. Repeat with the software override and compare
   visible frames, audio, input latency and stutter with default selection.
4. On Fedora KDE, confirm the session is Wayland and verify the taskbar icon,
   titlebar controls and native picker. On Windows 11, verify Explorer and
   taskbar icons, native controls, file picker and drag-and-drop. At 100%, 150%
   and 200%, check layout and preview sharpness. Move between monitors with
   different scale factors if available.
5. For a GTK comparison, use the same session, resolution, scaling, two clips
   and renderer class. Time from launch to a visible window, record process CPU
   and memory during 10 seconds of idle and continuous playback, and compare
   timeline input with the resulting frame. Note renderer/driver and package
   extraction or launch steps.

## Fedora AppImage

Make the versioned AppImage executable, then open it on Fedora 44 or newer.
The AppImage uses the host graphics drivers and installs a per-user launcher
for the Qt build. On KDE it requests native file dialogs through the XDG
Desktop Portal. For a 0.2.1 image, the fallback invocation is

```sh
./SpoolCut-0.2.1-Qt-Fedora-x86_64.AppImage --appimage-extract-and-run
./SpoolCut-0.2.1-Qt-Fedora-x86_64.AppImage --cli --version
```

Keep the matching Linux source archive available if redistributing the AppImage.
