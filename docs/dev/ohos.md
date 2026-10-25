# OHOS backend

`nsh-platform-ohos` is Native Shell's arm64 embedding backend for API 20 and
later. Each `NshSurface` owns one engine, QuickJS VM and native tree. The shared
core owns layout, reactivity, lists, module APIs and host policy; OHOS supplies
native presentation and platform services. `Platform.OS` is `"ohos"`, with
`Platform.select({ ohos, mobile, native, default })` and `ohos:` class variants.

The [embedding guide](../../website/docs/embedding-ohos.md) describes the public
host API. This page describes module ownership, native boundaries, builds and
verification. See [native interop](ohos-native-interop.md) for attachment,
retirement and input-routing details.

## Source map

| Domain | Responsibility |
|---|---|
| [`app`](../../crates/nsh-platform-ohos/src/app/mod.rs) | NativeAbility entry/session, instance surfaces, engine queues, options, environment, capabilities, development and trust |
| [`render`](../../crates/nsh-platform-ohos/src/render/mod.rs) | C nodes, mount batches, geometry, drawing, Canvas, decoration and animation |
| [`render/composition`](../../crates/nsh-platform-ohos/src/render/composition/mod.rs) | Export plans, worker-owned EGL/GLES readback, filters and blending |
| [`text`](../../crates/nsh-platform-ohos/src/text/mod.rs) | Engine-thread typography, owned raster payloads and selectable-text geometry/state |
| [`input`](../../crates/nsh-platform-ohos/src/input/mod.rs) | System editors and keyboard/touch routes |
| [`scroll`](../../crates/nsh-platform-ohos/src/scroll/mod.rs), [`navigation`](../../crates/nsh-platform-ohos/src/navigation/mod.rs) | Native scrolling/zoom and retained stack, header, modal and transition state |
| [`accessibility`](../../crates/nsh-platform-ohos/src/accessibility/mod.rs) | Semantic values, native metadata and optional providers |
| [`interop`](../../crates/nsh-platform-ohos/src/interop/mod.rs) | Native components, SDK batches, semantic/menu/texture wrappers and pointer forwarding |
| [`media`](../../crates/nsh-platform-ohos/src/media/mod.rs), [`services`](../../crates/nsh-platform-ohos/src/services/mod.rs) | Images, WebView, Video, assets, networking and dialogs |
| [`xtask/src/ohos.rs`](../../crates/xtask/src/ohos.rs) | Native SDK, generated declarations and source/import metadata |
| [`examples/embed-ohos`](../../examples/embed-ohos/README.md) | NativeAbility host, paired HAR/HAP assembly and regression fixtures |

## Ability integration

The host extends `NativeAbility`, sets `moduleName = "nsh_platform_ohos"` and
`defaultPage = false`, and loads its own page. Its `bridgePlugins` include
resource, accessibility, clipboard, window, WebView and URL factories, plus
`...nativeShellPlugins()` imported from `@nsh/ohos`. The complete registration
is in [EntryAbility.ets](../../examples/embed-ohos/entry/src/main/ets/entryability/EntryAbility.ets).
Rust registers the matching facades and implements Ability's lifecycle/bridge
contract with NSH's native-node renderer. Hosts supply no Rust entry adapter.

| Owner | Native state |
|---|---|
| Ability core | Session/lifecycle, shared UIContext, `ContentRoot` and native attachment ownership |
| Ability WebView | ArkWeb controller/builders, navigation, protocols and JS proxy callbacks |
| NSH `@nsh/ohos` HAR | System TextInput/TextArea, selectable-text menus, semantic SDK attributes and texture-producing BuilderNodes |
| NSH Rust | Controlled system editing, semantics, composition/input policy, media policy and AVPlayer controls |
| Binding crates | Generated SDK declarations, checked C operations, native owners/borrows and API feature gates |

Each embedded view creates a surface and puts its retained NodeContent in a
standard ContentSlot. The page also hosts Ability's DefaultXComponent, which
supplies the shared UIContext root. Its module renderer creates no implicit
NSH app. A minimal page after the registration above is:

```ts
import { DefaultXComponent } from "@ohos-rs/ability";
import { NshSurface } from "libnsh_platform_ohos.so";

@Entry
@Component
struct Index {
  private surface: NshSurface = new NshSurface();

  aboutToAppear(): void {
    this.surface.loadResource("bundle.js")
      .catch((error: Error) => console.error(error.message));
  }

  aboutToDisappear(): void {
    this.surface.dispose();
  }

  build() {
    Stack() {
      DefaultXComponent({ moduleName: "nsh_platform_ohos" })
        .width("100%").height("100%");
      ContentSlot(this.surface.content).width("100%").height("100%");
    }
  }
}
```

ArkUI containers own host layout, order and visibility. The native viewport
measures physical pixels, converts them to vp using session scale, and posts
size/attachment observations. Loading may precede attachment; the engine starts
when source and a positive attached viewport are ready. Resizing preserves VM
state. Detaching a ContentSlot preserves the surface; `stop()` closes its module
and pending loads while retaining options/factories; `dispose()` permanently
closes it. Source maps belong to each load.

`loadResource` reads trusted HAP source or QuickJS bytecode off the UI thread:
at most 64 MiB per resource and 32 concurrent reads per session. It recognizes
the linked bytecode header before UTF-8 decoding. With no `NshOptions.capabilities`
override, a companion `capabilities.json` in the resource's directory supplies
the module declaration; invalid capability JSON rejects that load. A generation rejects stale
results after replacement, stop, disposal or session teardown. Read failure
preserves the previous module. Accepting source replaces the engine and clears
`lastError`; the promise resolves before JS evaluation. Evaluation failure does
not restore the preceding app. Assets/fonts resolve under `rawfile/assets/` or
the surface's checked `assetDirectory`.

### Environment observations

Ability supplies configuration/lifecycle events, font scale and screen-reader
observations. The NSH HAR supplies the SDK device type before initial engine
creation; Rust maps it to the shared idiom. System-bar, cutout and navigation-
indicator avoid areas are intersected with each surface's bounds and merged by
the maximum exclusion at each edge. Overlapping areas are consumed once;
keyboard overlap remains a separate observation. Optional visual types that a
host cannot query preserve the available system/keyboard observations.
API-23 builds also observe reduced motion.
`NshOptions.environment` and `setEnvironment` provide per-instance overrides;
unobserved fields use shared defaults. `getCapabilities()` reports the actual
system API independently of those overrides. Foreground loss and detachment
pause frame delivery while preserving the VM and timers. System back targets
the activated live surface; hosts may call `activate()` when switching views.

## Threads and ownership

One `Services` session spans NativeAbility Create/Destroy. N-API entry points
resolve a thread-local weak session reference; surfaces and engines remain
independently owned. Each engine generation has a distinct dispatcher, so late
batches, service replies and component handles cannot reach its successor.
Session destruction invalidates routes and closes outstanding surfaces.

The engine submits one `MountBatch` through a N-API threadsafe function to the
ArkUI UI thread. Native nodes and SDK owners stay on that thread. Per-surface
NativeVSync callbacks use the same queue; idle surfaces request no frames.
Custom-node measure/layout operations run inside their respective native
callbacks. Typography remains on the engine thread because the binding's native
paragraphs are thread-confined. `FontCollection::shared()` supports multiple
paragraph builders. The UI consumes each RGBA transfer buffer once, retaining
only its bitmap and immutable layout/selection geometry. Position-only text
updates reuse that bitmap. Text rasters share a 96 MiB reservation, including a
transient transfer copy and admitted selection geometry. Long paragraphs reduce
raster scale to fit 12 million pixels and a 32,768-pixel side while preserving
the full logical extent. Native glyphs therefore retain bitmap resolution during
zoom; vector text needs a thread-safe display-list/paragraph binding. Budget or
selection-cache exhaustion skips the affected resource or interaction, preserving
the engine. Canvas retains its 16 MiB per-node and surface reservations.

Blocking fetch, image and WebView asset work use separate shared executors with
six, four and two workers, respectively, and 256 queued jobs per lane. Admission
is nonblocking; a full queue completes that request with an error. Cancellation
is checked again when queued work begins. Native SDK objects stay on their
owning threads.

Node callbacks retain weak routes. Retirement disables input, severs native
edges and defers disposal past active callbacks. `ContentRoot` retains its
NodeContent through teardown; `NativeAttachment` retains both native and SDK
owners until detachment succeeds. Cleanup failure retains the complete owner
for retry. A surface disposed inside a widget callback closes on a later UI
turn. Synchronous widget callbacks reject module mutation such as source
replacement; component events and intrinsic-size reports remain available.

SDK-only changes are batched on UI turns. Ordinary inputs retain one system
TextInput/TextArea BuilderNode, preserving full strings, controlled edit counts,
preview, selection, autofocus, capitalization and autofill. The inner SDK editor
clears its theme radius inside the core content box; core geometry supplies
padding, and the styled NSH container owns border and clipping. Secure inputs
use the system password TextInput, including when `multiline` is requested:
API-20 TextArea has no password mode. All focus, selection, IME and accessibility
behavior belongs to that system editor. Soft and hardware editing share one
SDK insertion/deletion callback path for character and Backspace key events;
window key/shortcut handlers emit their separate events and avoid duplicate
KeyPress delivery. Native editor, WebView, Video and Scroll accessibility trees
remain with their native owners. Optional providers represent eligible semantic
leaves and restore suppressed metadata on retirement.

Scroll and List keep their native Scroll inside a stable container. A Refresh
node is created only for `onRefresh` or `refreshing=true`; refresh events are
registered and delivered only with a callback. Removing both settings disposes
the Refresh node while preserving native Scroll/content. Their styled native
owner applies rounded clipping even without an explicit `overflow: hidden`, matching the
software composition clip rule.

Scene requirements are recomputed from the live visible tree. Removing the last
plus-darker/complex-backdrop trigger clears scene frames and retires texture
exports, restoring native rendering and scroll chrome. Foreground-filter exports
remain only on nodes that still need them. GPU creation posts an asynchronous
ready result; the UI never waits for EGL/GLES initialization. Texture input uses
window-relative positions. CSS blur and drop-shadow radii convert to physical
Gaussian sigma as `radius * scale * 0.5`.

The compositor worker owns EGL/GLES through community `glow` and static
`khronos-egl`; bindings expose NativeImage/window ownership and primitive SDK
calls. It saves/restores client API and current context, checks dimensions and
allocations, and retains the complete GPU owner if safe cleanup fails. NSH owns
its limits: 128 exports, four megapixels per source/filtered image, eight
megapixels of source frames per surface and bounded intermediate reservations.
These are logical pixel budgets, not a native RSS guarantee. NativeImage listener
removal does not drain callbacks: nonreused integer tokens route to weak wakes,
and retirement removes the route before releasing its owner.

Images cache raw `Arc<[u8]>` encoded bytes. UI-owned PixelMaps/DrawableDescriptors
present them directly, and Canvas decodes the bytes without base64 conversion.
WebView start/end events carry navigation state; the Ability plugin filters
subresource errors and applies native scrolling settings. Messages dispatch on
both `window` and `document`.

Video owns a C XComponent/window and C controls on the UI thread; AVPlayer and
source IO live on a media worker. Unrestricted HTTP(S) policies use native URL
streaming, including HLS/live sources. Restricted policies check every redirect
with Rust and stream into a private file using the current system proxy, without
a 64 MiB memory buffer or source ceiling. Local `file://` URLs require an
unrestricted host policy and open the original file. Filesystem asset stores
provide direct files; HAP resources stream in bounded chunks into the fallback
file because the pinned raw-resource binding does not expose descriptor offsets.
Memory-backed package/dev asset stores use their reader fallback. The engine's
`Assets::open_file` and `Assets::reader` extension points preserve those store
contracts. Native API-20 rates accept positive values, clamped to 0.125–4; zero
pauses. A manual Play/Pause choice survives unrelated props. Controls toggle on
tap and hide after three seconds of playback, with a VSync deadline independent
of progress-event frequency. Each source/context generation gets a
separate player and immutable callback identity. Callbacks copy borrowed
metadata before queueing. Explicit callback clearing precedes synchronous
player release, which runs off the UI thread. A successful worker acknowledgement
permits UI owner disposal even after engine stop; failure retains the complete
player/window/userdata group. See [retirement](ohos-native-interop.md#retirement-and-failures).

Touchscreen input uses owned native event clones; primary mouse-generated touch
uses a callback-scoped SDK event. Coordinates are restored after synchronous
posting. Native callbacks and SDK objects never become worker-safe payloads.
See [composition and input](ohos-native-interop.md#composition-and-input).

Explicit operations return N-API errors. Structural/native ownership failures
update `lastError` and stop the affected generation; mount batches are not atomic.
Unsupported commands and per-node budget failures log a warning and preserve
the engine and remaining batch. Toast uses the log sink; ExitApp reports that
Ability lifetime belongs to the embedding host. Oversized custom action lists
use their first 16 entries, and label-pool exhaustion retains static action names.

Back handling gives dialogs and modals priority, then root Rust/JS back listeners,
then stack navigation. `Command::Back` performs only the navigation fallback.
Headers use a 52-vp bar plus their actual top safe-area overlap; title slots are
identified explicitly. Modal roots receive their safe insets, and PageSheet and
FormSheet align at the bottom with rounded top corners. RTL lists reflect their
native scroll viewport and cells while retaining core logical offsets. Exit
animations retain active subtree resources through completion; an exiting
ancestor owns the animation of descendants removed in the same batch.

## Locked native dependencies

The root Cargo manifest declares one exact Ability revision and one binding
revision for all interacting native packages. Ability's ArkUI types and NSH's
direct binding types share that source. NSH uses binding crates for OHOS APIs;
missing wrappers belong in the binding repository. SDK declarations are generated
there. EGL/GLES policy, worker scheduling and application budgets belong in NSH.

`cargo xtask ohos` resolves the locked graph and records source revisions,
features, minimum API and library/declaration hashes in `build-metadata.json`.
The example stages HAR sources from the resolved Cargo checkouts, records
`paired-sources.json`, and rejects source/hash mismatches. The packaged native
library is preserved byte-for-byte and checked against SDK metadata.

### Binding development dependency

Local binding development may explicitly enable `NSH_ALLOW_PATH_DEPENDENCIES=1`.
Distributable SDK acceptance uses the pinned Git graph and matching Rust/HAR
sources. Resolve dependencies from the workspace manifest and lockfile rather
than substituting a separately installed HAR.

## API compatibility

| Build | Minimum system API | Enhancements |
|---|---|---|
| Default / `dev` | 20 | Baseline native UI and services |
| `api-23` | 23 | Native accessibility providers, state descriptions and reduced-motion observation |
| `api-26` | 26 | Includes API-23 support and named accessibility actions |

`MIN_API`, capabilities, SDK metadata and the example manifest agree. Startup
rejects lower system versions. The builder audits linked `OH_*` imports against
official headers, rejecting unknown symbols and imports newer than the selected
minimum. Runtime capability checks do not lower a linked symbol's requirement.

## Build and run

Install `ohrs` and the `aarch64-unknown-linux-ohos` Rust target. Set
`OHOS_NDK_HOME` to the SDK's `openharmony` directory containing `native`.

```sh
cargo xtask ohos                         # target/ohos/debug
cargo xtask ohos --release               # target/ohos/release
cargo xtask ohos --dev                   # target/ohos/debug-dev
cargo xtask ohos --features api-23        # target/ohos/debug-api-23
cargo xtask ohos --features api-26        # target/ohos/debug-api-26
cargo build -p nsh-cli
export DEVECO_CONTENTS=/path/to/DevEco-Studio.app/Contents
examples/embed-ohos/build.sh
# Select the corresponding SDK for an enhanced HAP:
NSH_PROFILE=debug-api-23 examples/embed-ohos/build.sh
```

The SDK contains the arm64 library, generated `index.d.ts`, build metadata and
native-import audit. The example copies the matching SDK, bundles the selected
app with the Rust CLI, and assembles its paired HAR/HAP through OHPM/Hvigor.
Sign the HAP with the host's development profile and select the intended target
explicitly when installing. See the [sample README](../../examples/embed-ohos/README.md)
for fixtures and deployment. CLI `run/package` and reusable `dist` integration
remain tooling work.

## Implementation status

The backend mounts View, Text, Input, Scroll, Image, Stack, Screen, HeaderItem,
Canvas, WebView, Video, native components and Modal. This table records current
contracts and remaining checks; implementation and compilation do not establish
runtime acceptance.

| Area | Current implementation | Remaining checks / work |
|---|---|---|
| Embedding/components | Independent retained surfaces; cancellable loads; per-instance factories, props/commands/events and intrinsic sizes; stop/dispose/reentry guards | Multi-window/hidden-container scheduling, stale handles and injected native failures |
| Appearance/composition | Borders/gradients, layered/inset shadows, transforms, ordered foreground/backdrop filters, live subtree alpha shadows, plus-darker and independent modal planes | Native-control pixels/input through nested transforms/clips/modals; GPU driver, frame and memory profiling |
| Text/selection | Native typography, UTF-16 spans/metrics/features, owned rasters, inline geometry, Unicode selection/handles, Copy/Select All and edge scrolling | RTL, ellipsis/attachments, transformed selection, handle crossing and screen-reader matrix |
| Input/gestures | System editors, including native password fields; keyboard repeat/modifiers/cancel; focus, preview, controlled UTF-16 selection/maxLength; touch/hover/pan/pinch/rotation/responder routing | Full IME/autofill/lifecycle, secure-field reader and multi-contact arbitration matrix |
| Scroll/navigation | Native scroll/refresh, snapping/insets/sticky cells, focal zoom; retained stack, header/modal, transitions and mounted interactive-back decisions | RTL, gesture interruption, simultaneous route changes and reduced-motion changes |
| Accessibility | Labels/roles/states/grouping, live regions and standard actions; optional range providers and API-26 actions | Renewed QEMU reader coverage, direction queries, audible announcement ordering, editable DOM and media controls |
| Media | Image IO coalescing/cancellation/cache; retained Ability WebView with navigation/protocol/JS/runtime options; native AVPlayer events, controls, seek/rate/resize and checked FD sources | ArkWeb runtime options/history, AVPlayer playback/release/cancellation, animated images and cache churn on target |
| Services/delivery | Ordered clipboard replies/presence, dialogs/status bar/URL; API-20 HTTP/TLS streaming with per-request proxy snapshots and checked redirects; signed packages and persistent trust/rollback/revocation | Clipboard permission/secure paste, proxy/HTTPS/backpressure and package/assets across restarts |
| Development | Per-surface client/HMR/reconnect, source maps, logs, lazy chunks and error overlay with asynchronous Copy/Dismiss | Native overlay interaction, reconnect/disposal and recovery matrix |
| Deferred by host decision | Independent per-editor autocorrection; automatic system/WebView cookie sharing with fetch/images | Preserve the existing shared editing/request contracts until separately designed |

## Verification

Cross-compile the actual OHOS code: a host build deliberately sees an empty
platform crate. Keep shared tests, portable production-algorithm tests, native
compilation/linking, SDK compilation and runtime acceptance distinct.

```sh
cargo test --workspace
cargo test -p nsh-platform-ohos --test ohos
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets
ohrs cargo -- clippy -p nsh-platform-ohos --lib -- -D warnings
ohrs cargo -- clippy -p nsh-platform-ohos --lib --features dev -- -D warnings
ohrs cargo -- clippy -p nsh-platform-ohos --lib --features api-23 -- -D warnings
ohrs cargo -- clippy -p nsh-platform-ohos --lib --features api-26 -- -D warnings
```

The 2026-10-23 audit fixes pass all eight commands above without warnings or
errors: workspace tests (including 62 portable OHOS tests), formatting,
workspace Clippy, and native Clippy for default, dev, API-23 and API-26.
All 26 ArkTS owner/algorithm tests pass. The API-20 embedding SDK also links
and passes its locked native-import audit. These checks preserve the existing
NativeAbility/DefaultXComponent/plugin host and minimum-API feature gates;
this audit has not rerun device IME, AVPlayer or GPU visual acceptance.

Audit regressions cover large text raster admission, graphemes with many visual
boxes, CSS sigma/bounds, scene requirements after style replacement, bounded IO
admission, direct-file lifetime, streamed video sources larger than 64 MiB,
control visibility, and editor key delivery through the shared edit path. Shared engine
tests exercise Rust back listeners with both Android and OHOS environments.

The earlier adaptation's host checks included 760 workspace tests and 59 portable OHOS
production-algorithm tests executed on macOS. Five Refresh ownership regressions
run the production controller with native effects doubled and enforce the
binding's strong-`Rc` disposal guard, including detach/disposal failure, retry
and re-enabling. Native Rust checks compile and link ARM64 OHOS code; they do
not execute its Rust tests on a device. The Scheme A shared-`Rc` teardown update
passed workspace tests/Clippy,
native Clippy for default, `api-23`, `api-26` and `dev`, and the locked API-20
SDK build/import audit with Ability `b6550250` and bindings `43aec6c1`.
The frozen API-20/dev20/API-23/API-26 baseline profiles passed native Clippy,
linking, minimum-API/import audits and paired HAR/HAP hash checks. The Scroll/Refresh and
inner-editor corner follow-up also passed a separate API-20 native Clippy/link/
import check and paired HAR/HAP verification; its runtime rerun remains pending.
The follow-up also passed 19 host Refresh lifecycle cases and 25 production
SDK-owner cases with platform effects doubled. These verify ownership and error
paths, rather than ArkWeb, IME, AVPlayer or GPU runtime acceptance.

On the MateBook simulator, the earlier EGL-linked source passed the complete
surfaces suite: independent engines, host events, layout exchange, ten slot
remounts, stop/resource/cancellation isolation and disposal/recreation. The
updated WebView lifecycle build reached both loaded controllers and one load
each; its interactive suite was interrupted. Subsequent receiver-transfer and
Video cache file fixes have not received runtime acceptance. UI regression is paused at the
host's request; secure input, composition, video and WebView must be rerun against
the final paired source.

Accessibility acceptance belongs exclusively to the dedicated OpenHarmony QEMU
with its system AccessibilityManagerService, app-local test extension and
physical input; see
[QEMU instructions](../../examples/embed-ohos/README.md#accessibility-acceptance-on-qemu).
System events alone do not establish audible speech or its ordering. Actual
API-20/23 system execution and native memory/frame baselines remain separate
checks. Historical fixture runs are not acceptance of the current source.
