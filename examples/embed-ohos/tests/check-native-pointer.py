#!/usr/bin/env python3
"""Compare SDK mouse callbacks on ordinary and texture-exported HostBadge widgets.

Build/install tests/native-pointer first. The explicitly selected device is used
as-is; this script never builds, signs or installs an app.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import time

module = importlib.util.spec_from_file_location(
    "surfaces", Path(__file__).with_name("check-surfaces.py"))
surfaces = importlib.util.module_from_spec(module)
module.loader.exec_module(surfaces)


def check(device):
    def states():
        result = {}
        for name in ("plain", "export"):
            nodes = device.matching(f"mouse-state:{name}:")
            if len(nodes) != 2:
                return {}
            for surface, node in enumerate(nodes):
                result[f"{name}:{surface}"] = list(map(int, node["id"].split(":")[2:]))
        return result

    def wait_state(condition, timeout=3):
        deadline = time.monotonic() + timeout
        actual = {}
        while time.monotonic() < deadline:
            actual = states()
            if actual and condition(actual):
                break
            time.sleep(0.1)
        # Stable counts detect duplicate asynchronous dispatch after a transient match.
        time.sleep(0.3)
        return states() or actual

    device.run("shell", "aa", "force-stop", "com.example.nsh")
    device.run("shell", "aa", "start", "-a", "EntryAbility", "-b", "com.example.nsh")
    initial = wait_state(lambda value: all(counts == [0] * 7 for counts in value.values()), 12)
    assert len(initial) == 4, ("four native widget probes must be live", initial)
    device.button("Status")
    cases = []
    failures = []
    axis_deltas = {}
    for name in ("plain", "export"):
        for surface in range(2):
            key = f"{name}:{surface}"
            left, top, right, bottom = device.bounds(device.matching(f"host-mouse-{name}")[surface])
            x, y = str((left + right) // 2), str((top + bottom) // 2)
            for action in ("hover", "left", "right", "axis"):
                device.run("shell", "uinput", "-M", "-m", "0", "0")
                before = states()
                command = ["shell", "uinput", "-M", "-m", x, y]
                if action == "axis":
                    command.extend(["-s", "3"])
                elif action != "hover":
                    command.extend(["-c", "0" if action == "left" else "1"])
                device.run(*command)
                expected = before[key].copy()
                if action == "left":
                    expected[0] += 1
                    expected[1] += 1
                    expected[5] += 1
                elif action == "right":
                    expected[2] += 1
                    expected[3] += 1
                elif action == "axis":
                    expected[6] += axis_deltas.get(surface, 0)

                def complete(value):
                    actual = value[key]
                    if action == "hover":
                        return actual[4] > before[key][4]
                    if action == "axis" and surface not in axis_deltas:
                        return actual[6] > before[key][6]
                    return (actual[:4] == expected[:4] and actual[5] == expected[5]
                            and actual[6] == expected[6])

                after = wait_state(complete)
                problems = []
                if not after or key not in after:
                    problems.append("widget state disappeared")
                else:
                    if action == "axis" and name == "plain":
                        axis_deltas[surface] = after[key][6] - before[key][6]
                        expected[6] += axis_deltas[surface]
                    if not complete(after):
                        problems.append("native callback counters did not match")
                    if action in ("hover", "axis") and (after[key][:4] != expected[:4] or after[key][5] != expected[5]):
                        problems.append("hover/axis changed press/release/click counters")
                    if action != "axis" and after[key][6] != expected[6]:
                        problems.append("mouse input changed axis counters")
                    if action == "axis" and axis_deltas.get(surface, 0) <= 0:
                        problems.append("plain widget did not receive wheel input")
                    if any(after.get(other) != value for other, value in before.items() if other != key):
                        problems.append("another widget or surface changed")
                case = {"widget": key, "action": action, "before": before, "after": after,
                        "expected_target": expected, "problems": problems}
                cases.append(case)
                if problems:
                    failures.append(f"{key} {action}: {', '.join(problems)}")
                print(f"{'FAIL' if problems else 'PASS'} {key} {action}: {after.get(key)}", flush=True)
    (device.directory / "mouse-callbacks.json").write_text(json.dumps(cases, indent=2) + "\n")
    assert not failures, "\n".join(failures)
    print("PASS exact native left/right press/release and click counters, hover, matching wheel callbacks, texture exports and independent surfaces")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    options = parser.parse_args()
    options.artifacts.mkdir(parents=True, exist_ok=True)
    check(surfaces.Device(options.target, options.artifacts))
