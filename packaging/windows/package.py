"""Build a portable SpoolCut ZIP and matching source archive in MSYS2 UCRT64."""

from collections import deque
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.parse
import urllib.request
import zipfile

MSYS2 = "https://repo.msys2.org/mingw"
UCRT = "mingw-w64-ucrt-x86_64-"


def run(*args, capture=False, cwd=None):
    try:
        return subprocess.run(args, check=True, capture_output=capture, cwd=cwd).stdout
    except subprocess.CalledProcessError as error:
        if error.stderr:
            sys.stderr.buffer.write(error.stderr)
            sys.stderr.flush()
        raise


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


def copy_pe_runtime(prefix, bin_dir, roots, files):
    packages = set()
    owners = {path: package for package, paths in files.items() for path in paths}
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
            packages.add(owners[source.relative_to(prefix).as_posix()])
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
    return packages


def installed_files():
    files = {}
    listing = run("pacman", "-Ql", capture=True).decode()
    for line in listing.splitlines():
        package, path = line.split(maxsplit=1)
        if path.startswith("/ucrt64/") and not path.endswith("/"):
            files.setdefault(package, []).append(path.removeprefix("/ucrt64/"))
    return files


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


def verify_sources(msys, directory, filenames):
    # Verify against the MSYS2 keyring installed by pacman without changing
    # its trust database or relying on access to its private key directory.
    keyring = msys / "usr/share/pacman/keyrings"
    revoked = set((keyring / "msys2-revoked").read_text().split())
    msys_bin = msys / "usr/bin"
    cygpath = str(msys_bin / "cygpath.exe")
    def posix_path(path):
        return run(cygpath, "-u", str(path), capture=True).decode().strip()

    verifier = str(msys_bin / "gpgv.exe")
    source_dir = posix_path(directory)
    with tempfile.TemporaryDirectory(prefix="spoolcut-gpg-") as home:
        home = posix_path(home)
        public_keys = f"{home}/msys2.gpg"
        run(str(msys_bin / "gpg.exe"), "--batch", "--homedir", home, "--dearmor",
            "--output", public_keys, posix_path(keyring / "msys2.gpg"))
        for name in filenames:
            archive = f"{source_dir}/{name}"
            status = run(verifier, "--homedir", home, "--keyring", public_keys,
                         "--status-fd", "1", archive + ".sig", archive, capture=True).decode()
            valid = [line.split() for line in status.splitlines()
                     if line.startswith("[GNUPG:] VALIDSIG ")]
            if not valid or any(fields[2] in revoked or fields[-1] in revoked for fields in valid):
                raise RuntimeError(f"Invalid or revoked MSYS2 signature: {name}")


VERSION = Path("VERSION").read_text(encoding="ascii").strip()


def package(prefix, executable, output, scratch):
    bundle = scratch / "SpoolCut"
    bin_dir = bundle / "bin"
    sources = scratch / f"SpoolCut-{VERSION}-Windows-sources"
    native_sources = sources / "MSYS2"
    native_sources.mkdir(parents=True)

    files = installed_files()
    qt_platforms = [p for p in files[UCRT + "qt6-base"] if p.endswith("/platforms/qwindows.dll")]
    if len(qt_platforms) != 1:
        raise RuntimeError(f"Expected one Qt Windows plugin, found {qt_platforms}")
    roots = [(executable, bin_dir / "SpoolCut.exe"),
             (executable.with_name("spoolcut.exe"), bin_dir / "spoolcut-cli.exe"),
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
    shutil.copy2("packaging/windows/PORTABLE.txt", bundle / "Read me.txt")
    revision = run("git", "rev-parse", "HEAD", capture=True).decode()
    (bundle / "source-revision.txt").write_text(revision, encoding="utf-8")
    source_archive = output / (sources.name + ".tar.zst")
    run(str(msys / "usr/bin/tar.exe"), "--force-local", "--zstd", "-cf",
        source_archive.as_posix(), "-C", scratch.as_posix(), sources.name)
    zip_path = output / f"SpoolCut-{VERSION}-Windows-x86_64.zip"
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
    with tempfile.TemporaryDirectory(prefix="spoolcut-package-") as scratch:
        package(prefix, executable, output, Path(scratch))
