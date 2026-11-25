# SpoolCut on Windows

Target Windows 11 x86-64 with the existing GTK 4 editor, libmpv preview, and
FFmpeg stream-copy export. Fedora remains a release target. The timeline,
project JSON, packet verification and lossless export rules are shared.

## 1. Core portability and native CI

- [x] Audit main and the original prototype, Fedora desktop and timeline branches.
  Main contains the current desktop editor and taskbar fix.
- [x] Resolve FFmpeg and ffprobe from explicit settings, the application bundle
  or PATH. Run the tools without a console window and wait for a cancelled child
  before removing its temporary files.
- [x] Check source identity on both platforms and preserve atomic publication
  when an output appears during export. Test Unicode, spaces and long paths.
- [x] Verify the UCRT64 Rust, GCC, pkg-config, GTK and libmpv combination and run
  the real H.264/HEVC export suite on the native Windows runner.

## 2. Desktop and preview

- [x] Exercise editing, shortcuts, undo/redo, project reopen and relinking on
  Windows with the libmpv software renderer.
- [x] Isolate Linux EGL and Windows WGL lookup. Exercise rapid seeking, source
  switches, repeated startup/shutdown and decoded pixels in the desktop smoke test.
- [ ] Validate WGL rendering on a Windows system with accelerated OpenGL.
- [ ] Verify Windows decorations, taskbar and Explorer icon and scaling at
  100%, 150% and 200%, including a window moved between monitors.

## 3. Portable distribution

- [x] Add CI packaging for the app, FFmpeg, ffprobe, Windows DLL closure, GTK
  resources and notices.
- [ ] Verify the relocated ZIP on a Windows runner with the development PATH removed.
- [ ] Record the exact inputs for redistribution and make matching corresponding
  source, build recipes and patches available with the binaries.
- [ ] Unzip and run on a clean Windows 11 VM without developer tools installed.
  Verify the file picker, playback with sound and export. Repeat using a second
  drive and an exFAT volume, including an existing or locked output.

## 4. Release verification

- [x] Run the native Windows and Linux Wayland smoke tests in CI.
- [ ] Repeat the editor and export tests on Fedora hardware.
- [ ] Record the results of the Windows VM checklist, including the renderer
  used. Release after verifying the package and resolving blocking failures.

## macOS foundations

Keep native file identity, publication, executable discovery, GTK graphics
lookup and packaging at their respective boundaries. Future work includes a
macOS GTK/libmpv build, app-bundle layout, graphics context validation, signing
and notarization. There is no scheduled macOS build or release in this milestone.
