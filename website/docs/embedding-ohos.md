---
title: OHOS
description: Embed Native Shell apps in an OHOS ArkUI host with an independent engine per surface.
order: 13
group: platforms
---

# OHOS

Native Shell embeds modules in an OHOS app through ArkUI custom
nodes, Taffy layout and native typography. The arm64 backend targets API 20.
Each `NshSurface` owns a platform `NodeContent`, engine, JS VM and VSync
connection. The app places it in a standard `ContentSlot` and controls its
layout and lifetime, just as it owns an embedded view on iOS or Android.

## Requirements

- **DevEco Studio**, its HarmonyOS SDK with the native toolchain, OHPM and
  Hvigor, and an arm64 device or emulator. The commands below use macOS.
- **Rust**, the `aarch64-unknown-linux-ohos` target and **ohrs** to build the SDK
  from the NSH checkout.
- The pinned Ability and native binding sources in the root Cargo manifest.
  Cargo fetches the exact revisions; the SDK metadata pairs their Rust and HAR
  sources. See the [binding setup](../../docs/dev/ohos.md#locked-native-dependencies).

The example sets its compatible and target SDK versions to `6.0.0(20)`.
Build and NDK import checks cover the API 20 baseline; the recorded runtime
checks use an API 26 arm64 emulator. See the
[verification record](../../docs/dev/ohos.md#verification) for the test scope.

## Get the library

From the NSH repository root:

```sh
rustup target add aarch64-unknown-linux-ohos
cargo install ohrs --locked
export DEVECO_CONTENTS=/path/to/DevEco-Studio.app/Contents
export OHOS_NDK_HOME="$DEVECO_CONTENTS/sdk/default/openharmony"
cargo xtask ohos
```

The generated SDK is:

```text
target/ohos/debug/
  arm64-v8a/libnsh_platform_ohos.so
  index.d.ts
  build-metadata.json
  native-imports.json
```

`cargo xtask ohos --release` writes the same layout under `target/ohos/release`.
The declarations come from the Rust API during the build.
`cargo xtask ohos --dev` includes the development client and writes `target/ohos/debug-dev`;
combine `--dev --release` for `target/ohos/release-dev`.

## API baseline and enhancements

The default SDK targets API 20. Choose Cargo features for additional capabilities:

| Build | Native capabilities |
|---|---|
| Default | System editors and password fields, checked semantics, live-region speech, ordered foreground/backdrop filters, content-alpha shadows and plus-darker blending |
| `api-23` | Accessibility providers on custom semantic leaves, increment/decrement actions, range information and system reduced-motion observation |
| `api-26` | Includes `api-23`; up to 16 named custom accessibility actions per node |

```sh
cargo xtask ohos --features api-23
cargo xtask ohos --dev --features api-26
```

These write `target/ohos/debug-api-23` and `target/ohos/debug-dev-api-26`.
The selected feature raises the deployment minimum to API 23 or API 26. The
example manifest follows SDK metadata; capabilities report the compiled features
and real system version:

```ts
import { getCapabilities } from 'libnsh_platform_ohos.so';
const capabilities = getCapabilities();
// apiLevel, minimumApiLevel, compiledApi23, compiledApi26, api23, api26
```

The paired `@nsh/ohos` HAR owns SDK controls and texture producers. Native Shell
owns semantic policy, composition and native video playback. Hosts use the same
NshSurface and ContentSlot integration for each feature set.

Use `ohos={{ accessibilityLiveRegion: "polite" }}` or `"assertive"` on a live region.
The embedding example provides a dedicated OpenHarmony QEMU acceptance suite
for system focus, actions and announcement events; see its
[test instructions](https://github.com/richerfu/nsh/tree/main/examples/embed-ohos#accessibility-acceptance-on-qemu).

## Add it to a DevEco project

Use this layout in the app's `entry` module:

```text
entry/
  libs/arm64-v8a/
    libnsh_platform_ohos.so
    libc++_shared.so
  src/main/cpp/types/libnsh_platform_ohos/
    Index.d.ts
    oh-package.json5
  src/main/resources/rawfile/bundle.js
```

Copy `index.d.ts` from the SDK as `Index.d.ts`. Copy `libc++_shared.so` from
`$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/` alongside the NSH library.
The native module's `oh-package.json5` is:

```json
{
  "name": "libnsh_platform_ohos.so",
  "version": "0.1.1",
  "types": "./Index.d.ts"
}
```

Add the module to `entry/oh-package.json5`, then run OHPM install:

```json
{
  "dependencies": {
    "libnsh_platform_ohos.so": "file:./src/main/cpp/types/libnsh_platform_ohos"
  }
}
```

## Ship a bundle with the app

Build the NSH module into the app's raw resources:

```sh
cargo build -p nsh-cli
./target/debug/nsh build examples/counter --release --out examples/embed-ohos/entry/src/main/resources/rawfile
```

`surface.loadResource("bundle.js")` reads the source through Ability's
resource plugin on a native worker. Imported assets load from `rawfile/assets/`
through the same plugin. Keep this resource in the host app's signed HAP. The source
runs with the runtime's normal hardening and default VM limits.

## Load a signed package

The host owns a persistent trust store in its private files directory:

```ts
import { NshTrust } from 'libnsh_platform_ohos.so';

const trust = await NshTrust.open(stateDirectory, [rootPublicKeyBase64]);
const app = await trust.openPackage(downloadedPackagePath);
surface.runPackage(app);
```

`openPackage` verifies signatures, certificates, capabilities, revocations and
rollback state, then extracts scoped assets on a worker. `openResource(name)`
performs the same verification for a signed package in the HAP's raw resources.
`app.info` contains `id`, `name`, `version` and an exact decimal `build` string;
`app.isConsumed` becomes true when one surface accepts it. Open the package again
for another surface. Host capability grants and VM limits apply to every run.

`trust.addRoot(base64)` adds a host root. `await trust.updateRevocations(json)`
verifies and persists a newer signed revocation list and returns its decimal
sequence number. Stores opened on the same private directory serialize state
updates and observe each other's accepted builds and revocations.

## Development connection

With a development SDK, run `nsh dev path/to/app` and call
`surface.connectDevServer('ws://host:8790/ws')`. Each view owns its connection,
reconnects automatically and receives source maps, hot updates and complete
bundle recovery. Native and JS logs reach the developer's terminal; build and
runtime errors appear over the initiating surface. The overlay keeps Copy and
Dismiss controls above the scrollable error text, including multiline stacks.
Copy writes the complete error to the clipboard and shows Copied after the write
succeeds; a failed write displays feedback alongside the original error. Dismiss
hides the overlay and retains `lastError`. A new error opens it again, and
accepting replacement source clears the previous error. Each surface keeps its
own error presentation, including build failures before its first module starts.
Hot updates follow the [shared state-preservation rules](../../docs/dev/tooling.md).
`disconnectDevServer()` retains the loaded module. Explicit loads and `stop()`
end that view's development connection.

## Show the app

Use `@nsh/ohos` in your DevEco app. Your `EntryAbility` stays a standard
HarmonyOS `UIAbility` (`extends UIAbility`). Keep each `NshSurface` in an
ordinary host field, load a resource, and place `surface.content` in a sized
`ContentSlot`:

```ts
import { NshSurface } from '@nsh/ohos';

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface();
  @State error: string = '';

  aboutToAppear(): void {
    this.surface.setErrorHandler((message: string) => { this.error = message; });
    this.surface.loadResource('bundle.js').catch((error: Error) => {
      this.error = error.message;
    });
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  build() {
    Column() {
      Text(this.error)
      Stack() { ContentSlot(this.surface.content) }
        .width('100%').layoutWeight(1)
    }.width('100%').height('100%')
  }
}
```

`@nsh/ohos` automatically attaches to the active `UIContext`, `UIAbilityContext`
and window lifecycle when the surface is created. Rust owns the `NodeContent`,
measures its native allocation and forwards viewport changes to that instance's
engine. A source loaded before attachment waits until the slot is attached and
has positive dimensions.

Clipboard reads follow the host app's `ohos.permission.READ_PASTEBOARD`
declaration, runtime grant and signing profile. Presence queries inspect the
plain-text MIME type without reading its contents.

## Multiple independent views

The host distinguishes views by their object references. Each can load its
own module, configuration and native component factories:

```ts
private primary: NshSurface = new NshSurface();
private secondary: NshSurface = new NshSurface({ memoryBytes: 67108864 });

// In the page's build method:
Row() {
  Stack() { ContentSlot(this.primary.content) }.width('35%').height('100%')
  Stack() { ContentSlot(this.secondary.content) }.width('65%').height('100%')
}.width('100%').height('100%')
```

Load each instance with `loadResource` or `loadSource`. Use ordinary ArkUI
containers to position, resize or reorder them. Each instance occupies one
ContentSlot at a time; the same bundle can run independently in several views.

Removing a ContentSlot while retaining its instance preserves the engine and
module state. Reattaching it restores that tree with the current allocation.
`stop()` cancels pending loads and clears the module while retaining the view,
options and factories. Loading again creates a fresh engine. `dispose()`
permanently releases an instance; create a new one for a new lifetime. Ability
teardown disposes every remaining instance. A page that disposes in
`aboutToDisappear` should create new instances if it retains and reuses that
page object.

## Embedding API

Create and use SDK objects on the ArkUI thread. Declarations are generated
from Rust. Every member below except `initLogging` belongs to its instance.

| Member | Description |
|---|---|
| `new NshSurface(options?: NshOptions)` | Creates an empty native view with validated limits and grants. |
| `readonly content: NodeContent` | Stable platform content for one standard ContentSlot. |
| `activate(): void` | Selects this surface for the system back button, for example when switching host tabs. |
| `runPackage(app: NshPackage): void` | Consumes a verified package and runs it with host grants and limits. |
| `connectDevServer(address: string): void` | Connects the development SDK to a WebSocket dev server. |
| `disconnectDevServer(): void` | Ends the connection and retains the current module. |
| `loadSource(source: string, sourceMap?: string): void` | Accepts trusted source and replaces this instance's module. |
| `loadResource(name: string, sourceMap?: string): Promise<void>` | Reads a relative source or precompiled HAP rawfile, bounded to 64 MiB, with companion capabilities, and accepts it for this instance. |
| `stop(): void` | Cancels pending loads and stops the current module; the view can load again. |
| `dispose(): void` | Permanently releases the engine, tree and factories; repeated calls are harmless. |
| `readonly isLoading: boolean` | Source is being read or waiting for attachment and positive dimensions. |
| `readonly isRunning: boolean` | The current engine has started. |
| `readonly isDisposed: boolean` | The instance has been permanently released. |
| `readonly lastError: string` | Latest load, engine or mount error, cleared when new source is accepted. |
| `setErrorHandler(handler: (message: string) => void): void` | Receives asynchronous load, engine and mount errors on the UI thread. |
| `setEnvironment(changes: NshEnvironment): void` | Applies supplied per-instance observations, including before the first load. |
| `registerComponent(name: string, factory: (id: number) => object): void` | Registers a native widget factory for this instance. |
| `unregisterComponent(name: string): void` | Removes the factory for future mounts; live widgets keep their owners. |
| `emitComponentEvent(handle: number, name: string, data: string \| number \| boolean \| object \| null): void` | Sends an event from a live native component to this instance's module. |
| `setComponentSize(handle: number, width: number, height: number): void` | Reports a live native component's intrinsic size in vp. |
| `initLogging(): void` | Installs the optional HiLog logger with tag `nsh`. |

`loadResource` resolves when source has been read and accepted. Engine
initialization and JS evaluation run asynchronously; observe `lastError` and
the error handler for their failures. `isRunning` describes engine lifetime.
A resource-read or package-verification failure keeps the previous module. Once new source is accepted,
it replaces the old engine; a later evaluation failure does not restore it.
The latest load wins: a later load, `stop`, `dispose` or Ability teardown
invalidates older resource reads. Handle rejected promises, including cancellation.
The session permits up to 32 simultaneous resource reads across its instances.

`NshOptions` configures `assetDirectory`, initial `environment`, `capabilities`
(the host grants JSON schema), `memoryBytes`, `stackBytes`, `maxNodes`,
`maxOpValues`, `maxCanvasOps`, `callBudgetMs` and `loadBudgetMs`. The constructor
validates options before allocation. A selected asset directory scopes
image/font reads; otherwise assets use the HAP resource plugin. Options and
component factories survive module replacement. Pass the source map with each
load. Native layout accepts viewports up to 16,384 vp and a session pixel ratio
in `[0.25, 8]`.

`NshEnvironment` accepts safe insets (`safeTop/Right/Bottom/Left`), keyboard
height/duration, font scale, color scheme (`light`/`dark`), locale, RTL,
reduced motion, screen-reader state, idiom (`phone`/`pad`/`desktop`/`tv`) and
API level. Insets and keyboard height are in vp relative to the surface.
Omitted fields keep their values. Resource configuration supplies initial locale,
appearance and device type. Ability configuration, the accessibility plugin and
window events update font scale, screen-reader state and insets. Host options
provide additional initial observations.
With `api-23` on API 23+, the accessibility plugin observes the system reduced-motion
preference automatically. Native editors receive accessibility labels and hints
directly, and accessibility-focus requests target the actual TextInput/TextArea.

Stack supports slide/fade/none transitions and leading-edge interactive back gestures,
using mounted back/gesture policy and RTL direction. HTTP and image workers use the
system proxy and bypass rules; identical image loads within a surface share work
while retaining independent cancellation.

A component factory receives a scoped handle. Its returned object owns
`node: ComponentContent` and receives `updateProps(NshNativeProperty[])`,
`command(name, args)` and `dispose()` callbacks. Register factories before
loading modules that use them. Handles from a stopped or replaced engine are
rejected. See [HostBadge](../../examples/embed-ohos/entry/src/main/ets/components/HostBadge.ets)
for an optional typed host widget demonstrating this contract. The sample owns
that widget and registers it like any other native component.

Constructor and explicit operation failures throw through N-API. Defer module
replacement or `stop()` requested inside a synchronous widget callback until
the next host turn. Disposal from a callback defers native teardown until that
callback returns. Native measure/layout/draw failures are logged.

## Limits

Each surface applies the shared Rust runtime's default limits:

| Limit | Default |
|---|---|
| JS heap | 256 MiB |
| Interpreter stack | 4 MiB |
| Bundle evaluation | 10 seconds |
| One JS event, timer or frame call | 2 seconds |
| Core nodes | 200,000 |

The OHOS text renderer adapts long paragraphs to its native raster budget while
preserving their logical size. Its native bitmap memory is separate from the JS heap. See
[Sandbox and capabilities](./sandbox-and-capabilities.md) for the runtime's
checks and [the backend reference](../../docs/dev/ohos.md#implementation-status)
for the embedding configuration surface.

## Platform behavior

`Platform.OS` is `"ohos"`; `Platform.select` looks for `ohos`, then `mobile`,
then `native`, then `default`. Use `ohos:` for platform-specific classes:

```tsx
import { Text, View } from "nsh";
import { Platform } from "nsh/device";

<View class="p-4 ohos:bg-zinc-100">
  <Text>{Platform.select({ ohos: "OHOS", default: "Native Shell" })}</Text>
</View>
```

The backend mounts View, Text, Input, Scroll, Image, native components and
Modal nodes. It renders flex/grid layout, solid backgrounds and borders, corner
radii, opacity, clipping, visibility and native press events. Text uses system fonts
with family, size, weight, italic, color, alignment, line height and letter
spacing. Reactive updates, `If`/`Each`, timers and `requestAnimationFrame` use
the shared Rust engine. Embedded NUL characters in text display as U+FFFD.

Content-alpha `drop-shadow()` filters use live native subtrees, including nested
filters and overflowing children. Foreground and backdrop filters run in declaration
order. Backdrop filters sample earlier content within the surface's backdrop root;
`mixBlendMode: "plus-darker"` combines premultiplied colors and alpha. Native
controls keep their input and scrolling behavior during composition.

`Input` uses the system TextInput or TextArea, with native focus, selection,
autofill and accessibility. Secure inputs use the system password TextInput,
including requests that also set `multiline`. Each editor keeps its own value,
focus, selection and lifetime. Soft keyboard character and Backspace edits
produce `onKeyPress` events.

Scroll and List apply their corner radii to viewport clipping; pull-to-refresh is
created when `onRefresh` is provided or `refreshing` is true.
Scroll views use native inertia and snapping, including `decelerationRate` and
`disableIntervalMomentum`. Zoom supports animated scale/offset updates, focal
pinching, bounded zoom bounce and `centerContent`. Touch coordinates follow
presentation transforms and scroll zoom. `keyboardShouldPersistTaps` controls
whether a tap dismisses the editor or reaches its handler; `keyboardDismissMode`
starts dismissal when dragging begins. Text decorations support point-sized
thickness, underline offsets and solid, double, dotted, dashed and wavy patterns.

Holding a selection handle at a scroll viewport edge scrolls and extends the
range on the native frame clock; release stops scrolling.

`Text selectable` supports word selection, draggable range handles and a native
Copy/Select All menu through the NSH-owned `@nsh/ohos` HAR. `selectionColor`
sets the highlight and handles. Copy preserves Unicode text and omits inline
attachment placeholders. Each surface owns its selection; editor focus, text
replacement and touches elsewhere clear it. Ctrl+C, Ctrl+A and Esc operate on
the active range. Selection uses the original engine-thread text layout.

## Build and run the example

After building the SDK and CLI from the repository root:

```sh
examples/embed-ohos/build.sh
# For target/ohos/release after cargo xtask ohos --release:
NSH_PROFILE=release examples/embed-ohos/build.sh
```

The script stages the native libraries and generated declarations, bundles
Counter, installs the ArkTS dependencies and assembles the HAP with Hvigor.
Open `examples/embed-ohos` in DevEco to configure development signing for
`com.example.nsh`. Install the signed HAP on an explicitly selected target:

```sh
hdc list targets
hdc -t <target> install -r <signed.hap>
hdc -t <target> shell aa start -a EntryAbility -b com.example.nsh
hdc -t <target> shell 'hilog -x -T nsh'
```

The toolbar exercises independent views, host layout, **Unmount / Mount**
with retained state, **Stop / Reload**, disposal and recreation. Counter exercises state, click dispatch,
text color changes and insertion/deletion of history rows. Rebuild the source
resource and HAP to deploy module changes; rebuild the SDK for native changes.

## Troubleshooting

| Symptom | Check |
|---|---|
| Cargo cannot fetch the pinned native sources | Fetch the exact Git revisions in `Cargo.lock`; see [native dependencies](../../docs/dev/ohos.md#locked-native-dependencies). |
| ArkTS cannot import the native module | Match the `.so` name, the module dependency and the generated `Index.d.ts`; run OHPM install. |
| The native library fails to load | Include both arm64 libraries and run the HAP on an arm64 target. |
| The surface is empty | Check that the source is ready and the container has a nonzero size; read the instance's `isLoading`, `isRunning`, `lastError` and HiLog. |
| The module requests another native element or operation | Check the [implementation status](../../docs/dev/ohos.md#implementation-status) against the module's primitives and commands. |

## See also

- [Platform overview](./platforms.md)
- [Embedding in iOS apps](./ios.md)
- [Embedding in Android apps](./android.md)
- [Backend architecture, implementation status and verification](../../docs/dev/ohos.md)

## Native WebView and video

Each WebView and video belongs to its NshSurface's native subtree. The Ability
WebView plugin owns the ArkWeb controller and BuilderNode; NSH retains its
native attachment until the SDK confirms retirement. NSH video owns a C
XComponent, native controls and an AVPlayer worker through the native bindings.
UIContext recreation restores components from their current props. Register
the WebView factory and `nativeShellPlugins()` from `@nsh/ohos` in the host.

WebView supports inline HTML, permitted URLs, scoped `asset:/` resources,
document initialization scripts, JavaScript injection, messages, history and
reload. Updating JavaScript access, initialization scripts or the user agent
preserves the controller and navigation history; options complete before the
next navigation. Video supports remote URLs and scoped local media, pause, mute/volume,
looping, fit modes, native controls, seek and playback events. Imported `.mp4`,
`.m4v`, `.webm` and `.mov` files become packaged asset URL strings. With an
unrestricted host policy, HTTP(S) videos stream through AVPlayer, including HLS
and live streams. Restricted URLs use a disk cache with checked redirects and
system proxy settings. Local files and filesystem assets use direct descriptors;
HAP assets stream into the cache in bounded chunks. Positive playback rates use
the native 0.125–4 range; zero pauses. Native controls toggle on tap, hide after
three seconds while playing and preserve manual Play/Pause through unrelated
prop updates.