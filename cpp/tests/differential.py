# SPDX-License-Identifier: GPL-3.0-only
"""Exercise the Rust regression fixtures and compare JSON and exported packets."""

import argparse
import contextlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
from decimal import Decimal


def run(*args, cwd=None, failure=None):
    command = [os.fspath(arg) for arg in args]
    result = subprocess.run(command, cwd=cwd, capture_output=True, timeout=45)
    error = result.stderr.decode("utf-8", errors="replace")
    if failure is None:
        assert result.returncode == 0, (command, result.returncode, error)
    else:
        assert result.returncode == 1 and failure in error, (command, result.returncode, error)
    return result.stdout


def fixture(path, codec, variant, time_scale, *, open_gop=False, sample_rate=48000):
    avc = codec in {"avc", "vfr"}
    rate = "25" if avc else "30000/1001"
    encoder, tag = ("libx264", "avc1") if avc else ("libx265", "hvc1")
    args = [
        "-nostdin", "-v", "error", "-f", "lavfi", "-i", f"testsrc2=size=96x64:rate={rate}:duration=3",
        "-f", "lavfi", "-i", f"sine=frequency={440 + 440 * variant}:sample_rate={sample_rate}:duration=3",
        "-c:v", encoder, "-tag:v", tag, "-threads:v", "1",
    ]
    options = f"keyint=16:min-keyint=16:scenecut=0:open-gop={int(open_gop)}:bframes={0 if codec == 'vfr' else 2}"
    if not avc:
        options += ":pools=none:frame-threads=1:log-level=error"
    args += ["-x264-params" if avc else "-x265-params", options]
    video_filter = f"trim=end_frame=64,hue=h={variant * 90}"
    if codec == "vfr":
        video_filter += ",setpts=N+floor((N+8)/16)"
    if codec == "main10":
        args += ["-pix_fmt", "yuv420p10le", "-profile:v", "main10"]
    run(
        "ffmpeg", *args, "-fps_mode", "vfr", "-vf", video_filter, "-c:a", "aac", "-ac", "2",
        "-video_track_timescale", str(time_scale), "-movflags", "+faststart", path,
    )


def probe(path, *args):
    return json.loads(run("ffprobe", "-v", "error", *args, "-of", "json", "-i", path))


def packets(path, stream):
    return probe(
        path, "-select_streams", stream, "-show_packets", "-show_data_hash", "sha256",
        "-show_entries", "packet=pts,pts_time,flags,data_hash",
    )["packets"]


def frames(path):
    decoded = run(
        "ffmpeg", "-nostdin", "-v", "error", "-xerror", "-threads", "1", "-i", path,
        "-map", "0:v:0", "-fps_mode", "passthrough", "-threads", "1", "-f", "framemd5", "-",
    )
    return [line.rsplit(b",", 1)[-1].strip() for line in decoded.splitlines() if not line.startswith(b"#")]


def unanimous(clis, *args, cwd=None):
    results = [json.loads(run(cli, *args, cwd=cwd)) for cli in clis]
    assert all(result == results[0] for result in results), results
    return results[0]


def save_project(path, sources, clips):
    path.write_text(json.dumps({"sources": sources, "clips": clips}, ensure_ascii=False), encoding="utf-8")


def clip(source, start, end):
    return {"source": source, "in_pts": start, "out_pts": end}


def selected(items, bounds):
    return items[bounds["start"]:bounds["end"]]


def verify_output(output, plan, source_packets, expected_frames, aligned):
    frame_hashes = frames(output)
    assert frame_hashes == expected_frames, output
    actual = [packets(output, stream) for stream in ("v:0", "a:0")]
    offsets = [0, 0]
    av_offsets = []
    for entry in plan["clips"]:
        video, audio = source_packets[entry["source"]]
        selected_video = selected(video, entry["video_packets"])
        selected_audio = selected(audio, entry["audio_packets"])
        start = Decimal(entry["timeline_start_us"]) / 1_000_000
        duration = Decimal(entry["duration_us"]) / 1_000_000
        source_start = Decimal(selected_video[0]["pts_time"])
        video_start = Decimal(actual[0][offsets[0]]["pts_time"])
        audio_start = Decimal(actual[1][offsets[1]]["pts_time"])
        assert "K" in actual[0][offsets[0]]["flags"]
        assert abs(video_start - start) < Decimal("0.001")
        if aligned:
            assert abs(audio_start - video_start) < Decimal("0.001")
        av_offsets.append(audio_start - video_start)
        aac_frame = Decimal(1024) / 48000
        assert start - Decimal("0.001") <= audio_start < start + aac_frame + Decimal("0.001")

        for stream, expected in enumerate((selected_video, selected_audio)):
            count = len(expected)
            copied = actual[stream][offsets[stream]:offsets[stream] + count]
            assert len(copied) == count
            if stream == 1:
                assert Decimal(copied[-1]["pts_time"]) + aac_frame <= start + duration + Decimal("0.001")
            for before, after in zip(expected, copied, strict=True):
                assert before["data_hash"] == after["data_hash"], (output, entry, before, after)
                delta = Decimal(after["pts_time"]) - (Decimal(before["pts_time"]) - source_start + start)
                assert abs(delta) <= Decimal("0.001001"), (output, entry, before, after)
            offsets[stream] += count

    assert offsets == list(map(len, actual))
    info = probe(output, "-show_entries", "format=duration:stream=profile:stream_disposition=default")
    assert len(info["streams"]) == 2
    assert all(stream["disposition"]["default"] == 1 for stream in info["streams"])
    duration = Decimal(info["format"]["duration"])
    assert abs(duration - Decimal(plan["duration_us"]) / 1_000_000) <= Decimal("0.002")
    print(f"{output.name}: {len(frame_hashes)} frames, packets {offsets}, {duration} s, A/V offsets {av_offsets}")
    return frame_hashes, actual, info, av_offsets


def sequence(case, root, clis, core_tests):
    # Distinct payloads, same decoder configuration. Include apostrophe, Unicode, space and leading dash.
    a = root / "A's kamera görüntüsü.mp4"
    b = root / "-B camera file.mp4"
    scales = (30000, 90000) if case in {"hevc", "main10"} else (12800, 12800)
    fixture(a, case, 0, scales[0])
    fixture(b, case, 1, scales[1])
    ia, ib = [unanimous(clis, "inspect", source) for source in (a, b)]
    a_cuts, b_cuts = ia["safe_in_pts"], ib["safe_in_pts"]
    assert len(a_cuts) == len(b_cuts) == 4
    gop = 2 if case == "vfr" else 3
    end = a_cuts[3] if case == "vfr" else ia["video_end_pts"]

    project = root / "kurgu ğ.json"
    save_project(project, {"a": a.name, "b": b.name}, [
        clip("a", a_cuts[0], a_cuts[1]), clip("b", b_cuts[1], b_cuts[2]), clip("a", a_cuts[gop], end),
    ])
    elsewhere = root / "different cwd"
    elsewhere.mkdir()
    plan = unanimous(clis, "plan", project, cwd=elsewhere)
    expected_us = {"avc": 1920000, "hevc": 1601600, "main10": 1601600, "vfr": 2040000}[case]
    assert abs(plan["duration_us"] - expected_us) <= 1
    assert [entry["source"] for entry in plan["clips"]] == ["a", "b", "a"]

    if case == "avc":
        long_dir = root / "export # çığ ' space"
        for _ in range(5):
            long_dir /= "long directory component with spaces and unicode ş"
        long_dir.mkdir(parents=True)
        destination = long_dir / "birleşik video.mkv"
        run(core_tests, project, destination)
        assert destination.is_file(), destination

    a_frames, b_frames = frames(a), frames(b)
    expected = a_frames[:16] + b_frames[16:32] + a_frames[16 * gop:16 * (gop + 1)]
    source_packets = {
        source: [packets(path, stream) for stream in ("v:0", "a:0")]
        for source, path in (("a", a), ("b", b))
    }
    summaries = []
    for index, cli in enumerate(clis):
        name = "cpp" if index == 0 else "rust"
        output = root / f"{name} {case}.mkv"
        run(cli, "export", project, output, cwd=elsewhere)
        summaries.append(verify_output(output, plan, source_packets, expected, aligned=case == "avc"))
        if case == "main10":
            assert summaries[-1][2]["streams"][0]["profile"] == "Main 10"
    assert all(summary == summaries[0] for summary in summaries), "differential export mismatch"


def rejected(root, clis):
    source = root / "open gop.mp4"
    other = root / "different codec.mp4"
    different_audio = root / "different audio.mp4"
    mov = root / "camera.mov"
    fixture(source, "hevc", 0, 30000, open_gop=True)
    fixture(other, "avc", 1, 12800)
    fixture(different_audio, "avc", 1, 12800, sample_rate=44100)
    fixture(mov, "avc", 0, 12800)
    assert len(unanimous(clis, "inspect", mov)["safe_in_pts"]) == 4
    info = unanimous(clis, "inspect", source)
    other_info = unanimous(clis, "inspect", other)
    audio_info = unanimous(clis, "inspect", different_audio)
    flagged = next(
        packet["pts"] for packet in packets(source, "v:0")
        if "K" in packet["flags"] and packet["pts"] not in info["safe_in_pts"]
    )
    project = root / "reject.json"
    output = root / "reject.mkv"
    sources = {"a": source.name, "b": other.name, "c": different_audio.name}
    a_valid = clip("a", info["safe_in_pts"][0], info["video_end_pts"])
    b_valid = clip("b", *other_info["safe_in_pts"][:2])
    c_valid = clip("c", *audio_info["safe_in_pts"][:2])
    cases = [
        ([clip("a", flagged, info["video_end_pts"])], "not a safe IDR boundary"),
        ([clip("a", info["safe_in_pts"][0], flagged)], "not a safe IDR boundary or video end"),
        ([clip("b", b_valid["out_pts"], b_valid["in_pts"])], "in_pts must be less"),
        ([clip("b", 9007199254740993, 9007199254740995)], "in_pts 9007199254740993"),
        ([a_valid, b_valid], "video streams differ"),
        ([b_valid, c_valid], "audio streams differ"),
        ([clip("missing", 0, 100)], "unknown source"),
        ([], "project has no clips"),
    ]
    for clips, message in cases:
        save_project(project, sources, clips)
        for cli in clis:
            run(cli, "plan", project, failure=message)
            run(cli, "export", project, output, failure=message)
            assert not output.exists()
    save_project(project, sources, [b_valid])
    for cli in clis:
        run(cli, "export", project, output.with_suffix(".mp4"), failure=".mkv extension")
        output.write_bytes(b"existing")
        run(cli, "export", project, output, failure="output already exists")
        assert output.read_bytes() == b"existing"
        output.unlink()
        run(cli, "inspect", root / "missing.mp4", failure="inspecting")
    assert not list(root.glob(".spoolcut-*"))

    if os.name != "nt":
        child = root / "subdirectory"
        child.mkdir()
        link_home = root / "outside"
        link_home.mkdir()
        link = link_home / "alias"
        link.symlink_to(child, target_is_directory=True)
        via_parent = link / ".."
        assert unanimous(clis, "inspect", via_parent / other.name) == other_info
        assert unanimous(clis, "plan", via_parent / project.name) == unanimous(clis, "plan", project)
        run(clis[0], "export", via_parent / project.name, via_parent / output.name)
        assert output.is_file() and not (link_home / output.name).exists()


@contextlib.contextmanager
def interrupt_process(child):
    if os.name != "nt":
        child.send_signal(signal.SIGINT)
        yield
        return

    # Attach only to the child's private console. Sending Ctrl+C to the
    # runner's shared console would also signal the test harness and CTest.
    import ctypes

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.FreeConsole()
    if not kernel.AttachConsole(child.pid):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        if not kernel.SetConsoleCtrlHandler(None, True):
            raise ctypes.WinError(ctypes.get_last_error())
        if not kernel.GenerateConsoleCtrlEvent(0, 0):  # CTRL_C_EVENT, all processes on this console
            raise ctypes.WinError(ctypes.get_last_error())
        yield
    finally:
        kernel.FreeConsole()


def interrupt(root, clis):
    a = root / "a.mp4"
    fixture(a, "avc", 0, 12800)
    info = unanimous(clis, "inspect", a)
    project = root / "interrupt.json"
    save_project(project, {"a": a.name}, [clip("a", *info["safe_in_pts"][:2])] * 25)
    output = root / "interrupt.mkv"
    for cli in clis:
        flags = subprocess.CREATE_NEW_CONSOLE if os.name == "nt" else 0
        with subprocess.Popen([cli, "export", project, output], stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, creationflags=flags) as child:
            try:
                deadline = time.monotonic() + 12
                while not list(root.glob(".spoolcut-*")) and child.poll() is None:
                    assert time.monotonic() < deadline, f"{cli} did not reach staging"
                    time.sleep(0.005)
                assert child.poll() is None and list(root.glob(".spoolcut-*"))
                with interrupt_process(child):
                    _, errors = child.communicate(timeout=10)
                assert child.returncode == 130 and b"cancelled" in errors, (cli, child.returncode, errors)
            finally:
                if child.poll() is None:
                    child.kill()
        assert not output.exists() and not list(root.glob(".spoolcut-*"))
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("case", choices=("avc", "hevc", "main10", "vfr", "rejected", "interrupt"))
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--core-tests", type=Path, required=True)
    parser.add_argument("--reference", type=Path)
    args = parser.parse_args()
    clis = [args.cli.resolve()]
    if args.reference:
        clis.append(args.reference.resolve())
    with tempfile.TemporaryDirectory(prefix="spoolcut-differential-") as tmp:
        root = Path(tmp)
        if args.case == "interrupt":
            return interrupt(root, clis)
        if args.case == "rejected":
            rejected(root, clis)
        else:
            sequence(args.case, root, clis, args.core_tests.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
