#!/usr/bin/env python3
"""Exercise the surfaces fixture in the installed, signed embedding example.

Build with NSH_EXAMPLE=examples/embed-ohos/tests/surfaces first. The caller
selects the HDC target explicitly; this script never installs or signs an app.
"""

import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
import time


class Device:
    def __init__(self, target, directory):
        self.hdc = ["hdc", "-t", target]
        self.directory = directory

    def run(self, *args):
        result = subprocess.run(
            [*self.hdc, *args], check=True, capture_output=True, text=True, timeout=20
        )
        if "[Fail]" in result.stdout:
            raise RuntimeError(result.stdout)
        return result.stdout.strip()

    def nodes(self):
        remote = "/data/local/tmp/nsh-surfaces.json"
        local = self.directory / "layout.json"
        self.run("shell", "uitest", "dumpLayout", "-b", "com.example.nsh", "-p", remote)
        self.run("file", "recv", remote, str(local))
        result = []

        def visit(value):
            if isinstance(value, dict):
                if "attributes" in value:
                    result.append(value["attributes"])
                for key, child in value.items():
                    if key != "attributes":
                        visit(child)
            elif isinstance(value, list):
                for child in value:
                    visit(child)

        visit(json.loads(local.read_text()))
        return result

    @staticmethod
    def bounds(node):
        return tuple(map(int, re.findall(r"-?\d+", node["bounds"])))

    def matching(self, prefix):
        return sorted(
            (n for n in self.nodes() if n.get("id", "").startswith(prefix)),
            key=self.bounds,
        )

    def click(self, node):
        left, top, right, bottom = self.bounds(node)
        self.run("shell", "uitest", "uiInput", "click",
                 str((left + right) // 2), str((top + bottom) // 2))

    def button(self, text):
        self.click(next(n for n in self.nodes() if n.get("text") == text))

    def counts(self, expected, prefix="count:"):
        deadline = time.monotonic() + 10
        actual = []
        while time.monotonic() < deadline:
            actual = [int(n["id"].split(":")[1]) for n in self.matching(prefix)]
            if actual == expected:
                return
            time.sleep(0.1)
        raise AssertionError(f"counts {actual}, expected {expected}")

    def split_layout(self, left_fraction):
        nodes = self.matching("viewport:")
        assert len(nodes) == 2, nodes
        a, b = map(self.bounds, nodes)
        assert abs(a[2] - b[0]) <= 1, (a, b)
        assert a[1] == b[1] and a[3] == b[3], (a, b)
        actual = (a[2] - a[0]) / (b[2] - a[0])
        assert abs(actual - left_fraction) < 0.005, (actual, left_fraction)
        # onLayout reports the viewport delivered to each independent engine.
        widths = [int(n["id"].split(":")[1]) for n in nodes]
        assert abs(widths[0] / sum(widths) - left_fraction) < 0.005, widths


def check(device):
    def label(identifier, expected=None):
        deadline = time.monotonic() + 10
        actual = ""
        while time.monotonic() < deadline:
            actual = next((n.get("text", "") for n in device.nodes()
                           if n.get("id") == identifier), "")
            if expected is None or actual == expected:
                return actual
        raise AssertionError((identifier, actual, expected))

    def status(expected):
        device.button("Status")
        label("surface-status", expected)

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    device.counts([0, 0])
    pid = device.run("shell", "pidof", "com.example.nsh")
    for count in range(1, 4):
        device.click(device.matching("increment")[0])
        device.counts([count, 0])
    device.split_layout(0.5)
    device.click(device.matching("host-badge")[0])
    device.counts([1, 0], "native-count:")
    for count in range(1, 3):
        device.click(device.matching("increment")[1])
        device.counts([3, count])
    device.button("Resize")
    device.counts([3, 2])
    device.split_layout(0.35)
    device.button("Swap")
    device.counts([2, 3])
    device.split_layout(0.65)
    print("PASS independent instances, native host layout and reordering", flush=True)
    for cycle in range(10):
        device.button("Unmount")
        device.counts([])
        status("left:running | right:running")
        device.button("Mount")
        device.counts([2, 3])
        device.split_layout(0.65)
        assert device.run("shell", "pidof", "com.example.nsh") == pid
        print(f"PASS retained state across slot remount {cycle + 1}/10", flush=True)
    device.button("Single")
    device.counts([3])
    device.button("Split")
    device.counts([0, 3])
    device.button("Stop left")
    device.counts([0])
    status("left:empty | right:running")
    device.button("Reload left")
    device.counts([0, 0])
    device.click(device.matching("host-badge")[1])
    device.counts([0, 1], "native-count:")
    print("PASS native widget events, instance isolation and factory retention after stop", flush=True)
    device.click(device.matching("increment")[1])
    device.counts([0, 1])
    device.button("Missing left")
    time.sleep(0.6)
    missing = label("surface-status")
    assert "left:" in missing and "running |" not in missing and "right:running" in missing, missing
    device.counts([0, 1])
    device.button("Reload left")
    device.counts([0, 0])
    status("left:running | right:running")
    device.button("Race")
    label("surface-checks", "race:cancelled")
    device.counts([0, 0])
    device.button("Invalid")
    label("surface-checks", "invalid:3")
    status("left:running | right:running")
    print("PASS stop/reload, resource failure isolation, load cancellation and validation", flush=True)
    device.button("Dispose left")
    device.counts([0])
    status("left:disposed | right:running")
    device.button("Disposed checks")
    label("surface-checks", "disposed:3")
    device.click(device.matching("increment")[0])
    device.counts([1])
    device.button("Recreate left")
    device.counts([1, 0])
    status("left:running | right:running")
    assert device.run("shell", "pidof", "com.example.nsh") == pid
    print("PASS idempotent disposal, rejected reuse and independent recreation", flush=True)
    device.button("Unmount")
    device.counts([])
    device.button("Source left")
    device.button("Mount")
    deadline = time.monotonic() + 10
    while not device.matching("source-loaded") and time.monotonic() < deadline:
        time.sleep(0.1)
    assert len(device.matching("source-loaded")) == 1
    device.counts([1])
    status("left:running | right:running")
    device.button("Reload left")
    device.counts([1, 0])
    print("PASS trusted source staged before attachment and replaced by resource", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, help="explicit HDC device identifier")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nsh-surfaces-") as directory:
        check(Device(args.target, Path(directory)))
