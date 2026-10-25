#!/usr/bin/env python3
"""Check live subtree alpha and ordered filters on the MateBook emulator.

Build/install tests/visual-composition first. Requires Pillow. The explicitly
selected target is used as-is; this script never installs or signs an app.
"""
import argparse
import importlib.util
from pathlib import Path
import time

try:
    from PIL import Image
except ImportError:
    Image = None

module = importlib.util.spec_from_file_location(
    "surfaces", Path(__file__).with_name("check-surfaces.py"))
surfaces = importlib.util.module_from_spec(module)
module.loader.exec_module(surfaces)
_images_spec = importlib.util.spec_from_file_location(
    "images", Path(__file__).with_name("check-images.py"))
_images = importlib.util.module_from_spec(_images_spec)
_images_spec.loader.exec_module(_images)


class _PngImage:
    def __init__(self, reader):
        self._reader = reader

    def getpixel(self, xy):
        return self._reader(xy[0], xy[1])


def check(device):
    def shot(name):
        if Image is not None:
            remote = "/data/local/tmp/nsh-composition.jpeg"
            local = device.directory / f"{name}.jpeg"
            device.run("shell", "snapshot_display", "-f", remote)
            device.run("file", "recv", remote, str(local))
            return Image.open(local).convert("RGB")
        remote = "/data/local/tmp/nsh-composition.png"
        local = device.directory / f"{name}.png"
        device.run("shell", "uitest", "screenCap", "-p", remote)
        device.run("file", "recv", remote, str(local))
        return _PngImage(_images.pixels(local))

    def pixel(image, bounds, point, expected):
        left, top, right, bottom = device.bounds(bounds)
        scale = (right - left) / 220
        x, y = point
        actual = image.getpixel((round(left + x * scale), round(top + y * scale)))
        assert max(abs(a - b) for a, b in zip(actual, expected)) < 24, (point, actual, expected)

    def masks(name, shifted=False, composed=True):
        deadline = time.monotonic() + 12
        error = None
        while time.monotonic() < deadline:
            nodes = device.matching("alpha-shadow")
            if len(nodes) != 2:
                error = AssertionError(f"expected two live exports, found {len(nodes)}")
                time.sleep(0.1)
                continue
            image = shot(name)
            try:
                for index, bounds in enumerate(nodes):
                    dx = 60 if shifted and index == 0 else 0
                    pixel(image, bounds, (20 + dx, 20), (255, 0, 0))
                    pixel(image, bounds, (42 + dx, 20), (255, 255, 0))
                    pixel(image, bounds, (46 + dx, 30), (0, 0, 255))
                    pixel(image, bounds, (120, 55), (0, 170, 0))
                    pixel(image, bounds, (146, 65), (0, 0, 255))
                    pixel(image, bounds, (80, 80), (255, 255, 255))
                    pixel(image, bounds, (-8, -6), (170, 0, 255))
                    if dx:
                        pixel(image, bounds, (20, 20), (255, 255, 255))
                for bounds in device.matching("blurred-shadow"):
                    pixel(image, bounds, (40, 35), (54, 54, 54))
                for bounds in device.matching("backdrop-chain"):
                    pixel(image, bounds, (20, 30), (26, 51, 102) if composed else (204, 153, 51))
                    pixel(image, bounds, (100, 30), (204, 153, 51))
                    pixel(image, bounds, (140, 30), (51, 26, 26) if composed else (102,128,230))
                    pixel(image, bounds, (200, 30), (127, 89, 38) if composed else (153,141,141))
                for bounds in device.matching("composition-outline"):
                    left, top, right, bottom = device.bounds(bounds)
                    scale = (right - left) / 100
                    actual = image.getpixel((round(left + scale), round(top + 10 * scale)))
                    assert max(actual) < 24, ("outline must paint above its opaque child", actual)
                return
            except AssertionError as failure:
                error = failure
                time.sleep(0.1)
        raise error

    def exact_press(index, mouse=False):
        before = [int(node["id"].split(":")[1])
                  for node in device.matching("composition-presses:")]
        assert len(before) == 2, before
        node = device.matching("composition-background:")[index]
        if mouse:
            left, top, right, bottom = device.bounds(node)
            device.run("shell", "uinput", "-M", "-m", "0", "0")
            device.run("shell", "uinput", "-M", "-m", str((left + right) // 2),
                       str((top + bottom) // 2), "-c", "0")
        else:
            device.click(node)
        expected = before.copy()
        expected[index] += 1
        device.counts(expected, "composition-presses:")
        # A duplicate asynchronous dispatch must not pass at a transient count.
        time.sleep(0.3)
        device.counts(expected, "composition-presses:")

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    masks("initial", composed=False)
    # A newly opened MateBook window consumes its first click for activation.
    device.button("Status")
    exact_press(1, mouse=True)
    exact_press(1, mouse=True)
    device.click(device.matching("composition-scene:false")[0])
    # Each surface enables its own compositor independently.
    device.click(device.matching("composition-scene:false")[-1])
    masks("composed")
    exact_press(0)
    deadline = time.monotonic() + 12
    while True:
        image = shot("backdrop-updated")
        try:
            bounds = device.matching("backdrop-chain")[0]
            pixel(image, bounds, (20, 30), (102, 77, 51))
            pixel(image, bounds, (140, 30), (0, 0, 128))
            pixel(image, bounds, (200, 30), (0, 38, 140))
            break
        except AssertionError:
            if time.monotonic() > deadline:
                raise
    exact_press(0)
    masks("backdrop-restored")
    exact_press(0, mouse=True)
    exact_press(0, mouse=True)
    masks("mouse-backdrop-restored")
    left, top, right, bottom = device.bounds(device.matching("composition-scroll")[0])
    device.run("shell", "uitest", "uiInput", "swipe", str((left + right) // 2), str(bottom - 20), str((left + right) // 2), str(top + 20), "500")
    deadline = time.monotonic() + 12
    while True:
        offsets = [int(n["id"].split(":")[1]) for n in device.matching("composition-offset:")]
        if len(offsets) == 2 and offsets[0] > 0 and offsets[1] == 0:
            break
        assert time.monotonic() < deadline, ("native scroll input forwarding", offsets)
        time.sleep(.1)
    before = [int(node["id"].split(":")[1])
              for node in device.matching("composition-offset:")]
    device.run("shell", "uinput", "-M", "-m", "0", "0")
    device.run("shell", "uinput", "-M", "-m", str((left + right) // 2),
               str((top + bottom) // 2), "-s", "3")
    deadline = time.monotonic() + 12
    while True:
        offsets = [int(node["id"].split(":")[1])
                   for node in device.matching("composition-offset:")]
        if len(offsets) == 2 and offsets[0] > before[0] and offsets[1] == before[1]:
            break
        assert time.monotonic() < deadline, ("native wheel input forwarding", before, offsets)
        time.sleep(0.1)
    device.click(device.matching("composition-shift:false")[0])
    masks("moved", True)
    for cycle in range(5):
        device.click(device.matching("composition-retire")[0])
        assert len(device.matching("alpha-shadow")) == 1
        device.click(device.matching("composition-remount")[0])
        masks(f"remounted-{cycle}", True)
    before = [int(node["id"].split(":")[1])
              for node in device.matching("composition-presses:")]
    left, top, right, bottom = device.bounds(device.matching("composition-background:")[0])
    device.run("shell", "uinput", "-M", "-m", "0", "0")
    expected = [0, before[1]]
    try:
        device.run("shell", "uinput", "-M", "-m", str((left + right) // 2),
                   str((top + bottom) // 2), "-d", "0")
        deadline = time.monotonic() + 10
        while True:
            states = [node["id"].endswith(":true")
                      for node in device.matching("composition-pressed:")]
            if states == [True, False]:
                break
            assert time.monotonic() < deadline, ("active mouse owner before retirement", states)
            time.sleep(0.1)
        device.button("Reload left")
        device.counts(expected, "composition-presses:")
    finally:
        # Leave no physical mouse button pressed if retirement or an assertion fails.
        device.run("shell", "uinput", "-M", "-u", "0")
    time.sleep(0.3)
    device.counts(expected, "composition-presses:")
    states = [node["id"].endswith(":true")
              for node in device.matching("composition-pressed:")]
    assert states == [False, False], ("retired owner must not hold the new generation pressed", states)
    device.click(device.matching("composition-scene:false")[0])
    exact_press(0, mouse=True)
    print("PASS nested alpha shadows, overflow, ordered filters/backdrops, exact opaque/alpha plus-darker, dynamic background, outline ordering, native touch scrolling and MMI wheel, exactly-once touch/mouse presses, independent surfaces, five export cycles and pressed-owner retirement/recreation")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    options = parser.parse_args()
    options.artifacts.mkdir(parents=True, exist_ok=True)
    check(surfaces.Device(options.target, options.artifacts))
