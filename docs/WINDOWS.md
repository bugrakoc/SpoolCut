# Windows 11 x86-64

## Portable package

Extract the ZIP and run `SpoolCut/bin/SpoolCut.exe`. The folder also
contains the Qt platform plugin, libmpv, FFmpeg, ffprobe and
`spoolcut-cli.exe`. The portable package requires no MSYS2 installation.

Software preview is the default. It works with the software graphics
configuration tested in a Windows 11 QEMU VM. To opt in to OpenGL, start
the program from PowerShell with:

```powershell
$env:SPOOLCUT_QT_PREVIEW = 'opengl'
& .\SpoolCut\bin\SpoolCut.exe
Remove-Item Env:SPOOLCUT_QT_PREVIEW
```

OpenGL initialization can depend on the installed driver. The preview
tooltip shows the selected renderer and any fallback. Set display scaling
through Windows Settings. The native pickers, titlebar controls and taskbar
icon have been exercised by the maintainer.

Project paths may be Unicode and include spaces. Sources on another
drive can be stored as absolute paths. Export stages beside the
destination and publishes with a same-volume rename that preserves an
existing output. The maintainer exported to the VM's VirtIO shared folder
on Z:. Removable drives and exFAT remain deferred. For the latest test
evidence, see [migration status](CPP_QT_STATUS.md).

## Native build

Open the MSYS2 UCRT64 shell. Install the CMake toolchain and libraries from
that prefix.

```bash
pacman -Syu
pacman -S --needed git mingw-w64-ucrt-x86_64-{gcc,cmake,ninja,pkgconf,qt6-base,mpv,ffmpeg,python}
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure
./build/spoolcut-desktop.exe
```

Run from UCRT64 so Qt, GCC, libmpv and FFmpeg use the same ABI. The build
also produces `build/spoolcut.exe`. See [packaging](PACKAGING.md) for the
portable ZIP and its corresponding sources.
