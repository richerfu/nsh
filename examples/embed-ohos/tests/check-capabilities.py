#!/usr/bin/env python3
"""Device regression for the capabilities fixture; requires an explicitly selected HDC target."""
import argparse
from pathlib import Path
import tempfile
import time
import importlib.util
_adapter = importlib.util.spec_from_file_location("check_surfaces", Path(__file__).with_name("check-surfaces.py"))
_module = importlib.util.module_from_spec(_adapter)
_adapter.loader.exec_module(_module)
Device = _module.Device


def wait(device, predicate, label):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        nodes = device.nodes()
        if predicate(nodes):
            return nodes
        time.sleep(0.15)
    raise AssertionError(label)


def check(device):
    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    wait(device, lambda ns: len([n for n in ns if n.get("id") == "native-canvas"]) == 2, "both canvas surfaces")
    time.sleep(0.5)
    device.click(device.matching("home-inc")[0])
    device.counts([1, 0], "home-count:")
    device.click(device.matching("nav-push")[0])
    wait(device, lambda ns: any(n.get("id") == "route:detail" for n in ns), "push detail")
    device.button("Native back")
    wait(device, lambda ns: any(n.get("id") == "back-requests:1" for n in ns), "prevent-back request")
    device.click(device.matching("allow-back")[0])
    device.button("Native back")
    wait(device, lambda ns: len([n for n in ns if n.get("id") == "route:home"]) == 2, "pop to retained home")
    device.counts([1, 0], "home-count:")
    device.click(device.matching("alert-open")[0])
    nodes = wait(device, lambda ns: any(n.get("text") == "Confirm" for n in ns), "native alert")
    device.click(next(n for n in nodes if n.get("text") == "Confirm"))
    wait(device, lambda ns: any(n.get("id") == "dialog:alert:1" for n in ns), "alert result")
    device.click(device.matching("sheet-open")[0])
    nodes = wait(device, lambda ns: any(n.get("text") == "Remove" for n in ns), "native action sheet")
    device.click(next(n for n in nodes if n.get("text") == "Remove"))
    wait(device, lambda ns: any(n.get("id") == "dialog:sheet:1" for n in ns), "sheet result")
    assert len(device.matching("dialog:none")) == 1, "other surface receives no dialog result"
    device.button("Package checks")
    wait(device, lambda ns: any(n.get("id") == "surface-checks" and n.get("text") == "packages:8:true" for n in ns), "signed-package checks")
    print("PASS: navigation/back, retained state, dialogs, surface isolation and signed packages")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nsh-ohos-capabilities-") as directory:
        check(Device(args.target, Path(directory)))
