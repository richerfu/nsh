# Embed Native Shell in OHOS

This API 20+ arm64 example embeds two independent `NshSurface` instances.
Each owns a native `NodeContent`, engine and VM. The app places each content
in a standard `ContentSlot` and controls its layout and lifetime. NativeAbility
provides the shared resource, clipboard, window and URL plugins; one standard
DefaultXComponent supplies their UI context.

## Build

From the NSH repository root, with the matching binding checkout at
`../../ohos-rs/ohos-native-bindings` and Ability checkout at
`../../ohos-rs/openharmony-activity`:

```sh
rustup target add aarch64-unknown-linux-ohos
cargo install ohrs --locked
export DEVECO_CONTENTS=/path/to/DevEco-Studio.app/Contents
export OHOS_NDK_HOME="$DEVECO_CONTENTS/sdk/default/openharmony"
cargo xtask ohos
cargo build -p nsh-cli
examples/embed-ohos/build.sh
```

The native build writes `target/ohos/debug/arm64-v8a/libnsh_platform_ohos.so`
and `target/ohos/debug/index.d.ts`. The example script copies those files and
the SDK's `libc++_shared.so`, bundles `examples/counter` into raw resources,
installs the ArkTS dependencies and invokes Hvigor. Its unsigned output is
`entry/build/default/outputs/default/entry-default-unsigned.hap` under this
example directory. Generated declarations, bundles and build outputs are
ignored by Git.

The default build targets API 20. Optional enhancements are selected at build
time and gated again against the real runtime:

```sh
cargo xtask ohos --dev --features api-26
NSH_PROFILE=debug-dev-api-26 examples/embed-ohos/build.sh
```

`api-23` requires API 23 and enables custom-node accessibility providers;
`api-26` requires API 26, includes it and
adds named custom actions. Both retain the API-20 deployment baseline.
`getCapabilities()` reports compiled and runtime availability. The toolbar's
**Capabilities** control displays this snapshot. `tests/api-features` exercises
subtree hue effects, layered/inset shadows, native capitalization, checked
semantics, live regions and optional accessibility actions.

For a release native library:

```sh
cargo xtask ohos --release
NSH_PROFILE=release examples/embed-ohos/build.sh
```

`NSH_EXAMPLE=examples/embed-ohos/tests/ability` selects the resource/clipboard
fixture instead of Counter. `NSH_PROFILE` chooses the native SDK directory. The script assembles the ArkTS
host in debug mode so that DevEco development signing can be used for testing.

## Install and run

Open this directory in DevEco and configure development signing for
`com.example.nsh`. Install its signed HAP on an explicitly selected target:

```sh
hdc list targets
hdc -t <target> install -r <signed.hap>
hdc -t <target> shell aa start -a EntryAbility -b com.example.nsh
hdc -t <target> shell 'hilog -x -T nsh'
```

The sample sets both compatible and target SDK versions to `6.0.0(20)` and
uses the arm64 library. Runtime checks recorded for this checkout use an
API 26 emulator; API 20 system execution is still a verification item.

## Check the instances

1. Increment the two counters separately: their values and histories are independent.
2. **Resize** and **Swap** change ordinary host layout while retaining both states.
3. **Unmount / Mount** removes and reattaches ContentSlots while retaining instances and state.
4. **Single** disposes the right instance; **Split** creates a new one at zero.
5. **Stop left / Reload left** restarts only the left module. **Dispose left** is permanent;
   **Recreate left** creates a new view. **Missing left**, **Race** and **Invalid** exercise errors.
6. **Status** reports instance errors and engine state.

Build with `NSH_EXAMPLE=examples/embed-ohos/tests/surfaces`, sign/install the
HAP, then run the automated native regression:

```sh
python3 examples/embed-ohos/tests/check-surfaces.py --target <target>
```

The script checks independent counters, host viewport updates, ten remount
cycles retaining state, stop/reload, failed and cancelled loads, invalid input,
idempotent disposal and recreation with a stable process ID.

## Host code

`EntryAbility` extends the existing `NativeAbility` and registers standard
resource, clipboard, accessibility, window, WebView and URL factories, plus
`nativeShellPlugins()` from `@nsh/ohos`. The app's `Index.ets` owns two
ordinary `NshSurface` fields and a ContentSlot per instance. Call `loadResource`
or `loadSource` to run a module; source can be supplied before attachment.
Rust measures the native container and resizes the engine automatically.
The page disposes its instances when the page owner disappears.

One DefaultXComponent serves Ability's UI-plugin root; rendering belongs to
the explicit view instances. NSH's native entry adapts Ability's existing
Module contract and delegates session, lifecycle and bridge dispatch to it.
The app writes its own ArkUI layout and uses the standard plugin packages.

NSH ships a native library, generated declarations and its thin `@nsh/ohos` HAR.
`cargo xtask ohos` records exact locked Ability/binding sources, features, minimum
API and artifact hashes in `build-metadata.json`. `prepare-ability.py` stages
Ability HARs from that Cargo revision and the NSH HAR in
`target/ohos/arkts-deps`; `paired-sources.json` records staged hashes. The example
checks source identity, the packaged native SHA256 and manifest minimum. Use
`NSH_UNSIGNED=1` for an unsigned local acceptance HAP; build-profile changes are
restored after assembly.

See [the backend guide](../../docs/dev/ohos.md) for binding dependencies,
ownership, verification and remaining platform work, and the
[embedding API reference](../../website/docs/embedding-ohos.md) for SDK setup
and the instance contract.

`HostBadge.ets` is an optional native-component example. The base view requires
NativeAbility, NshSurface and its ContentSlot; applications register only the
native components their modules use. The badge's `pulse` command animates its
opacity from 0.3 to 1 over 600 ms, matching the iOS and Android examples.

The development SDK is built with `cargo xtask ohos --dev`; select it with
`NSH_PROFILE=debug-dev` when assembling this example. The toolbar can connect
the left view to `ws://127.0.0.1:8790/ws`; use an explicit HDC reverse port for
a local server (`hdc -t <target> rport tcp:8790 tcp:8790`).

`tests/capabilities` exercises native navigation, alerts, action sheets and
Canvas. `tests/check-capabilities.py --target <target>` checks navigation/back,
retained state, dialog isolation and signed-package verification. Its package
checks require test packages and a signed revocation list generated with
`nsh keygen/certify/publish/revoke`, placed in rawfile as `package{1,2}.nshpkg`,
`package-tampered.nshpkg`, `package-root.pub` and `package-revocations.json`.
Use a dedicated test key and app ID. `tests/scrolling` covers native scrolling,
refresh, sticky cells, zoom and touch delivery through a development connection.

`tests/webview` checks actual native WebViews inside Scroll, initialization and
injected scripts, two-way messages, reload and per-node remount. `tests/video`
uses a generated eight-second local H.264 fixture to check native playback,
pause, seek, controls, progress and remount. `tests/alignment` covers layout
animation, style transitions, keyframes, hardware shortcuts, input key events,
status bars and accessibility commands. Build each with `NSH_EXAMPLE` as above.
The **Plugin context** toolbar control removes/recreates Ability's shared
DefaultXComponent while retaining both explicit surfaces.

The `tests/images` fixture and `tests/check-images.py --target <hdc-target>`
verify rendered tint/alpha, blur and fade pixels. `tests/scrolling` and
`tests/check-scroll.py --target <hdc-target>` exercise native touch, offsets,
zoom, sticky cells and refresh in a 90-vp viewport. Build and install the
matching signed fixture before running either script.

For the video HTTP regression, run `python3 tests/serve-media.py`, forward
`hdc -t <target> rport tcp:18769 tcp:18769`, then run
`python3 tests/check-media.py --target <target> --kind video --http`. The
fixture server serves a relative 302 redirect, the local H.264 sample, an
oversized response and a slow response used to check source cancellation.

After the HTTP check, stop the fixture server and remove only its forwarding
rule with `hdc -t <target> fport rm tcp:18769 tcp:18769`.

## Accessibility acceptance on QEMU

Run accessibility checks on the dedicated OpenHarmony QEMU. Use DevEco's
emulator for other native regressions. Select an `ohos-qemu` package whose
manifest advertises `accessibility_test`, `accessibility_cli` and
`virtio_multitouch`, and start its launcher with a visible window:

```sh
"$OHOS_QEMU_PACKAGE/launch/macos.command" --display cocoa --a11y \
  --hdc-port 5555 --qmp-socket /tmp/nsh-qemu-a11y-5555.sock
```

Build the `api-26` SDK, then assemble the acceptance HAP:

```sh
NSH_PROFILE=debug-api-26 NSH_A11Y_E2E=1 \
  NSH_EXAMPLE=examples/embed-ohos/tests/accessibility ./build.sh
```

Use the actual SDK output directory as `NSH_PROFILE` (a dev SDK has the
`debug-dev-api-26` suffix). The flag stages an app-local accessibility test
extension for this HAP and restores the ordinary module manifest afterward.
The test extension drives system accessibility focus and `performAction`;
normal embedding apps use only the standard Ability plugins.

Sign the unsigned HAP with QEMU's development signer and its explicit UDID
(`hdc -t 127.0.0.1:5555 shell bm get --udid`), install it on that target, then:

```sh
python3 tests/check-qemu-accessibility.py --target 127.0.0.1:5555 \
  --package "$OHOS_QEMU_PACKAGE" --socket /tmp/nsh-qemu-a11y-5555.sock \
  --api26 --output ../../target/ohos/qemu-a11y-check
```

The suite uses QMP physical multitouch, checks the system's touch-exploration
and gesture state, and retains tree/event diagnostics. It disables its test
extension on exit. It never installs a HAP or selects a physical device.
Use `--api23` for an `api-23` build and omit both feature flags for the default
SDK. Every variant checks its reported capabilities against the expected set.
`--navigation` adds a separate direction-query diagnostic, first on a stock
ArkUI toolbar Button and then across a provider boundary. Image-specific
results are recorded in [the backend verification notes](../../docs/dev/ohos.md).
