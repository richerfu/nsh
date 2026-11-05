# The OHOS backend (ArkUI)

`crates/nsh-platform-ohos` renders NSH apps on HarmonyOS / OpenHarmony (API 20+, `arm64-v8a`) with ArkUI custom nodes
and lays text out with native `OH_Drawing_Typography`. Each `NshSurface` owns one `NodeContent`, one engine thread and
one QuickJS VM. User-facing page: [website/docs/ohos.md](../../website/docs/ohos.md).

## Building and checking

Building requires the OpenHarmony NDK (discovered from `OHOS_NDK_HOME`, `DEVECO_SDK_HOME` or DevEco Studio's default
location on macOS, Windows and Linux) and the `aarch64-unknown-linux-ohos` Rust target. Off `target_env = "ohos"` the
crate is a host stub, so check the real backend with `xtask`:

```sh
cargo xtask ohos                         # target/ohos/debug
cargo xtask ohos --release               # target/ohos/release
cargo xtask ohos --dev                   # target/ohos/debug-dev
cargo xtask ohos --features api-23       # target/ohos/debug-api-23
cargo xtask ohos --features api-26       # target/ohos/debug-api-26
cargo xtask clippy-ohos                  # clippy across default, dev, api-23 and api-26
```

`cargo xtask ohos` builds `libnsh_platform_ohos.so`, generates `index.d.ts`, and stages the companion `@nsh/ohos`
ArkTS package under `target/ohos/arkts-deps/nsh/`.
The SDK enables the core's shader compiler and the base library's editor, highlighting, math, plot and rich-editor
features explicitly, so the shared component exports remain present after the 0.4.0 feature split.

Features:
- Default / `dev`: API 20 baseline (`OH_ArkUI_*` custom nodes, `OH_Drawing_Typography`, `NativeVSync`, `AVPlayer`,
  system `TextInput` / `TextArea`, and `ArkWeb`).
- `api-23`: native accessibility providers, state descriptions and reduced-motion observation.
- `api-26`: includes `api-23` plus named custom accessibility actions.

## Architecture

- **One `NshSurface` per embedded view** (`src/app/`): the host creates `new NshSurface(this.getUIContext())` and places
  `surface.content` (`NodeContent`) inside a standard ArkUI `ContentSlot`. `@nsh/ohos` binds each window session
  explicitly from the provided `UIContext` (`UIAbilityContext` and `Window`) in a standard `UIAbility` without a custom
  base class or process-global context guessing.
- **Commits and frames** (`src/render/`): the engine submits each `MountBatch` through a N-API threadsafe function to
  the ArkUI UI thread. Per-surface `NativeVSync` callbacks drive animations on the UI thread; idle surfaces request no
  frames.
- **Shaders** (`src/render/shader/`): wgpu runs on GLES only, which waits for the GPU on the calling thread, so each
  shader view renders into a native surface of its own (an XComponent first among its children) from a thread of the
  surface's (`nsh-shader`), paced by the surface's `NativeVSync`, skipping views out of sight; nothing is read back
  (`docs/dev/shaders.md`).
- **Typography** (`src/text/`): the engine thread shapes and measures paragraphs with `ohos-drawing-binding`
  (`FontCollection::shared()`, `Typography`) and hands owned RGBA bitmaps and selection geometry to the UI thread.
- **Controls and media** (`src/input/`, `src/scroll/`, `src/media/`, `src/interop/`): `Input` uses the system
  `TextInput` / `TextArea` BuilderNode; `Scroll` and `List` use native ArkUI `Scroll` (with on-demand `Refresh`);
  `WebView` uses `ArkWeb`; `Video` uses a C `XComponent` surface and an `AVPlayer` worker.
- **Navigation** (`src/navigation/`): OHOS is a native-header platform, as iOS and Android: each `Screen` draws its own
  bar (back button, title) and places only the `headerLeft` / `headerTitle` / `headerRight` items in it; a screen's
  `header` slot (the bar on platforms without a native header) is never shown, as UIKit's and the Android toolbar's
  bars never place it. Screens and modals report `onInsets` as the other backends do: a screen's top inset is 0 below
  a visible, opaque header, else the safe area's top.
- **Hit areas**: `hitSlop` and overflowing children widen `NODE_RESPONSE_REGION`, whose width and height are
  percentages of the node's size (`100` is the node itself), not ratios.

## Upstream integration: 2026-11-05

Baseline: `201d780a`; integrated main: `2fd62283` (193 commits, NSH 0.4.0). This is a source-level adaptation audit;
cross-compilation and host tests do not establish native rendering or interaction parity.

The integration updates `EngineConfig::speech`, the shader built-ins' 32-byte buffer (`progress` and `overflow`),
and the exhaustive mount protocol. `TreeOrder` and `ProjectStates` are opt-in projections used by the terminal
backend; OHOS does not request them. Shader fills continue to draw in their XComponents. Effects show their
children directly, transitions change them immediately, and pages and dialogs retain their ordinary `animation`,
following the fallback also used by GPUI. Only `ShaderRole::Fill` is passed to the OHOS GPU renderer.

The following work remains for native feature parity, in suggested order:

| Priority | API or behavior | Current OHOS behavior and implementation boundary |
|---|---|---|
| High | `lifecycle()` / Rust `lifecycle()` | `Environment::lifecycle` stays at its default `Active`. `arkts/src/main/ets/NshHost.ets` folds window inactivity and background into one foreground boolean; `src/app/runtime.rs` uses it to gate frames but does not publish lifecycle changes. Preserve three states and send the environment to the engine, including the initial state. |
| High | `fontFamily` lists; WOFF / WOFF2 imports | `src/text/typography.rs` and `src/render/canvas.rs` pass a list as one font name. Use `nsh_core::style::font_families` with native generic-family mappings; check the native editor path too. `register_font` parses raw bytes as TTF/OTF: call bounded `nsh_core::text::font_data` before parsing and retain the decompressed bytes. |
| High | `ShaderEffect`, `ShaderTransition`, `ScreenEffect`, `ScreenTransition`, `transitionShader` | The fallback above preserves content. Full support needs subtree/surface/backdrop pictures, texture bindings, invalidation, transition clocks and presentation above live children in `src/render/shader/` and navigation. Passing effect programs to the fill pipeline is insufficient. |
| Medium | `Speech.available()`, `Speech.start()` | `Environment::speech` is false and `EngineConfig::speech` is `None`; the core answers unavailable (after capability checks). Add a native recognizer or expose an embedding backend, with permission, cancellation and session teardown. |
| Medium | `Notifications` permission, show, cancel, badge and press events | `Environment::notifications` is false; the core answers unavailable. `Command::Notification` has no OHOS handler. Add permission requests, native notification IDs, badge handling and cold-start press delivery before enabling the flag. |
| Medium | `ViewProps::continues_text`, selectable views | OHOS selection is per paragraph (`src/text/selection/`), with no `SelectionTexts` adapter. Cross-view selection was already a gap; the new soft-wrapped `DiffView` continuation contract additionally needs joins without inserted line breaks. |
| Medium | `AccessibilityState::has_popup`; diagonal resize cursors | The new popup distinction and `Cursor::NwseResize` / `NeswResize` are not consumed by the OHOS backend. Extend native accessibility and pointer presentation where supported. |
| Optional extension | `WebView.sendInput`, `onDownload` | These land with macOS implementations. OHOS handles neither trusted `Command::SendInput` nor native download events; an ArkWeb implementation is separate work, including its input capability and download lifetime/storage rules. |
| Follow-up | System alert/action-sheet and TabBar theme colors | Upstream now derives these from design tokens. OHOS does not handle `Command::DesignTokens`; connect token changes to the ArkTS system controls when adopting that behavior. |

Shared-layer additions need no new platform protocol: `Carousel`, `Tree`, `HoverCard`, `Command`, the new plot
marks (`Rule`, `Dot`, `Cell`, `Treemap`, per-row `Bar` bases and a second y scale), Editor decorations, DiffView
review/scroll APIs, `focusWithinStyle`, `data-*` state classes, and controls' `hitSlop` use the existing primitives.
They still need native smoke tests, especially focus, popup placement, scrolling and accessibility.
`fetch(asset:/...)` is served by the engine through the existing OHOS `Assets` implementation.
`nsh/updater` is for packaged apps; the OHOS embedding host supplies no updater host, so checks return no update.

Validation for this integration: `cargo xtask ohos` builds the API 20 SDK; `cargo xtask clippy-ohos` passes default,
dev, API 23 and API 26 with warnings denied; `cargo test -p nsh-platform-ohos --locked` passes 68 host tests;
`cargo test -p nsh-core --lib platform:: --locked` passes 10 tests, including OHOS desktop/terminal classification.
The CI path check and formatting of the touched Rust files pass. The workspace-wide formatting check still reports
six files identical to upstream (base library, macOS, Windows and xtask); its whitespace check also reports the
upstream KaTeX license and browser-size document. No device smoke tests were run in this integration.
