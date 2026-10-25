#!/usr/bin/env python3
"""Check held selection edge scrolling on the MateBook DevEco emulator."""
import argparse
import importlib.util
from pathlib import Path
import re
import subprocess
import time

from PIL import Image


def check(args):
    assert re.fullmatch(r"127\.0\.0\.1:\d+", args.target), "Select the DevEco emulator"
    spec = importlib.util.spec_from_file_location("surfaces", Path(__file__).with_name("check-surfaces.py"))
    base = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(base)
    args.output.mkdir(parents=True, exist_ok=True)
    device = base.Device(args.target, args.output)
    assert device.run("shell", "param", "get", "const.product.name") == "emulator"
    assert device.run("shell", "param", "get", "const.product.devicetype") == "2in1"
    scale = args.scale

    def wait(find):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            value = find()
            if value:
                return value
            time.sleep(0.1)
        raise AssertionError("Selection-scroll fixture did not reach the expected state")

    def menu(text):
        return [n for n in device.nodes() if n.get("text") == text]

    def offset():
        return int(device.matching("offset:")[0]["id"].split(":")[1])

    def capture(name):
        remote = "/data/local/tmp/nsh-selection-scroll.jpeg"
        local = args.output / f"{name}.jpeg"
        device.run("shell", "snapshot_display", "-f", remote)
        device.run("file", "recv", remote, str(local))
        return Image.open(local).convert("RGB")

    def selected_handles():
        left, top, _, bottom = device.bounds(device.matching("selection-scroll")[0])
        text_top = max(top, device.bounds(device.matching("scroll-text")[0])[1])
        device.run("shell", "uitest", "uiInput", "longClick", str(round(left + 12 * scale)), str(round(text_top + 10 * scale)))
        wait(lambda: menu("Copy"))
        device.run("shell", "uitest", "uiInput", "click", str(round(left + 12 * scale)), str(round(text_top + 10 * scale)))
        wait(lambda: not menu("Copy"))
        image = capture("handles")
        blue = [(x, y) for y in range(top, min(bottom, top + round(58 * scale)))
                for x in range(max(0, left - round(6 * scale)), left + round(90 * scale))
                if (lambda c: c[0] < 115 and 110 < c[1] < 195 and c[2] > 215)(image.getpixel((x, y)))]
        assert blue, "Visible selection handles required"
        return left, top, bottom, blue

    def hold(start, end, direction):
        command = [*device.hdc, "shell", "uinput", "-T", "-m", *map(str, (*start, *end)), "-k", "7000", "400"]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            time.sleep(1)
            first = offset()
            time.sleep(0.7)
            second = offset()
            assert (second - first) * direction > 50, (first, second, direction)
            stdout, stderr = process.communicate(timeout=15)
            assert process.returncode == 0, (stdout, stderr)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
                device.run("shell", "uinput", "-T", "-u", *map(str, end))
        wait(lambda: menu("Copy"))
        stopped = offset()
        time.sleep(0.4)
        assert abs(offset() - stopped) <= 2, "Selection scrolling must stop on release"
        print(f"PASS: stationary contact scrolls {'down' if direction > 0 else 'up'}: {first} -> {second}; stops at {stopped}", flush=True)

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    wait(lambda: len(device.matching("selection-scroll")) == 2)
    device.button("Single")
    wait(lambda: len(device.matching("selection-scroll")) == 1)
    left, top, bottom, blue = selected_handles()
    end_x = max(x for x, _ in blue) - round(4 * scale)
    end_y = max(y for x, y in blue if x >= end_x - round(3 * scale)) - round(4 * scale)
    hold((end_x, end_y), (round(left + 100 * scale), bottom - 2), 1)
    capture("down-menu")
    device.click(menu("Copy")[0])
    device.click(device.matching("read-copy")[0])
    copied = wait(lambda: [n for n in device.matching("copied:") if n["id"] != "copied:unset"])[0]["id"].removeprefix("copied:")
    assert copied.startswith("Line 00"), repr(copied)
    assert "Line 10" in copied, "Copy must include text beyond the original viewport"
    assert "Line 39" in copied, "Selection must reach the final line past the content insets"
    device.click(device.matching("jump-scroll")[0])
    wait(lambda: abs(offset() - 600) <= 2)
    left, top, bottom, blue = selected_handles()
    start_x = min(x for x, _ in blue) + round(4 * scale)
    start_y = max(y for x, y in blue if x <= start_x + round(3 * scale)) - round(4 * scale)
    hold((start_x, start_y), (round(left + 5 * scale), top + 2), -1)
    capture("up-menu")
    device.click(menu("Copy")[0])
    device.click(device.matching("retire-on-scroll")[0])
    left, top, bottom, blue = selected_handles()
    end_x = max(x for x, _ in blue) - round(4 * scale)
    end_y = max(y for x, y in blue if x >= end_x - round(3 * scale)) - round(4 * scale)
    device.run("shell", "uinput", "-T", "-m", str(end_x), str(end_y),
               str(round(left + 100 * scale)), str(bottom - 2), "-k", "1500", "400")
    wait(lambda: not device.matching("scroll-text"))
    assert not menu("Copy"), "Retirement clears the selection menu"
    device.click(device.matching("remove-text")[0])
    wait(lambda: device.matching("scroll-text"))
    before = offset()
    device.run("shell", "uitest", "uiInput", "swipe", str(round(left + 120 * scale)), str(bottom - 20),
               str(round(left + 120 * scale)), str(top + 20), "500")
    wait(lambda: offset() > before + 20)
    print("PASS: offscreen Copy, retirement during a held drag and restored native scroll interaction", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--scale", type=float, required=True)
    parser.add_argument("--output", type=Path, default=Path("target/ohos/selection-scroll-regression"))
    check(parser.parse_args())
