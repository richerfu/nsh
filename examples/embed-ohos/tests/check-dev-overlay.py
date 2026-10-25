#!/usr/bin/env python3
"""Regress native dev overlays through the real protocol-v2 WebSocket client.

Build/install tests/dev-overlay with its Index override and clipboard-read grant
first, then create HDC rport tcp:8797 tcp:8797 on the explicit MateBook target.
This script starts a controlled local server; it never builds, signs or installs.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import threading
import time
from urllib.parse import parse_qs, urlsplit


class Peer:
    def __init__(self, connection, side, serial):
        self.connection = connection
        self.side = side
        self.serial = serial
        self.lock = threading.Lock()

    def text(self, value):
        with self.lock:
            self.connection.send(json.dumps(value, ensure_ascii=False))

    def bundle(self, version, source):
        with self.lock:
            self.connection.send(json.dumps({
                "type": "bundle", "version": version, "format": "source", "source_map": False,
            }))
            self.connection.send(source.encode("utf-8"))


class DevServer:
    """Only the actual nsh-dev control messages needed by this regression."""

    def __init__(self, directory, port=8797):
        self.directory = directory
        self.port = port
        self.condition = threading.Condition()
        self.log_lock = threading.Lock()
        self.peers = {}
        self.serial = 0
        self.versions = {"left": 0, "right": 0}
        self.failures = []

    def record(self, value):
        with self.log_lock:
            with (self.directory / "protocol.jsonl").open("a", encoding="utf-8") as stream:
                stream.write(json.dumps(value, ensure_ascii=False) + "\n")

    def handle(self, connection):
        peer = None
        try:
            request = urlsplit(connection.request.path)
            side = parse_qs(request.query).get("side", [""])[0]
            assert request.path == "/ws" and side in ("left", "right"), connection.request.path
            hello = json.loads(connection.recv(timeout=10))
            assert hello.get("type") == "hello" and hello.get("protocol") == 2, hello
            self.record({"direction": "client", "side": side, "message": hello})
            connection.send(json.dumps({"type": "hello", "protocol": 2, "session": 21001}))
            with self.condition:
                self.serial += 1
                peer = Peer(connection, side, self.serial)
                self.peers[side] = peer
                self.condition.notify_all()
            for raw in connection:
                self.record({"direction": "client", "side": side, "message": json.loads(raw)})
        except Exception as error:
            from websockets.exceptions import ConnectionClosed
            if not isinstance(error, ConnectionClosed):
                with self.condition:
                    self.failures.append(str(error))
                    self.condition.notify_all()
        finally:
            if peer:
                with self.condition:
                    if self.peers.get(peer.side) is peer:
                        del self.peers[peer.side]
                    self.condition.notify_all()

    def __enter__(self):
        from websockets.sync.server import serve
        self.directory.mkdir(parents=True, exist_ok=True)
        self.server = serve(self.handle, "127.0.0.1", self.port, compression=None)
        self.port = self.server.socket.getsockname()[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *_args):
        self.server.shutdown()
        self.thread.join(timeout=10)

    def peer(self, side, after=0, timeout=15):
        deadline = time.monotonic() + timeout
        with self.condition:
            while time.monotonic() < deadline:
                assert not self.failures, self.failures
                peer = self.peers.get(side)
                if peer and peer.serial > after:
                    return peer
                self.condition.wait(min(.2, max(0, deadline - time.monotonic())))
        raise AssertionError(f"no protocol-v2 {side} connection; check dev SDK and HDC rport tcp:8797 tcp:8797")

    def error(self, side, message, kind="build_failed"):
        value = {"type": kind, "message": message}
        if kind == "cannot_run":
            value["missing"] = ["fixture/unavailable"]
        self.peer(side).text(value)
        self.record({"direction": "server", "side": side, "message": value})

    def bundle(self, side, source):
        self.versions[side] += 1
        self.peer(side).bundle(self.versions[side], source)
        self.record({"direction": "server", "side": side, "bundle_version": self.versions[side],
                     "source_bytes": len(source.encode("utf-8"))})


def stack_message(label, lines=96):
    frames = [f"\tat 模块_{index:03d}_😀_e\u0301_👩🏽‍💻.render (路径/文件-{index}.tsx:{index + 1}:17)"
              for index in range(lines)]
    return f"{label}: 编译错误 — Unicode 😀 👩🏽‍💻 e\u0301\n" + "\n".join(frames) + f"\r\nEND::{label}::終"


class OverlayCheck:
    def __init__(self, device, server, source, timeout, trace_actions=False):
        self.device = device
        self.server = server
        self.source = source
        self.timeout = timeout
        self.results = []
        self.trace_actions = trace_actions
        self.require_zero = False
        self.latest_nodes = []

    def wait(self, check, description):
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            assert not self.server.failures, self.server.failures
            result = check()
            if result:
                return result
            time.sleep(.1)
        raise AssertionError(description)

    def exact(self, identifier):
        return [node for node in self.nodes() if node.get("id") == identifier]

    def nodes(self):
        self.latest_nodes = self.device.nodes()
        return self.latest_nodes

    def node(self, identifier):
        return self.wait(lambda: next(iter(self.exact(identifier)), None), identifier)

    @staticmethod
    def text(node):
        return node.get("text") or node.get("originalText") or ""

    def host_text(self, identifier):
        return self.text(self.node(identifier))

    def click(self, identifier):
        self.input_click(self.node(identifier), identifier)

    def click_side(self, side, identifier):
        self.input_click(self.side_node(side, identifier), f"{side}:{identifier}")

    def input_click(self, node, requested):
        event = self.input_before(node, requested)
        # Derive the point from this exact layout snapshot, without another query in between.
        self.device.click(node)
        self.input_after(event)

    def input_swipe(self, node, requested, start, end, velocity):
        event = self.input_before(node, requested)
        if event is not None:
            event.update(start=start, end=end, velocity=velocity)
        self.device.run("shell", "uitest", "uiInput", "swipe", *map(str, (*start, *end, velocity)))
        self.input_after(event)

    def input_before(self, node, requested):
        if self.trace_actions:
            return {"action": requested, "time": time.time(), "bounds": node.get("bounds"),
                    "origBounds": node.get("origBounds"), "hierarchy": node.get("hierarchy"),
                    "before": self.counts_from(self.latest_nodes)}
        return None

    def input_after(self, event):
        if event is not None:
            time.sleep(.25)
            current = self.nodes()
            event["after"] = self.counts_from(current)
            event["slots_after"] = {side: next((value.get("bounds") for value in current
                if value.get("id") == f"dev-slot-{side}"), None) for side in ("left", "right")}
            self.trace(event)
            if self.require_zero:
                assert all(value in (None, 0) for value in event["after"].values()), event

    def trace(self, event):
        with (self.device.directory / "actions.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(event, ensure_ascii=False) + "\n")
        print("TRACE " + json.dumps(event, ensure_ascii=False), flush=True)

    def checkpoint(self, name, expected=None):
        current = self.counts_from(self.nodes())
        self.trace({"checkpoint": name, "counts": current, "time": time.time()})
        if expected is not None:
            assert current == expected, (name, current, expected)

    def inspect(self):
        self.click("dev-inspect")
        errors = [self.text(node) for node in self.exact("dev-host-error")]
        assert not any(errors), errors

    def side_nodes(self, side, identifier, visible=True):
        nodes = self.nodes()
        scope = next(node for node in nodes if node.get("id") == f"dev-slot-{side}")
        prefix = scope["hierarchy"] + ","
        return [node for node in nodes
                if node.get("id") == identifier and node.get("hierarchy", "").startswith(prefix)
                and (not visible or node.get("visible") != "false")]

    def overlay(self, side):
        return self.side_nodes(side, "nsh-dev-overlay")

    def side_node(self, side, identifier):
        return self.wait(lambda: next(iter(self.side_nodes(side, identifier)), None), (side, identifier))

    def message(self, side):
        return self.text(self.side_node(side, "nsh-dev-overlay-message"))

    def status(self, side):
        nodes = self.side_nodes(side, "nsh-dev-overlay-status")
        return self.text(nodes[0]) if nodes else ""

    def last_error(self, side):
        return json.loads(self.host_text(f"dev-last-error-{side}"))

    def expect_error(self, side, expected):
        self.wait(lambda: self.overlay(side) and self.message(side) == expected,
                  f"{side} full native error text")
        self.inspect()
        assert self.last_error(side) == expected, (side, "lastError differs from native error text")

    def copied(self, side, expected):
        self.click_side(side, "nsh-dev-overlay-copy")
        self.wait(lambda: self.status(side) == "Copied", (side, "successful Copy status"))
        assert self.overlay(side) and self.message(side) == expected, "Copy must preserve overlay/message"
        before = int(self.host_text("dev-clipboard-revision"))
        self.click("dev-read-clipboard")
        self.wait(lambda: int(self.host_text("dev-clipboard-revision")) > before, "fresh system clipboard read")
        state = self.host_text("dev-clipboard-state")
        assert state == "ok", f"clipboard read failed ({state}); check READ_PASTEBOARD signing ACL and user grant"
        actual = json.loads(self.host_text("dev-clipboard-text"))
        assert actual == expected, (side, "clipboard differs", len(actual), len(expected))
        (self.device.directory / f"copied-{len(self.results):02d}-{side}.txt").write_text(actual, encoding="utf-8", newline="")

    def dismiss(self, side, expected):
        self.click_side(side, "nsh-dev-overlay-dismiss")
        self.wait(lambda: not self.overlay(side), (side, "Dismiss must hide overlay"))
        self.inspect()
        assert self.last_error(side) == expected, "Dismiss must preserve lastError"

    def count(self, side):
        return self.counts_from(self.nodes())[side]

    @staticmethod
    def counts_from(nodes):
        result = {}
        for side in ("left", "right"):
            scope = next((node for node in nodes if node.get("id") == f"dev-slot-{side}"), None)
            prefix = scope["hierarchy"] + "," if scope else "unmounted,"
            values = [int(node["id"].split(":")[1]) for node in nodes
                      if node.get("id", "").startswith("dev-underlay-count:")
                      and node.get("hierarchy", "").startswith(prefix)]
            result[side] = values[0] if len(values) == 1 else None
        return result

    def healthy(self, side):
        if self.trace_actions:
            self.checkpoint(f"before-healthy-{side}")
        self.server.bundle(side, self.source)
        self.wait(lambda: self.count(side) == 0 and not self.overlay(side), (side, "successful healthy bundle"))
        if self.trace_actions:
            self.checkpoint(f"mounted-{side}-before-inspect")
        self.inspect()
        assert self.last_error(side) == "", "successful reload must clear lastError"
        assert self.host_text(f"dev-state-{side}") == "running"

    def record(self, name, **details):
        self.results.append({"case": name, **details})
        (self.device.directory / "results.json").write_text(
            json.dumps(self.results, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        remote = "/data/local/tmp/nsh-dev-overlay.jpeg"
        self.device.run("shell", "snapshot_display", "-f", remote)
        self.device.run("file", "recv", remote, str(self.device.directory / f"{len(self.results):02d}-{name}.jpeg"))
        print(f"PASS {name}", flush=True)

    def start(self):
        self.device.run("shell", "aa", "force-stop", "com.example.nsh")
        self.device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
        self.server.peer("left")
        self.server.peer("right")
        self.click("dev-inspect")
        self.inspect()
        assert self.host_text("dev-state-left") == self.host_text("dev-state-right") == "empty"

    def run(self):
        self.start()
        left = stack_message("LEFT-BUILD")
        right = stack_message("RIGHT-CANNOT-RUN", 48)
        self.server.error("left", left)
        self.server.error("right", right, "cannot_run")
        self.expect_error("left", left)
        self.expect_error("right", right)
        self.copied("left", left)
        self.inspect()
        assert self.host_text("dev-state-left") == "empty", "first Copy must run before any Runtime"
        assert self.status("right") == "", "left Copy must not change right status"
        self.copied("right", right)
        assert self.status("left") == "Copied" and self.message("left") == left
        self.record("pre-runtime-copy-and-independent-surfaces", left_characters=len(left), right_characters=len(right))

        self.dismiss("left", left)
        assert self.overlay("right") and self.message("right") == right
        self.server.error("left", left)
        self.expect_error("left", left)
        assert self.status("left") == "", "a repeated error must reopen with fresh Copy feedback"
        self.dismiss("left", left)
        replacement = stack_message("LEFT-AFTER-DISMISS", 72)
        self.server.error("left", replacement)
        self.expect_error("left", replacement)
        assert self.status("left") == "", "new error must reset prior Copied status"
        self.copied("left", replacement)
        self.record("dismiss-preserves-error-next-error-reopens")

        root_before = self.device.bounds(self.side_node("left", "nsh-dev-overlay"))
        header_before = self.device.bounds(self.side_node("left", "nsh-dev-overlay-copy"))
        message_before = self.side_node("left", "nsh-dev-overlay-message")
        text_top = self.device.bounds({"bounds": message_before.get("origBounds") or message_before["bounds"]})[1]
        scroll = self.side_node("left", "nsh-dev-overlay-scroll")
        x1, y1, x2, y2 = self.device.bounds(scroll)
        self.input_swipe(scroll, "left:overlay-body-scroll", ((x1 + x2) // 2, y2 - 24),
                         ((x1 + x2) // 2, y1 + 24), 500)
        def scrolled():
            node = self.side_node("left", "nsh-dev-overlay-message")
            return self.device.bounds({"bounds": node.get("origBounds") or node["bounds"]})[1] < text_top - 10
        self.wait(scrolled, "native multiline error body must scroll")
        assert self.device.bounds(self.side_node("left", "nsh-dev-overlay-copy")) == header_before
        assert self.device.bounds(self.side_node("left", "nsh-dev-overlay")) == root_before
        self.click("dev-resize")
        self.wait(lambda: self.device.bounds(self.side_node("left", "nsh-dev-overlay"))[2] - root_before[0]
                  < root_before[2] - root_before[0], "overlay must follow resized surface")
        for side in ("left", "right"):
            overlay = self.device.bounds(self.side_node(side, "nsh-dev-overlay"))
            slot = self.device.bounds(self.node(f"dev-slot-{side}"))
            assert all(abs(a - b) <= 3 for a, b in zip(overlay, slot)), (side, "overlay fills only its surface", overlay, slot)
        self.copied("left", replacement)
        self.record("native-scroll-fixed-header-and-resize")

        self.healthy("left")
        assert self.overlay("right") and self.message("right") == right
        blocker = stack_message("LEFT-TOUCH-BLOCKER", 24)
        self.server.error("left", blocker)
        self.expect_error("left", blocker)
        body = self.side_node("left", "nsh-dev-overlay-scroll")
        self.input_click(body, "left:overlay-body-touch-blocked")
        time.sleep(.3)
        assert self.count("left") == 0, "overlay body touch must not activate underlying app"
        self.dismiss("left", blocker)
        self.input_click(body, "left:same-body-point-after-dismiss")
        self.wait(lambda: self.count("left") == 1, "Dismiss must restore underlying interaction")
        self.record("recovery-and-underlying-touch-blocking")

        runtime_message = stack_message("RUNTIME-THROW", 80)
        source = "function overlayCrash() { throw new Error(" + json.dumps(runtime_message, ensure_ascii=False) + "); }\noverlayCrash();"
        self.server.bundle("left", source)
        self.wait(lambda: self.overlay("left") and runtime_message in self.message("left"), "native runtime error overlay")
        self.inspect()
        runtime_error = self.last_error("left")
        assert runtime_message in runtime_error
        self.expect_error("left", runtime_error)
        self.copied("left", runtime_error)
        self.healthy("left")
        self.healthy("right")
        self.record("runtime-stack-copy-and-successful-reload")

        self.lifecycle()
        print("PASS native dev overlay: real pre-runtime/runtime failures, exact Unicode clipboard, independent surfaces, copy/dismiss/reopen/recovery, touch blocking, scrolling, resize and lifecycle", flush=True)

    def lifecycle(self):
        if self.trace_actions:
            self.checkpoint("before-copy-revision-race", {"left": 0, "right": 0})
        pending = stack_message("COPY-REPLACEMENT", 56)
        self.server.error("left", pending)
        self.expect_error("left", pending)
        self.click_side("left", "nsh-dev-overlay-copy")
        observed = self.status("left")
        newest = stack_message("NEW-ERROR-AFTER-COPY", 36)
        self.server.error("left", newest)
        self.expect_error("left", newest)
        time.sleep(.4)
        assert self.status("left") == "", "stale Copy completion must not label the new error Copied"
        self.copied("left", newest)
        self.record("copy-error-revision-race", timing="best-effort", observed_status=observed)

        self.click("dev-mount-left")
        detached = stack_message("ERROR-WHILE-UNMOUNTED", 20)
        self.server.error("left", detached)
        self.inspect()
        self.wait(lambda: not self.overlay("left"), "unmounted ContentSlot hides its overlay")
        self.click("dev-mount-left")
        self.expect_error("left", detached)
        self.copied("left", detached)
        assert self.count("left") == 0 and self.count("right") == 0
        self.record("retained-surface-remount")

        old_peer = self.server.peer("left")
        self.click_side("left", "nsh-dev-overlay-copy")
        observed = self.status("left")
        self.click("dev-dispose-left")
        self.wait(lambda: not self.overlay("left"), "disposed overlay must retire")
        old_send = False
        try:
            old_peer.text({"type": "build_failed", "message": "STALE-DISPOSED-ERROR"})
            old_send = True
        except Exception:
            pass
        self.click("dev-recreate-left")
        self.server.peer("left", after=old_peer.serial)
        fresh = stack_message("FRESH-SURFACE", 28)
        self.server.error("left", fresh)
        self.expect_error("left", fresh)
        time.sleep(.4)
        assert self.status("left") == "", "old Copy completion must not change recreated surface"
        self.copied("left", fresh)
        assert self.count("right") == 0 and not self.overlay("right")
        self.healthy("left")
        self.record("dispose-recreate-rejects-stale-actions", timing="best-effort", observed_status=observed,
                    old_connection_send_accepted=old_send)

    def lifecycle_only(self):
        self.start()
        warmup = stack_message("LIFECYCLE-WARMUP", 80)
        self.server.error("left", warmup)
        self.expect_error("left", warmup)
        self.copied("left", warmup)
        self.healthy("left")
        self.healthy("right")
        self.checkpoint("both-healthy", {"left": 0, "right": 0})
        self.require_zero = True
        self.click("dev-resize")
        self.lifecycle()
        print("PASS focused Copy/error/remount/dispose lifecycle with action-by-action counts", flush=True)

    def scroll_only(self):
        self.start()
        message = stack_message("SCROLL-TO-END", 96)
        self.server.error("left", message)
        self.expect_error("left", message)
        root = self.device.bounds(self.side_node("left", "nsh-dev-overlay"))
        header = self.device.bounds(self.side_node("left", "nsh-dev-overlay-copy"))
        deadline = time.monotonic() + 40
        gestures = 0
        while True:
            scroll = self.side_node("left", "nsh-dev-overlay-scroll")
            viewport = self.device.bounds(scroll)
            text = self.side_node("left", "nsh-dev-overlay-message")
            original = text.get("origBounds")
            assert original, "native dump must expose unclipped text bounds for END visibility"
            text_bounds = self.device.bounds({"bounds": original})
            if viewport[1] < text_bounds[3] <= viewport[3] + 3:
                time.sleep(.3)
                settled = self.side_node("left", "nsh-dev-overlay-message")
                bottom = self.device.bounds({"bounds": settled.get("origBounds") or settled["bounds"]})[3]
                if viewport[1] < bottom <= viewport[3] + 3:
                    break
            assert time.monotonic() < deadline, ("END marker not reachable", viewport, text_bounds, gestures)
            x = (viewport[0] + viewport[2]) // 2
            self.input_swipe(scroll, "left:overlay-scroll-toward-end", (x, viewport[3] - 20),
                             (x, viewport[1] + 20), 4000)
            gestures += 1
            time.sleep(.25)
        assert self.message("left").endswith("END::SCROLL-TO-END::終")
        assert self.device.bounds(self.side_node("left", "nsh-dev-overlay-copy")) == header
        assert self.device.bounds(self.side_node("left", "nsh-dev-overlay")) == root
        self.copied("left", message)
        self.record("full-stack-end-reachable-fixed-header", gestures=gestures)
        print("PASS focused long-stack END visibility and exact full clipboard copy", flush=True)


def self_test(directory):
    from websockets.sync.client import connect
    with DevServer(directory, port=0) as server:
        with connect(f"ws://127.0.0.1:{server.port}/ws?side=left") as client:
            client.send(json.dumps({"type": "hello", "protocol": 2, "bytecode_version": "test", "device": "host self-test"}))
            assert json.loads(client.recv()) == {"type": "hello", "protocol": 2, "session": 21001}
            peer = server.peer("left")
            message = stack_message("SELF-TEST", 2)
            server.error("left", message)
            assert json.loads(client.recv()) == {"type": "build_failed", "message": message}
            server.bundle("left", "throw new Error('source frame');")
            assert json.loads(client.recv()) == {"type": "bundle", "version": 1, "format": "source", "source_map": False}
            assert client.recv() == b"throw new Error('source frame');"
            serial = peer.serial
        with connect(f"ws://127.0.0.1:{server.port}/ws?side=left") as client:
            client.send(json.dumps({"type": "hello", "protocol": 2}))
            assert json.loads(client.recv())["protocol"] == 2
            assert server.peer("left", after=serial).serial > serial
        assert not server.failures, server.failures
    print("PASS local protocol-v2 handshake, Unicode failure frame, source text/binary framing and reconnect identity")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="explicit MateBook emulator, must be 127.0.0.1:10090")
    parser.add_argument("--artifacts", type=Path, default=Path("target/ohos/dev-overlay-e2e"))
    parser.add_argument("--bundle", type=Path,
                        default=Path(__file__).parents[1] / "entry/src/main/resources/rawfile/bundle.js",
                        help="compiled tests/dev-overlay source bundle from the HAP build")
    parser.add_argument("--port", type=int, default=8797, help="local WebSocket port; fixture URL uses 8797")
    parser.add_argument("--timeout", type=float, default=15, help="seconds allowed for each native transition")
    parser.add_argument("--trace-actions", action="store_true", help="record clicked/swiped bounds and both underlying counts for any native scope")
    scope = parser.add_mutually_exclusive_group()
    scope.add_argument("--self-test", action="store_true", help="check the local server without HDC or a device")
    scope.add_argument("--lifecycle-only", action="store_true", help="diagnose Copy/error/remount/dispose with every clicked bound and both counts")
    scope.add_argument("--scroll-only", action="store_true", help="check final stack line is reachable, fixed header and exact copying")
    args = parser.parse_args()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    if args.self_test:
        self_test(args.artifacts)
    else:
        if args.target != "127.0.0.1:10090":
            parser.error("--target 127.0.0.1:10090 is required; this fixture only operates the dedicated MateBook emulator")
        spec = importlib.util.spec_from_file_location("surfaces", Path(__file__).with_name("check-surfaces.py"))
        surfaces = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(surfaces)
        source = args.bundle.read_text(encoding="utf-8")
        with DevServer(args.artifacts, args.port) as server:
            check = OverlayCheck(surfaces.Device(args.target, args.artifacts), server, source, args.timeout,
                                 trace_actions=args.trace_actions or args.lifecycle_only)
            if args.lifecycle_only:
                check.lifecycle_only()
            elif args.scroll_only:
                check.scroll_only()
            else:
                check.run()
