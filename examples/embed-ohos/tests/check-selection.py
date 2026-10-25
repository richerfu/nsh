#!/usr/bin/env python3
"""Check native range selection on the installed MateBook emulator fixture.

The caller builds/signs/installs the fixture; this suite never selects a real device.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import time

from PIL import Image


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def check(args):
    assert re.fullmatch(r"127\.0\.0\.1:\d+", args.target), "select the MateBook emulator target"
    assert 0 < args.scale <= 4, "use the emulator's display density"
    scale = args.scale
    args.output.mkdir(parents=True, exist_ok=True)
    base = module("selection_device", "check-surfaces.py").Device

    class Device(base):
        def click(self, node):
            left, top, right, bottom = self.bounds(node)
            tap((left + right) // 2, (top + bottom) // 2)
            time.sleep(0.15)

        def nodes(self):
            # The UI tree can be unavailable while a native subtree retires.
            for attempt in range(3):
                try:
                    remote = "/data/local/tmp/nsh-selection-layout.json"
                    local = self.directory / "layout.json"
                    self.run("shell", "uitest", "dumpLayout", "-p", remote)
                    self.run("file", "recv", remote, str(local))
                    nodes = []

                    def visit(node):
                        if "attributes" in node:
                            nodes.append(node["attributes"])
                        for child in node.get("children", []):
                            visit(child)

                    visit(json.loads(local.read_text()))
                    return nodes
                except (json.JSONDecodeError, subprocess.TimeoutExpired) as error:
                    if attempt == 2:
                        raise
                    print(f"RETRY: UI-tree snapshot: {type(error).__name__}", flush=True)
                    time.sleep(0.2)

    device = Device(args.target, args.output)
    assert device.run("shell", "param", "get", "const.product.name") == "emulator", "DevEco emulator required"
    assert device.run("shell", "param", "get", "const.product.devicetype") == "2in1", "MateBook emulator required"

    def wait(find):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            value = find()
            if value:
                return value
            time.sleep(0.1)
        raise AssertionError("Selection fixture did not reach the expected state")

    def menu(label):
        return [n for n in device.nodes() if n.get("text") == label]

    def tap(x, y):
        device.run("shell", "uitest", "uiInput", "click", str(round(x)), str(round(y)))

    def long_press(x, y):
        device.run("shell", "uitest", "uiInput", "longClick", str(round(x)), str(round(y)))
        wait(lambda: menu("Copy"))

    def copy():
        device.click(wait(lambda: menu("Copy"))[0])
        wait(lambda: not menu("Copy"))
        count = int(device.matching("copied:")[0]["id"].split(":", 2)[1]) + 1
        device.click(wait(lambda: device.matching("read-copy"))[0])
        prefix = f"copied:{count}:"
        return wait(lambda: device.matching(prefix))[0]["id"].removeprefix(prefix)

    def screenshot(name):
        remote = "/data/local/tmp/nsh-selection.jpeg"
        local = args.output / f"{name}.jpeg"
        device.run("shell", "snapshot_display", "-f", remote)
        device.run("file", "recv", remote, str(local))
        return Image.open(local).convert("RGB")

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    wait(lambda: len(device.matching("select-words")) == 2)
    device.button("Single")
    wait(lambda: len(device.matching("select-words")) == 1)
    left, top, _, _ = device.bounds(device.matching("select-words")[0])
    long_press(left + 12 * scale, top + 10 * scale)
    screenshot("word-menu")
    word = copy()
    assert word == "Alpha", f"native Copy must use the actual selected range: {word!r}"

    print("PASS: native word Copy", flush=True)

    long_press(left + 12 * scale, top + 10 * scale)
    # Dismiss the popup without clearing the selection, then drag its trailing handle.
    tap(left + 12 * scale, top + 10 * scale)
    wait(lambda: not menu("Copy"))
    tap(left + 24 * scale, top + 10 * scale)
    wait(lambda: menu("Copy"))
    tap(left + 12 * scale, top + 10 * scale)
    wait(lambda: not menu("Copy"))
    image = screenshot("word-handles")
    blue = [(x, y) for y in range(top, min(top + round(42 * scale), image.height))
            for x in range(max(0, left - round(6 * scale)), left + round(90 * scale))
            if (lambda c: c[0] < 115 and 110 < c[1] < 195 and c[2] > 215)(image.getpixel((x, y)))]
    assert blue, "selection must draw visible handles"
    end_x = max(x for x, _ in blue) - round(4 * scale)
    end_y = max(y for x, y in blue if x >= end_x - round(3 * scale)) - round(4 * scale)
    device.run("shell", "uitest", "uiInput", "swipe", str(end_x), str(end_y),
               str(round(left + 104 * scale)), str(round(top + 10 * scale)), "200")
    wait(lambda: menu("Copy"))
    screenshot("extended-menu")
    extended = copy()
    assert extended.startswith("Alpha bravo"), extended
    print(f"PASS: trailing handle Copy: {extended!r}", flush=True)

    long_press(left + 12 * scale, top + 10 * scale)
    device.click(wait(lambda: menu("Select All"))[0])
    wait(lambda: not menu("Copy"))
    tap(left + 90 * scale, top + 10 * scale)
    all_text = copy()
    assert all_text == "Alpha bravo Charlie. 👩🏽‍💻 Café 中文测试. A second line wraps for selection.", repr(all_text)

    long_press(left + 12 * scale, top + 10 * scale)
    tap(left + 12 * scale, top + 10 * scale)
    wait(lambda: not menu("Copy"))
    device.click(device.matching("replace-text")[0])
    assert not device.matching("select-words")
    device.click(device.matching("replace-text")[0])
    wait(lambda: device.matching("select-words"))
    assert not menu("Copy"), "retired text must not retain its popup"
    long_press(left + 12 * scale, top + 10 * scale)
    tap(left + 12 * scale, top + 10 * scale)
    wait(lambda: not menu("Copy"))
    device.click(device.matching("selection-editor")[0])
    wait(lambda: device.matching("selection-focus:true"))
    left, top, _, _ = device.bounds(device.matching("select-words")[0])
    image = screenshot("focused-editor")
    assert not any(
        c[0] < 115 and 110 < c[1] < 195 and c[2] > 215
        for y in range(max(0, top), min(top + round(42 * scale), image.height))
        for x in range(max(0, left - round(6 * scale)), left + round(90 * scale))
        for c in [image.getpixel((x, y))]
    ), "editor focus must clear the text handles"
    print("PASS: word copy, native menu, handle extension, Unicode Select All, input focus and text retirement")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--scale", required=True, type=float, help="display density (MateBook Pro: 1.9)")
    parser.add_argument("--output", type=Path, default=Path("target/ohos/selection-regression"))
    check(parser.parse_args())
