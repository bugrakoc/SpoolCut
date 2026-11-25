# C++ and Qt feasibility evidence

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

Existing issues and open checks are documented in [Windows VM results](WINDOWS.md).
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

The build in [qt-prototype](../qt-prototype/README.md) is independent of the
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

Follow the [manual checklist](../qt-prototype/README.md#manual-checks), including
actual audio and playback on both systems, the preview renderer tooltip and
100%, 150% and 200% display scaling. The results above verify the prototype's
scope. Milestones C through E await maintainer approval.

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
remain open. The lossless core port awaits an explicit go/no-go.
