#!/usr/bin/env python3
"""Exercise the accessibility fixture through a dedicated OpenHarmony QEMU.

Build with NSH_A11Y_E2E=1 and the accessibility fixture, then sign/install the
HAP. The suite requires the QEMU package manifest and its multitouch QMP socket;
it never installs packages or selects a physical device or DevEco emulator.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import time


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


class Suite:
    service = "/system/bin/cli_tool/executable/ohos-a11yManager"
    extension = "com.example.nsh/AccessibilityE2ETestExtension"

    def __init__(self, args):
        assert re.fullmatch(r"127\.0\.0\.1:\d+", args.target), "select the dedicated QEMU loopback target"
        assert args.socket.is_socket(), "QEMU multitouch QMP socket is required"
        capabilities = json.loads(args.package.joinpath("manifest.json").read_text())["capabilities"]
        assert all(capabilities.get(key) for key in ("accessibility_test", "accessibility_cli", "virtio_multitouch"))
        self.args = args
        self.hdc = ["hdc", "-t", args.target]
        args.output.mkdir(parents=True, exist_ok=True)
        self.touch = module("touch", "qemu-multitouch.py")
        self.remote = "/data/local/tmp/nsh-qemu-a11y.json"

    def run(self, *args):
        result = subprocess.run([*self.hdc, *args], check=True, capture_output=True, text=True, timeout=20)
        assert "[Fail]" not in result.stdout, result.stdout
        return result.stdout

    def reader(self, enabled):
        command = "ability-enable" if enabled else "ability-disable"
        args = ["shell", self.service, command, "--name", self.extension]
        if enabled:
            args += ["--capabilities", "7"]
        result = self.run(*args)
        messages = [json.loads(line) for line in result.splitlines() if line.startswith("{")]
        assert messages, result
        message = messages[-1]
        if not enabled and message.get("errCode") == "ERR_DISABLE_ABILITY" and message.get("errMsg") == "Accessibility ability is not enabled.":
            return
        assert message["status"] == "success", result
        time.sleep(0.5)

    def nodes(self):
        # The open-source 2in1 UITest omits this window when filtering by bundle.
        self.run("shell", "uitest", "dumpLayout", "-p", self.remote)
        path = self.args.output / "layout.json"
        self.run("file", "recv", self.remote, str(path))
        nodes = []
        def visit(node):
            if "attributes" in node:
                nodes.append(node["attributes"])
            for child in node.get("children", []):
                visit(child)
        visit(json.loads(path.read_text()))
        return nodes

    @staticmethod
    def bounds(node):
        return tuple(map(int, re.findall(r"-?\d+", node["bounds"])))

    def wait(self, check, description):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            result = check()
            if result:
                return result
            time.sleep(0.15)
        raise AssertionError(description)

    def matching(self, prefix):
        return sorted((node for node in self.nodes() if node.get("id", "").startswith(prefix)), key=self.bounds)

    def states(self, prefix, expected):
        return self.wait(lambda: [node.get("id") for node in self.matching(prefix)] == expected, (prefix, expected))

    def logs(self):
        logs = self.run("shell", "hilog", "-x")
        logs = "\n".join(line for line in logs.splitlines() if "NshA11yE2E" in line)
        self.args.output.joinpath("events.log").write_text(logs + "\n")
        lines = logs.splitlines()
        connected = [index for index, line in enumerate(lines) if line.endswith(": connected")]
        return "\n".join(lines[connected[-1]:]) if connected else logs

    def logged(self, message):
        self.wait(lambda: message in self.logs(), message)

    def gesture(self, *points):
        client = self.touch.QmpClient(self.args.socket)
        try:
            for index, (x, y) in enumerate(points):
                frame = self.touch.touch_frame("begin" if index == 0 else "update",
                    self.touch.scaled(x, self.args.width), self.touch.scaled(y, self.args.height))
                client.input(frame)
                time.sleep(0.06)
            client.input(self.touch.touch_end())
        finally:
            client.close()
        time.sleep(0.5)

    def focus(self, identifier, index=0):
        nodes = self.wait(lambda: [node for node in self.matching(identifier)
            if node.get("visible") == "true" and self.bounds(node)[2] > self.bounds(node)[0]], identifier)
        node = nodes[index]
        left, top, right, bottom = self.bounds(node)
        before = set(self.logs().splitlines())
        self.gesture(((left + right) // 2, (top + bottom) // 2))
        marker = "focused a11y=Volume " if identifier == "a11y-range" else f" component={node['accessibilityId']} "
        self.wait(lambda: any(line not in before and "focused a11y=" in line and marker in line
            for line in self.logs().splitlines()), f"system focus did not reach {identifier}")

    def down(self):
        self.gesture((200, 260), (200, 320), (200, 380), (200, 440))

    def up(self):
        self.gesture((200, 440), (200, 380), (200, 320), (200, 260))

    def check(self):
        self.reader(False)
        self.run("shell", "aa", "force-stop", "com.example.nsh")
        self.wait(lambda: not self.run("shell", "pidof", "com.example.nsh").strip(), "previous app process did not stop")
        self.run("shell", "power-shell", "wakeup")
        self.run("shell", "uitest", "uiInput", "keyEvent", "Home")
        time.sleep(0.5)
        started = self.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
        assert "start ability successfully" in started, started
        self.wait(lambda: self.run("shell", "pidof", "com.example.nsh").strip(), "app process did not start")
        time.sleep(1)
        self.states("a11y-event:", ["a11y-event:none", "a11y-event:none"])
        choice_id = self.matching("a11y-choice")[0]["accessibilityId"]
        button = next(node for node in self.nodes() if node.get("text") == "Capabilities")
        left, top, right, bottom = self.bounds(button)
        self.gesture(((left + right) // 2, (top + bottom) // 2))
        expected = f"api:26:min:20:23:{str(self.args.api23).lower()}:26:{str(self.args.api26).lower()}"
        self.wait(lambda: any(node.get("text") == expected for node in self.matching("surface-checks")), expected)
        self.reader(True)
        state = self.run("shell", "hidumper", "-s", "AccessibilityManagerService", "-a", "-u")
        self.args.output.joinpath("manager-state.log").write_text(state)
        assert all(re.search(rf"{key}:\s+1", state) for key in ("accessible", "touchGuide", "gesture")), state
        if self.args.navigation:
            # Probe a stock ArkUI toolbar Button before any NSH provider.
            self.gesture(((left + right) // 2, (top + bottom) // 2))
            self.gesture((200, 320), (260, 320), (320, 320), (380, 320))
            self.logged("moved-focus from=")
        self.focus("a11y-choice")
        self.logged("focused a11y=Example choice")
        self.down()
        self.states("a11y-event:", ["a11y-event:activate", "a11y-event:none"])
        assert [n["checked"] for n in self.matching("a11y-choice")] == ["true", "false"]
        assert [n["selected"] for n in self.matching("a11y-choice")] == ["true", "false"]
        if self.args.api26:
            self.up()
            self.states("a11y-event:", ["a11y-event:toggle", "a11y-event:none"])
            self.logged("performed=executeCustomAction")
            assert [n["checked"] for n in self.matching("a11y-choice")] == ["false", "false"]
            self.focus("a11y-choice", 1)
            self.up()
            self.states("a11y-event:", ["a11y-event:toggle", "a11y-event:toggle"])
            assert [n["checked"] for n in self.matching("a11y-choice")] == ["false", "true"]
            print("PASS QEMU API-26 custom actions", flush=True)
        else:
            self.focus("a11y-choice", 1)
            self.down()
            self.states("a11y-event:", ["a11y-event:activate", "a11y-event:activate"])
        print("PASS QEMU system focus, Click, checked/selected state and surface isolation", flush=True)
        if self.args.api23:
            right = "a11y-event:toggle" if self.args.api26 else "a11y-event:activate"
            self.focus("a11y-range")
            self.logged("focused a11y=Volume")
            self.logged("range min=0 max=10 now=5")
            self.gesture((200, 320), (300, 320), (400, 320), (300, 320), (200, 320))
            self.states("a11y-event:", ["a11y-event:increment", right])
            self.logged("performed=scrollForward")
            self.gesture((400, 320), (300, 320), (200, 320), (300, 320), (400, 320))
            self.states("a11y-event:", ["a11y-event:decrement", right])
            self.logged("performed=scrollBackward")
            if self.args.navigation:
                self.gesture((200, 320), (260, 320), (320, 320), (380, 320))
                def moved():
                    matches = re.findall(r"moved-focus from=(\d+) to=(\d+) type=(\S+) label=([^\n]+)", self.logs())
                    return any(source != target and label == "Update polite region" for source, target, _, label in matches)
                self.wait(moved, "forward navigation did not leave the provider leaf")
            print("PASS QEMU API-23 provider range actions", flush=True)
        else:
            self.focus("a11y-polite")
            self.down()
        self.logged("event=announceForAccessibilityNotInterrupt announcement=Current volume 6")
        urgent = "increment" if self.args.api23 else "activate"
        self.logged(f"event=announceForAccessibility announcement=Latest event {urgent}")
        print("PASS QEMU polite and assertive announcement events", flush=True)
        self.reader(False)
        for _ in range(6):
            nodes = self.matching("a11y-announce")
            if nodes and nodes[0].get("visible") == "true":
                break
            self.up()
        assert nodes and nodes[0].get("visible") == "true", "announcement button is not visible"
        self.reader(True)
        self.focus("a11y-announce")
        self.down()
        self.logged("event=announceForAccessibility announcement=Native Shell announcement")
        self.focus("a11y-focus")
        self.down()
        self.logged("event=requestFocusForAccessibility")
        self.logged(f"requested-focus component={choice_id} label=Example choice")
        print("PASS QEMU explicit announcement and programmatic semantic focus", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--width", type=int, default=800)
    parser.add_argument("--height", type=int, default=500)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--api23", action="store_true", help="expect a build with api-23")
    parser.add_argument("--api26", action="store_true", help="expect a build with api-26")
    parser.add_argument("--navigation", action="store_true", help="also diagnose the QEMU direction-query path, including a stock ArkUI Button")
    args = parser.parse_args()
    args.api23 = args.api23 or args.api26
    suite = Suite(args)
    try:
        suite.check()
    finally:
        suite.logs()
        suite.reader(False)
