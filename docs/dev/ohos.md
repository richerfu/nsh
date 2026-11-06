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
ArkTS package under `target/ohos/<profile>/arkts-deps/nsh/`.
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
- **Shader fills** (`src/render/shader/`, `src/render/composition/fill.rs`): each fill presents through an XComponent
  first among its children. Fills and effects share the surface's GLES compositor worker (`nsh-compositor`) and
  native VSync, compiling the core's GLSL ES and skipping fills out of sight; pixels stay on the GPU.
- **Shader effects and transitions** (`src/render/composition/gles.rs`): ArkUI exports the native subtree to a
  NativeImage texture while its original nodes retain input. The compositor consumes the external texture and
  renders the core's GLSL ES program with GLES 3 on that same context. Inputs, intermediate targets and output
  remain on the GPU; an inert XComponent Surface outside the exported subtree presents through EGL and keeps
  alpha for RenderService composition. Its geometry changes only when the capture bounds change.
  The worker advances shader clocks on the window's native VSync, requesting 120 Hz during active rendering.
  UI frame callbacks update ownership and animated uniforms. Actual presentation cadence is measured separately
  from the requested rate (see `performance.md`). Exported CSS filters use ordered GLES passes; masks and ordered
  backdrop roots use the separate CPU scene compositor (`composition/raster.rs`).
  Content transitions hold their previous input texture; an interrupted transition starts from its displayed
  output. Navigation holds the outgoing side once and releases its overlay after the transition completes.
  At completion the GPU keeps the transition's terminal frame until the UI removes its layer; it resumes
  ordinary content only after that handoff, so a returning page cannot flash its outgoing content again.
  Screen shaders target the embedded root and stay active until their declaring node is unmounted, including
  while its page is covered. Native dialogs have their own surface and retain their wrappers until exit completes.
  The root's content wrapper is established at initial mount, before scroll descendants are laid out; enabling
  a screen effect then changes its render target while preserving the root's layout host and scroll positions.
  Navigation takes one outgoing ArkUI snapshot before changing visibility. Page transitions compose on the
  root (the nearest dialog root for nested stacks), with its live destination export; dialogs compose on their
  own surface. Inputs align in physical screen coordinates. The compositor clock drives progress and a
  generation-scoped completion message retires the layer. Continuous frames require no CPU pixel transfer.
- **Typography** (`src/text/`): the engine thread shapes and measures paragraphs with `ohos-drawing-binding`
  (`src/text/fonts.rs`, `Typography`) and hands owned native drawing commands and selection geometry to the UI thread.
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
backend; OHOS does not request them. Shader fills draw in XComponents; effects, content/screen transitions and
pages' and dialogs' `transitionShader` use the compositor path described above.

Native feature support and platform boundaries:

| Priority | API or behavior | Current OHOS behavior and implementation boundary |
|---|---|---|
| Supported | `lifecycle()` / Rust `lifecycle()` | WindowStage publishes active, inactive and background, including the initial state; only background suspends frames. |
| Supported | `fontFamily` lists; WOFF / WOFF2 imports | Core family parsing and bounded font decoding feed the shared engine/Canvas collection (`src/text/fonts.rs`). |
| Follow-up | Shader backdrop pictures, overflow and CSS scene composition | Content/from/to pictures are implemented. Backdrop shader inputs, expanded shader capture bounds and combinations with the CSS scene compositor still need dedicated capture integration and device coverage. Screen effects apply to the embedded root; ArkUI dialogs have their own surface. |
| Platform service | `Speech.available()`, `Speech.start()` | HarmonyOS SpeechRecognizer supplies offline zh-CN recognition with microphone permission, cancellation and teardown; availability follows its system capability. |
| Supported | `Notifications` | Native permission, show, cancel, badge and press delivery; hosts forward Ability wants to `onNotificationWant`, including cold starts. |
| Supported | `ViewProps::continues_text`, selectable views | `src/text/selection/area.rs` adapts mounted text to core `SelectionTexts`, with cross-view ranges and continuation joins. |
| API gap | `AccessibilityState::has_popup` | ArkUI C `ArkUI_AccessibilityState`, native accessibility element info and ArkTS accessibility state expose no popup-kind field. Diagonal resize cursors use the UIContext cursor controller. |
| Supported | `WebView.sendInput`, `onDownload` | ArkWeb native touch/key dispatch and browser editing commands; scoped download delegates write unique private files and cancel on teardown. |
| Supported | System alert/action-sheet colors; TabBar registration | `Command::DesignTokens` colors native dialogs; OHOS registers a native ArkUI TabBar with selection and tint updates. |

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
upstream KaTeX license and browser-size document. The later shader adaptation is validated separately on ALN-AL00
(API 26) using local SVG/text/color fixtures so network image loading does not determine the shader result.

## Shader presentation lifetime

The UI owns XComponent nodes and the worker owns their EGL windows and textures. On retirement, the UI keeps the
node until the worker has released its window; the worker's completion message also collects nodes removed from
the mount tree. Surface teardown joins the worker before destroying the remaining native presentation nodes.

A dialog's content participates in its native Column's measurement: its root has width/height and no absolute
`Position`. An absolutely positioned root leaves that Column at zero size and displaces hit testing even when a
recorded composite still appears at the expected position.

## Known issues

The 2026-11-06 audit and its prioritized, executable fix list are in
[ohos-audit.md](ohos-audit.md).
