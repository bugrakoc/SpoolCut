# SpoolCut — C++ / Qt 6 migration roadmap

Status. The maintainer approved the migration after the Fedora KDE Wayland and Windows 11 prototype retests. Milestones C and D passed hosted verification on Linux and Windows. The Milestone E test packages passed their relocated smoke tests and are ready for maintainer retesting. See [migration evidence](docs/CPP_QT_STATUS.md) for build instructions, results and open checks.

## 1. Decision and scope

SpoolCut is a lightweight GPLv3-licensed desktop and CLI editor for lossless video cutting and joining. The proposed replacement stack is C++20, Qt 6 Widgets, CMake/Ninja, libmpv, FFmpeg and ffprobe. Fedora (Wayland, including KDE) and Windows 11 x86-64 are first-class targets; retain a clean path to macOS without implementing, packaging or claiming support for it in this migration.

Qt Widgets is selected for conventional desktop controls, C++-only UI code, custom timeline painting, and mature platform integration and deployment tooling. Do not introduce QML/Qt Quick, a web UI, a second application language, a persistent Rust/C++ bridge or a new multimedia engine. Qt's advantages are hypotheses until tested. Rust's memory-safety advantages are real and must be compensated for with disciplined C++ ownership and testing.

Non-goals: new editing features, new codecs or containers, smart rendering, transcoding, visual redesign, changing project format or weakening export validation. Keep MKV as the existing export target during parity work; MP4/MOV export is a separate future decision. Preview may decode for display; export must never re-encode or silently convert media.

## 2. Baseline and branch policy

- Rust reference: [main at 39cb4ee](https://github.com/bugrakoc/SpoolCut/tree/39cb4eecef8914f187871aac2187f8f023d14e0d), including the merged Windows port. The `cpp-qt` branch incorporates this baseline; keep this exact commit available for differential testing.
- Windows reference: [PR #3](https://github.com/bugrakoc/SpoolCut/pull/3) was merged into main on 25 November 2026 at `39cb4eecef8914f187871aac2187f8f023d14e0d`. Audit the integrated implementation, [ROADMAP_WINDOWS.md](ROADMAP_WINDOWS.md), [docs/WINDOWS.md](docs/WINDOWS.md), CI, tests and packaging. Preserve relevant behavior and fixes, not GTK-specific workarounds. No separate Windows-branch merge is needed. Merged does not mean all manual checks passed: accelerated preview, fractional DPI scaling, outstanding package tests and HEVC Main 10 player compatibility remain baseline verification items.
- All migration work goes to `cpp-qt`. Do not change main, force-push, merge the migration into main, change the license, or publish releases without maintainer approval.
- Keep the Rust code and fixtures runnable as a behavioral reference until parity is demonstrated. Temporary coexistence is for validation only, not a permanent dual-language architecture. Preserve history; remove obsolete Rust/GTK build paths from the replacement branch only at the final cleanup checkpoint.
- Document actual commits, toolchain versions, tests, platform results and unresolved issues in this file or a linked migration status file. Mark a checkbox complete only with evidence.

## 3. Target architecture

| Layer | Choice |
| --- | --- |
| Domain/core | C++20 library, Qt Core where useful; no Widgets or libmpv dependency |
| Media inspection and export | Existing FFmpeg/ffprobe executable-based stream-copy workflow; QProcess or a small process boundary |
| Desktop shell | Qt 6 Widgets, QMainWindow, layouts/splitters, QAction and native dialogs |
| Timeline | Custom QWidget/QPainter; shared timeline math and edit model; no per-frame widget explosion |
| Preview | libmpv C/render API, QOpenGLWidget and QOpenGLContext; software-rendered QWidget fallback |
| Persistence | Qt JSON or equivalent exact-integer-safe serialization, compatible with existing JSON |
| CLI | Thin frontend to the same core; preserve inspect, plan and export behavior |
| Build/test | CMake, Ninja, CTest, Qt Test as needed; GitHub Actions |
| Distribution | Fedora AppImage; Windows x86-64 portable ZIP; macOS app bundle only in a future milestone |

Use explicit ownership, RAII, standard containers and smart pointers where appropriate; QObject parent ownership must not conflict with C++ ownership. No owning raw pointers or manual new/delete pairs where avoidable. Use signed, checked integer/rational timestamp arithmetic; never silently round large JSON integers via floating point. Keep UI thread responsive: blocking probing and export run off it, with cancellation, lifetime-safe callbacks and deterministic cleanup. Keep platform-specific code local to real OS boundaries (paths/file identity, atomic publication, executable discovery, graphics contexts and packaging). Prefer native Qt functionality over unnecessary third-party dependencies.

## 4. Milestones and decision gates

### A — Baseline audit and regression inventory

- [x] Inspect current main, this branch, the merged Windows implementation and all source/test/build files; record exact reference SHAs and actual behavior, including known limitations. Check for newer main commits before beginning work; incorporate relevant fixes without rewriting branch history.
- [x] Inventory existing desktop and CLI features, supported media and errors. Capture reference test fixtures and results; preserve project JSON, source-relative paths, timestamp semantics and CLI contract.
- [x] Run available Rust tests and identify physical hardware/manual tests not possible in CI. Distinguish existing bugs from migration regressions.
- [x] Choose a supported Qt 6 baseline available to target platforms; document compiler/toolchain, architecture, dependencies and GPLv3/third-party license obligations.

### B — Qt feasibility prototype (hard gate before full rewrite)

- [x] Build minimal C++20/Qt Widgets editor with native menus/window controls, resizable media/preview/timeline layout, file open, libmpv H.264/HEVC playback with audio, pause, seek, source switching and a proportional custom timeline with playhead and zoom.
- [x] Render through QOpenGLWidget using the current Qt context and correct framebuffer, device pixel ratio, context lifetime and libmpv callback threading. Do not assume EGL/WGL/GLX specifics or a particular OpenGL implementation. Implement a tested software fallback; never let a failed graphics initialization crash the app.
- [x] Exercise rapid seeking, resizing, source switches, repeated open/close, playback and clean shutdown. The maintainer confirmed Fedora KDE Wayland playback with NVIDIA OpenGL and Windows 11 VM playback with software preview. See the [CI and maintainer results](docs/CPP_QT_STATUS.md).
- [x] Build relocatable Fedora AppImage and Windows portable ZIP including required Qt platform plugins, libmpv, FFmpeg/ffprobe, runtime dependencies and license notices. Smoke-test on CI without development PATH and on clean target machines where possible.
- [ ] Compare with the GTK reference on equivalent footage and hardware: startup, idle/playing CPU and memory, scrubbing responsiveness, packaging/install friction, DPI scaling, taskbar icons and decoration behavior. Record comparable measurements and limitations; no unsupported claims.
- [x] Present artifacts and evidence for maintainer Fedora and Windows VM/hardware testing. Obtain explicit go/no-go before porting the full lossless engine. If the prototype cannot establish a reliable preview and practical packaging advantage, fix bounded blockers or recommend retaining Rust/GTK. Software-only hosted CI is not proof of accelerated physical playback.

### C — Port and verify the core

- [x] Reimplement the existing algorithms, not a simplified approximation; keep project model, probe/index, timestamp math, compatibility validation, planning, export, verification and process/file handling separable.
- [ ] Preserve accepted inputs (currently compatible MP4/MOV H.264/HEVC video and AAC-LC audio), stream selection, decoder-configuration checks, closed IDR and presentation-order constraints, exclusive out points, full AAC packet selection, existing timestamp tolerances and explicit unsupported-media errors. Confirm details against the integrated Linux/Windows baseline, including the HEVC Main 10 export/player compatibility case and default track dispositions.
- [x] Preserve the stream-copy staging/join/verification pipeline, cancellation, safe cleanup, destination no-overwrite and atomic publication. Maintain Unicode/space/long-path and source identity behavior across OSes. Clearly reject unsupported filesystems or unsafe publication cases; do not silently weaken guarantees.
- [x] Keep project JSON schema, relative paths and integer PTS exact; load reference projects in both implementations and compare plan results. Preserve CLI inspect/plan/export, error handling and exit behavior.
- [x] Port existing unit/integration tests and generated fixtures. Differentially compare reference and C++ plans, accepted/rejected cut points, video/audio payload hashes where framing permits, decoded frame order, duration, A/V offsets and cancellation behavior. Cover B-frames, fractional/VFR time bases, repeated A-B-A ranges, mismatched streams, destination races and failures. No reliance on output file byte equality when muxer metadata differs.
- [x] Run sanitizer builds (ASan/UBSan where supported), compiler warnings, and static analysis on appropriate runners. Record actual results. A passing GUI smoke test is not sufficient to validate lossless export.

### D — Desktop feature parity

- [x] Media bin: multi-file import, drag/drop, resolution, codec and frame rate, relink and missing-file handling.
- [x] Gapless timeline: clip widths proportional to durations, scalable ruler, zoom/pan/fit, playhead, safe keyframe markers, clip reordering, edge-drag trim, split at playhead, scrubbing and click-to-nearest-safe-keyframe behavior.
- [x] Keep preview seek/playback and effective cuts consistent with export; preserve source switching, playback state, audio and known packet-boundary limitations.
- [x] Preserve project new/open/save and existing edit operations, undo/redo, keyboard shortcuts (Ctrl+X/C/V/Z, Ctrl+Shift+Z, Space and keyframe navigation), selection and native menu actions.
- [ ] Preserve export progress, cancellation, errors and nonblocking UI. Retain existing icon/assets and desktop integration; verify minimize/maximize/close controls on KDE, taskbar icon on both platforms and high-DPI behavior.
- [x] No feature removals or unrelated changes. Add automated edit/model tests and smoke tests covering project reopen, playback, rapid scrubbing and shutdown.

### E — Native CI, packaging and acceptance

- [x] Maintain reproducible documented Linux and Windows builds; use a coherent compiler/ABI/runtime for Qt/libmpv and a supported dependency source. Avoid hardcoded developer paths. Use CMake presets if they simplify the workflow.
- [x] Run core/export regression tests and desktop smoke tests on both targets. Use actual FFmpeg encoders for generated fixtures. Exercise packaged executables in clean/relocated environments. Record skipped tests and actual renderer.
- [ ] Include Qt platform plugins (Wayland/X11 as appropriate; Windows), image/icon plugins as needed, libmpv, FFmpeg/ffprobe and compatible dependencies. Check executable discovery, non-ASCII paths, spaces, existing/locked outputs, different drives, supported and unsupported filesystems.
- [ ] Test Fedora KDE Wayland with accelerated preview and Windows 11 x86-64 with clean VM and real accelerated graphics where available; include 100/150/200% scaling and monitor movement. Require maintainer confirmation for hardware cases CI cannot validate.
- [ ] Preserve GPLv3 licensing. Verify redistribution terms for exact Qt modules, FFmpeg, libmpv and every bundled dependency, provide required notices and corresponding source/build recipes as applicable. Prefer redistributable dynamic Qt builds and avoid proprietary-only dependencies.
- [x] Document macOS boundaries (file identity/publication, paths, .app layout, Qt plugins, graphics backend, signing/notarization). Do not implement or claim macOS support yet.
- [ ] Record a feature parity matrix, known limitations and comparative performance evidence. Update README, build instructions and general roadmap to the approved stack. Remove obsolete Rust/GTK production build paths only after parity; preserve reference commit and regression fixtures.
- [ ] Open a reviewable PR to main when all required gates pass. Do not merge or release without explicit maintainer approval.

## 5. Release acceptance

The migrated version must reproduce the verified reference's safe cuts, packet-copy behavior, project interoperability and existing functionality. There must be no silent re-encoding, no unexplained regression in timing/verification, no new corruption or destructive overwrite risk, and no known blocking issue in native Fedora/Windows packages. All required CI and manual checks must be evidenced. Resolve significant performance regressions or obtain explicit approval for documented tradeoffs. A successful prototype alone is not a release candidate.

At the end of each milestone, commit the work on `cpp-qt`, update progress, provide tested build/artifact instructions, report precise failures and untested items, and stop for review at the prototype and final release gates.
