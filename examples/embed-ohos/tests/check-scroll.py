#!/usr/bin/env python3
"""Exercise touch, scroll, zoom, sticky cells and refresh in the scrolling fixture."""
import argparse
import importlib.util
from pathlib import Path
import tempfile
import time

_spec = importlib.util.spec_from_file_location("surfaces", Path(__file__).with_name("check-surfaces.py"))
_surfaces = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_surfaces)


def check(device):
    def wait(predicate, label):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            nodes = device.nodes()
            if predicate(nodes):
                return nodes
            time.sleep(0.1)
        raise AssertionError(label)

    def state(expected):
        wait(lambda ns: any(n.get("id") == expected for n in ns), expected)

    def click(identifier):
        device.click(device.matching(identifier)[0])

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    wait(lambda ns: sum(n.get("id") == "scroll-state:0:0:false:1" for n in ns) == 2, "two ready scrollers")
    click("touch-pad")
    state("touch:end")
    click("scroll-jump")
    state("scroll-state:300:0:false:1")
    click("zoom-double")
    state("scroll-state:300:0:false:2")
    click("zoom-reset")
    state("scroll-state:300:0:false:1")
    click("sticky-jump")
    sticky = device.matching("sticky-row:5")[0]
    # List's public ref resolves its internal scroller; inspect that actual
    # native viewport rather than relying on a wrapper's testID forwarding.
    candidates = [n for n in device.nodes() if n.get("type") == "Scroll"
                  and sticky["hierarchy"].startswith(n["hierarchy"] + ",")]
    viewport = max(candidates, key=lambda n: len(n["hierarchy"]))
    assert abs(device.bounds(sticky)[1] - device.bounds(viewport)[1]) < 3, (sticky, viewport)
    # Scroll back to the top; the following independent pull must refresh
    # even in this fixture's small 90-vp native Refresh container.
    for _ in range(3):
        l, t, r, b = device.bounds(device.matching("native-scroll")[0])
        x = (l + r) // 2
        device.run("shell", "uitest", "uiInput", "swipe", str(x), str(t + 12), str(x), str(b - 12), "1000")
    time.sleep(0.3)
    state("scroll-state:0:0:false:1")
    device.run("shell", "uitest", "uiInput", "swipe", str(x), str(t + 12), str(x), str(b + 300), "200")
    state("scroll-state:0:1:true:1")
    click("refresh-finish")
    state("scroll-state:0:1:false:1")
    assert len(device.matching("scroll-state:0:0:false:1")) == 1, "right surface remains unchanged"
    print("PASS: native touch, scroll offset, zoom, sticky list, small-viewport refresh and surface isolation")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="nsh-scroll-") as tmp:
        check(_surfaces.Device(args.target, Path(tmp)))
