"""Build the Qt desktop and CLI portable ZIP with corresponding source."""

from concurrent.futures import ThreadPoolExecutor
import hashlib
from pathlib import Path
import shutil
import sys
import tempfile
import urllib.parse
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "packaging/windows"))
from package import (
    MSYS2, UCRT, copy_file, copy_pe_runtime, download_source, installed_files,
    package_database, run, source_url, verify_sources,
)


VERSION = Path("VERSION").read_text(encoding="ascii").strip()


def package(prefix, executable, output, scratch):
    bundle = scratch / "SpoolCut Qt"
    bin_dir = bundle / "bin"
    sources = scratch / f"SpoolCut-{VERSION}-Qt-Windows-sources"
    native_sources = sources / "MSYS2"
    native_sources.mkdir(parents=True)

    files = installed_files()
    qt_platforms = [p for p in files[UCRT + "qt6-base"] if p.endswith("/platforms/qwindows.dll")]
    if len(qt_platforms) != 1:
        raise RuntimeError(f"Expected one Qt Windows plugin, found {qt_platforms}")
    roots = [(executable, bin_dir / "SpoolCutQt.exe"),
             (executable.with_name("spoolcut.exe"), bin_dir / "spoolcut.exe"),
             (prefix / qt_platforms[0], bin_dir / "plugins/platforms/qwindows.dll")]
    for tool in ("ffmpeg.exe", "ffprobe.exe"):
        roots.append((prefix / "bin" / tool, bin_dir / tool))
    packages = copy_pe_runtime(prefix, bin_dir, roots, files)
    (bin_dir / "qt.conf").write_text("[Paths]\nPlugins=plugins\n", encoding="utf-8")

    msys = prefix.parent
    database = package_database(msys)
    versions = dict(line.split(maxsplit=1)
                    for line in run("pacman", "-Q", capture=True).decode().splitlines())
    packages.update(UCRT + name for name in ("crt", "headers", "gcc"))
    filenames = set()
    inventory = ["Package\tVersion\tLicenses\tBinary SHA-256\tBinary package\tCorresponding source"]
    for name in sorted(packages):
        record = database[name]
        version = versions[name]
        if version != record["VERSION"]:
            raise RuntimeError(f"Installed and repository versions of {name} differ")
        filename = f"{record['BASE']}-{version.replace(':', '~')}.src.tar.zst"
        filenames.add(filename)
        licenses = ", ".join(record.get("LICENSE", "").splitlines())
        url = MSYS2 + "/ucrt64/" + urllib.parse.quote(record["FILENAME"])
        inventory.append(f"{name}\t{version}\t{licenses}\t{record['SHA256SUM']}\t{url}\t"
                         + source_url(filename))
        for path in files[name]:
            if path.startswith("share/licenses/"):
                copy_file(prefix, path, bundle)

    for directory in (bundle, sources):
        (directory / "MSYS2-packages.tsv").write_text("\n".join(inventory) + "\n", encoding="utf-8")
    print(f"Collecting {len(filenames)} corresponding MSYS2 sources", flush=True)
    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(lambda name: download_source(name, native_sources), sorted(filenames)))
    verify_sources(msys, native_sources, sorted(filenames))

    run("git", "archive", "--format=tar", "-o", str(sources / "SpoolCut-source.tar"), "HEAD")
    shutil.copy2("LICENSE", bundle)
    shutil.copy2("qt-prototype/README.md", bundle / "Read me.md")
    revision = run("git", "rev-parse", "HEAD", capture=True).decode()
    (bundle / "source-revision.txt").write_text(revision, encoding="utf-8")
    source_archive = output / (sources.name + ".tar.zst")
    run(str(msys / "usr/bin/tar.exe"), "--force-local", "--zstd", "-cf",
        source_archive.as_posix(), "-C", scratch.as_posix(), sources.name)
    zip_path = output / f"SpoolCut-{VERSION}-Qt-Windows-x86_64.zip"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(bundle.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(scratch))
    for path in (zip_path, source_archive):
        with path.open("rb") as file:
            digest = hashlib.file_digest(file, "sha256").hexdigest()
        (output / (path.name + ".sha256")).write_text(f"{digest}  {path.name}\n")


if __name__ == "__main__":
    prefix, executable, output = (Path(arg).resolve() for arg in sys.argv[1:])
    if run("git", "status", "--porcelain", "--untracked-files=no", capture=True).strip():
        sys.exit("Commit the SpoolCut source before packaging.")
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="spoolcut-qt-") as scratch:
        package(prefix, executable, output, Path(scratch))
