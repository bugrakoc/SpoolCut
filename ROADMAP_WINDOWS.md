# Windows roadmap

Windows 11 x86-64 shares the C++ core and Qt Widgets application with
Fedora. The portable ZIP packages its own UCRT64 dependencies. See the
[Windows build guide](docs/WINDOWS.md), [migration roadmap](ROADMAP_CPP_QT.md)
and [test results](docs/CPP_QT_STATUS.md).

- [x] Keep Unicode paths, file identity, process cancellation, locked
  project handling and same-volume publication behind native boundaries.
- [x] Verify HEVC Main 10 playback with the software renderer and export
  playback in VLC in a Windows 11 VM.
- [x] Verify that a relocated portable ZIP discovers its own Qt plugin,
  media tools and shared libraries with a system-only PATH in hosted CI.
- [ ] Complete final manual acceptance of the 0.3.0 package.
- [ ] Test hardware OpenGL, mixed-DPI monitors and supported publication
  semantics on removable/exFAT destinations when available.

The [original Rust/GTK Windows roadmap](https://github.com/bugrakoc/SpoolCut/blob/39cb4eecef8914f187871aac2187f8f023d14e0d/ROADMAP_WINDOWS.md)
records the initial port.
