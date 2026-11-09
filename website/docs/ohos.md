---
title: OHOS
description: Embed NSH apps in an OHOS app with @nsh/ohos and NshSurface.
order: 13
group: platforms
---

# OHOS

NSH apps run on OHOS with ArkUI custom nodes and native typography, from API 20, on `arm64-v8a`. Existing apps can embed modules in an `NshSurface`, load bundles or signed packages, and run them with limits and capabilities of their own choosing. On OHOS, `os()` is `Os::Ohos` (`platform.os` is `"ohos"` in TSX) and the `ohos={{…}}` prop on TSX elements applies.

## Requirements

- **DevEco Studio** with the HarmonyOS / OpenHarmony SDK (API 20 or later), OHPM, Hvigor, and an `arm64-v8a` device or emulator.
- **Rust** (`rustup target add aarch64-unknown-linux-ohos`) when building the SDK from source on macOS, Windows or Linux. See [Installation](./installation.md#building-the-ohos-embedding-sdk).

## Embedding in an OHOS app

An existing OHOS app can show NSH apps inside its own UI. The runtime ships as a native shared library (`libnsh_platform_ohos.so`) and its companion ArkTS package (`@nsh/ohos`): create an `NshSurface`, place `surface.content` in a standard `ContentSlot`, and load a TSX bundle or a signed package into it. NSH apps render with native ArkUI nodes alongside your own components.

Every `NshSurface` owns its engine, JS VM and `NodeContent`, so you can run several at once. Window services and bounded resource pools coordinate shared system resources.

<div class="diagram-scroll">
<img class="diagram diagram-light" src="/diagrams/host-app-light.svg" alt="A host app with two nsh views: each is an isolated engine with its own thread, VM, limits and capabilities; the host grants capabilities, sets limits and registers native components like a map.">
<img class="diagram diagram-dark" src="/diagrams/host-app-dark.svg" alt="A host app with two nsh views: each is an isolated engine with its own thread, VM, limits and capabilities; the host grants capabilities, sets limits and registers native components like a map.">
</div>

## Get the library

Build the SDK from the NSH repository (`xtask` finds the OpenHarmony NDK in DevEco Studio's default location or from `OHOS_NDK_HOME`):

```sh
cargo xtask ohos --release          # release: bundles and signed packages
cargo xtask ohos --release --dev    # also connects to a `nsh dev` server
```

This writes `arm64-v8a/libnsh_platform_ohos.so` and generated TypeScript declarations `index.d.ts` to `target/ohos/release/` (or `target/ohos/release-dev/`), and stages the companion `@nsh/ohos` package under `target/ohos/<profile>/arkts-deps/nsh/`. Optional Cargo features `--features api-23` and `--features api-26` enable higher-API accessibility providers and reduced-motion observation when targeting API 23 or API 26+.

Place `libnsh_platform_ohos.so` and `libc++_shared.so` (from `$OHOS_NDK_HOME/native/llvm/lib/aarch64-linux-ohos/`) in `entry/libs/arm64-v8a/`, copy `index.d.ts` to `entry/src/main/cpp/types/libnsh_platform_ohos/Index.d.ts`, and reference both packages in `entry/oh-package.json5`:

```json5
{
  "dependencies": {
    "@nsh/ohos": "file:../ohos_nsh",
    "libnsh_platform_ohos.so": "file:./src/main/cpp/types/libnsh_platform_ohos"
  }
}
```

## Ship a bundle with the app

The SDK includes `Editor`, `DiffView`, rich editing, charts, syntax highlighting and formula typesetting. `Shader`
draws a fill through an XComponent. Shader effects process native content textures on the GPU and present through
XComponent surfaces; content, screen, page and dialog transitions use shader-driven progress. The native controls keep their input while an effect is shown. Screen
effects apply to the embedded root, and dialogs use their own `transitionShader` on their native surface.

Shader overflow extends the capture and output around the view's logical box. Large targets render at a
proportionally lower resolution. Effects can sample the composed backdrop and run with CSS masks and filters;
navigation retains its outgoing picture on the compositor worker.

Build the TSX app into the module's `rawfile` resources:

```sh
nsh build App.tsx --release --out entry/src/main/resources/rawfile
```

This writes `bundle.nshc` (bytecode), `bundle.js` (source), `bundle.js.map`, `capabilities.json` (when `nsh.json` declares [capabilities](./sandbox-and-capabilities.md)) and `assets/` (images, fonts and media imported by the app).

## Show the app

```ts
import { NshSurface } from '@nsh/ohos';

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface(this.getUIContext());

  aboutToAppear(): void {
    this.surface.setErrorHandler((message: string) => {
      console.error(`nsh: ${message}`);
    });
    this.surface.loadResource('bundle.js');
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  onBackPress(): boolean {
    return this.surface.onBackPress();
  }

  build() {
    Stack() {
      ContentSlot(this.surface.content)
    }
    .width('100%')
    .height('100%')
  }
}
```

`NshSurface` can also sit inside any ArkUI layout, for example below your own toolbar or side by side in a `Row`:

```ts
Column() {
  Toolbar()
  Stack() {
    ContentSlot(this.surface.content)
  }
  .width('100%')
  .layoutWeight(1)
}
.width('100%')
.height('100%')
```

The slot's size is the app's window: NSH lays the app out at the slot's size and relayouts when it resizes. Loading again replaces the running app. See `examples/embed-ohos/entry/src/main/ets/pages/Index.ets` for a complete example.

## `NshSurface`

| Member | Description |
|---|---|
| `new NshSurface(uiContext: UIContext, options?: NshOptions)` | Create an empty surface bound to `uiContext` with optional VM limits, granted capabilities, asset directory and initial environment. |
| `readonly content: NodeContent` | The platform `NodeContent` to mount inside a standard `ContentSlot`. |
| `setErrorHandler(handler: (message: string) => void)` | Receive fatal app errors on the UI thread (failed loads, uncaught JS exceptions, VM stopped by a limit). Errors are always logged. |
| `setThemeOverrides(json?: string \| null)` | Put the host's brand over the app's [theme](./tailwind.md#theme-tokens): a theme file in the format of `nsh.json`'s `"theme"`. Applies at once and to apps loaded later; `null` restores the app's own theme. Throws if the JSON does not parse. |
| `setEnvironment(changes: NshEnvironment)` | Override locale, font scale, color scheme (`"light"` / `"dark"`), safe-area insets or keyboard height for this surface. Omitted fields keep their current values. |
| `loadResource(name: string, sourceMap?: string): Promise<void>` | Run a bundle (`bundle.js` or `bundle.nshc`) from `resources/rawfile/`; app assets are read from `rawfile/assets/`. |
| `loadSource(source: string, sourceMap?: string)` | Run trusted JS source directly, replacing the running app. |
| `runPackage(pkg: NshPackage)` | Run a verified signed package, replacing the running app. A package can be run once per `openPackage` call. |
| `registerComponent(name: string, factory: (id: number) => object)` | Register a [native component](./native-components.md) for this surface. |
| `emitComponentEvent(handle: number, name: string, data: object \| string \| number \| boolean \| null)` | Emit an event from a mounted native component to JS. |
| `setComponentSize(handle: number, width: number, height: number)` | Report a mounted native component's natural size in `vp`. |
| `setParams(params?: object \| null)` | Launch parameters (`Record<string, Object>`) for the next app loaded: the app reads them as `host.params` at startup ([`nsh/host`](../modules/nsh-host.md)). |
| `send(name: string, data?: object \| string \| number \| boolean \| null)` | Send an event to the app (`host.on(name, …)`); kept until the app listens, or for the next app when none is running. |
| `handle(name: string, handler?: ((data: object \| string \| number \| boolean \| null, reply: NshHostReply) => void) \| null)` | Handle the app's calls (`host.call`) and posts (`host.post`) for `name` on the UI thread; `null` (or `removeHandler(name)`) removes the handler. Answer once via `reply.resolve(value)` or `reply.reject(message)`. |
| `setOpenUrlHandler(handler?: ((url: string) => boolean) \| null)` | Intercept URLs the app opens; return `true` if handled by the host, or `false` to let the system open it. |
| `openUrl(url: string)` | Hand a custom-scheme URL the Ability was opened with (`Want.uri` in `onCreate` / `onNewWant`) to the app's `UrlScheme.on("open")` listeners; queued if no app is running yet. |
| `connectDevServer(url: string)` | Development: load from a `nsh dev` server and reload on every change (requires the `--dev` SDK). |
| `disconnectDevServer()` | Disconnect from the dev server while keeping the current app running. |
| `activate()` | Mark this surface as the active target for the system back gesture when switching between multiple surfaces. |
| `onBackPress(): boolean` | Route a page `onBackPress()` event to this surface's window session; returns `true` when consumed by the app. |
| `stop()` | Stop the running app and cancel pending loads while keeping the surface, its options and registered components for later loads. |
| `dispose()` | Permanently stop the app and release the surface's native resources. |
| `setStorageDirectory(path?: string \| null)` | Set an absolute storage directory for subsequent loads; omitted or `null` uses the host default. |
| `setCacheOptions(options?: NshCacheOptions \| null)` | Set disk caching for subsequent loads; omitted or `null` restores host defaults. |
| `unregisterComponent(name: string)` | Remove a factory for future mounts; mounted components retain their owner. |
| `removeHandler(name: string)` | Remove a handler registered with `handle`. |
| `readonly isRunning: boolean` | Whether the current module's engine is running. |
| `readonly isDisposed: boolean` | Whether the surface has been permanently released. |
| `readonly isLoading: boolean` | Whether source is being read or awaiting an attached, positive-size slot; evaluation starts asynchronously afterward. |
| `readonly lastError: string` | The latest load, engine or native mount error; cleared by a newly accepted source. |

## Talking to the app

A surface and the app it runs talk over a channel of their own ([`nsh/host`](../modules/nsh-host.md)): launch parameters the app reads at startup, events from the host app, and calls and posts from the app, all as JSON values. Neither a view in the app nor a native component is needed.

```ts
// Before loading: what the app should open to (`host.params` in TSX).
surface.setParams({ conversation: 'conv-42' });
surface.loadResource('bundle.js');

// The app's `await host.call("user.profile", { id })`, handled on the UI thread.
surface.handle('user.profile', (data, reply) => {
  const req = data as Record<string, Object>;
  api.fetchProfile(req['id'] as string)
    .then((profile) => reply.resolve({ id: req['id'], name: profile.name }))
    .catch((err: Error) => reply.reject(err.message));
});

// The app's `host.post("analytics", …)`: nothing to answer.
surface.handle('analytics', (data, _reply) => {
  analytics.log(data);
});

// Later: received by `host.on("conversation.open", …)` in the app.
surface.send('conversation.open', { id: tappedConversation });
```

Handlers run on the UI thread; calls wait in the app for the reply without blocking either side. Events of a name the app has not subscribed to yet are kept until it does, and events sent while no app is running are kept for the next app.

## Theme overrides

An app embedded in several branded host apps takes on each host's brand: the host gives the tokens its brand changes as a theme file in the format of `nsh.json`'s `"theme"` (shadcn/ui's `cssVars`, values as CSS writes them). They override the app's tokens of the same names while the app's other tokens stay as they are. Give a token's dark-mode value under `dark` (a token set only under `light` or `theme` has that color in both appearances):

```ts
surface.setThemeOverrides(JSON.stringify({
  cssVars: {
    theme: { radius: '0.75rem' },
    light: { primary: '#e11d48', 'primary-foreground': '#ffffff' },
    dark: { primary: '#fb7185', 'primary-foreground': '#1c1917' },
  },
}));
surface.loadResource('bundle.js');
```

Call it before or after loading: classes like `bg-primary` and `rounded-(--radius)` restyle at once, and every app loaded later starts with the overrides in its first frame. `setThemeOverrides(null)` restores the app's own theme.

## Links the app opens

URLs the app opens ([`openUrl`](../modules/nsh-device.md#openurl) and links in a `TextView` with `links="open"`) go to the surface's `setOpenUrlHandler` callback first, on the UI thread: return `true` when the host app handles the URL itself (for example with its own router), or `false` to let the system open it. Without a handler, every URL is opened by the system. Only URLs allowed by the app's `openUrl` capability reach the handler.

```ts
surface.setOpenUrlHandler((url: string): boolean => {
  if (url.startsWith('lb://') || url.startsWith('https://longbridge.com/')) {
    router.open(url);
    return true;
  }
  return false;
});
```

## Limits

Pass VM and node limits in `NshOptions` when creating the surface:

```ts
const surface = new NshSurface(this.getUIContext(), {
  memoryBytes: 128 * 1024 * 1024, // 128 MiB heap
  stackBytes: 4 * 1024 * 1024,    // 4 MiB stack
  loadBudgetMs: 10_000,           // 10 s to load
  callBudgetMs: 1_000,            // 1 s per call
  maxNodes: 50_000,               // 50,000 nodes
});
```

The defaults are: 256 MiB heap, 4 MiB stack, 10 s to load, 2 s per call, 200,000 nodes. See [Sandbox & Capabilities](./sandbox-and-capabilities.md) for what happens when each limit is reached.

## `NshOptions`

Every field is optional.

| Field | Default and purpose |
|---|---|
| `assetDirectory?: string` | Directory containing the build's `assets/`; defaults to host rawfiles. |
| `storageDirectory?: string` | Absolute storage directory for subsequent loads; defaults to the host's `filesDir`. |
| `cacheOptions?: NshCacheOptions` | Disk caching under host `cacheDir`, with a 256 MiB image cache per app by default. |
| `managesStatusBar?: boolean` | `true`; explicit status-bar settings or fullscreen video take control of window bars. Passive embedded surfaces preserve the host's settings. |
| `environment?: NshEnvironment` | Initial system-environment overrides. |
| `capabilities?: string` | Host grant using the `nsh.json` capabilities JSON format. |
| `memoryBytes?: number` | 256 MiB; at least 1 MiB. |
| `stackBytes?: number` | 4 MiB; from 64 KiB through 4 MiB. |
| `maxNodes?: number` | 200,000; at least one. |
| `maxOpValues?: number` | 8,000,000 queued operation values; at least one. |
| `maxCanvasOps?: number` | 1,000,000 operations per canvas drawing; at least one. |
| `loadBudgetMs?: number` | 10,000 ms; at least one. |
| `callBudgetMs?: number` | 2,000 ms; at least one. |

`NshCacheOptions` has `directory?: string` (absolute), `disabled?: boolean` (default `false`) and
`imagesMaxMegabytes?: number` (`0` disables image caching). Set either `disabled: true` or `directory`.
`NshEnvironment` has `pixelRatio`, `fontScale`, `colorScheme`, `locale`, `rtl`, `reduceMotion`,
`screenReader`, `idiom`, `lifecycle`, `apiLevel`, `safeTop`, `safeRight`, `safeBottom`, `safeLeft`,
`keyboardHeight` and `keyboardDuration`; lengths are vp and durations are ms.

## Capabilities

Pass the host's capability grant in `NshOptions.capabilities` (a JSON string in the same format as `capabilities` in `nsh.json`):

```ts
const surface = new NshSurface(this.getUIContext(), {
  capabilities: JSON.stringify({
    network: { hosts: ['api.example.com'] },
    webview: { hosts: ['help.example.com'] },
    host: true,
  }),
});
surface.loadResource('bundle.js');
```

A request must be allowed by both the app's `nsh.json` (shipped next to the bundle as `capabilities.json` or inside a signed package) and this grant. See [Sandbox & Capabilities](./sandbox-and-capabilities.md).

## Signed packages

Verify a downloaded package off the UI thread with `NshTrust`, then run it on the surface:

```ts
import { NshTrust } from '@nsh/ohos';

const trust = await NshTrust.open(`${context.filesDir}/nsh-trust`, [rootPublicKey]);
const pkg = await trust.openPackage(downloadedFilePath);
surface.runPackage(pkg);
```

| API | Description |
|---|---|
| `NshTrust.open(stateDirectory: string, rootPublicKeys: string[]): Promise<NshTrust>` | Open or create a trust store. Its state (latest revocation list, highest accepted build per app) persists in that directory, and package assets are extracted under it. |
| `addRoot(publicKeyBase64: string): Promise<void>` | Trust a root public key (base64, the contents of a `.pub` file from `nsh keygen`). |
| `updateRevocations(revocationsJson: string): Promise<string>` | Apply a signed revocation list from `nsh revoke` (must be signed by a trusted root and newer than the store's current list). |
| `openPackage(packagePath: string): Promise<NshPackage>` | Verify the package on a worker thread, extract its assets and accept it (older builds of that app are refused from then on). |
| `openResource(name: string, uiContext?: UIContext): Promise<NshPackage>` | Verify and open a signed `.nshpkg` package bundled in `resources/rawfile/`. |
| `NshPackage` | `info` (`id`, `name`, `version`, and decimal-string `build`), `isConsumed`. |

See [Signed Packages](./signed-packages.md) for how keys, certificates and revocations work.

`openResource` uses the sole active NSH window session when `uiContext` is omitted. Pass a `UIContext`
before creating a surface, or when several windows are active.

## Module exports

Import host APIs from `@nsh/ohos`.

| Export | Purpose |
|---|---|
| `getCapabilities(): NshCapabilities` | Read the actual runtime API level and compiled enhancements. |
| `NshCapabilities` | `apiLevel`, `minimumApiLevel`, `compiledApi23`, `compiledApi26`, `api23`, `api26`. |
| `initLogging(): void` | Route Rust logs to HiLog. |
| `onBackPressIntercept(uiContext: UIContext): boolean` | Route back to the specified window session; return whether it was consumed. |
| `onNotificationWant(want: Want): void` | Forward notification presses from Ability `onCreate` and `onNewWant`, including cold starts. |
| `NshHostReply` | `resolve(value)`, `reject(message)` and `isPost` for host-channel replies. |
| `NshNativeProperty` | `name`, `value` and `removed` in native-component property batches. |

The package also exports `NshSurface`, `NshTrust`, `NshPackage`, `NshPackageInfo`, `NshOptions`,
`NshEnvironment` and `NshCacheOptions`.

## Development

With the `--dev` SDK (`cargo xtask ohos --dev`):

```ts
surface.connectDevServer('ws://127.0.0.1:8790/ws');
```

Run `nsh dev App.tsx` on your computer. For an emulator or a device over USB, forward the port with `hdc` so `127.0.0.1` on the device reaches the server:

```sh
hdc rport tcp:8790 tcp:8790
```

Otherwise use the LAN address `nsh dev` prints.

## Ability setup

- **Standard `UIAbility`.** `@nsh/ohos` attaches automatically to the active `UIContext`, `UIAbilityContext` and window lifecycle when a surface is created—your `EntryAbility` stays a standard `UIAbility` (`extends UIAbility`) with no custom base class required. The package attaches its WebView, input, accessibility and native-component fragments to each surface automatically.
- **Back button.** System back targets the active `NshSurface` (dialogs and modals first, then `BackHandler` listeners, then navigation stacks) before falling back to the Ability. When showing multiple surfaces (for example in tabs), call `surface.activate()` on the active surface.
- **Keyboard and safe area.** `@nsh/ohos` observes system bar, cutout and keyboard avoid areas automatically and reports only the portion overlapping each `NshSurface`. `Input` uses the system `TextInput` / `TextArea` and scrolls the focused field clear of the keyboard.
- **Lifecycle.** Detaching a `ContentSlot` keeps the surface and its running VM alive; re-mounting it reattaches the native tree. Call `surface.stop()` to clear the running app while keeping the surface, or `surface.dispose()` when the page or component permanently disappears.

Native inputs report text paste, selection, caret bounds and IME composition. Read-only inputs
support focus, selection and copy. Multiline input refs scroll through a native scroll controller.
Input-method autocorrection follows the system keyboard's settings.

Window resizing refreshes open dialogs. Hosts can update `pixelRatio` through `setEnvironment`;
configuration changes refresh native geometry and text density. Rust embedders register additional
UI sets before loading modules with `nsh_platform_ohos::set_ui_sets(Vec<Registry>)`. Native component
names beginning with `Nsh` belong to the SDK; host factories use their own prefix.

## See also

- [Rust Reference](./rust.md)
- [Embedding in iOS apps](./ios.md)
- [Embedding in Android apps](./android.md)
- [Native Components](./native-components.md)
- [Signed Packages](./signed-packages.md)
- [Sandbox & Capabilities](./sandbox-and-capabilities.md)

## Bundle delivery and host policy

`nsh build --split` writes `app.json` with `entryChunks`, the static dependencies to preload for
`bundle.js` and `bundle.nshc`. Copy the chunk files and companions beside the entry in rawfile.
Single-file entries can omit `app.json`; older source entries discover their static imports.
Preloading accepts up to 1024 chunks and 64 MiB total, including the entry, companions and source maps.
`capabilities.json` sets the app's declaration even when `NshOptions.capabilities` supplies a host grant;
each operation must satisfy both.

`NshTrust.openPackage` and `openResource` accept packages up to 256 MiB. A synchronous `runPackage`
preflight or acceptance failure leaves the verified package available for retry. Acceptance consumes
it once, including when the surface stages the app until layout.

On OHOS, `nsh/mobile.isAvailable` is `true`, `TabBar` uses ArkUI, and `BackHandler` receives system back
presses. `process.platform` and `os.platform()` return `"ohos"`. Process launch reports
`ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`. The embedding host manages bundle and HAP updates;
`installAndRestart` reports that host responsibility through the same error code.

## System UI and media

Navigation, dialogs and page backgrounds use the app's semantic theme tokens. The back control has a vector
chevron and a button accessibility role; the title occupies the space between measured side controls.
Dialog action labels come from the app and text measurement follows the system font scale.
Explicit status-bar settings and fullscreen video manage system bars by default; `managesStatusBar: false`
keeps their ownership with the embedding host.

`<WebView ohos={{ sharedCookies: false }}>` uses ArkWeb's private cookie store, cleared before its first
navigation; cookies are shared by default. HTML file uploads open the system photo or document picker,
following single or multiple selection. Picked images are read on a worker. Images and core SVG loads use
system proxy policy and the shared cache budget. Each surface admits up to 32 concurrent fetch requests;
open response streams use independent reader threads.

API 20 exposes selected and checked states through ArkTS wrappers. `api-23` enables native accessibility
providers, state descriptions, reduced-motion observation and animated images' per-frame durations;
API 20 spreads their total duration evenly across frames. `api-26` adds named custom accessibility actions.
Popup controls can describe their purpose with an app-supplied `activate` action label. Video audio follows
the embedding host's audio policy. The host owns Ability exit; `BackHandler.exitApp()` records the request in logs.
