"""Compare the two release editors on identical footage in a hosted X11 session."""

import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

from Xlib import X, display


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def wait_for(predicate, seconds=5):
    start = time.monotonic()
    while time.monotonic() - start < seconds:
        result = predicate()
        if result:
            return result, (time.monotonic() - start) * 1000
        time.sleep(0.007)
    raise RuntimeError("Timed out waiting for the window or preview frame")


def process_sample(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    pss = next(line for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines()
               if line.startswith("Pss:"))
    return int(fields[11]) + int(fields[12]), int(pss.split()[1]) / 1024


def measure(pid):
    before, _ = process_sample(pid)
    start = time.monotonic()
    time.sleep(5)
    after, pss = process_sample(pid)
    cpu = 100 * (after - before) / (time.monotonic() - start) / os.sysconf("SC_CLK_TCK")
    return round(cpu, 1), round(pss, 1)


def frame(window, width, height):
    b, g, r, _ = window.get_image(int(width * 0.70), int(height * 0.35),
                                  1, 1, X.ZPixmap, 0xffffffff).data[:4]
    if r > 175 and b < 80 and g < 80:
        return "red"
    if b > 175 and r < 80 and g < 80:
        return "blue"
    if g > 150 and r < 80 and b < 80:
        return "green"
    return None


def click(window_id, x, y):
    command("xdotool", "mousemove", "--window", str(window_id), str(x), str(y), "click", "--delay", "0", "1")


def ruler_y(window, height, qt):
    if qt:
        return height - 118
    start = height - 265
    data = window.get_image(140, start, 1, 200, X.ZPixmap, 0xffffffff).data
    natural = [(242, 237, 232), (51, 41, 36)]
    for row in range(194):
        pixels = [data[(row + i) * 4:(row + i) * 4 + 3] for i in range(6)]
        if any(all(all(abs(left - right) <= 2 for left, right in zip(pixel, target))
                   for pixel in pixels) for target in natural):
            return start + row + 4
    raise RuntimeError("GTK time ruler was not found")


def compare(xserver, executable, inputs, qt, env):
    start = time.monotonic()
    with subprocess.Popen([executable, *inputs], env=env) as process:
        try:
            def visible():
                result = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid",
                                         str(process.pid)], capture_output=True, text=True)
                return int(result.stdout.splitlines()[-1]) if result.returncode == 0 else 0

            wid = wait_for(visible, 15)[0]
            startup = (time.monotonic() - start) * 1000
            command("xdotool", "windowsize", str(wid), "1024", "720")
            command("xdotool", "windowfocus", str(wid))
            window = xserver.create_resource_object("window", wid)
            geometry = window.get_geometry()
            width, height = geometry.width, geometry.height

            def color_is(expected):
                return lambda: frame(window, width, height) == expected

            wait_for(color_is("red"), 15)
            idle = measure(process.pid)
            command("xdotool", "key", "space")
            playing = measure(process.pid)
            wait_for(color_is("green"), 4)
            command("xdotool", "key", "space")
            ruler = ruler_y(window, height, qt)
            latency = []
            for i in range(8):
                before = time.monotonic()
                blue = i % 2 == 0
                click(wid, int(width * (0.72 if blue else 0.24)), ruler)
                wait_for(color_is("blue" if blue else "red"))
                latency.append(1000 * (time.monotonic() - before))
            return {"mapped_ms": round(startup, 1), "idle_cpu_pct_of_one_core": idle[0],
                    "idle_pss_mib": idle[1], "playing_cpu_pct_of_one_core": playing[0],
                    "playing_pss_mib": playing[1],
                    "source_switch_median_ms": round(statistics.median(latency), 1)}
        finally:
            process.terminate()
            process.wait(timeout=5)


def main():
    gtk, qt, inspector = [Path(p).resolve() for p in sys.argv[1:]]
    with tempfile.TemporaryDirectory() as temp:
        root = Path(temp)
        alsa = root / "asound.conf"
        alsa.write_text("pcm.!default { type null }\n", encoding="utf-8")
        files = []
        clips = []
        for name, length in (("red", 10), ("blue", 12)):
            path = root / f"{name}.mp4"
            filters = ["-vf", "drawbox=c=lime:t=fill:enable='between(t,6,8)'"] if name == "red" else []
            command("ffmpeg", "-nostdin", "-v", "error", "-f", "lavfi", "-i",
                    f"color={name}:size=1280x720:rate=25:duration={length}", "-f", "lavfi", "-i",
                    f"sine=sample_rate=48000:duration={length}", "-c:v", "libx264",
                    *filters, "-threads", "4", "-tag:v", "avc1", "-x264-params",
                    "keyint=25:min-keyint=25:scenecut=0:open-gop=0", "-c:a", "aac", path)
            info = json.loads(command(inspector, "inspect", path))
            files.append(path)
            clips.append({"source": name, "in_pts": info["safe_in_pts"][0],
                          "out_pts": info["video_end_pts"]})
        project = root / "benchmark.spoolcut.json"
        project.write_text(json.dumps({"sources": {p.stem: p.name for p in files}, "clips": clips}),
                           encoding="utf-8")
        env = dict(os.environ, QT_QPA_PLATFORM="xcb", GDK_BACKEND="x11", GSK_RENDERER="cairo",
                   GTK_A11Y="none", SPOOLCUT_QT_PREVIEW="software", SPOOLCUT_PREVIEW="software",
                   ALSA_CONFIG_PATH=str(alsa))
        xserver = display.Display()
        results = {"GTK": [], "Qt": []}
        apps = [(gtk, [project], False, "GTK"), (qt, [project], True, "Qt")]
        for trial in range(3):
            for app, inputs, is_qt, label in apps[::(-1 if trial % 2 else 1)]:
                values = compare(xserver, app, inputs, is_qt, env)
                results[label].append(values)
                print(label, json.dumps(values), flush=True)
        medians = {name: {key: statistics.median(run[key] for run in runs) for key in runs[0]}
                   for name, runs in results.items()}
        print("Median", json.dumps(medians, sort_keys=True), flush=True)


if __name__ == "__main__":
    main()
