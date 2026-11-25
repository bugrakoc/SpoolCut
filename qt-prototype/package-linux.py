"""Bundle the Qt desktop, CLI and codec-enabled media tools for Fedora x86-64."""

import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import urllib.request

RUNTIME = ("https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64",
           "617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46")
RUNTIME_REV = "fedfad51acc2860b2916a1eef32ba4a500ff31ef"
# Leave the kernel-facing graphics stack and the base system ABI to the host.
HOST = re.compile(r"^(?:ld-linux[^/]*|lib(?:c|m|dl|rt|pthread|resolv|util|nss_\w+|"
                  r"GL[^.]*|EGL[^.]*|OpenGL|gbm|drm[^.]*|vulkan|wayland-[^.]+))\.so")
LIB = Path("/usr/lib/x86_64-linux-gnu")
QT_PLUGINS = LIB / "qt6/plugins"


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def fetch(url, target):
    with urllib.request.urlopen(url, timeout=60) as response, target.open("wb") as stream:
        shutil.copyfileobj(response, stream)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(*args, **kwargs):
    subprocess.run(args, check=True, **kwargs)


def copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def bundle(exe, app):
    files = set()
    cli = exe.with_name("spoolcut")
    roots = [(exe, app / "bin/spoolcut-qt"),
             (cli, app / "bin/spoolcut"),
             (Path("/usr/bin/ffmpeg"), app / "bin/ffmpeg"),
             (Path("/usr/bin/ffprobe"), app / "bin/ffprobe")]
    platforms = [QT_PLUGINS / "platforms/libqxcb.so",
                 *QT_PLUGINS.glob("platforms/libqwayland*.so")]
    if len(platforms) < 2:
        raise RuntimeError("Expected Qt X11 and Wayland platform plugins")
    for directory in ("xcbglintegrations", "wayland-decoration-client",
                      "wayland-graphics-integration-client", "wayland-shell-integration"):
        platforms.extend(QT_PLUGINS.glob(directory + "/*.so"))
    platforms.append(QT_PLUGINS / "platformthemes/libqxdgdesktopportal.so")
    roots.extend((p, app / "plugins" / p.relative_to(QT_PLUGINS)) for p in platforms)
    for source, target in roots:
        copy(source, target)
        if source not in {exe, cli}:
            files.add(source)
        dependencies = output("ldd", source)
        if "not found" in dependencies:
            raise RuntimeError(f"Incomplete runtime for {source}\n{dependencies}")
        for match in re.finditer(r"^\s*(\S+) => (/\S+)", dependencies, re.M):
            name, path = match.groups()
            if HOST.match(name):
                continue
            library = Path(path)
            copy(library, app / "lib" / name)
            files.add(library.resolve())

    (app / "bin/qt.conf").write_text("[Paths]\nPlugins=../plugins\n", encoding="utf-8")
    copy(Path("qt-prototype/AppRun"), app / "AppRun")
    copy(Path("qt-prototype/io.github.bugrakoc.SpoolCut.QtPrototype.desktop"),
         app / "io.github.bugrakoc.SpoolCut.QtPrototype.desktop")
    copy(Path("assets/io.github.bugrakoc.SpoolCut.png"), app / "io.github.bugrakoc.SpoolCut.QtPrototype.png")
    copy(Path("LICENSE"), app / "LICENSE")
    copy(Path("qt-prototype/README.md"), app / "Read me.md")
    (app / "source-revision.txt").write_text(output("git", "rev-parse", "HEAD") + "\n", encoding="utf-8")
    return files


def collect_sources(files, app, sources):
    # Debian records installed package ownership and source versions. Keep the
    # exact signed-package source version rather than tracking a moving URL.
    binaries = set()
    for path in files:
        candidates = [path, Path(str(path).removeprefix("/usr"))] if str(path).startswith("/usr/lib/") else [path]
        for candidate in candidates:
            result = subprocess.run(["dpkg-query", "-S", str(candidate)], text=True, capture_output=True)
            if result.returncode == 0:
                binaries.add(result.stdout.split(": ", 1)[0])
                break
        else:
            raise RuntimeError(f"Could not identify Debian package for {path}")
    records = output("dpkg-query", "-W",
                     "-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n",
                     *sorted(binaries)).splitlines()
    manifest = "Binary package\tBinary version\tSource package\tSource version\n" + "\n".join(records) + "\n"
    for directory in (app, sources):
        (directory / "Debian-packages.tsv").write_text(manifest, encoding="utf-8")
    notice_dir = app / "licenses"
    notice_dir.mkdir()
    wanted = set()
    for line in records:
        binary, _, source, version = line.split("\t")
        name = binary.split(":")[0]
        copy(Path("/usr/share/doc") / name / "copyright", notice_dir / (name + ".copyright"))
        wanted.add(f"{source}={version}")

    native = sources / "Debian"
    native.mkdir()
    run("apt-get", "source", "--download-only", "--only-source", *sorted(wanted), cwd=native)
    run("git", "archive", "--format=tar", "-o", sources / "SpoolCut-source.tar", "HEAD")


def main():
    executable, dist = (Path(arg).resolve() for arg in sys.argv[1:])
    if output("git", "status", "--porcelain", "--untracked-files=no"):
        sys.exit("Commit the SpoolCut source before packaging.")
    dist.mkdir(parents=True, exist_ok=True)
    app = dist / "SpoolCutQt.AppDir"
    app.mkdir()
    sources = dist / "SpoolCut-Qt-Linux-sources"
    sources.mkdir()

    runtime = dist / "runtime-x86_64"
    fetch(RUNTIME[0], runtime)
    if digest(runtime) != RUNTIME[1]:
        raise RuntimeError("AppImage runtime digest changed")
    runtime.chmod(0o755)
    notice = sources / "AppImage-runtime"
    notice.mkdir()
    fetch(f"https://codeload.github.com/AppImage/type2-runtime/tar.gz/{RUNTIME_REV}",
          notice / "type2-runtime.tar.gz")
    fetch(f"https://raw.githubusercontent.com/AppImage/type2-runtime/{RUNTIME_REV}/LICENSE",
          app / "AppImage-runtime-LICENSE")
    app_sources = Path("qt-prototype/runtime-sources.sh").resolve()
    run("bash", app_sources, notice, app / "runtime-licenses")
    files = bundle(executable, app)
    collect_sources(files, app, sources)

    fs = dist / "app.squashfs"
    run("mksquashfs", app, fs, "-comp", "zstd", "-noappend", "-processors", "2", "-root-owned")
    image = dist / "SpoolCut-Qt-Fedora-x86_64.AppImage"
    with image.open("wb") as stream:
        for part in (runtime, fs):
            with part.open("rb") as reader:
                shutil.copyfileobj(reader, stream)
    image.chmod(0o755)
    source_archive = dist / (sources.name + ".tar.zst")
    run("tar", "--zstd", "-cf", source_archive, "-C", dist, sources.name)
    for path in (image, source_archive):
        (dist / (path.name + ".sha256")).write_text(f"{digest(path)}  {path.name}\n", encoding="utf-8")


if __name__ == "__main__":
    main()
