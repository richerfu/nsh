#!/usr/bin/env python3
"""Check transformed touches, zoom motion, keyboard tap policy and text decoration pixels.

Build the interaction or text-input fixture matching --kind before running.
The caller selects the HDC target; this script never signs or installs apps.
"""
import argparse
import importlib.util
import subprocess
from pathlib import Path
import tempfile
import time


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def check(device, kind, directory):
    def wait(prefix, predicate=lambda values: bool(values)):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            values = device.matching(prefix)
            if predicate(values):
                return values
            time.sleep(0.1)
        raise AssertionError((prefix, values))

    def click(identifier):
        device.click(wait(identifier)[0])

    def state(identifier):
        wait(identifier)

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    if kind == "gestures":
        wait("hit:none", lambda ns: len(ns) == 2)
        click("rotated-pad")
        hit = wait("hit:rotated:")[0]["id"].split(":")
        assert abs(int(hit[2]) - 50) <= 2 and abs(int(hit[3]) - 30) <= 2, hit
        left, top, right, bottom = device.bounds(wait("clip-box")[0])
        assert right + 20 < device.bounds(wait("zoom-scroll")[0])[2], "fixture needs a wider surface"
        device.run("shell", "uitest", "uiInput", "click", str(right + 20), str((top + bottom) // 2))
        assert wait("hit:rotated:")[0]["id"].split(":") == hit
        device.run("shell", "uitest", "uiInput", "click", str(right - 10), str((top + bottom) // 2))
        wait("hit:clip:")
        click("zoom-animated")
        zoom = wait("zoom:2.00:")[0]["id"].split(":")
        assert int(zoom[2]) >= 3, zoom
        click("rotated-pad")
        click("zoomed-pad")
        hit = wait("hit:zoomed:")[0]["id"].split(":")
        assert 0 <= int(hit[2]) <= 100 and 0 <= int(hit[3]) <= 60, hit
        click("center-zoom")
        viewport = device.bounds(wait("center-scroll")[0])
        pad = device.bounds(wait("centered-pad")[0])
        assert abs(pad[0] - viewport[0] - (viewport[2] - viewport[0]) / 4) <= 3, (pad, viewport)
        assert abs(pad[1] - viewport[1] - (viewport[3] - viewport[1]) / 4) <= 3, (pad, viewport)
        click("centered-pad")
        hit = wait("hit:centered:")[0]["id"].split(":")
        assert abs(int(hit[2]) - 40) <= 2 and abs(int(hit[3]) - 20) <= 2, hit
        left, top, right, bottom = device.bounds(wait("snap-scroll")[0])
        x = (left + right) // 2
        device.run("shell", "uitest", "uiInput", "swipe", str(x), str(bottom - 20), str(x), str(bottom - 60), "2000")
        state("snap:100")
        time.sleep(0.5)
        state("snap:100")
        click("snap-jump")
        state("snap:500")
        click("zoom-reset")
        wait("zoom:1.00:", lambda ns: len(ns) == 2)
        left, top, right, bottom = device.bounds(wait("zoomed-pad")[0])
        old_center = (left + right) / 2
        cx, cy = int(old_center - 25), (top + bottom) // 2
        viewport = device.bounds(wait("zoom-scroll")[0])
        assert viewport[0] < cx - 120 < cx + 120 < viewport[2] and viewport[1] < cy < viewport[3], "pinch contacts must stay inside the surface"
        device.run("shell", "uinput", "-T", "-m", str(cx - 40), str(cy), str(cx - 80), str(cy), str(cx + 40), str(cy), str(cx + 80), str(cy))
        values = wait("zoom:", lambda ns: any(abs(float(n["id"].split(":")[1]) - 2) < 0.05 for n in ns))
        actual_scale = float(values[0]["id"].split(":")[1])
        left, top, right, bottom = device.bounds(wait("zoomed-pad")[0])
        assert abs((left + right) / 2 - (cx + (old_center - cx) * actual_scale)) <= 5, (left, right, cx, old_center, actual_scale)
        gesture = subprocess.Popen([*device.hdc, "shell", "uinput", "-T", "-m", str(cx - 40), str(cy), str(cx - 120), str(cy), str(cx + 40), str(cy), str(cx + 120), str(cy), "-k", "2000"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            peak = 0.0
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                peak = max(peak, *(float(n["id"].split(":")[1]) for n in wait("zoom:")))
                if peak > 3.05:
                    break
            assert 3.05 < peak <= 4.5, peak
        finally:
            gesture.wait(timeout=10)
        wait("zoom:3.00:")
        assert len(wait("zoom:1.00:0")) == 1 and len(wait("hit:none")) == 1
        print("PASS: transformed/scaled touch coordinates, clipping with hit slop, intermediate zoom frames, focal pinch and bounce, centered content, one-step snapping, controller animation and surface isolation")
        return

    wait("input:never:false:0", lambda ns: len(ns) == 2)
    for identifier, expected in [
        ("editor", "input:never:true:0"),
        ("inside-button", "input:never:false:0"),
        ("policy-always", "input:always:false:0"),
        ("editor", "input:always:true:0"),
        ("inside-button", "input:always:true:1"),
        ("policy-handled", "input:handled:true:1"),
        ("inside-button", "input:handled:true:2"),
        ("blank-pad", "input:handled:false:2"),
    ]:
        click(identifier)
        state(expected)
        if identifier == "inside-button" and expected == "input:never:false:0":
            assert len(wait("feedback:0:0")) == 2
    state("feedback:2:2")
    click("a11y-checkbox")
    states = wait("a11y-checkbox", lambda ns: [n.get("checked") for n in ns] == ["true", "false"])
    assert [n["checked"] for n in states] == ["true", "false"], states
    assert [n["selected"] for n in states] == ["true", "false"], states
    click("policy-always")
    click("editor")
    state("input:always:true:2")
    left, top, right, bottom = device.bounds(wait("blank-pad")[0])
    x = (left + right) // 2
    device.run("shell", "uitest", "uiInput", "swipe", str(x), str(bottom - 10), str(x), str(top + 10), "400")
    state("input:always:false:2")
    remote = "/data/local/tmp/nsh-text-decoration-check.png"
    local = directory / "decorations.png"
    device.run("shell", "uitest", "screenCap", "-p", remote)
    device.run("file", "recv", remote, str(local))
    pixels = module("images", "check-images.py").pixels(local)
    solid_y = None
    for identifier, color in [("decoration-solid", (239, 68, 68)), ("decoration-wave", (59, 130, 246))]:
        left, top, right, bottom = device.bounds(wait(identifier)[0])
        rows = [sum(max(abs(a - b) for a, b in zip(pixels(x, y), color)) <= 6 for x in range(left, right)) for y in range(top, bottom + 12)]
        assert sum(rows) > 100, (identifier, rows)
        if identifier == "decoration-solid":
            # Three vp becomes six physical pixels on this emulator; allow
            # other densities while requiring a continuous, thick line.
            strong = [y for y, count in enumerate(rows) if count > max(rows) * 0.8]
            assert len(strong) >= 3 and max(strong) - min(strong) == len(strong) - 1, rows
            solid_y = sum(y * count for y, count in enumerate(rows)) / sum(rows)
        else:
            assert sum(count > 0 for count in rows) >= 5, rows
    click("decoration-offset:")
    state("decoration-offset:8")
    device.run("shell", "uitest", "screenCap", "-p", remote)
    device.run("file", "recv", remote, str(local))
    shifted = module("images_shifted", "check-images.py").pixels(local)
    left, top, right, bottom = device.bounds(wait("decoration-solid")[0])
    rows = [sum(max(abs(a - b) for a, b in zip(shifted(x, y), (239, 68, 68))) <= 6 for x in range(left, right)) for y in range(top, bottom + 12)]
    new_y = sum(y * count for y, count in enumerate(rows)) / sum(rows)
    assert abs(new_y - solid_y - 4 * (bottom - top) / 42) <= 2, (solid_y, new_y, bottom - top)
    assert len(wait("input:never:false:0")) == 1
    print("PASS: never/always/handled keyboard taps, native checked/selected metadata, point-sized and wavy decoration pixels, and surface isolation")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--kind", choices=["gestures", "text"], required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nsh-interaction-") as tmp:
        directory = Path(tmp)
        check(module("surfaces", "check-surfaces.py").Device(args.target, directory), args.kind, directory)
