# Native development error overlay

This fixture uses two real `NshSurface.connectDevServer` connections and the
`nsh-dev` protocol-v2 client. A local community WebSocket server sends compiler
failures before any bundle, source bundles, runtime exceptions and later
recovery bundles. The first Copy assertion runs while both surfaces have no
runtime. Clipboard verification reads the actual system pasteboard and compares
the entire Unicode stack, including combining accents, emoji sequences, Chinese,
tabs and multiline endings.

Build the API-20-baseline development SDK and acceptance HAP from the repository
root, with the normal OHOS SDK/DevEco environment configured:

```sh
cargo xtask ohos --dev
NSH_PROFILE=debug-dev NSH_DEV_OVERLAY_E2E=1 NSH_CLIPBOARD_E2E=1 \
NSH_EXAMPLE=examples/embed-ohos/tests/dev-overlay \
bash examples/embed-ohos/build.sh
```

The build temporarily copies the complete fixture `Index.ets` into the host and
restores the ordinary page on exit, including a failed build. It also temporarily
declares the clipboard read-back permission. Sign the HAP with a development
profile whose ACL and restricted permissions allow `ohos.permission.READ_PASTEBOARD`,
then install it on the dedicated windowed API-26 MateBook emulator. This follows
the [text-selection fixture's read-back signing procedure](../selection/README.md).

Read the installed application's `applicationInfo.accessTokenId` and grant its
test permission explicitly:

```sh
hdc -t 127.0.0.1:10090 shell bm dump -n com.example.nsh
hdc -t 127.0.0.1:10090 shell atm perm -g -i <token-id> -p ohos.permission.READ_PASTEBOARD
hdc -t 127.0.0.1:10090 rport tcp:8797 tcp:8797
```

Use Python with the community `websockets` package; the fixture was checked with
17.3. A task-local environment keeps it separate from production dependencies:

```sh
python3 -m venv target/ohos/dev-overlay-venv
target/ohos/dev-overlay-venv/bin/python -m pip install websockets==17.3
target/ohos/dev-overlay-venv/bin/python examples/embed-ohos/tests/check-dev-overlay.py --help
target/ohos/dev-overlay-venv/bin/python examples/embed-ohos/tests/check-dev-overlay.py \
  --self-test --artifacts target/ohos/dev-overlay-server-self-test
target/ohos/dev-overlay-venv/bin/python examples/embed-ohos/tests/check-dev-overlay.py \
  --target 127.0.0.1:10090 --artifacts target/ohos/dev-overlay-e2e
```

Focused runs restart the same installed fixture and use the same real server and
clipboard checks:

```sh
target/ohos/dev-overlay-venv/bin/python examples/embed-ohos/tests/check-dev-overlay.py \
  --target 127.0.0.1:10090 --lifecycle-only --artifacts target/ohos/dev-overlay-lifecycle
target/ohos/dev-overlay-venv/bin/python examples/embed-ohos/tests/check-dev-overlay.py \
  --target 127.0.0.1:10090 --scroll-only --artifacts target/ohos/dev-overlay-scroll
```

`--lifecycle-only` warms up real clipboard copying, loads both engines, resizes
the slots, then checks replacement, retained remount and disposal/recreation.
Its `actions.jsonl` records each clicked native bound and both underlying counters
before and after input, stopping at the first unexpected press. `--scroll-only`
scrolls the long native message until its unclipped final line fits inside the
body viewport, checks that the actions stay fixed, and copies the complete text.
Add `--trace-actions` to any native scope to record every click and swipe with
its bounds and both underlying counters. The full scope retains its intentional
underlying tap after Dismiss and checks its expected counter increment.

The runner starts its server on host loopback port 8797 and the native fixture
connects through the explicit HDC reverse port. It accepts only the dedicated
MateBook target. It reads the compiled source bundle left by this fixture's HAP
build; `--bundle` can select a preserved copy. It never builds, signs, installs,
grants permissions or selects another device. `--self-test` uses only a local
WebSocket client and server and verifies handshake, exact Unicode framing,
text/binary bundle framing and reconnect identity without HDC.

The native stages check:

1. BuildFailed/CannotRun, pre-runtime Copy success and isolated per-surface status.
2. Dismiss preserving `lastError`, repeated/new errors reopening, and reset Copy feedback.
3. Native body scrolling with fixed actions, resize bounds and full-message copying.
4. Successful bundle recovery, overlay touch interception and resumed app touches.
5. Runtime stack copying and a later successful source reload clearing the error.
6. Copy followed by a new error and reset feedback (best-effort race attempt).
7. Retained ContentSlot unmount/remount with an error received while detached.
8. Copy followed by disposal/recreation and fresh recovery (best-effort race attempt).

`protocol.jsonl`, stage screenshots, exact copied text files, `results.json` and
the latest native layout are retained in the artifact directory. Copy/replacement
and Copy/disposal races are marked best-effort with observed status because the
system clipboard may finish before the next input command. In both final native
attempts, the observed status was already `Copied` before replacement/disposal;
the old peer was already closed (`old_connection_send_accepted=false`). No native
in-flight completion was observed. Deterministic production `Presentation` tests prove
stale-completion rejection and cover failed writes.

Native acceptance passed with the API-20-baseline development library on the
dedicated API-26 MateBook emulator (`127.0.0.1:10090`):

- All eight stages with `--trace-actions`: `target/ohos/dev-overlay-traced-full`.
- Focused Copy/replacement, remount and recreation: `target/ohos/dev-overlay-lifecycle`.
- Full-stack END visibility, fixed actions and exact clipboard contents:
  `target/ohos/dev-overlay-scroll`.

The final full trace and focused lifecycle trace kept the independent right
underlying counter at zero throughout replacement, remount and recreation.
Host validation also passed `--self-test`, Python syntax checking, fixture
TypeScript checking and `bash -n examples/embed-ohos/build.sh`.

TODO: deterministic native delayed/failing clipboard-write injection, keyboard and
screen-reader navigation, rotation/font-scale matrices, actual API-20/23 systems,
and native memory/frame measurements.
