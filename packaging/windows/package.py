"""Build a portable SpoolCut ZIP and matching source archive in MSYS2 UCRT64."""

import argparse
from collections import deque
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import time
import urllib.parse
import urllib.request
import zipfile

MSYS2 = "https://repo.msys2.org/mingw"
UCRT = "mingw-w64-ucrt-x86_64-"


def run(*args, capture=False, cwd=None):
    return subprocess.run(args, check=True, capture_output=capture, cwd=cwd).stdout


def package_database(msys):
    db_path = msys / "var/lib/pacman/sync/ucrt64.db"
    data = run(str(msys / "usr/bin/zstd.exe"), "-dc", db_path.as_posix(), capture=True)
    packages = {}
    with tarfile.open(fileobj=io.BytesIO(data)) as database:
        for member in database:
            if not member.name.endswith("/desc"):
                continue
            content = database.extractfile(member).read().decode("utf-8")
            fields = {
                name.strip("%"): value
                for name, value in (block.split("\n", 1) for block in content.strip().split("\n\n"))
            }
            packages[fields["NAME"]] = fields
    return packages


def copy_runtime(prefix, bundle, release):
    bin_dir = bundle / "bin"
    bin_dir.mkdir(parents=True)
    roots = [
        (release / "spoolcut-desktop.exe", bin_dir / "SpoolCut.exe"),
        (release / "spoolcut.exe", bin_dir / "spoolcut-cli.exe"),
        (prefix / "bin/ffmpeg.exe", bin_dir / "ffmpeg.exe"),
        (prefix / "bin/ffprobe.exe", bin_dir / "ffprobe.exe"),
    ]
    pixbuf = prefix / "lib/gdk-pixbuf-2.0"
    loaders = sorted(pixbuf.glob("*/loaders/*.dll"))
    roots.extend((dll, bundle / dll.relative_to(prefix)) for dll in loaders)
    if loaders:
        query_loaders = prefix / "bin/gdk-pixbuf-query-loaders.exe"
        roots.append((query_loaders, bin_dir / query_loaders.name))

    packages = set()
    known_dlls = {p.name.lower(): p for p in (prefix / "bin").glob("*.dll")}
    pending = deque(roots)
    copied = set()
    while pending:
        source, target = pending.popleft()
        if target in copied:
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        copied.add(target)
        if source.is_relative_to(prefix):
            msys_path = "/ucrt64/" + source.relative_to(prefix).as_posix()
            packages.add(run("pacman", "-Qoq", msys_path, capture=True).decode().strip())
        imports = run("objdump", "-p", str(source), capture=True).decode(errors="replace")
        for name in re.findall(r"DLL Name:\s+(\S+)", imports, re.IGNORECASE):
            lower = name.lower()
            if lower == "msys-2.0.dll":
                raise RuntimeError(f"{source.name} depends on the MSYS runtime")
            if lower in known_dlls:
                dependency = known_dlls[lower]
                pending.append((dependency, bin_dir / dependency.name))
            elif not (
                lower.startswith(("api-ms-win-", "ext-ms-win-"))
                or (Path(os.environ["SystemRoot"]) / "System32" / lower).is_file()
            ):
                raise RuntimeError(f"Missing DLL {name} imported by {source.name}")

    for package in ("gtk4", "glib2", "fontconfig", "adwaita-icon-theme", "hicolor-icon-theme"):
        owner = UCRT + package
        packages.add(owner)
        for path in installed_files(owner):
            if path.startswith((
                "share/icons/", "share/glib-2.0/schemas/", "share/gtk-4.0/",
                "share/locale/", "etc/fonts/",
            )):
                copy_file(prefix, path, bundle)
    schemas = bundle / "share/glib-2.0/schemas"
    if schemas.is_dir():
        run(str(prefix / "bin/glib-compile-schemas.exe"), "--strict", str(schemas))
    if loaders:
        helper = bin_dir / "gdk-pixbuf-query-loaders.exe"
        run(str(helper), "--update-cache")
        helper.unlink()
    return packages


def installed_files(package):
    for path in run("pacman", "-Qlq", package, capture=True).decode().splitlines():
        if path.startswith("/ucrt64/") and not path.endswith("/"):
            yield path.removeprefix("/ucrt64/")


def copy_file(prefix, path, destination):
    source = prefix / path
    if source.is_file():
        target = destination / path
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def source_url(name):
    return f"{MSYS2}/sources/{urllib.parse.quote(name)}"


def download_source(name, directory):
    for suffix in ("", ".sig"):
        url = source_url(name + suffix)
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=60) as response:
                    with (directory / (name + suffix)).open("wb") as output:
                        shutil.copyfileobj(response, output)
                break
            except OSError as error:
                if attempt == 2 or getattr(error, "code", None) == 404:
                    raise RuntimeError(f"Cannot fetch corresponding source: {url}") from error
                time.sleep(2 ** attempt)


def write_archives(prefix, bundle, sources, output, packages):
    msys = prefix.parent
    database = package_database(msys)
    versions = dict(
        entry.split(maxsplit=1)
        for entry in run("pacman", "-Q", capture=True).decode().splitlines()
    )
    inventory = ["Package\tVersion\tLicenses\tBinary SHA-256\tBinary package\tCorresponding source\n"]
    filenames = set()
    # Rust's standard library is linked into the application executables.
    packages.add(UCRT + "rust")
    for package in sorted(packages):
        record = database[package]
        version = versions[package]
        if record["VERSION"] != version:
            raise RuntimeError(f"Installed and repository versions of {package} differ")
        name = f"{record['BASE']}-{version.replace(':', '~')}.src.tar.zst"
        filenames.add(name)
        license_ids = ", ".join(record.get("LICENSE", "").splitlines())
        inventory.append(
            f"{package}\t{version}\t{license_ids}\t{record['SHA256SUM']}\t"
            f"{MSYS2}/ucrt64/{urllib.parse.quote(record['FILENAME'])}\t{source_url(name)}\n"
        )
        for path in installed_files(package):
            if path.startswith("share/licenses/"):
                copy_file(prefix, path, bundle)

    manifest = "".join(inventory)
    (bundle / "MSYS2-packages.tsv").write_text(manifest, encoding="utf-8")
    (sources / "MSYS2-packages.tsv").write_text(manifest, encoding="utf-8")
    native_sources = sources / "MSYS2"
    native_sources.mkdir()
    filenames = sorted(filenames)
    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(lambda name: download_source(name, native_sources), filenames))
    # Use MSYS2 Bash explicitly. Windows may resolve an unqualified bash.exe
    # to the WSL launcher before consulting PATH.
    bash = str(msys / "usr/bin/bash.exe")
    for name in filenames:
        archive = native_sources / name
        run(bash, "/usr/bin/pacman-key", "--verify",
            archive.with_name(name + ".sig").as_posix(), archive.as_posix())

    # Include the tracked source at this exact revision and locked Rust dependencies.
    run("git", "archive", "--format=tar", "-o", str(sources / "SpoolCut-source.tar"), "HEAD")
    cargo_config = run("cargo", "vendor", "--locked", str(sources / "vendor"), capture=True)
    # Cargo prints the local absolute vendor path. Use a relocatable path in the archive.
    cargo_config = cargo_config.decode()
    (sources / "vendor-config.toml").write_text(
        re.sub(r'(?m)^directory = ".*"$', 'directory = "vendor"', cargo_config),
        encoding="utf-8",
    )
    notices = bundle / "share/licenses/rust-crates"
    for crate in (sources / "vendor").iterdir():
        for license_file in crate.iterdir():
            if license_file.is_file() and license_file.name.upper().startswith(
                ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE")
            ):
                target = notices / crate.name / license_file.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(license_file, target)
    shutil.copy2("LICENSE", bundle / "LICENSE")
    shutil.copy2("packaging/windows/PORTABLE.txt", bundle / "Read me.txt")
    run(str(msys / "usr/bin/tar.exe"), "--force-local", "--zstd", "-cf",
        (output / "SpoolCut-Windows-sources.tar.zst").as_posix(),
        "-C", sources.parent.as_posix(), sources.name)
    with zipfile.ZipFile(output / "SpoolCut-Windows-x86_64.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(bundle.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(bundle.parent))
    for path in output.iterdir():
        if path.suffix in (".zip", ".zst"):
            with path.open("rb") as file:
                digest = hashlib.file_digest(file, "sha256").hexdigest()
            (output / (path.name + ".sha256")).write_text(f"{digest}  {path.name}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path, help="native Windows path to the UCRT64 prefix")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if os.environ.get("MSYSTEM") != "UCRT64":
        parser.error("run this script from MSYS2 UCRT64")
    # The source archive includes the committed tree. Refuse to package an
    # uncommitted source tree whose binaries could differ from that archive.
    dirty = run("git", "diff", "--name-only", "HEAD", "--", capture=True)
    if dirty:
        parser.error("commit the source changes before packaging: " + dirty.decode().strip())
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="spoolcut-package-") as temp:
        workspace = Path(temp)
        bundle = workspace / "SpoolCut"
        sources = workspace / "SpoolCut-Windows-sources"
        sources.mkdir()
        packages = copy_runtime(args.prefix, bundle, Path("target/release"))
        write_archives(args.prefix, bundle, sources, output, packages)


if __name__ == "__main__":
    main()
