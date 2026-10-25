#!/usr/bin/env python3
"""Regress secure multiline editing on the explicitly selected MateBook emulator.

Build/install tests/secure-input first. This checks native input and lifecycle;
screen-reader acceptance belongs to the separate QEMU suite.
"""
import argparse
import importlib.util
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location("surfaces", Path(__file__).with_name("check-surfaces.py"))
surfaces = importlib.util.module_from_spec(spec)
spec.loader.exec_module(surfaces)


def check(device):
    def wait(check, description):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            result = check()
            if result:
                return result
            time.sleep(.1)
        raise AssertionError(description)

    def states():
        return [node["id"].split(":")[1:] for node in device.matching("password-state:")]

    def state(length, lines=None, focused=None):
        def matches():
            current = states()
            return len(current) == 2 and int(current[0][0]) == length and (lines is None or int(current[0][1]) == lines) and (focused is None or current[0][3] == str(focused).lower()) and current[1] == ["8", "2", "0", "false"]
        wait(matches, (length, lines, focused, states()))

    def click(identifier):
        device.click(wait(lambda: device.matching(identifier), identifier)[0])

    def key(*codes):
        device.run("shell", "uitest", "uiInput", "keyEvent", *map(str, codes))

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    state(8, 2, False)
    device.button("Status")
    click("password-emoji")
    wait(lambda: device.matching("password-selection:2:4"), "UTF-16 emoji selection")
    key(2055)
    state(6, 2, True)
    click("password-reset")
    state(8)
    click("password-end")
    key(2017)
    state(9, 2, True)
    key(2054)
    state(10, 3, True)
    click("password-submit:false")
    click("password-end")
    key(2054)
    state(10, 3, False)
    wait(lambda: device.matching("password-submitted:1"), "submit callback")
    click("password-reset")
    state(8, 2)
    click("password-clipboard-seed")
    click("password-end")
    state(8, 2, True)
    key(2072, 2038)
    time.sleep(.4)
    paste = [node for node in device.nodes() if node.get("type") == "PasteButton"]
    if paste:
        device.click(paste[0])
    state(20, 3, True)
    key(2072, 2017)
    key(2072, 2019)
    key(2072, 2040)
    state(20, 3)
    editor = device.matching("password-editor")[0]
    left, top, right, bottom = device.bounds(editor)
    device.run("shell", "uitest", "uiInput", "longClick", str((left + right) // 2), str((top + bottom) // 2))
    paste = wait(lambda: [node for node in device.nodes() if node.get("type") == "PasteButton"], "visible system paste menu")
    device.click(paste[0])
    state(12, 2, True)
    click("password-clipboard-read")
    wait(lambda: device.matching("password-clipboard:seed"), "password copy/cut must preserve clipboard")
    click("password-end")
    key(2072, 2017)
    device.run("shell", "uitest", "uiInput", "text", "Z" * 100)
    state(80, 1, True)
    click("password-editable:true")
    state(80, 1, False)
    click("password-focus")
    state(80, 1, False)
    click("password-editable:false")
    for cycle in range(5):
        click("password-focus")
        state(80 - cycle, focused=True)
        click("password-retire")
        wait(lambda: len(device.matching("password-editor")) == 1, "retired editor")
        click("password-remount")
        wait(lambda: len(device.matching("password-editor")) == 2, "remounted editor")
        click("password-end")
        key(2055)
        state(79 - cycle, 1, True)
    # The last remount has a nonzero native edit count. A mode replacement must
    # inherit that value/count once, then keep accepting real edits in the new owner.
    state(75, 1, True)
    click("password-multiline:true")
    state(75, 1, False)
    click("password-end")
    key(2017)
    state(76, 1, True)
    click("password-multiline:false")
    state(76, 1, False)
    click("password-end")
    key(2055)
    state(75, 1, True)
    # The app exposes lengths and masked pixels, never its password as UI text.
    assert all("ab😀" not in node.get("text", "") for node in device.nodes())
    print("PASS UTF-16 selection/deletion, hardware keys, newline/submit, password clipboard policy, maxLength, read-only state, independent surfaces, five focused retire/remount cycles and nonzero-count secure mode replacement")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    args = parser.parse_args()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    check(surfaces.Device(args.target, args.artifacts))
