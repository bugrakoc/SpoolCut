# C++ and Qt feasibility evidence

## A. Rust reference

The migration branch started at `be176aa096275d502020b9cfc9d25f2afa14a5bb`.
The latest `main` at audit was
[`39cb4eecef8914f187871aac2187f8f023d14e0d`](https://github.com/bugrakoc/SpoolCut/tree/39cb4eecef8914f187871aac2187f8f023d14e0d),
the merged Windows PR #3. This commit was already an ancestor of the branch.

| Area | Reference contract and regression coverage |
| --- | --- |
| CLI | `inspect SOURCE`, `plan PROJECT.json`, `export PROJECT.json OUTPUT.mkv`. Inspection and planning emit JSON. Errors go to stderr with exit 1. Ctrl+C cancellation exits 130. |
| Projects | Strict JSON with a source ID-to-path map and ordered clips carrying signed integer `in_pts`/`out_pts`. Paths are relative to the project directory when possible; sources on another Windows drive remain absolute. Project save stages then replaces the project. Locked Windows projects retain their prior contents. |
| Media and time | One non-cover H.264 `avc1` or HEVC `hvc1` video track and one AAC-LC track, with matching stream parameters and decoder configuration across edited sources. Packet-level closed-IDR indexing checks presentation order and cut eligibility. Uses integer rational comparisons, exclusive out PTS, video end as a conditional boundary and only whole AAC frames. Discard-marked packets, unsupported stream side data and unsafe ranges fail explicitly. |
| Export | Copy exact input packet ranges through fragmented MP4 staging with a common video time base, then concat into MKV with default audio and video dispositions. Compare timestamps, counts, durations and HEVC configuration before publication. Regression tests use synthetic AVC, HEVC, B-frames, fractional and variable frame rates, mixed source time bases and A–B–A sequences. Tests compare payload hashes where framing permits, decoded frame order and A/V timing. |
| Process and files | Explicit FFmpeg/ffprobe overrides, application-relative lookup and PATH fallback. Cancel and reap the subprocess before cleanup. Check source identity and metadata before publication. Stage on the output filesystem. Linux uses a hard link and Windows uses a same-volume Unicode rename without replacement. Tests cover cancellation, destination races, existing files, path identity, Unicode and locked projects on Windows. |
| Desktop | Native/project chrome, media bin import and Explorer drop path, resolution/codec/frame rate, split layout and preview. Timeline fits, zooms, pans, scrubs, snaps, reorders, trims and splits. Handles project reopening, relinking, dirty state, editing history, keyboard controls and export progress/cancellation. GTK smoke covers editing, rendered pixels and rapid seeks, repeated open/close, persistence and export. |
| Graphics | GTK GLArea/GL uses EGL symbol lookup on Linux and WGL on Windows. Native Windows defaults to software, supports an OpenGL override and can fall back. Preview decodes in software and can sound slightly different from exported AAC at unaligned joins. |
| Distribution | Existing Windows UCRT64 packaging walks native DLL imports, bundles GTK resources, validates source signatures, and smokes the portable ZIP with a clean PATH. Release packaging is gated to PRs/tags. Existing Linux CI uses Debian/Weston. The Rust AppImage launcher has XDG registration and relocation tests; there is no Rust AppImage job on this branch. |

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
