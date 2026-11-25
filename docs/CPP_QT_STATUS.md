# C++ and Qt migration status

## Maintainer acceptance of Qt 0.2.2

The maintainer tested Qt 0.2.2 on Fedora KDE Wayland with NVIDIA OpenGL and
Windows 11 under QEMU. The reviewed branch head was
[`f6f1a5a`](https://github.com/bugrakoc/SpoolCut/commit/f6f1a5a4e1fa332c34630eb934a1cea078982de8).

| Check | Fedora KDE Wayland | Windows 11 / QEMU |
| --- | --- | --- |
| Export and playback | The original project with the 46.795 versus 46.785 second verification discrepancy exported and played correctly. | The original HEVC Main 10 export played in VLC. |
| Preview | OpenGL played across joins without observed regressions. Forced software rendered correctly without fading to white. | Software was the default and played HEVC Main 10. Playback across joins worked. |
| Missing media | Reopening a project after a rename and reaching a renamed source during playback both entered missing-media mode. Bin and timeline styling matched. The preview still showed the ordinary empty message, and relinking caused an apparent restart. | Playback detected the missing file and relinking recovered in place. |
| Editing and persistence | Trim, split, reorder, scrub and undo/redo worked. | Project save/reopen, editing and playback worked. |
| Cancellation and destinations | | Export cancellation cleaned staging files. Existing output survived. Export to the VirtIO shared folder on Z: worked. |
| Earlier desktop checks | Native KDE picker, icon and window controls worked. | Native desktop integration and display scaling at 100%, 150% and 200% worked. |

The VirtIO result covers that shared folder. exFAT, physical Windows OpenGL,
mixed-DPI monitor movement and further hardware benchmarking are deferred.

### Relinking and native window lifetime

Relinking updates the editor in place. Qt 6.4 and newer can recreate a shown
top-level native window when the first `QOpenGLWidget` is added. In 0.2.2, a
project opened with a missing clip could have no preview renderer until relinking
made its first frame available. That can replace the native surface while the
application process and project remain live.
See [Qt's QOpenGLWidget documentation](https://doc.qt.io/qt-6/qopenglwidget.html).

The GL widget now belongs to the initial window hierarchy when OpenGL is
selected. libmpv initialization remains deferred until the first video.
Missing media shows a placeholder using the timeline's fill, and the existing
preview is reused once timing is restored. Windows continues to default to
software. The final Fedora KDE retest should confirm the observed relinking
symptom is gone with the maintainer's original project and NVIDIA configuration.

## A. Rust reference

The migration branch started at `be176aa096275d502020b9cfc9d25f2afa14a5bb`.
The latest `main` at audit was
[`39cb4eecef8914f187871aac2187f8f023d14e0d`](https://github.com/bugrakoc/SpoolCut/tree/39cb4eecef8914f187871aac2187f8f023d14e0d),
the merged Windows PR #3. This commit was already an ancestor of the branch.

| Area | Reference contract and regression coverage |
| --- | --- |
| CLI | `inspect SOURCE`, `plan PROJECT.json`, `export PROJECT.json OUTPUT.mkv`. Inspection and planning emit JSON. Errors go to stderr with exit 1. Ctrl+C cancellation exits 130. |
| Projects | Strict JSON with a source ID-to-path map and ordered clips carrying signed integer `in_pts`/`out_pts`. Paths are relative to the project directory when possible. Sources on another Windows drive use absolute paths. Project save stages then replaces the project. Locked Windows projects retain their prior contents. |
| Media and time | One non-cover H.264 `avc1` or HEVC `hvc1` video track and one AAC-LC track, with matching stream parameters and decoder configuration across edited sources. Packet-level closed-IDR indexing checks presentation order and cut eligibility. Uses integer rational comparisons, exclusive out PTS, video end as a conditional boundary and only whole AAC frames. Discard-marked packets, unsupported stream side data and unsafe ranges fail explicitly. |
| Export | Copy exact input packet ranges through fragmented MP4 staging with a common video time base, then concat into MKV with default audio and video dispositions. Compare timestamps, counts, durations and HEVC configuration before publication. Regression tests use synthetic AVC, HEVC, B-frames, fractional and variable frame rates, mixed source time bases and A–B–A sequences. Tests compare payload hashes where framing permits, decoded frame order and A/V timing. |
| Process and files | Explicit FFmpeg/ffprobe overrides, application-relative lookup and PATH fallback. Cancel and reap the subprocess before cleanup. Check source identity and metadata before publication. Stage on the output filesystem. Linux uses a hard link and Windows uses a same-volume Unicode rename without replacement. Tests cover cancellation, destination races, existing files, path identity, Unicode and locked projects on Windows. |
| Desktop | Native/project chrome, media bin import and Explorer drop path, resolution/codec/frame rate, split layout and preview. Timeline fits, zooms, pans, scrubs, snaps, reorders, trims and splits. Handles project reopening, relinking, dirty state, editing history, keyboard controls and export progress/cancellation. GTK smoke covers editing, rendered pixels and rapid seeks, repeated open/close, persistence and export. |
| Graphics | GTK GLArea/GL uses EGL symbol lookup on Linux and WGL on Windows. Native Windows defaults to software, supports an OpenGL override and can fall back. Preview decodes in software and can sound slightly different from exported AAC at unaligned joins. |
| Distribution | Existing Windows UCRT64 packaging walks native DLL imports, bundles GTK resources, validates source signatures, and smokes the portable ZIP with a clean PATH. Release packaging is gated to PRs/tags. Existing Linux CI uses Debian/Weston. The Rust AppImage launcher has tests for XDG registration and relocation. |

[Reference CI run 45576403374](https://github.com/bugrakoc/SpoolCut/actions/runs/45576403374)
passed Linux format/lint, Rust tests and a Weston desktop smoke, plus native
Windows Rust tests, forced software and OpenGL selection/fallback. Packaging
was skipped on that main push. The [earlier portable-build run](https://github.com/bugrakoc/SpoolCut/actions/runs/45530147921)
exercised relocated Windows packaging.

The [archived Rust Windows guide](https://github.com/bugrakoc/SpoolCut/blob/rust-gtk-baseline/docs/WINDOWS.md) records the baseline Windows results.
They include 150% editor scaling, occasional software preview stutter and a
HEVC Main 10 sample that FFmpeg decoded but some players failed to play. The
latest Rust revision marks both output tracks as default. The effect on the
affected players still needs testing. The earlier Explorer drop and duplicate
project-title fixes also need confirmation in the corrected package. Windows
accelerated OpenGL, mixed monitor scaling, removable/exFAT destination semantics
and acceleration and desktop integration on Fedora KDE need physical or VM checks.

## Toolchain and distribution boundaries

The prototype requires Qt 6.8 or newer and C++20. Linux uses GCC, Qt Widgets,
Qt OpenGL Widgets, libmpv and a codec-enabled FFmpeg. Windows uses one MSYS2
UCRT64 dependency prefix for Qt, GCC, libmpv and FFmpeg. Linux ASan/UBSan checks
the prototype's ownership and teardown. File selection, process startup and
OpenGL dispatch use Qt interfaces. Timing on this preview timeline is in
display seconds. The existing integer project and export algorithms stay in
the Rust reference for subsequent differential testing.

The repository and prototypes carry GPLv3. Use dynamically linked Qt modules
available under open-source terms. The package inventory needs to identify the
exact set of binaries, licenses and corresponding sources with build recipes
and patches, including the FFmpeg build configuration and the AppImage runtime
when used. Keep companion corresponding-source artifacts available with any
redistributed package. On macOS the future native work belongs at the file
identity/publication, process lookup, OpenGL context, Qt plugin and bundle
boundaries. Signing and notarization belong in packaging.

## B. Prototype and results

The build in [qt-prototype](https://github.com/bugrakoc/SpoolCut/blob/f6f1a5a4e1fa332c34630eb934a1cea078982de8/qt-prototype/README.md) is independent of the
Rust reference. It decodes H.264 and HEVC through libmpv, opens files through the
platform picker or drag-and-drop, and shows a media bin and proportional
QPainter timeline with a playhead, zoom and fit. QProcess probes one file at a
time and is reaped on close. OpenGL uses Qt's current context, widget
framebuffer and device-pixel viewport. Qt restores its framebuffer after mpv
renders. The software fallback uses an aligned RGB buffer and a regular
QWidget. Both preview paths use CPU decoding. Project persistence, editing and lossless export use the Rust reference.

| Test | Evidence |
| --- | --- |
| Rust core, local Debian 13 | Rust 1.100.0. Two core unit tests and seven stream-copy integration tests passed. |
| Qt release, hosted Debian 13 | Qt 6.8.2 and GCC 14.2.0, libmpv 0.40.0 (API 2.5) and FFmpeg 7.1.5. CTest passed with forced software and OpenGL llvmpipe on X11 and Weston Wayland. It checked decoded AVC and Main 10 HEVC frames, AAC channel output, playback, pause, seeks, source changes, resize, close during probing and reopen. |
| Qt failure and sanitizer checks | The offscreen platform could not provide a GL widget context and CTest verified successful software fallback. Local Qt also passed offscreen at a synthetic 1.5× scale. ASan and UBSan passed the software lifecycle tests with third-party leak detection disabled. |
| Fedora 44 AppImage, hosted | Debian-built AppImage relocated under a Unicode path and ran under Weston 15 inside a Fedora 44 container with its own media tools. Both codec fixtures passed using forced software and OpenGL llvmpipe (Mesa 26.2.7, LLVM 22.1.8). The test used extract-and-run. |
| Windows x86-64 ZIP, hosted | Native Windows Server 2025, MSYS2 UCRT64, Qt 6.12.0, GCC 16.2.0, libmpv API 2.6 and FFmpeg 9.1. Both CTest modes passed. Auto selected software after Qt was unable to create an OpenGL widget context. The relocated package generated an AVC/AAC clip using bundled FFmpeg and used the bundled ffprobe and Qt plugin with only the Windows system directories on PATH. |

The hosted audio tests use a timed null output. See the maintainer tests
below for Fedora KDE Wayland and Windows 11 playback and desktop integration.
Accelerated OpenGL on Windows and multi-monitor DPI remain open.

Four prototype issues were fixed before the gate. A stale libmpv
end event could abort a newer load. The native Windows CMake build received
MSYS-format paths for libmpv. OpenGL tests exposed missed frames and a
framebuffer state issue that were corrected with the reference fast render
profile and explicit Qt framebuffer restoration. The first AppImage also used
a compression mode unsupported by the pinned runtime. The Rust reference's HEVC Main 10 export/player issue remains a separate
export test.

### Side-by-side test

[Hosted comparison job](https://github.com/bugrakoc/SpoolCut/actions/runs/45580824957/job/139090130656)
compared release executables on the same Debian Xvfb session. It used software
video rendering in both editors, two generated 720p25 AVC/AAC clips, a 1024×720
window and a null audio route. Each median covers three interleaved launches
and two five-second process CPU samples per launch. Startup ends at the first
visible window. Source changes time a click on the other clip's ruler to its
first visible frame, over eight switches per launch. The samples exclude
ffprobe children and the display server. PSS is proportional set size. The
GTK editor still indexes cuts and manages a full project model, and the two
editors have different timeline seek semantics. Real-system 4K, GPU and end-to-end startup performance remain open.

| Measurement | Rust / GTK | C++ / Qt |
| --- | ---: | ---: |
| First visible window | 170.5 ms | 95.3 ms |
| Idle CPU (% of one core) | 0.4% | 0.6% |
| Playing CPU (% of one core) | 15.8% | 10.8% |
| Idle PSS | 171.0 MiB | 120.7 MiB |
| Playing PSS | 180.1 MiB | 126.8 MiB |
| Source-switch latency | 74.7 ms | 25.8 ms |

Both Windows packages use the same extract-and-run workflow and include native
dependencies. The earlier Rust test-package artifact was 102.0 MB and the Qt
test-package artifact was about 104 MB, including the outer Actions ZIP and
checksum. The Qt Fedora AppImage was about 117 MB in Actions and required no
system FFmpeg on the test container. Windows fractional scaling, Fedora KDE window controls and taskbar integration
still need a side-by-side manual check. The Qt AppImage carries an icon and
desktop entry. Launcher registration in the Qt version is part of desktop
integration testing.

### Approval checkpoint

[Prototype workflow run 45581777506](https://github.com/bugrakoc/SpoolCut/actions/runs/45581777506)
builds the AppImage and portable ZIP from
[`f009d05`](https://github.com/bugrakoc/SpoolCut/tree/f009d0598b55bb63a0c669e5c70a235110b7ced0).
The run provides the two test packages, checksums and companion corresponding-source
archives. Extract the outer Actions download. Make the Fedora AppImage executable.
On Windows extract the inner ZIP and open `SpoolCut Qt Prototype/bin/SpoolCutQt.exe`.

Follow the [manual checklist](https://github.com/bugrakoc/SpoolCut/blob/f6f1a5a4e1fa332c34630eb934a1cea078982de8/qt-prototype/README.md#manual-checks), including
actual audio and playback on both systems, the preview renderer tooltip and
100%, 150% and 200% display scaling. The results above verify the prototype's
scope. The maintainer later approved Milestones C through E after the follow-up retests below.

### Maintainer test on 25 November 2026

| Check | Fedora KDE Wayland | Windows 11 |
| --- | --- | --- |
| Media and preview | Import, drag and drop, scrubbing and resize worked. Playback through a join paused for about one second. Preview looked sharp. The tooltip reported `OpenGL NVIDIA RTX 4070 PCIe SSE2`. | Opening HEVC via the picker or drag and drop left a white, unresponsive window. Playback and the renderer tooltip could not be checked. |
| Desktop integration | Window controls worked. The taskbar showed a generic W icon. Open media used the generic Qt file picker. | Explorer and taskbar icons looked correct. Open media used the native Windows picker. |
| Comparison with GTK | The prototype felt faster and looked worse. | The prototype felt faster and looked worse. |

The Windows stall initially blocked prototype approval. The earlier hosted Windows run
could reach the fallback timer when Qt failed to create its OpenGL context.
A driver that blocks while creating the widget can prevent the timer from
running. That is a plausible cause of the white window, and the observation
alone cannot isolate it from codec, audio-device or other runtime behavior.

The follow-up defaults to software preview on Windows, as the Rust reference
does, and retains explicit OpenGL selection. The Windows portable smoke test
now covers 4K HEVC Main 10 plus AAC with the development PATH removed. The
Fedora AppImage installs a launcher with its own app ID and bundles Qt's
XDG Desktop Portal plugin for KDE file dialogs. Preview loading now supplies
the start position with the load command, avoiding a second seek at joins.
In six local offscreen software-rendered joins per build, the median from
libmpv's end-of-file event to the next visible frame fell from 318 ms to
85 ms. The test used generated 320×180 AVC and HEVC clips and a null
audio device. It excludes audio-device drain and waits before the event.
Retest the join with the original footage on KDE to check the reported pause.

The follow-up retest covered Windows 11 playback, plus the launcher, picker
and join on Fedora. Visual polish remains part of the desktop evaluation.

### Follow-up test builds

Hosted Linux lifecycle, fallback and sanitizer tests passed. The relocated
Fedora AppImage loaded the portal theme, registered a valid desktop entry
and previewed both fixtures. Native Windows tests passed in forced software,
default and explicit OpenGL modes. The relocated Windows ZIP opened AVC
and 4K HEVC Main 10 with AAC using the software default and only the system
PATH. The hosted audio tests use libmpv's null output.
[Follow-up builds and evidence](https://github.com/bugrakoc/SpoolCut/actions/runs/45587125568)

### Maintainer retest on 25 November 2026

The maintainer tested the follow-up builds from commit
[`996acbe`](https://github.com/bugrakoc/SpoolCut/tree/996acbe69e382a18be071064e8f4fe949954960b).

| Platform | Result |
| --- | --- |
| Fedora KDE Wayland | Icons and the KDE file picker displayed correctly. Playback across joins was considerably faster. The maintainer reported the improvement qualitatively. |
| Windows 11 VM | The prototype started with software preview and worked correctly. The reported opening stall cleared with this configuration. |

The reported playback and desktop integration blockers are cleared on the
tested configurations. Accelerated Windows OpenGL and multi-monitor DPI
remain open. The maintainer gave the go-ahead for the lossless core port on 25 November 2026.

## C. C++ stream-copy core and CLI

The Qt Core library and CLI were introduced at
[`b07b330`](https://github.com/bugrakoc/SpoolCut/tree/b07b3306341a5dfed085df5ee1861fb528304c42)
and compared against the pinned Rust baseline at
[`39cb4ee`](https://github.com/bugrakoc/SpoolCut/tree/39cb4eecef8914f187871aac2187f8f023d14e0d).
The Qt prototype remains independently buildable. The new library links Qt
Core and invokes FFmpeg and ffprobe with QProcess. Each export uses
packet-ordinal selection, fragmented MP4 staging, Matroska verification and
an atomic, no-overwrite publication on the destination filesystem.

### Build and reproduce

Use Qt 6.8+, a GCC or Clang toolchain with C++20 and signed 128-bit integer
support, CMake 3.25+, Ninja, Python 3 and FFmpeg with libx264 and libx265.
The Windows build uses the MSYS2 UCRT64 packages specified in
[Qt core CI](../.github/workflows/qt-core.yml). These commands are for
Debian, Fedora or an MSYS2 UCRT64 shell from the repository root.

```sh
git worktree add --detach ../spoolcut-reference 39cb4eecef8914f187871aac2187f8f023d14e0d
cargo test --manifest-path ../spoolcut-reference/Cargo.toml --locked
cargo build --manifest-path ../spoolcut-reference/Cargo.toml --locked --bin spoolcut
reference="$(cd ../spoolcut-reference && pwd)/target/debug/spoolcut"
# On Windows, append .exe and use cygpath -m for the CMake cache value.
cmake -S . -B build-qt-core -G Ninja -DCMAKE_BUILD_TYPE=Release -DSPOOLCUT_RUST_REFERENCE="$reference"
cmake --build build-qt-core
ctest --test-dir build-qt-core -V
```

Run `build-qt-core/spoolcut --help` for the CLI. On Windows use
`build-qt-core/spoolcut.exe`. The CLI probes media for `inspect SOURCE`,
plans the existing project JSON for `plan PROJECT.json` and publishes
`export PROJECT.json OUTPUT.mkv`. The core CTest suite also runs without
the Rust CLI. Setting `SPOOLCUT_RUST_REFERENCE` enables the differential
comparisons.

### Linux evidence

Debian 13 x86-64 with GCC 14.2.0, Qt 6.8.2, FFmpeg/ffprobe 7.1.5, and
Rust 1.100.0 passed two Rust unit tests, seven Rust integration tests and
all seven CTest cases. The GCC build treated warnings as errors. Clang
19.1.7 static analysis reported no diagnostics across the core, CLI and
test translation units. AddressSanitizer and UndefinedBehaviorSanitizer
passed the same seven CTest cases locally with leak detection disabled
because the local executor prevented LeakSanitizer from starting.

Both CLIs returned equal plans and safe-point indexes for accepted fixtures.
The open-GOP non-IDR in/out cuts, reversed range, out-of-range exact large
PTS, mismatched video/audio streams and missing sources were rejected with
the same reason. Both CLI processes exited 130 after SIGINT during staging
on Linux and removed their staging directories. The C++ API also exercised
cancellation with an active tool, an existing destination and a file
created by a competing writer during verification, and a long Unicode
output path containing spaces and an apostrophe.

Each row is a generated A–B–A export. Both implementations matched the
source's expected presentation frame order and the selected video/AAC
SHA-256 packet hashes. The table lists actual MKV duration and audio-minus-
video start offsets at the three clip boundaries.

| Fixture | Frames | Video / AAC packets | Duration | A/V offsets |
| --- | ---: | ---: | ---: | --- |
| AVC with B-frames and aligned cuts | 48 | 48 / 90 | 1.920 s | 0, 0, 0 ms |
| HEVC with fractional frame rate and two source time bases | 48 | 48 / 73 | 1.601 s | 0, +21, +19 ms |
| HEVC Main 10 with fractional frame rate and two source time bases | 48 | 48 / 73 | 1.601 s | 0, +21, +19 ms |
| Variable-frame-rate AVC | 48 | 48 / 93 | 2.040 s | 0, +3, +5 ms |

Both tracks were marked default and FFmpeg decoded the generated HEVC Main
10 exports. The affected Windows player and original user footage still
need a manual test.

### Recovered verification checkpoint

The branch was recovered at [`039fe0b`](https://github.com/bugrakoc/SpoolCut/commit/039fe0b46e67bc0e231b5ed4d5c1795489125b8f).
The pinned reference remains `39cb4ee`. Three commits after the initial core
port had already added staged export failure tests, fixed a Windows sharing
violation by closing the staging file before project replacement, and kept
Linux symlink traversal intact when resolving project and source parents.

[Core parity run 45593604734](https://github.com/bugrakoc/SpoolCut/actions/runs/45593604734)
passed all seven Linux cases in the warnings-as-errors and ASan/UBSan builds.
Clang static analysis passed. The native Windows job passed all 11 Rust
reference tests and six of seven C++ CTest cases. Its generated AVC, HEVC,
Main 10 and VFR exports matched the Rust reference. The Windows Ctrl+C
case returned 0 after publication instead of 130. The test now explicitly
enables Ctrl+C delivery before launching each program on its own console.
[Core parity run 45627893282](https://github.com/bugrakoc/SpoolCut/actions/runs/45627893282)
then passed all seven CTest cases on both Linux and native Windows without
skips. Windows used MSYS2 UCRT64, GCC 16.2.0, Qt 6.12.0, FFmpeg 9.1 and
Rust 1.100.0. Ctrl+C cancelled the C++ and Rust CLIs during staging,
returned 130, and left neither final output nor temporary staging files.
The core API also passed cancellation with a media tool active, a locked
project, a competing destination, source identity changes and Unicode paths.
Linux repeated the warnings-as-errors, ASan/UBSan and static analysis checks.
Local Debian 13 also passed the seven differential tests against the reference
built with Rust 1.85.1.

### Remaining roadmap gates

- **A:** Complete. **B:** Prototype approved. Matched target-machine performance
  measurements and the full display-scaling comparison are still open.
- **C:** Automated core parity passed on Linux and Windows. Check original
  Main 10 footage with the affected player and removable/exFAT behavior during
  platform acceptance.
- **D:** Qt desktop shares the core and passed Linux and Windows desktop
  tests. Maintainer retests on the migration packages are open.
- **E:** Hosted verification and artifacts are recorded below. Physical
  platform checks and the final review remain open.

## D. Qt desktop integration

The approved Qt Widgets window, custom timeline and libmpv renderer now use
the same C++ core as the CLI. Open and save existing SpoolCut JSON projects,
import or relink media, and append or drag sources from the bin. The bin
shows resolution, codec, frame rate and unavailable sources. Timeline edits
keep the project’s integer PTS and validate candidate ranges with the
stream-copy planner. The ruler snaps clicks to safe points and supports
drag scrubbing, zoom and pan. The editor supports clip reordering, edge
trim, split, cut/copy/paste and undo/redo. Preview opens the effective
source range at the planned frame and advances across clips. Windows
defaults to software preview. Explicit OpenGL selection remains available.

Indexing and export run in a cancellable worker. The UI reports progress
and errors. Closing requests cancellation and joins the worker before
teardown. Both frontends use the native no-overwrite Matroska export
pipeline. The source-format and lossless-export rules remain in section C.

Build and run the desktop with the commands in the
[Qt desktop README](https://github.com/bugrakoc/SpoolCut/blob/f6f1a5a4e1fa332c34630eb934a1cea078982de8/qt-prototype/README.md). On Debian 13 with GCC 14.2,
Qt 6.8.2, libmpv 0.40 and FFmpeg 7.1.5, the desktop compiled with warnings
as errors. The local software smoke verified proportional geometry, ruler
snapping, clip and edge gestures, exact project reopen, missing media,
relink validation, trim/split/undo/redo, a lossless export, cancellation
during staging and preservation of an existing destination. The H.264 and
Main 10 HEVC preview lifecycle cases verified decoded video and audio,
rapid scrubbing, source switches, resize and teardown. The seven core
differential tests also passed locally after the shared library build
change. The native Linux/Windows package checks and manual machine
checks are tracked in Milestone E.

## E. Replacement package verification

The migration packages build the Qt desktop and standalone CLI against the
same library. Both packages carry native Qt plugins, libmpv, FFmpeg and
ffprobe. The Windows ZIP also exposes `spoolcut.exe` in its `bin` directory.
The Fedora AppImage dispatches `--cli` to the bundled console executable.
The package smoke scripts use relocated paths with spaces and Unicode and a
system-only PATH. They inspect, plan, export and decode generated AVC and
HEVC Main 10 files with bundled tools. The desktop still verifies decoded
video and audio with the selected renderer.

The local Debian software-renderer suite also passed under ASan and UBSan
with third-party leak detection disabled. The first hosted ASan/UBSan desktop
run failed a rapid-scrubbing assertion. It sampled the red and blue channels
from separate widget grabs, which can render different frames. The test now
uses both channels from one image per attempt. The corrected Linux CI passed,
and four repeated local sanitized runs passed. Local software-renderer runs
also passed at synthetic 1×, 1.5× and 2× scales.

[Qt core parity run 45631202259](https://github.com/bugrakoc/SpoolCut/actions/runs/45631202259)
passed on Linux and Windows for the package revision
[`e55891a`](https://github.com/bugrakoc/SpoolCut/commit/e55891a5203f76cb3bc69c3cf246b33cd9be6b9c).
[Desktop and package run 45631201615](https://github.com/bugrakoc/SpoolCut/actions/runs/45631201615)
tested the same revision. Debian Linux passed X11 and Weston Wayland using
software and Mesa llvmpipe, offscreen software fallback and the ASan/UBSan
suite. The relocated Fedora 44 AppImage passed the AVC and HEVC Main 10
preview, inspect, plan, export and decode cases under Weston. It selected
software or llvmpipe as requested and loaded the portal theme.

The [Fedora x86-64 AppImage artifact](https://github.com/bugrakoc/SpoolCut/actions/runs/45631201615/artifacts/15031047267)
contains the AppImage and its checksum. Its
[corresponding source archive](https://github.com/bugrakoc/SpoolCut/actions/runs/45631201615/artifacts/15031635678)
carries the matching SpoolCut revision, Debian package sources and inventory,
and the pinned AppImage runtime and source recipes.

The [Windows 11 x86-64 portable ZIP artifact](https://github.com/bugrakoc/SpoolCut/actions/runs/45631201615/artifacts/15032340457)
contains the ZIP and its checksum. Extract the outer Actions archive, then
the inner ZIP, and open `SpoolCut Qt/bin/SpoolCutQt.exe`. The native
Windows runner passed forced and default software rendering. The explicit
OpenGL test fell back to software when the hosted runner could not create
a context. The relocated package used only the system PATH
and passed AVC and 4K HEVC Main 10 inspection, planning, export, decode and
desktop playback. The desktop used the Windows software default and a null
audio sink. Windows hardware OpenGL still needs a maintainer retest.

The [Windows corresponding source archive](https://github.com/bugrakoc/SpoolCut/actions/runs/45631201615/artifacts/15030224747)
carries the matching SpoolCut revision, exact MSYS2 package inventory,
115 corresponding source packages and recipes verified during packaging.
Both packages include dependency
notices. Keep matching source available alongside any redistributed binary
package.

### Matched-project hosted comparison

The [comparison job](https://github.com/bugrakoc/SpoolCut/actions/runs/45631201615/job/139264544377)
used the same two-clip 720p25 AVC/AAC project in both editors on Debian
Xvfb. Both used the software renderer and a null audio sink. As in the
prototype comparison, figures are medians over three interleaved launches
and eight source switches per launch. PSS excludes media-tool children and
the display server. Startup ends at the first visible window, before source
indexing completes.

| Measurement | Rust / GTK | C++ / Qt |
| --- | ---: | ---: |
| First visible window | 144.3 ms | 86.6 ms |
| Idle CPU (% of one core) | 0.2% | 0.6% |
| Playing CPU (% of one core) | 12.8% | 8.6% |
| Idle PSS | 170.0 MiB | 120.2 MiB |
| Playing PSS | 180.3 MiB | 128.8 MiB |
| Source-switch latency | 74.2 ms | 23.6 ms |

These measurements apply to the hosted setup. Playback with real 4K footage,
native audio, accelerated graphics and the maintainer's normal display
configurations is part of the checklist below.

### Feature parity and acceptance

| Area | Evidence on `cpp-qt` | Remaining target-machine checks |
| --- | --- | --- |
| Core planning and lossless MKV export | Differential packet hashes, decoded frame order, durations, A/V offsets, unsafe cuts and cancellation on Linux and Windows. | Re-export the original HEVC Main 10 footage and play the result in the affected Windows player. |
| Projects and media bin | Exact integer JSON round trips, portable source paths, reopen, resolution/codec/frame rate, missing source and relink validation. | Retest native file pickers and Explorer drop against the migration packages. |
| Editing and preview | Proportional ruler, snapping, drag scrubbing, trim, split, reorder and history tests. AVC and Main 10 HEVC playback with forced software, OpenGL and fallback on CI. | Fedora KDE with NVIDIA OpenGL, Windows software on original footage, and an accelerated Windows OpenGL run where available. |
| Jobs and filesystem | Worker cancellation, staging cleanup, existing output preservation, Windows locked project and source identity tests. | Removable/exFAT destinations and clean VM or physical target testing. |
| Native desktop and performance | Hosted matched-project measurements and Linux synthetic scaling. | 100%, 150% and 200% on actual displays, mixed-DPI monitor movement, controls and taskbar icons on the current packages, and comparison with the same real footage. |

### Maintainer checklist

1. Import two MP4/MOV clips with different lengths and include H.264 and HEVC
   Main 10 footage with audible AAC. Use paths with spaces and non-ASCII
   characters. Append or drag the sources from the bin, then trim, split,
   reorder and undo. Save and reopen the project. Move one source and relink it.
2. Drag the ruler across joins in both directions, resize the window and
   splitters during playback, and verify audio and visible cuts. Export a
   compatible sequence and play the MKV. Compare preview and export around
   the cuts. Export the original Main 10 footage and check it in the Windows
   player that previously failed. Cancel a second export and check that staging
   is removed. Try an existing destination and confirm that its contents survive.
3. On Fedora KDE, confirm Wayland, the native picker, taskbar icon and window
   controls. Record the renderer from the preview tooltip. Compare
   `SPOOLCUT_QT_PREVIEW=software` with the usual OpenGL renderer.
4. On Windows 11, extract the ZIP on a clean VM with no development PATH and
   confirm Explorer and taskbar icons, native controls, picker and file
   drag-and-drop. Verify the software default with Main 10 playback. Test
   `SPOOLCUT_QT_PREVIEW=opengl` on accelerated hardware when available.
5. Check preview sharpness and layout at 100%, 150% and 200% scaling. Move the
   window between monitors with different scale factors when available. Exercise
   another drive, a locked project, and a removable or exFAT destination.
   Export must publish atomically or report an error while preserving existing files.
6. For a GTK comparison, use the same session, resolution, scaling, project
   and renderer class. Record time to the first visible window, process CPU
   and memory during idle and continuous playback, and the delay from timeline
   input to the corresponding frame. Record package extraction or launch steps.

### Future macOS boundaries

Keep native file ID and atomic, no-replace publication at the filesystem
boundary. Test Unicode and normalization, APFS volume and mount behavior,
and application-relative discovery of bundled FFmpeg, ffprobe, and libmpv.
A future package must choose an executable/resource layout for a signed
`.app`, bundle the platform and other required Qt plugins, and verify the
preview context and fallback on supported hardware. Signing, notarization,
and dependency/source inventory belong to that package workflow.

## Maintainer results and Qt 0.2.1 follow-up

| Version | Revision and purpose |
| --- | --- |
| 0.1.0 | Rust/GTK baseline at [`39cb4ee`](https://github.com/bugrakoc/SpoolCut/tree/39cb4eecef8914f187871aac2187f8f023d14e0d). |
| 0.2.0 | First Qt maintainer build at [`d1c896c`](https://github.com/bugrakoc/SpoolCut/commit/d1c896c5faac919a01f171c243c644bb500626df), [test packages](https://github.com/bugrakoc/SpoolCut/actions/runs/45632588206). This assignment records the build already tested. |
| 0.2.1 | First versioned Qt follow-up. [VERSION](../VERSION) drives both executables and package filenames. |

The maintainer confirmed the KDE picker, taskbar icon and window controls, and
reported the NVIDIA RTX 4070 OpenGL renderer. Windows 11 in QEMU passed the
desktop integration and software Main 10 playback checks. The reported scaling
checks passed at 100%, 150%, and 200%. Accelerated Windows OpenGL, a second drive,
locked projects on the target VM, and removable/exFAT remain open for manual
testing.

| 0.2.0 finding | 0.2.1 change |
| --- | --- |
| Reopening a project after moving a source hid the timeline. | The timeline keeps its clips visible, marks the unavailable rows, and preserves the selection during relink. It suppresses absolute time and playback until all clip timings can be resolved. |
| Export rejected 46.795 s for a 46.785 s plan. | A generated clip reproduced the 10 ms excess. When only duration verification fails, the join retries with the planned final video endpoint and again verifies all packets, streams, and duration before publication. The export stays stream-copy. |
| Fedora software preview faded to white. | The software renderer now initializes every pixel's padding/alpha byte before giving the RGB image to Qt. Playback checks sample colors over time. Retest on Fedora with the original footage. |
| Qt UI lost the GTK timeline and media-bin visual cues. | The editor uses per-source colors, a distinct ruler and playhead, a two-line media bin, and grouped controls. Zooming out past Fit now leaves usable empty timeline space. |

The key retests are the original export and Windows player, the Fedora software
preview, and reopening the project with a moved source. See the
[maintainer checklist](#maintainer-checklist) for hardware and filesystem
acceptance.

### Verified 0.2.1 packages

The [core run](https://github.com/bugrakoc/SpoolCut/actions/runs/45650843971)
passed all eight CTest cases on Linux and Windows, including the 10 ms
duration regression. It exercised the existing packet-hash, frame-order,
timing, cancellation, and destination protection checks. Local Linux
software preview tests also passed at 100%, 150%, and 200% synthetic scaling.

The [desktop and package run](https://github.com/bugrakoc/SpoolCut/actions/runs/45651333003)
passed at [`7bfffad`](https://github.com/bugrakoc/SpoolCut/commit/7bfffad255ca5f8a8d4275541e5671fc2818ffc4).
It covered missing-source reopen and relink, zoom below Fit, renderer smoke
tests, the software-renderer sanitizer suite, and relocated AVC and HEVC
Main 10 exports. The portable Windows smoke verified the Qt executable's
embedded product version and the CLI version. The Fedora smoke verified
the CLI version from the extracted AppImage.

| Platform | Test package | Corresponding source |
| --- | --- | --- |
| Fedora x86-64 | [SpoolCut 0.2.1 AppImage](https://github.com/bugrakoc/SpoolCut/actions/runs/45651333003/artifacts/15040107093) | [Linux sources](https://github.com/bugrakoc/SpoolCut/actions/runs/45651333003/artifacts/15039689778) |
| Windows x86-64 | [SpoolCut 0.2.1 ZIP](https://github.com/bugrakoc/SpoolCut/actions/runs/45651333003/artifacts/15040927274) | [Windows sources](https://github.com/bugrakoc/SpoolCut/actions/runs/45651333003/artifacts/15040822412) |

## Qt 0.2.2 missing-source recovery

When a source disappears after indexing, a failed preview load or playback
error now updates the source's cached availability. The editor enters the
same missing-media state as on project reopen, keeps the clips visible,
selects the missing source for Relink and restores the empty preview.
Successful joins keep their existing path. There is no background scan.
Missing rows in the media bin use the timeline's red fill and dashed border.

The desktop regression moves the next source while the timeline is playing,
checks the missing-media display at the join, and relinks it without reopening
the project. A Linux case also checks error handling when libmpv already has
an open handle to the renamed file.

## 0.3.0 production audit

- CMake builds the shared core in `cpp/core`, the CLI in `cpp` and Qt
  Widgets in `cpp/desktop`. The
  [rust-gtk-baseline tag](https://github.com/bugrakoc/SpoolCut/tree/rust-gtk-baseline)
  resolves to `39cb4eecef8914f187871aac2187f8f023d14e0d`. CI uses a
  detached worktree at that commit for the reference tests.
- The native preview surface test covers first preview after relinking,
  repeated frames, recovery of editing history and ordinary empty preview.
  The playback test also covers a source disappearing before a join.
  [Production-layout desktop CI](https://github.com/bugrakoc/SpoolCut/actions/runs/45678519467)
  passed both X11 and Weston checks after the undo test was changed to
  trigger the QAction directly. KDE with NVIDIA remains part of final
  manual acceptance.
- The audit followed QObject ownership, GL context destruction and the
  mpv callback lifetime. Preview errors now stop the active session
  before notifying the editor. Preview opens local indexed media with
  automatic sidecar loading and media references disabled. The worker
  owns the job cancellation token and joins at completion or shutdown.
- Automatic joins load the next file directly. A libmpv error triggers
  the filesystem check and missing-media recovery. Explicit source
  selections and export retain their source identity checks.
- Source metadata is cached with each immutable index. This avoids
  rebuilding keyframe lists for repeated clips during editor refresh.
  Nonfinite playback positions are ignored before timestamp conversion.
  The project reader rejects malformed Unicode strings while preserving
  exact signed integer PTS and portable source paths.
- Verified stream-copy cases compare the C++ and Rust plans, video and
  audio payloads, decoded frame order and A/V offsets. Cases include
  VFR, B-frames, HEVC Main 10, short video tails, invalid cuts, source
  replacement, Ctrl+C cancellation, and no-overwrite publication races.
  The [production-layout core run](https://github.com/bugrakoc/SpoolCut/actions/runs/45678519480)
  covers Linux and Windows.
- Production packages verify dependency inventories, versions, FFmpeg
  configuration and the source revision. The AppImage runtime source set
  now includes the exact Alpine recipes and static library sources in
  addition to libfuse and squashfuse. See [packaging](PACKAGING.md) for
  build and redistribution information.

The migration PR records the final candidate revision, its CI runs and
download links. Physical Windows OpenGL, mixed-DPI monitor testing,
additional real-footage benchmarks and exFAT/removable media are deferred.
