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
- `api-23`: native accessibility providers, state descriptions, reduced-motion observation and animated-image
  per-frame durations/autoplay. The API 20 ImageAnimator uses the animation’s total duration with uniform frame
  timing; API 23 preserves the source’s individual frame delays.
- `api-26`: includes `api-23` plus named custom accessibility actions.

## Architecture

- **One `NshSurface` per embedded view** (`src/app/`): the host creates `new NshSurface(this.getUIContext())` and places
  `surface.content` (`NodeContent`) inside a standard ArkUI `ContentSlot`. `@nsh/ohos` binds each window session
  explicitly from the provided `UIContext` (`UIAbilityContext` and `Window`) in a standard `UIAbility` without a custom
  base class or process-global context guessing.
- **Commits and frames** (`src/render/`): the engine submits each `MountBatch` through a N-API threadsafe function to
  the ArkUI UI thread. Per-surface `NativeVSync` callbacks drive animations on the UI thread (honoring capped animation
  `frameRate`s through `OH_NativeVSync_SetExpectedFrameRateRange`); idle surfaces request no frames.
- **Shader fills** (`src/render/shader/`, `src/render/composition/fill.rs`): each fill presents through an XComponent
  first among its children. Fills and effects share the surface's GLES compositor worker (`nsh-compositor`) and
  native VSync, compiling the core's GLSL ES and skipping fills out of sight; pixels stay on the GPU.
- **Shader effects and transitions** (`src/render/composition/gles.rs`): ArkUI exports the native subtree to a
  NativeImage texture while its original nodes retain input. The compositor consumes the external texture and
  renders the core's GLSL ES program with GLES 3 on that same context. Inputs, intermediate targets and output
  remain on the GPU; an inert XComponent Surface outside the exported subtree presents through EGL and keeps
  alpha for RenderService composition. Its geometry changes only when the capture bounds change.
  The worker advances shader clocks on the window's native VSync and votes for the display's current refresh rate.
  UI frame callbacks update ownership and animated uniforms. Actual presentation cadence is measured separately
  from the requested rate (see `performance.md`). Exported CSS filters use ordered GLES passes; masks and ordered
  backdrop roots use the CPU scene compositor (`composition/raster.rs`). Scene groups run their shader layers
  after CSS filters and masks; backdrop samplers read the already composed destination behind that group.
  Texture notifications coalesce into one scene composition on the next VSync, keeping navigation commands
  from waiting behind a full scene raster for every exported child.
  Content transitions hold their previous input texture; an interrupted transition starts from its displayed
  output. Navigation holds the outgoing side once and releases its overlay after the transition completes.
  At completion the GPU keeps the transition's terminal frame until the UI removes its layer; it resumes
  ordinary content only after that handoff, so a returning page cannot flash its outgoing content again.
  Screen shaders target the embedded root and stay active until their declaring node is unmounted, including
  while its page is covered. Native dialogs have their own surface and retain their wrappers until exit completes.
  The root's content wrapper is established at initial mount, before scroll descendants are laid out; enabling
  a screen effect then changes its render target while preserving the root's layout host and scroll positions.
  When a node no longer needs texture export, `Wrapper::retire_composite` keeps `TextureRoot` attached for one
  frame so `RSRenderThread` exports the settled frame, presents that unshaded frame for two frames on the
  `XComponent`, and retains the old `Composite` in `lingering` while `DirectRoot` rebuilds and styles restore.
  Navigation prepares live texture exports while mounting screens and dialogs, then queues a GPU copy of the
  outgoing picture before changing visibility. Page transitions compose on the
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

## Bundle and policy loading

`loadResource` reads `app.json` and `capabilities.json` beside the selected rawfile, off the UI thread
(`src/app/chunks.rs`). `app.json` is optional for single-file bundles. Its `entryChunks` array seeds both
source and bytecode entries, and its `schemes` array populates the custom URL schemes accepted by
`UrlScheme.on("open")` (delivered from the host via `NshSurface::open_url`); source imports also discover
transitive chunks for older artifacts. Chunk ids
are validated as local `.js` file names, deduplicated and bounded to 1024 chunks and 64 MiB total preload
bytes (entry, companions, chunks and maps). A present malformed companion rejects the load. Declarations
are read independently of host grants; `Policy::access()` enforces both.

`NshTrust` is exported by `@nsh/ohos`. `addRoot(publicKeyBase64)` returns `Promise<void>`;
`openResource(name, uiContext?)` returns `Promise<NshPackage>`. An explicit context binds that window;
an omitted context resolves the sole active window session and rejects ambiguity. `NshSurface` construction
still takes its window’s `UIContext`.

`NshTrust.openPackage` and `openResource` share a 256 MiB package limit. Ordinary rawfiles retain their
64 MiB limit. `runPackage` validates the surface and reserves a load generation before taking the package;
a synchronous acceptance failure restores it for retry. A staged or running package is consumed.

`nsh/mobile` reports available on OHOS, mounts the registered ArkUI `TabBar`, and delivers back presses to
`BackHandler`. `process.platform` and `os.platform()` report `"ohos"`; process launch rejects with
`ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`. The updater uses the `ohos` manifest key for host-managed updates.
Its shared checking API remains usable; installation rejects with a host-update message and never compiles
the Linux desktop replacement path for `target_env = "ohos"`.

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
| Device-verified combinations (Batch 5) | Shader backdrop pictures, overflow and CSS scene composition | Scene groups supply ordered backdrop pictures and run shaders after filters and masks. Logical box sizes stay independent of capture insets; captures and fills scale proportionally above 4096 pixels. Screen effects apply to the embedded root; ArkUI dialogs have their own surface. |
| Platform service | `Speech.available()`, `Speech.start()` | HarmonyOS SpeechRecognizer supplies offline zh-CN recognition with microphone permission, cancellation and teardown; availability follows its system capability. |
| Supported | `Notifications` | Native permission, show, cancel, badge and press delivery; hosts forward Ability wants to `onNotificationWant`, including cold starts. |
| Supported | `ViewProps::continues_text`, selectable views, `onSelectionChange` | `src/text/selection/area.rs` adapts mounted text to core `SelectionTexts`, with cross-view ranges, continuation joins and UTF-16 selection start offsets. |
| App-labelled semantics | `Accessibility::has_popup` | Popup controls include the app’s Activate action label in their spoken value. API 20 wrappers supply checked/selected state and adjustable-control semantics; API 23 adds native providers, and API 26 named custom actions. Diagonal resize cursors use the UIContext cursor controller. |
| Supported | `WebView.sendInput`, `onDownload`, `load`, `incognito` | ArkWeb native touch/key dispatch and browser editing commands; `loadUrl` with custom request headers; private cookie stores for `incognito` / `sharedCookies: false`; scoped download delegates write unique private files and cancel on teardown. |
| Supported | System alert/action-sheet colors; TabBar registration | `Command::Tokens` colors native navigation, dialogs and screen backgrounds; OHOS registers a native ArkUI TabBar with selection and tint updates. |

Shared-layer additions need no new platform protocol: `Carousel`, `Tree`, `HoverCard`, `Command`, the new plot
marks (`Rule`, `Dot`, `Cell`, `Treemap`, per-row `Bar` bases and a second y scale), Editor decorations, DiffView
review/scroll APIs, `focusWithinStyle`, `data-*` state classes, and controls' `hitSlop` use the existing primitives.
They still need native smoke tests, especially focus, popup placement, scrolling and accessibility.
`fetch(asset:/...)` is served by the engine through the existing OHOS `Assets` implementation.
`nsh/updater` is for packaged apps; the OHOS embedding host supplies no updater host, so checks return no update.

Validation is recorded in the current Batch 5 section below. Historical counts and checks for earlier
integration stages are retained in [ohos-audit.md](ohos-audit.md), separately from device acceptance.

## Shader presentation lifetime

The UI owns XComponent nodes and the worker owns their EGL windows and textures. On retirement, the UI keeps the
node until the worker has released its window; the worker's completion message also collects nodes removed from
the mount tree. Switching between direct and scene presentation uses the same acknowledgement before disposing
an XComponent. Surface close drains the worker and keeps the UI dispatcher alive until both fill and composite
retirement queues are collected. Engine destruction runs on a cleanup thread; worker shutdown releases EGL
resources on the compositor lane. Before disposing ArkTS builders, `NativeViews` clears the nested
NodeControllers and rebuilds their NodeContainers to detach exported children. Transient EGL output failures during native reparenting
retain the shader and retry presentation; a transition retires after its terminal frame is published.

Capture geometry is in `composition/geometry.rs`: logical view boxes, picture coverage and bounded pixel sizes.
Each shader input is reframed to the layer's symmetric overflow, and the output returns to the presentation's
coverage. The GLES program cache belongs to the texture context; framebuffers remain per effect. Fill admission
checks layer and pixel budgets. Composite bounds and shader visibility are invalidated by tree, geometry, style
and scroll changes, so idle animation frames reuse them.

A dialog's content participates in its native Column's measurement: its root has width/height and no absolute
`Position`. An absolutely positioned root leaves that Column at zero size and displaces hit testing even when a
recorded composite still appears at the expected position.

## Mounting, editors and density

Layout animation markers are consumed in order within each batch, including empty markers;
removal preflight retains each removal's own configuration. Stale node and native detach failures
are logged per item so the rest of a commit can apply. Root creation remains fatal. The animator's
active collections use dense entries with indexed lookup; frame work visits active entries only.

`input/editor.rs` and `NativeEditors.ets` share the SDK editor event route. Text paste honors
`pasteBehavior`, selection also schedules caret bounds in vp, and `onChange` preview text supplies
composition events. Controlled values wait for composition to finish and retain edit-count checks.
Read-only editors keep focus, selection and copy while insertion, deletion and preview mutations
are rejected. Multiline editors use a native `Scroll` with a `Scroller` for ref scrolling and
`EngineControl::input_scroll` reports. Caret geometry includes the input content origin and scroll.
`autoCorrect` reaches `EditorSettings`; API 20's TextInput/TextArea SDK has no per-editor correction
setter, so the system input method still owns that behavior.

Text color changes start the core's color transition and tint recorded glyphs on each native frame.
The explicit animation graph's color takes precedence. Native foreground, backdrop and image blur
attributes receive nonnegative vp radii. Zero-blur inset shadows use the decoration's inset path.
Stack, Image, Input and Scroll clip children. Pointer page coordinates use the same matrix as input
picking, including transforms, sticky pinning and presentation offsets.

Resize updates the shared window environment before remeasuring open modals. `NshEnvironment.pixelRatio`
and host configuration changes update native geometry, painting and engine text recordings.
`set_ui_sets(Vec<nsh_core::element::Registry>)` adds Rust UI sets to `nsh/base` for future engines;
it follows the current desktop/iOS registry API. Host component registration and removal reserve
names beginning with `Nsh` for the SDK.

## System services, chrome and resources

`managesStatusBar` defaults to `true`. A dispatcher becomes a status-bar owner only after receiving
`SetStatusBar` or entering/leaving fullscreen video. Passive embedded surfaces preserve the Ability’s
window bars. The window session arbitrates active surfaces and restores its saved host settings.
`false` leaves all bar ownership with the embedding host.

`UiContext` retains `Command::Tokens` for adaptive semantic colors. Headers use `card`/`foreground`,
dialogs use `popover`/`popover-foreground` (falling through card/background and foreground), and screen
backgrounds use `background`. Actions use `primary`/`destructive`. Neutral light/dark colors are the
last fallback. Header titles occupy the space left by measured leading/trailing items and the vector
back button. Export wrappers preserve header-slot placement, measurement and z-order above the native bar.
Back, dialog and video controls expose button roles; app labels supply their text.
Empty system-dialog action lists complete with index -1. Dialog typography respects font scale;
action sheets align to the window bottom with a separate cancel gap. The native alert content owns its
background and 16 vp system-card corner treatment, so ArkUI's host-theme blur does not replace app colors.

`services/system.rs` and `NativeServiceTypes.ets` carry typed requests/completions over N-API, including
error/code fields. Dispatch failures complete pending requests, and the mounter owns the single open-URL
fallback. Clipboard APIs retain the core’s empty-result contract for unavailable reads. Notification
subscription replacement releases the old request; a 300-listen host regression covers the cap.
`BackHandler.exitApp()` reports the embedding boundary; the host Ability owns application termination.
The app clipboard route is `NativeClipboard.ets`; development error-copy support alone uses the native
pasteboard helper. Image and file picking open read-only descriptors on the UI thread (`PhotoViewPicker` for images,
`DocumentViewPicker` for arbitrary files); Rust duplicates ownership
before ArkTS closes its handles, then bounded reads, MIME inference and image decoding run on `nsh-ohos-host` workers.

`WebView`’s cross-platform `incognito` prop and OHOS `sharedCookies` platform prop (default `true`) control cookie
isolation. With `incognito` or
`ohos={{ sharedCookies: false }}`, ArkWeb uses its private cookie store and clears that store before
controller attachment navigation, subsequent document loads and teardown. `WebViewRef.load({ src, headers })` forwards
custom request headers to ArkWeb’s `loadUrl`. File inputs select PhotoViewPicker
for image/video MIME types and DocumentViewPicker for other types; single/multiple/folder modes and
cancellation are forwarded to ArkWeb’s result callback.

Native images use `nsh_core::image::loader` and `MemoryCache` with the process image budget. OHOS’s
bounded, proxy-aware transport uses the shared redirect policy, including HTTPS downgrade rejection.
Runtime setup installs that transport before `EngineConfig` supplies assets, access and cache to
`Core::set_image_context`, so core SVG loads follow the same policy. Fetch connections release the
six-worker global pool after headers; dedicated response readers hold each surface’s 32-request quota
through stream completion. Upload streams also use a dedicated reader lane. Cancellation wakes paused
readers; an active blocking transport read still ends through its transport timeout.

`VideoProps::audio_session` is retained and updated on the player. Released `ohos-avplayer-binding`
0.1.0 exposes playback but no audio-interrupt/renderer setter; audio policy follows the host’s session.
Accessibility popup announcements use app-provided action text, keeping built-in labels out of NSH.
The API 20 ArkTS wrapper carries selected/checked states; API 23 native providers deliver the full
native action path. API 26 additionally supplies named custom actions.

## Batch 5 real-device verification (2026-11-07)

Signed source and bytecode split bundles were installed and run on **ALN-AL00
(`2LQ0223A26006180`), API 26, `OpenHarmony-7.0.0.111`**, at 1260×2720 pixels and 3.25 px/vp.
The current local debug profile removed Batch 4's signing blocker. The profile's application ID was
used only during assembly; signing configuration, application ID and host fixture were restored after
every build, including failures. DevEco's bundled JDK required the build-only option
`-Djdk.util.zip.disableZip64ExtraFieldValidation=true` for its HAP signer.

The fixture, screenshots, layout dumps, HiLog, build logs and signed-artifact hash are retained locally
in ignored `.nsh/batch5/`; the result matrix and remaining limits are recorded in
[the Batch 5 audit record](ohos-audit.md#batch-5-real-device-acceptance-2026-11-07).

- Split source and bytecode loading, dynamic imports, OHOS identities, TabBar/BackHandler, capability
  intersection and the stable process-launch rejection were observed on the device.
- Screenshots show animated fills, effect origin/overflow with an asymmetric shadow, real backdrop
  sampling, shader/scene coexistence, content/page/dialog transitions and resumed animation after Home.
  Large-fill logical geometry, input paste/selection/scrolling, inset rings, layout/text-color animation,
  native navigation chrome, status-bar requests, WebView rendering/cookie clearing and light/dark alerts
  were exercised. The audit record names the artifacts and distinguishes partial criteria.
- Device failures fixed in this batch: transient native-window output failures stopped shaders;
  scene capture clipped fill overflow; per-export scene work delayed navigation; header wrappers lost their bar placement/z-order; native
  dialog blur overrode the app's dark palette; a newly attached private WebView skipped its first cookie
  clear; nested NodeContainers retained graphics buffers after reload until their controllers were detached.
  Twenty reloads then completed without reload/surface errors, with Graph memory stable at 113,660 KiB
  at reloads 5, 10 and 20.
  Generated WebView props now expose the existing OHOS-specific options.
- Final checks: `cargo test -p nsh-platform-ohos` (**96 passed**), `cargo xtask ohos`,
  `cargo xtask clippy-ohos` (default/dev/API 23/API 26) and `git diff --check`.
- The test bundle and temporary device files were removed. Example signing/build caches were removed,
  original embedded resources rebuilt, and `git diff -- examples/embed-ohos` is empty.

## Known issues

The 2026-11-06 audit and its prioritized, executable fix list are in
[ohos-audit.md](ohos-audit.md).
