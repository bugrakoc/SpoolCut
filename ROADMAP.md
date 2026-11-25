# SpoolCut roadmap

SpoolCut is a desktop and command-line tool for cutting and joining
compatible footage without re-encoding. Keep Fedora and Wayland first-class
alongside Windows 11 x86-64. Keep media indexing, project persistence, and
export planning in the shared core.

## 0.3.0 C++/Qt transition

- [x] Port safe-cut indexing, stream compatibility, project JSON and
  verified MKV export to the C++ core.
- [x] Port the gapless timeline, media bin, project editing and preview
  to Qt Widgets with libmpv.
- [x] Use CMake for the production desktop and CLI. Keep the exact
  Rust/GTK baseline available in Git for differential testing.
- [x] Record Fedora KDE and Windows 11 acceptance for the 0.2.2 test build.
- [ ] Verify the final candidate packages in CI and complete maintainer
  acceptance of the same revision on Fedora and Windows.
- [ ] Merge the accepted revision and publish 0.3.0 with its matching
  binary, checksum and corresponding-source archives.

See the [migration roadmap](ROADMAP_CPP_QT.md) and
[verification results](docs/CPP_QT_STATUS.md).

## Follow-up

- Expand accepted media combinations using representative footage and
  regression cases.
- Keep the filesystem, executable lookup and rendering boundaries
  portable for a future macOS implementation.
