# SpoolCut Qt feasibility prototype

This GPLv3 preview editor is an experiment on the `cpp-qt` branch. Use the
Rust editor for projects, precision cuts and lossless export. The prototype
opens local MP4/MOV footage for playback and shows full sources on a
proportional timeline. It reads media metadata with ffprobe and uses libmpv
for decoding. The prototype never writes an output video or a project.

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

Launch `build-qt/spoolcut-qt` or `build-qt/spoolcut-qt.exe`. Set
`SPOOLCUT_QT_PREVIEW=software` before launch to force CPU rendering. The status
bar initially reports the renderer, and hovering the preview shows the renderer
and any OpenGL fallback reason. Both renderers currently use CPU video decoding.

## Windows portable ZIP

Extract the ZIP and run `SpoolCut Qt Prototype/bin/SpoolCutQt.exe`. The
package contains its own Qt platform plugin, libmpv and media tools. Keep the
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
