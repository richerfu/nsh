# Embedding Native Shell in an OpenHarmony / HarmonyOS Next app

An ArkTS (`UIAbility`) app that embeds Native Shell (`NshSurface`) below its own native UI (`entry/src/main/ets/pages/Index.ets`), registers a custom ArkUI component (`embed.badge` in `entry/src/main/ets/components/HostBadge.ets`), and runs `examples/embedded` (or any NSH app).

## Prerequisites

- OpenHarmony / HarmonyOS Next SDK with API 20+ NDK (`OHOS_NDK_HOME`)
- DevEco Studio (`DEVECO_CONTENTS=/Applications/DevEco-Studio.app/Contents`)
- `ohrs` CLI (`cargo install ohrs`)
- Rust target `aarch64-unknown-linux-ohos` (`rustup target add aarch64-unknown-linux-ohos`)

## Build

From the repository root, build the NSH CLI and the OHOS embedding SDK:

```sh
export DEVECO_CONTENTS="/Applications/DevEco-Studio.app/Contents"
export OHOS_NDK_HOME="$DEVECO_CONTENTS/sdk/default/openharmony"

cargo build -p nsh-cli
cargo xtask ohos --release
```

`cargo xtask ohos --release` compiles `libnsh_platform_ohos.so` and `index.d.ts` into `target/ohos/release/` and stages the companion ArkTS packages under `target/ohos/arkts-deps/`. Pass `--dev` for hot reload and error overlay support (`target/ohos/release-dev/`), or `--features api-23` / `--features api-26` to opt into higher API baselines.

Then assemble the HAP:

```sh
cd examples/embed-ohos
NSH_PROFILE=release ./build.sh
```

By default `build.sh` bundles `examples/embedded` into `entry/src/main/resources/rawfile/`. Set `NSH_EXAMPLE=examples/counter` (or another app path) to bundle a different example, and `NSH_SPLIT=1` to emit lazy-loaded chunk bundles.

## Run on a simulator or device

Configure signing in DevEco Studio (`File -> Project Structure -> Signing Configs`), rebuild, and install with `hdc`:

```sh
hdc install entry/build/default/outputs/default/entry-default-signed.hap
hdc shell aa start -a EntryAbility -b com.example.nsh
```

## What `Index.ets` shows

- **`EntryAbility` (`entry/src/main/ets/entryability/EntryAbility.ets`)**: Standard HarmonyOS `UIAbility` (`extends UIAbility`) with no custom base class required.
- **`NshSurface`**: Each `NshSurface` owns an independent NSH engine and exposes `surface.content` (`NodeContent`) for `ContentSlot(surface.content)`. Multiple surfaces can be mounted side by side (`Split` / `Single`).
- **Native component (`embed.badge`)**: Registered per surface with `surface.registerComponent("embed.badge", ...)`, backed by an ArkUI `ComponentContent` builder (`HostBadge.ets`) that receives prop diffs, reports its intrinsic size via `surface.setComponentSize`, and emits events via `surface.emitComponentEvent`.
- **Lifecycle**: `surface.loadResource("bundle.js")` loads a bundled app from `rawfile/`, `surface.stop()` stops the engine while preserving the slot, and `surface.dispose()` permanently releases native resources when the page disappears.
