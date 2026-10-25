#!/usr/bin/env python3
"""Native WebView/video regression in a signed HAP built with the matching fixture.

The caller must select the HDC target and fixture explicitly. This script
operates only com.example.nsh and never signs or installs an application.
"""
import argparse
import importlib.util
from pathlib import Path
import tempfile
import time

_adapter = importlib.util.spec_from_file_location("check_surfaces", Path(__file__).with_name("check-surfaces.py"))
_module = importlib.util.module_from_spec(_adapter)
_adapter.loader.exec_module(_module)
Device = _module.Device


def wait(device, predicate, label):
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        nodes = device.nodes()
        errors = [n["id"] for n in nodes if n.get("id", "").startswith(("web-state:error:", "video-state:error:"))]
        assert not errors, errors
        if predicate(nodes):
            return nodes
        time.sleep(0.15)
    raise AssertionError(label)


def check(device, kind, http=False):
    prefix = "web" if kind == "webview" else "video"
    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    wait(device, lambda ns: sum(n.get("id") == f"{prefix}-state:loaded" for n in ns) == 2, "two loaded media instances")
    device.counts([1, 1], "media-loads:")

    def click(identifier):
        device.click(device.matching(identifier)[0])

    if kind == "webview":
        click("web-inject")
        wait(device, lambda ns: any(n.get("id") == "web-message:injected" for n in ns), "injected JS message")
        assert len(device.matching("web-message:none")) == 1, "messages remain surface-local"
        click("web-post")
        wait(device, lambda ns: any(n.get("id") == "web-message:received:host-message" for n in ns), "host/page round trip")
        click("web-reload")
        device.counts([2, 1], "media-loads:")
        expected = [2, 1]
    else:
        click("video-pause")
        time.sleep(0.3)
        before = device.matching("video-time:")[0]["id"]
        time.sleep(1.2)
        assert device.matching("video-time:")[0]["id"] == before, "paused video keeps its position"
        click("video-seek")
        wait(device, lambda ns: any(n.get("id") == "video-time:4" for n in ns), "seek completes at four seconds")
        click("video-controls")
        wait(device, lambda ns: any(n.get("text") == "Play" for n in ns), "native playback controls")
        click("video-pause")
        wait(device, lambda ns: any(n.get("id") == "video-time:5" for n in ns), "resumed playback progresses")
        expected = [1, 1]

    click(f"{prefix}-toggle")
    wait(device, lambda ns: sum(n.get("id") == f"embedded-{prefix}" for n in ns) == 1, "one media node is disposed")
    click(f"{prefix}-toggle")
    expected[0] += 1
    device.counts(expected, "media-loads:")
    device.button("Plugin context")
    time.sleep(0.4)
    device.button("Plugin context")
    expected = [n + 1 for n in expected]
    device.counts(expected, "media-loads:")
    click("media-overlap")
    device.counts([1, 0], "media-overlays:")
    click("media-geometry")
    device.counts(expected, "media-loads:")
    click("media-geometry")
    click("media-modal-open")
    expected[0] += 1
    device.counts(expected, "media-loads:")
    wait(device, lambda ns: any(n.get("id") == "media-modal" for n in ns), "media is owned by its native modal")
    click("media-modal-close")
    expected[0] += 1
    device.counts(expected, "media-loads:")
    wait(device, lambda ns: not any(n.get("id") == "media-modal" for n in ns), "modal and its media are disposed")
    if kind == "webview":
        click("web-asset")
        wait(device, lambda ns: any(n.get("id") == "web-message:asset-script" for n in ns), "scoped HTML and relative script resource")
        expected[0] += 1
        device.counts(expected, "media-loads:")
    if kind == "video" and http:
        click("video-http")
        expected[0] += 1
        device.counts(expected, "media-loads:")
        click("video-http")
        deadline = time.monotonic() + 10
        while not any("video exceeds 64 MiB" in n["id"] for n in device.matching("video-state:error:")):
            assert time.monotonic() < deadline, "oversized remote video is rejected"
            time.sleep(0.1)
        click("video-http")
        expected[0] += 1
        device.counts(expected, "media-loads:")
        time.sleep(3)
        device.counts(expected, "media-loads:")
        print("PASS: HTTP video redirect, 64-MiB rejection and obsolete source cancellation")
    device.button("Status")
    wait(device, lambda ns: any(n.get("id") == "surface-status" and n.get("text") == "left:running | right:running" for n in ns), "healthy independent engines")
    print(f"PASS: {kind} native attachment, commands/events, surface isolation, remount, UIContext recreation, native overlap and modal containment")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--kind", choices=["webview", "video"], required=True)
    parser.add_argument("--http", action="store_true", help="also load the video fixture HTTP redirect at device port 18769")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nsh-ohos-media-") as directory:
        check(Device(args.target, Path(directory)), args.kind, args.http)
