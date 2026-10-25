#!/usr/bin/env python3
"""Check the api-features fixture on an explicitly selected emulator/device."""
import argparse
import importlib.util
from pathlib import Path
import tempfile
import time

def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

def check(device, directory, api23, api26):
    def wait(prefix, predicate):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            nodes = device.matching(prefix)
            if predicate(nodes):
                return nodes
            time.sleep(0.1)
        raise AssertionError((prefix, nodes))

    def ids(prefix, values):
        return wait(prefix, lambda ns: [n.get("id") for n in ns] == values)

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    ids("hue:", ["hue:120", "hue:120"])
    device.button("Capabilities")
    wait("surface-checks", lambda ns: any(n.get("text") == f"api:26:min:20:23:{str(api23).lower()}:26:{str(api26).lower()}" for n in ns))

    def screenshot():
        remote = "/data/local/tmp/nsh-api-features.png"
        local = directory / "effects.png"
        device.run("shell", "uitest", "screenCap", "-p", remote)
        device.run("file", "recv", remote, str(local))
        return module("pixels", "check-images.py").pixels(local)

    pixels = screenshot()
    for node in device.matching("hue-host"):
        left, top, right, bottom = device.bounds(node)
        assert pixels((left + right) // 2, (top + bottom) // 2) == (0, 255, 0)
    for node in device.matching("layered-shadow"):
        left, top, right, bottom = device.bounds(node)
        unit = (right - left) / 160
        assert pixels(round(right + 4 * unit), (top + bottom) // 2) == (255, 0, 0)
        assert pixels((left + right) // 2, round(bottom + 4 * unit)) == (0, 0, 255)
    for node in device.matching("inset-shadow"):
        left, top, right, bottom = device.bounds(node)
        unit = (right - left) / 180
        assert pixels(round(left + 4 * unit), (top + bottom) // 2) == (0, 0, 255)
        assert pixels((left + right) // 2, (top + bottom) // 2) == (255, 255, 255)
    device.click(wait("hue-host", lambda ns: len(ns) == 2)[0])
    ids("hue:", ["hue:0", "hue:120"])
    pixels = screenshot()
    left, top, right, bottom = device.bounds(device.matching("hue-host")[0])
    assert pixels((left + right) // 2, (top + bottom) // 2) == (255, 0, 0)
    device.click(wait("feature-choice", lambda ns: len(ns) == 2)[0])
    wait("feature-choice", lambda ns: [n.get("checked") for n in ns] == ["true", "false"])
    assert all(n.get("checkable") == "true" for n in device.matching("feature-choice"))
    pid = device.run("shell", "pidof", "com.example.nsh")
    for _ in range(3):
        device.button("Plugin context")
        device.button("Plugin context")
        ids("hue:", ["hue:0", "hue:120"])
        wait("feature-choice", lambda ns: [n.get("checked") for n in ns] == ["true", "false"])
    device.click(wait("hue-host", lambda ns: len(ns) == 2)[0])
    ids("hue:", ["hue:120", "hue:120"])
    assert device.run("shell", "pidof", "com.example.nsh") == pid
    device.button("Status")
    wait("surface-status", lambda ns: any(n.get("text") == "left:running | right:running" for n in ns))
    print("PASS: feature gates, native subtree hue pixels/updates, layered/inset shadow pixels, checked semantics, context recreation and surface isolation")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--api23", action="store_true")
    parser.add_argument("--api26", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nsh-api-features-") as path:
        check(module("surfaces", "check-surfaces.py").Device(args.target, Path(path)), Path(path), args.api23 or args.api26, args.api26)
