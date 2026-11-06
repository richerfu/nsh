# OHOS backend audit and fix list

Read-only audit of `crates/nsh-platform-ohos` on 2026-11-06, at `667f8dd5` (after the GLES shader
unification in `770ae0a7`). The audit compared the backend with `nsh-core`'s contracts, the other
platform backends, and `docs/dev/ohos.md` / `website/docs/ohos.md`.

Nothing was changed by the audit. Each item below is executable: scope, evidence, the change, its
acceptance criteria and the check that proves it. Line numbers are for the audit baseline and drift as
the code changes; the symbols named beside them are the stable reference.

Revised after the render/mount pass: P1-11–P1-13, P2-14 (draw callbacks), P2-15–P2-18 and
P3-8–P3-10. Revised again for the binding dependency: P0-6.

Items marked **device-verify** need a run on an OHOS device (the reference device is ALN-AL00,
API 26). Everything else was read in the code or reproduced on this machine.

> Working-tree note: `.cargo/config.toml` and `Cargo.lock` patch every `ohos-*` crate to a local
> binding checkout, and `examples/embed-ohos/build-profile.json5` carries signing material. Keep them
> out of commits. The checkout's only functional addition over released `ohos-arkui-binding 0.6.0` is
> `ArkUINode::snapshot()` (P0-6); the branch must build against released crates before it can merge.

## Summary

| ID | Priority | Item | Size |
|---|---|---|---|
| P0-1 | P0 | Split bundles lose static chunks (app fails to start) | M |
| P0-2 | P0 | `capabilities.json` is ignored when the host supplies a grant | S |
| P0-3 | P0 | Shader `nsh.size` / `overflow` are derived from the capture target | M |
| P0-4 | P0 | Shader layers are dropped when the CSS scene compositor is active | M |
| P0-5 | P0 | No CI builds, lints or tests the OHOS backend | S |
| P0-6 | P0 | The branch needs an unreleased `ohos-arkui-binding` wrapper | S |
| P1-1 | P1 | `nsh/mobile` `TabBar` is unreachable; the native TabBar is dead code | S |
| P1-2 | P1 | Status bar and fullscreen-video bars are silent no-ops by default | M |
| P1-3 | P1 | Input events missing: paste, caret, composition, autoCorrect, readonly | M |
| P1-4 | P1 | Navigation snapshots and teardown joins block the ArkUI UI thread | M |
| P1-5 | P1 | Frame pacing hardcodes 120 Hz; resume can stall animation | S |
| P1-6 | P1 | One GLSL compile per shader view; per-fill XComponents are uncapped | M |
| P1-7 | P1 | Action sheets are presented as centered alerts | S |
| P1-8 | P1 | Failed system commands leave promises unsettled; duplicate openUrl result | S |
| P1-9 | P1 | `os.platform()` / `process.platform` report `"linux"` on OHOS | S |
| P1-10 | P1 | `sample_backdrop` is always transparent | M |
| P1-11 | P1 | Layout-animation scoping is ignored; only the first animation per batch is used | S |
| P1-12 | P1 | Text color transitions are not implemented | S |
| P1-13 | P1 | Blur/shadow unit conversions disagree with ArkUI's docs (device-verify) | S |
| P2-1 | P2 | Navigation header, alerts and video chrome are hand-drawn and inaccessible | L |
| P2-2 | P2 | Rust ↔ ArkTS system commands are stringly typed, without stable codes | M |
| P2-3 | P2 | Picker copying and host glue run on the UI thread; state is duplicated | M |
| P2-4 | P2 | Process-global IO/network pools starve other surfaces | M |
| P2-5 | P2 | Duplicated network transport and image cache | M |
| P2-6 | P2 | Full close does not drain the shader retirement handshake | M |
| P2-7 | P2 | Dialogs use the pre-resize window; pixel ratio never updates | S |
| P2-8 | P2 | No `set_ui_sets`; reserved component names are not refused | S |
| P2-9 | P2 | Accessibility provider only with `api-23`; `has_popup` gap | L |
| P2-10 | P2 | Public docs claim APIs and behavior the backend does not have | S |
| P2-11 | P2 | 4096-pixel cap rejects large targets instead of downscaling | S |
| P2-12 | P2 | Shader layer races: `from`, `active`, progress | M |
| P2-13 | P2 | EGL config requests ES2 while creating an ES3 context (device-verify) | S |
| P2-14 | P2 | Panics can cross the main TSFN callbacks; GL error-path leaks | S |
| P2-15 | P2 | NodeId-keyed state never shrinks; per-frame full scans | M |
| P2-16 | P2 | `Stack`, `Image` and `Input` children are not clipped | S |
| P2-17 | P2 | Inset box-shadows with zero blur are dropped | S |
| P2-18 | P2 | Compositor walks the tree every UI frame; scenes rebuild every batch | M |
| P3-1 | P3 | Animated image frame durations need `api-23` | S |
| P3-2 | P3 | Signed-package/resource limits disagree; packages are consumed too early | S |
| P3-3 | P3 | Notification listeners accumulate; `exitApp` is a no-op | S |
| P3-4 | P3 | Default strings and hardcoded colors/radii in NSH chrome | S |
| P3-5 | P3 | `process.run` has no OHOS sandbox gate (device-verify) | S |
| P3-6 | P3 | Host-side tests cover none of the compositor/mounter state machines | M |
| P3-7 | P3 | Stale validation numbers in the OHOS docs | S |
| P3-8 | P3 | `pagePosition` ignores transforms and pins (device-verify) | S |
| P3-9 | P3 | Stricter fatal mount policy than the other backends | S |
| P3-10 | P3 | Mount parity leftovers: `InputAccessory`, `scroll_to`, large titles | M |

---

## P0 — fix before the next device build

### P0-1 — Split bundles lose static chunks

- **Area:** `crates/nsh-platform-ohos/src/app/chunks.rs`, `crates/nsh-cli/src/build.rs`,
  `crates/nsh-cli/src/web.rs`.
- **Status:** Reproduced on this machine.
- **Problem:** `nsh build --split` writes a minified entry whose static chunk imports sit on one
  line. `static_chunk_imports` (`chunks.rs:107-135`) is line-based, stops at the first non-import
  line, and `extract_relative_chunk_specifier` (`chunks.rs:137-150`) uses `rfind("./")`, so it
  returns only the **last** `./x.js` on the line. `nsh build` writes `entryChunks` only for web
  (`nsh-cli/src/web.rs:247`), so native `app.json` carries none and the scanner is the only source.
  Loading `bundle.nshc` (bytecode) skips the scanner entirely and preloads nothing.
- **Reproduction:** an app whose entry statically imports two modules that are also imported by
  dynamic chunks:
  ```js
  import { a } from "./a";
  import { b } from "./b";
  // dynamic import("./Screen1") and import("./Screen2") use a and b
  ```
  `nsh build App.ts --release --split` produces
  `import{...}from"./a.js";import{...}from"./b.js";...` on one line. Emulating the scanner on that
  line returns `b.js` only. `a.js` never enters `Bundle::chunks`, and QuickJS resolves static imports
  before evaluating the entry (`nsh-engine/src/script.rs:155-160`,
  `nsh-js/src/modules.rs:1002`), so the app fails to load.
- **Change:**
  - Write the bundler's `entry_imports` into `app.json` (`entryChunks`) in `nsh build`, for every
    target, next to `id`.
  - Delete `static_chunk_imports` / `extract_relative_chunk_specifier`; seed from the manifest only.
  - If a bundle has no manifest (old artifacts), fail the load with a clear error instead of
    guessing.
- **Acceptance:**
  - [ ] `nsh build --release --split` writes `app.json` with the entry's transitive static chunk ids.
  - [ ] A bundle with two shared chunks loads from both `bundle.js` and `bundle.nshc`.
  - [ ] The OHOS loader no longer parses JavaScript.
- **Verify:** add a CLI test next to the split-bundle tests in `nsh-bundler/src/bundle.rs` (or
  `nsh-cli`) that builds the two-shared-chunk app and asserts `entryChunks == ["a.js", "b.js"]`;
  run the bundle through an engine test that evaluates it. Run `cargo test -p nsh-cli -p
  nsh-bundler` and, on device, load the artifact from rawfile.

### P0-2 — `capabilities.json` is ignored when the host supplies a grant

- **Area:** `crates/nsh-platform-ohos/src/app/surface.rs`.
- **Status:** Confirmed.
- **Problem:** `load_resource` computes `has_override` from `policy.granted.is_some()`
  (`surface.rs:914`) and reads the companion `capabilities.json` only when `!has_override`
  (`surface.rs:970-983`). With no `declared`, `Policy::access()` treats the app as declaring
  everything (`nsh-engine/src/lib.rs:252-258`), so a rawfile bundle's own narrower declaration is
  not enforced; only the host grant applies. This contradicts `docs/dev/sandbox.md` ("a request
  must be allowed by both") and `website/docs/ohos.md`. Signed packages are unaffected.
- **Change:** always read the companion declaration, then intersect it with the host grant in
  `Policy` (declared stays as the app wrote it; `granted` as the host set it).
- **Acceptance:**
  - [ ] A bundle declaring `network.hosts: ["a.example"]`, loaded with a host grant of
    `["a.example", "b.example"]`, cannot reach `b.example`.
  - [ ] A bundle without `capabilities.json` keeps today's behavior.
- **Verify:** engine test around `Policy::access` + a host test for `load_resource`; run on device
  with a rawfile bundle.

### P0-3 — Shader `nsh.size` / `overflow` are derived from the capture target

- **Area:** `crates/nsh-platform-ohos/src/render/composition/{gles.rs,worker.rs,fill.rs}`.
- **Status:** Confirmed; the old wgpu path passed the true view size
  (`git show 770ae0a7^:crates/nsh-platform-ohos/src/render/shader/gpu.rs:475-485`).
- **Problem:** `gles.rs:619-633` uploads
  `nsh.size = target_pixels / scale - 2 * overflow`. The target size comes from
  `capture_bounds()` plus style insets (`worker.rs:891-943`), and `ShaderLayer::overflow` is never
  added to the capture bounds; fills use exactly the view box (`fill.rs:80-95`). The core contract
  is the opposite: the picture covers the box plus `overflow` on each side, and `nsh.size` is the
  box (`nsh-core/src/shader.rs:48-54,108-112,176-197`). Consequences:
  - `maxSampleOffset > 0` on a fill: `nsh.size` is too small and the overflow region is clipped.
  - An effect on a node with box-shadow/outline/overflowing children: the extra padding is folded
    into `nsh.size`, so `shade(uv)` and `sample_content(uv)` are stretched/misaligned.
- **Change:** compute the target as `frame + 2 * overflow` for shader layers (in addition to the
  style capture needed by CSS), pass `nsh.size = frame` and `nsh.overflow = layer.overflow`, and
  align the capture origin to the expanded bounds. Do the same for fills.
- **Acceptance:**
  - [ ] A fill with `maxSampleOffset: 24` draws 24 points beyond the box and `uv` goes below 0 /
    above 1 as documented.
  - [ ] A `ShaderEffect` on a view with a 20 pt box-shadow keeps `uv == 0` at the view's corner and
    `uv == 1` at the opposite corner.
  - [ ] No regression for plain, undecorated views.
- **Verify:** add a host-side test for the bounds/`nsh.size` math (pure function), then a device
  screenshot fixture with a shadowed effect and `maxSampleOffset`.

### P0-4 — Shader layers are dropped when the CSS scene compositor is active

- **Area:** `crates/nsh-platform-ohos/src/render/composition/worker.rs`.
- **Status:** Confirmed.
- **Problem:** `Entry::render` takes `direct = !settings.scene` (`worker.rs:565`) and only the
  direct branch calls `shaders.render(...)` (`worker.rs:609-638`). `scene` is set for any
  `maskImage`, more than one backdrop filter, a non-blur backdrop filter, or `plus-darker`
  (`composition/geometry.rs:8-18`). A single such node under the root therefore disables a screen
  shader on that root, and `effects.rs:142-144` also refuses navigation shaders once the scene is
  prepared. The app sees the unfiltered UI with no error.
- **Change:** either compose the shader layers after the CPU scene output (the shader uniforms and
  `nsh.size` still apply), or reject the combination explicitly with an app-visible error and a
  documented limitation. Do not silently skip.
- **Acceptance:**
  - [ ] A root `ScreenEffect` stays visible when a descendant uses `maskImage` or `plus-darker`.
  - [ ] If the combination is unsupported, the app receives `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM`
    (or a documented warning + the shader view's normal background) instead of a silent drop.
- **Verify:** device fixture with a screen shader + a masked child; screenshot in light and dark.

### P0-5 — No CI builds, lints or tests the OHOS backend

- **Area:** `.github/workflows/`, `.github/ci/paths.py`.
- **Status:** Confirmed (no workflow or path entry mentions OHOS).
- **Problem:** `cargo test -p nsh-platform-ohos` (83 host tests) and
  `cargo xtask clippy-ohos` (default / dev / api-23 / api-26 with `-D warnings`) run only on
  developers' machines; `cargo +nightly fmt --all` is not enforced for the crate either. A new
  crate must be listed in the CI path graph, and nothing builds this one.
- **Change:** add an OHOS workflow that runs on changes to `crates/nsh-platform-ohos/**`, its
  dependency set (from the dependency graph) and `crates/xtask/**`:
  - host tests: `cargo test -p nsh-platform-ohos` (works off-ohos because the test target includes
    the pure modules by path);
  - `cargo xtask clippy-ohos` on a runner with the NDK and the `aarch64-unknown-linux-ohos` target,
    or a docker image that has them;
  - formatting: `cargo +nightly fmt --all -- --check`.
  Register the crate in `.github/ci/paths.py` (`python3 .github/ci/paths.py --print`).
- **Acceptance:**
  - [ ] A PR touching `crates/nsh-platform-ohos` runs the new job.
  - [ ] `CI paths` passes and the job is a required status check (or at least reported).
- **Verify:** open a draft PR that touches one comment in the crate.

### P0-6 — The branch needs an unreleased `ohos-arkui-binding` wrapper

- **Area:** `.cargo/config.toml`, `Cargo.lock`,
  `crates/nsh-platform-ohos/src/render/composition/snapshot.rs`, the `ohos-rs/ohos-native-bindings`
  checkout.
- **Status:** Confirmed.
- **Problem:** the working tree patches every `ohos-*` crate to a local checkout
  (`.cargo/config.toml`'s `[patch.crates-io]`). The checkout differs from released
  `ohos-arkui-binding 0.6.0` by one uncommitted 24-line change: a public
  `ArkUINode::snapshot() -> ohos_image_native_binding::PixelMap` around
  `OH_ArkUI_GetNodeSnapshot`, gated on `api-15` + `image`
  (`crates/arkui/src/common/node.rs`). The released crate already makes the same native call, but
  only as `pub(crate) NodeUtils::get_node_snapshot`, which returns `PixelMapNativeHandle`;
  `node_utils` is `pub(crate)`, and `PixelMapNativeHandle` has no `image_info()`/`read_pixels()`,
  the methods NSH needs. The only consumer is `render/composition/snapshot.rs:56` (the shader
  navigation snapshot). Without the patch the OHOS SDK does not compile against crates.io; with it,
  the branch builds only on machines that have the checkout and `NSH_ALLOW_PATH_DEPENDENCIES=1`.
  Every `ohos-*` crate is patched because a path-patched `ohos-arkui-binding` pulls its path
  dependency closure, and one package cannot come from both the registry and a path.
- **Change:** one of:
  1. Upstream `ArkUINode::snapshot()` to `ohos-native-bindings`, release it (0.6.1 or the next
     version), bump the workspace pin, then delete `.cargo/config.toml`'s patch block and regenerate
     `Cargo.lock`.
  2. Remove the snapshot consumer first (P1-4's texture-export/worker replacement), then delete the
     patch; the released crate then covers every API the backend uses.
- **Acceptance:**
  - [ ] `cargo xtask ohos` and `cargo xtask clippy-ohos` build with no `[patch.crates-io]` entries
    and no path dependencies.
  - [ ] The released binding versions named in the root `Cargo.toml` contain every API the crate
    uses.
- **Verify:** on a clean checkout without the override, `cargo update -p ohos-arkui-binding`, then
  `cargo xtask ohos` and `cargo xtask clippy-ohos`.

---

## P1 — user-visible correctness and jank

### P1-1 — `nsh/mobile` `TabBar` is unreachable

- **Area:** `crates/nsh-core/src/mobile.rs`, `crates/nsh-js/src/platform_modules.rs`.
- **Status:** Confirmed.
- **Problem:** the core component returns a hidden view unless `os()` is `Os::Ios | Os::Android`
  (`nsh-core/src/mobile.rs:83`), and `nsh/mobile`'s `isAvailable` is the same check
  (`nsh-js/src/platform_modules.rs:463-465`). OHOS nonetheless ships `NativeTabBar.ets`, the
  reserved-name factory (`interop/component.rs:122,145-152`) and documents the support
  (`docs/dev/ohos.md:101`).
- **Change:** add `Os::Ohos` to the core gate and to `isAvailable`, or delete the OHOS
  implementation and correct the docs. The first is intended by the implementation; the second is
  cheaper if TabBar is not meant for OHOS.
- **Acceptance:**
  - [ ] `isAvailable` is true on OHOS, `TabBar` mounts `TAB_BAR_COMPONENT`, selection and tint
    update, and `onChange` fires.
  - [ ] If removed: no dead ArkTS/factory code remains and all docs say iOS/Android only.
- **Verify:** host test for the gate; device screenshot with a three-tab bar and a selection
  change.

### P1-2 — Status bar and fullscreen-video bars are silent no-ops by default

- **Area:** `app/options.rs`, `app/session.rs`.
- **Status:** Confirmed; the option is absent from `website/docs/ohos.md` and `docs/dev/ohos.md`.
- **Problem:** `NshOptions.manages_status_bar` defaults to false (`options.rs:19,192`).
  `Services::status_bar` applies only when the dispatcher manages the bars
  (`session.rs:390-414`), and `video_fullscreen` returns early when it does not
  (`session.rs:343-374`). `nsh/base`'s `statusBar` prop, `nsh.json`'s `statusBar` grant and the
  `Video` fullscreen bars therefore do nothing.
- **Change:** decide the default (manage the window's bars, restoring the host's values when the
  surface stops) and document `managesStatusBar` in the `NshSurface` table. At minimum, log once
  when a status-bar command arrives unmanaged and document the opt-in.
- **Acceptance:**
  - [ ] With the default options, a page hiding the status bar hides it; a fullscreen video hides
    the system bars; both restore on exit.
  - [ ] `managesStatusBar: false` still works and is documented.
- **Verify:** device screenshots of a hidden status bar, a fullscreen video, and the restore.

### P1-3 — Input events missing

- **Area:** `interop/sdk.rs`, `arkts/src/main/ets/NativeEditors.ets`.
- **Status:** Confirmed against the core contract.
- **Problem:** `EditorSettings` (`interop/sdk.rs:17-48`) and the ArkTS builder
  (`NativeEditors.ets:265-305`) have no `onPaste`/`pasteBehavior`, `onCaretMove`,
  `onCompositionChange` or `autoCorrect`; the core declares all of them
  (`nsh-core/src/ops.rs:241,249,250`, `mount.rs:548,561`) and other backends emit them.
  `readonly` is implemented as `.enabled(false)` (`NativeEditors.ets:235`), so the field cannot be
  focused, selected or copied, contrary to `website/docs/forms-and-keyboard.md:183`.
- **Change:**
  - ArkUI exposes text-selection change, editing change and paste through the editor component;
    forward them as `EventKind::CaretMove` / `Composition` / `Paste`.
  - Add `autoCorrect` to the settings and map it to the platform attribute.
  - Implement readonly with the editor's read-only mode (or intercept `onChange`), not
    `.enabled(false)`.
  - Wire the input's own scrollbar back to the core (`EngineControl::input_scroll` and
    `Command::ScrollTo` on inputs) if the themed bar is to work.
- **Acceptance:**
  - [ ] Each event fires on device with the same payload as iOS/macOS.
  - [ ] A readonly input can be focused, selected and copied, and is not announced as disabled.
- **Verify:** device test script (typing, paste, CJK composition) and screenshots of caret-anchored
  UI (`@` mention popover) if the gallery has one.

### P1-4 — Navigation snapshots and teardown joins block the ArkUI UI thread

- **Area:** `render/shader/effects.rs`, `composition/snapshot.rs`, `app/runtime.rs`,
  `render/composition/worker.rs`.
- **Status:** Confirmed code path; impact needs device measurement.
- **Problem:** `Shaders::begin_navigation` is called while applying a mount batch on the UI thread
  (`navigation/stack.rs:344-360`, `modal.rs:238`). `Snapshot::capture` (`snapshot.rs:38-104`) calls
  ArkUI `snapshot()` and `read_pixels()` synchronously, then converts BGRA→RGBA and copies the
  frame at least twice — a full-screen snapshot is ~12.6 MB at 1260×2506. Separately,
  `Runtime::close` takes the engine (`runtime.rs:593-613`, joining the engine thread) and
  `Worker::drop` joins the compositor (`composition/worker.rs:497-503`) on the UI thread when
  loading, stopping or disposing.
- **Change:**
  - Capture the outgoing picture through the existing texture-export path (GPU), or move the
    snapshot and its conversion to the compositor worker and begin the transition when it arrives
    (the native animation is the fallback until then). This snapshot is the only consumer of the
    unreleased `ArkUINode::snapshot()` wrapper (P0-6), so replacing it also lets the local binding
    patch go.
  - Move the teardown joins off the UI thread: post the close to a worker, or retain the engine and
    join on the render/engine lane, then dispose the native owners on the next UI turn.
- **Acceptance:**
  - [ ] Push/pop with `transitionShader` does not run a full-frame CPU readback on the UI thread
    (trace or Hilog timing).
  - [ ] `stop()` / `dispose()` / loading another app does not join threads on the UI thread.
- **Verify:** device trace with `OH_ArkUI_PostFrameCallback` timing, plus a `nsh test --trace` run
  for the non-shader path.

### P1-5 — Frame pacing hardcodes 120 Hz; resume can stall animation

- **Area:** `app/frame_rate.rs`, `app/runtime.rs`, `render/composition/worker.rs`.
- **Status:** Confirmed (120); resume stall is a code-path risk.
- **Problem:** the shader vote requests min/max/expected 120 whenever any shader animates
  (`frame_rate.rs:44-53`); the surface configures 0..120/60 at start (`runtime.rs:678-684`). The
  window's supported range is never read. `Task::Resume` clears neither `frame_pending` nor the
  claimed VSync (`worker.rs:207-217`), and a new frame is requested only `if !frame_pending`
  (`worker.rs:441-446`) — if the callback was dropped while backgrounded, animation can stall.
- **Change:**
  - Read the window/display's supported refresh range (WindowManager/Display bindings) and clamp
    the vote to it; keep the app's window rate as the target.
  - On resume, reset `frame_pending` and re-arm the VSync request.
  - Fold the runtime's 4-frame pulse and the FrameRate vote into one owner, documented.
- **Acceptance:**
  - [ ] On a 60 Hz device no 120 Hz range is requested; on a 120 Hz device the range is the
    window's.
  - [ ] Background → foreground resumes a running shader within one frame.
- **Verify:** device Hilog of `OH_NativeVSync` calls on a 60 Hz and a 120 Hz device; background /
  foreground an animated shader.

### P1-6 — One GLSL compile per shader view; per-fill XComponents are uncapped

- **Area:** `render/composition/gles.rs`, `render/composition/fill.rs`, `render/shader/mod.rs`.
- **Status:** Confirmed.
- **Problem:** `Program::new` runs per `Effect` instance (`gles.rs:231-320,466`) and `layers()`
  recompiles whenever the program/slot set changes; unlike `nsh-engine::shader::Renderer`
  (`nsh-engine/src/shader.rs:73-75,192-243`) there is no compile cache by source hash. Compilation
  runs on the single compositor thread and blocks every presentation. Fills each own an XComponent
  and EGL window, with no count limit (`fill.rs`); `MAX_LAYERS = 128` covers only composite
  entries (`worker.rs:30,241-252`).
- **Change:**
  - Cache compiled programs by source hash (and the uniform/slot layout), across views and across
    app loads on the same surface; delete on worker shutdown.
  - Cap visible fills and report the limit (or reuse a pooled window per size class).
  - Record a `nsh::perf` timing for compile and first-frame latency.
- **Acceptance:**
  - [ ] N views with the same shader compile once.
  - [ ] A list with many shader rows stays within the documented budget and fails visibly, not
    silently, beyond it.
- **Verify:** device fixture with 50 identical shader cards; count `glCompileShader` calls in
  Hilog or a counter in the trace.

### P1-7 — Action sheets are presented as centered alerts

- **Area:** `services/dialogs.rs`.
- **Status:** Confirmed.
- **Problem:** `Dialog::show` (`dialogs.rs:79-209`) never reads `request.sheet`; every request
  becomes a centered custom `Dialog` with vertical text buttons. iOS maps it to
  `UIAlertControllerStyle::ActionSheet`, Android passes `sheet` to `SystemUi.dialog`.
- **Change:** present `sheet` as a bottom sheet (ArkUI sheet or a bottom-aligned custom dialog)
  with the separated Cancel action, or document that OHOS presents both as alerts.
- **Acceptance:**
  - [ ] `AlertDialog.show({ presentation: "actionSheet" })` shows a bottom sheet on device.
- **Verify:** device screenshots of an alert and an action sheet.

### P1-8 — Failed system commands leave promises unsettled; duplicate openUrl result

- **Area:** `services/system.rs`, `services/session.rs`, `render/mount.rs`.
- **Status:** Confirmed.
- **Problem:** `System::command` removes the tracked request and returns `Err` when
  `send_host_command` fails (`system.rs:164-172`); the app's clipboard/notification/speech promise
  gets no result. `Services::open_url` sends `OpenUrl { opened: false }` and returns `Err`
  (`session.rs:858-876`), then the mounter sends the same event again
  (`render/mount.rs:1123-1137`).
- **Change:** answer the request with a failure (stable code) before returning; make the mount
  layer the single owner of the fallback event.
- **Acceptance:**
  - [ ] A failed dispatch rejects the app's promise with a stable code.
  - [ ] One `openUrl` request produces exactly one result event.
- **Verify:** host test with a closed host session; device log with an unknown URL.

### P1-9 — `os.platform()` / `process.platform` report `"linux"` on OHOS

- **Area:** `crates/nsh-core/src/runtime/os.rs`, `process.rs`, `nsh-updater`.
- **Status:** Confirmed.
- **Problem:** both return `std::env::consts::OS` (`os.rs:18-23`, `process.rs:29-33`), and the OHOS
  target triple is `aarch64-unknown-linux-ohos` (`target_os = "linux"`), so the runtime platform
  string is `"linux"` while `platform.os` is `"ohos"`. `nsh-updater`'s platform key follows the
  same value (`"linux-aarch64"`). The public docs promise `platform.os === "ohos"` only.
- **Change:** make the runtime's platform string `"ohos"` when `cfg!(target_env = "ohos")` (and the
  updater key likewise), or document the divergence explicitly in `docs/dev/ohos.md` and
  `website/docs/device-and-environment.md`.
- **Acceptance:**
  - [ ] Either `os.platform() === "ohos"` or the divergence is documented and tested.
- **Verify:** host test is not enough (`cfg!` is false off-ohos); run on device or unit-test the
  mapping function.

### P1-10 — `sample_backdrop` is always transparent

- **Area:** `render/composition/gles.rs`, `interop/texture_export.rs`.
- **Status:** Confirmed; documented as a follow-up (`docs/dev/ohos.md:95`,
  `docs/dev/shaders.md:157`).
- **Problem:** `ShaderInput::Backdrop` is bound to the 1×1 transparent `empty` target
  (`gles.rs:353-355`); no parent/behind capture exists.
- **Change:** export the backdrop subtree (the parent's content behind the view) and bind it, or
  reject shaders that declare `sample_backdrop` with an app-visible error.
- **Acceptance:**
  - [ ] A backdrop shader samples what is behind the view (device fixture) or fails visibly.
- **Verify:** device fixture with a photo behind a `backdrop-filter`-style shader.

### P1-11 — Layout-animation scoping is ignored

- **Area:** `crates/nsh-platform-ohos/src/render/mount.rs`.
- **Status:** Confirmed by code and the core contract.
- **Problem:** the batch pre-pass captures a layout animation only while none was seen
  (`mount.rs:675-677`, `layout.is_none()`); every later `MountItem::LayoutAnimation` is a no-op
  (`mount.rs:919`), and that single animation is used for every `Insert` (`mount.rs:886`), `Frame`
  (`mount.rs:913-917`) and removal (`mount.rs:726-745,894-912`). The core leads **each run of
  frames** with the layout animation it takes, an empty one for the rest
  (`nsh-core/src/core/layout_scope.rs:61-125`), and `docs/dev/animation.md:81-84` states that
  platforms animate a frame with the last `LayoutAnimation` before it. macOS and Windows update
  their current animation per item (`nsh-platform-macos/src/mount.rs:488`,
  `nsh-platform-windows/src/mount.rs:360`). A batch with two scoped animations (two
  `ElementRef::animate_layout` regions, or a scoped reveal plus a `View::follow` placement) uses
  the first scope for both, so one region either does not animate or animates with the wrong
  transition.
- **Change:** track the current animation while applying items: a `LayoutAnimation` sets it, and
  each following `Insert`/`Frame` uses it. Keep the batch-level animation for removals (the core
  decides those per batch, not per scope) and say so in the code. If the pre-pass needs the first
  animation for retirement, keep that separate from the per-item current scope.
- **Acceptance:**
  - [ ] A batch with two different scoped transitions animates each run with its own.
  - [ ] `Tabs`' indicator (`View::follow`, a spring) moves with its spring when another scoped
    animation is in the same commit.
  - [ ] An unscoped run in the same batch (the core's empty animation) moves at once.
- **Verify:** extract the scope-selection into a pure helper with a unit test; device test with a
  scoped reveal + `Tabs` in one commit.

### P1-12 — Text color transitions are not implemented

- **Area:** `render/mount.rs`, `render/transition.rs`, `render/node/paint.rs`.
- **Status:** Confirmed.
- **Problem:** `MountItem::Text` discards the attributed text (`mount.rs:925-944` destructures
  `text: _`) and installs the final typeset drawing; the local `Live` wrapper has no
  `text_color`/`recolor` (`render/transition.rs:23-129`). Desktop backends apply
  `nsh_core::transition::recolor` and `Live::text_color` every frame
  (`nsh-platform-macos/src/transition.rs:199-211`, `nsh-platform-windows/src/transition.rs:191-203`),
  and `docs/dev/animation.md:95-107` says mobile backends follow the same rule. On OHOS a
  `transition: { color }` on text jumps to the final color; only the animation graph's `color`
  binding works via the tint.
- **Change:** call `nsh_core::transition::recolor(old, new, style, reduce_motion)` when a text
  layout replaces another whose single run color differs; keep the result in `Live`, and draw the
  whole laid-out text in the in-between color over its runs (as an animated `color` is drawn),
  without re-typesetting.
- **Acceptance:**
  - [ ] `transition: { color }` on `Text` interpolates on OHOS.
  - [ ] Reduced motion jumps at once; an animated `color` prop keeps working.
- **Verify:** device fixture; the core already unit-tests `recolor`/`Live::text_color`.

### P1-13 — Blur and shadow units disagree with ArkUI's documented units

- **Area:** `render/decor.rs`, `render/node/style.rs`, `media/image.rs`.
- **Status:** Confirmed against the binding docs; the on-device effect needs a screenshot.
- **Problem:** the SDK documents `NODE_BLUR` in **vp** (blur radius), `NODE_BACKDROP_BLUR` in
  **px**, and `NODE_CUSTOM_SHADOW` radius in **vp** with x/y offsets in **px**
  (`ohos-arkui-sys`'s `NODE_BLUR`, `NODE_CUSTOM_SHADOW`, `NODE_BACKDROP_BLUR` docs). The code:
  - `decor.rs:130-138`: foreground `Blur = blur * scale` (vp field given px) and
    `BackdropBlur = backdrop_blur * scale` (px field, correct);
  - `node/style.rs:257-265`: custom shadow radius `blur * scale * 0.5` (vp field given px),
    offsets `* scale` (px, correct);
  - `media/image.rs:694-700`: `Blur = blur_sigma(radius, scale) = radius * scale * 0.5`.
  At density 3 the two `NODE_BLUR` call sites differ by 2× from each other (3× vs 1.5×), so at
  least one is wrong; foreground blur and the custom-shadow radius are inflated by density if the
  docs hold.
- **Change:** one conversion per attribute, named and documented from the SDK units, in
  `render/geometry.rs` (e.g. `blur_vp(points)` for `NODE_BLUR`/shadow radius, `blur_px(points,
  scale)` for backdrop, offsets in px). Use them at all call sites and note the attribute's unit
  beside each.
- **Acceptance:**
  - [ ] A 12 pt foreground blur, backdrop blur, box-shadow and image blur look the same at density
    1 and 3.
  - [ ] No call site multiplies a vp attribute by the pixel ratio.
- **Verify:** device fixture with side-by-side references at density 1/2/3; screenshot both
  appearances.

---

## P2 — design, consistency and robustness

### P2-1 — Navigation header, alerts and video chrome are hand-drawn and inaccessible

- **Area:** `navigation/stack.rs`, `services/dialogs.rs`, `media/video/ui.rs`, `render/decor.rs`,
  `render/node/style.rs`.
- **Status:** Confirmed.
- **Problem:** the header is a `Stack` with a `Text` title and a `"‹"` back glyph, fixed sizes
  (52/44/28/18) and a title region of `width - 112` (`stack.rs:51-97,180-215`), with no
  accessibility role/label and no font-scale adaptation. Alerts are plain `Text` buttons with no
  roles; their default button is the English `"OK"` (`dialogs.rs`). Video controls have no labels
  (`media/video/ui.rs:177-190`) and ignore `audio_session`. Modal corners default to 16 pt and
  surfaces to hardcoded white/dark backgrounds (`style.rs:161-200`).
- **Change:**
  - Use ArkUI's navigation/dialog/video system components where they exist, or at least take
    colors from the theme's design tokens, sizes from the environment, the system back icon, and
    add `accessibilityText`/roles and labels.
  - Remove the built-in `"OK"` (the core always supplies buttons) and the `"‹"` glyph fallback.
  - Honor `audio_session` with AVPlayer's audio focus/interruption APIs.
- **Acceptance:**
  - [ ] Screen reader announces "Back", the title, alert buttons and player controls correctly.
  - [ ] No hardcoded English strings or colors outside design tokens.
- **Verify:** device with TalkBack/screen reader; screenshots in light/dark and with large font.

### P2-2 — Rust ↔ ArkTS system commands are stringly typed

- **Area:** `services/system.rs`, `interop/sdk.rs`, `arkts/src/main/ets/NativeSystem.ets`.
- **Status:** Confirmed.
- **Problem:** requests cross as `{"op": "...", ...}` JSON strings and responses as JSON values;
  unknown ops return `from_reason` messages without a stable `code`. This is the only
  stringly-typed protocol in the backend and it drifts easily (already: notification/speech
  lifecycle, clipboard).
- **Change:** define `#[napi(object)]` request/response enums (or a typed tag enum) and give
  failures the core's stable codes; keep JSON only for app payloads.
- **Acceptance:**
  - [ ] Command variants are exhaustively matched on both sides; a new op fails compilation until
    ArkTS handles it.
  - [ ] Failures carry `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` / `ERR_NSH_HOST_ERROR` as appropriate.
- **Verify:** host tests for encoding/decoding, ArkTS compilation for exhaustiveness.

### P2-3 — Picker copying and host glue run on the UI thread; state is duplicated

- **Area:** `arkts/src/main/ets/NshHost.ets`, `NativeSystem.ets`.
- **Status:** Confirmed.
- **Problem:** the photo picker copies each picked image with synchronous `fs.copyFileSync` in a
  promise callback on the UI thread; `NativeSystem.active/listening` mirrors Rust
  `System::requests`, and the listen path already orphans Rust requests. Cursor mapping and system
  bar bookkeeping live in ArkTS while Rust keeps a second copy.
- **Change:** copy picked files asynchronously (or in Rust), make one side the owner of request
  state, and move any logic that does not need an ArkTS-only API into Rust.
- **Acceptance:**
  - [ ] Picking 9 large photos does not block the UI thread (trace).
  - [ ] One listener/request table, with a removal path.
- **Verify:** device trace while picking; reload the page repeatedly and check
  `System::requests` stays bounded.

### P2-4 — Process-global IO/network pools starve other surfaces

- **Area:** `services/executor.rs`, `services/net.rs`, `app/session.rs`.
- **Status:** Confirmed.
- **Problem:** `Executor` lanes (fetch 6, image 4, assets 2) and the ureq agent pool are
  process-global (`executor.rs:30-47`, `net.rs:60-64`); a surface with paused/endless streams can
  occupy all fetch workers, and another surface then sees "IO queue is full". `reserve_load` is per
  Ability, not per surface (`session.rs:534-543`). The website promises per-surface isolation.
- **Change:** add per-surface (or per-Ability) quotas over the shared pools, and make rejection a
  retryable error; or move the pools into the surface. Document the actual isolation model.
- **Acceptance:**
  - [ ] One surface saturating its fetch budget does not fail another surface's fetches.
  - [ ] `reserve_load` is charged per surface.
- **Verify:** host test with two surfaces and a blocked transport.

### P2-5 — Duplicated network transport and image cache

- **Area:** `services/net.rs`, `media/image.rs`.
- **Status:** Confirmed.
- **Problem:** `services/net.rs` is a near-copy of `nsh-engine/src/net.rs` (~330 lines) with only
  proxy lookup and admission added; `media/image.rs` keeps its own 32 MB memory cache while other
  backends use `nsh_core::image::loader` + `Images`.
- **Change:** parameterize the engine transport with proxy lookup and the bounded executor, and
  use the core image loader/cache policy (keep Avalonia/native decoding). Add a test that both
  transports share redirect/streaming/cancellation semantics.
- **Acceptance:**
  - [ ] One transport implementation, one image cache policy; no behavior change on macOS/Linux.
- **Verify:** `cargo test -p nsh-engine` and the OHOS unit tests; `fetch`/image fixtures on device.

### P2-6 — Full close does not drain the shader retirement handshake

- **Area:** `app/runtime.rs`, `render/shader/mod.rs`, `render/composition/surface.rs`.
- **Status:** Confirmed mechanism; impact is device-verify.
- **Problem:** `Runtime::close` takes the engine first (`runtime.rs:605`); after that `receive`
  returns early when `engine.is_none()` (`runtime.rs:288-290`), so the worker's `Message::Shader`
  is dropped, `Shaders::collect` (`shader/mod.rs:260-283`) never releases fill surfaces queued in
  `retiring`, and `composition::surface::Surface` has no `Drop`. `docs/dev/ohos.md:118-122`
  promises the opposite ordering.
- **Change:** drain `Shaders::collect` (and the worker's retired presentations) in `Mounter::close`
  before native teardown, and join the compositor before the content root is unmounted.
- **Acceptance:**
  - [ ] Loading a second app in the same surface leaves no XComponents/EGL windows behind
    (device handles count stays flat across 20 reloads).
- **Verify:** device `hidumper`/handle count or ArkUI node count across reloads; Hilog teardown
  order.

### P2-7 — Dialogs use the pre-resize window; pixel ratio never updates

- **Area:** `render/mount.rs`, `services/dialogs.rs`, `app/session.rs`, `app/entry.rs`.
- **Status:** Confirmed.
- **Problem:** `Mounter::resize` updates the root frame but not `UiContext.platform`
  (`mount.rs:1381-1394`), which `Dialogs::show` reads for its width and max height
  (`dialogs.rs:96,176`). `UiContext.platform` is otherwise written only by
  `Mounter::environment` (`mount.rs:149-162`), reached from `layout_changed` only when
  `window_areas()` is `Some` (`app/runtime.rs:911-926`), so a host that never reports avoid areas
  also keeps stale safe-area and modal sheet insets (`navigation/modal.rs:359-445`).
  `Services.pixel_ratio` is immutable (`session.rs:160,545-553`) and `nshUpdateConfig` carries no
  density.
- **Change:** update the platform window in `Mounter::resize` (and derive insets from the last
  known areas); add density to the configuration update (and recompute scale/typography/insets on
  change).
- **Acceptance:**
  - [ ] Rotate/resize, then open an alert: it is sized for the new window.
  - [ ] Change display density / fold state: `platform.pixelRatio`, text and layouts follow.
- **Verify:** device rotation screenshot; developer option for display size.

### P2-8 — No `set_ui_sets`; reserved component names are not refused

- **Area:** `app/runtime.rs`, `app/surface.rs`, `interop/component.rs`.
- **Status:** Confirmed.
- **Problem:** `ui_sets` is hardcoded to `nsh_base::registry()` (`runtime.rs:777`), so an embedding
  host cannot serve an app's own Rust UI set (iOS/Android/macOS/Linux have `set_ui_sets`).
  `register_component` accepts any name (`surface.rs:1273-1281`), so a host can shadow `nsh/*`;
  iOS refuses them with `is_reserved_component` (`nsh-platform-ios/src/component.rs:131-140`).
- **Change:** expose `set_ui_sets` on `NshSurface` (per surface, like the other backends) and
  reject reserved names.
- **Acceptance:**
  - [ ] A host registers an app's UI set and `nativeModule("app/native")` resolves.
  - [ ] Registering `nsh/tab-bar` logs and is ignored.
- **Verify:** host test + the `rust-exports` example bundled for OHOS.

### P2-9 — Accessibility provider only with `api-23`; `has_popup` gap

- **Area:** `accessibility/mod.rs`, `accessibility/state.rs`, docs.
- **Status:** Confirmed (documented as an API gap only for `has_popup`).
- **Problem:** the API-20 build sets ArkUI's label/group/mode/description attributes only
  (`input/events.rs:360-385`); roles, states, actions, hierarchy and focus need the provider
  compiled with `api-23` (`accessibility/mod.rs:1-5`). The website calls `api-23` an optional
  enhancement, which reads as if the baseline were complete.
- **Change:** document the baseline clearly (label-only) and the `api-23` requirement for full
  semantics; consider enabling `api-23` in the default SDK once API 23 is the supported floor.
- **Acceptance:**
  - [ ] Public docs state what API 20 exposes; `docs/dev/ohos.md` says the provider requires
    `api-23`.
- **Verify:** doc review; device screen-reader walkthrough on an API 20 build and an API 23 build.

### P2-10 — Public docs claim APIs and behavior the backend does not have

- **Area:** `website/docs/ohos.md`, `website/zh-CN/docs/ohos.md`, `docs/dev/ohos.md`.
- **Status:** Confirmed.
- **Problem:**
  - `nativeShellPlugins()` is referenced (`website/docs/ohos.md:276`, `zh-CN:275`) but does not
    exist (`arkts/index.ets` exports only `onNotificationWant`).
  - TabBar support is claimed (`docs/dev/ohos.md:101`) but unreachable (P1-1).
  - `docs/dev/ohos.md:108` says `nsh/updater` checks return no update, while the engine provides
    an updater host whenever network + assets exist (`nsh-engine/src/lib.rs:1747-1753`, with
    `canInstall: false`).
  - `managesStatusBar`, `assetDirectory`, `storageDirectory`, `cacheOptions`, `environment`,
    `maxOpValues` and `maxCanvasOps` are missing from the `NshSurface` table.
  - The platform value of `os.platform()` is not documented (P1-9).
- **Change:** correct these pages in both languages in the same PR as the behavior fixes.
- **Acceptance:**
  - [ ] A doc build finds no symbol that does not exist; the option table matches
    `NshOptions`.
- **Verify:** `cd website && bun run dev` (or `bun run build`) and a grep for each option.

### P2-11 — 4096-pixel cap rejects large targets instead of downscaling

- **Area:** `render/composition/gles.rs`, `gles/filters.rs`, `readback.rs`, `fill.rs`.
- **Status:** Confirmed.
- **Problem:** any side > 4096 fails (`gles.rs:63-65,69-113`, `gles/filters.rs:165-167`,
  `readback.rs:491-494`) while the settings gate allows 16 MP (`worker.rs:916-920`); the engine
  downscales instead (`nsh-engine/src/shader.rs:704-715,789-797`). Fills clamp each side to 4096
  but keep the requested `scale` (`fill.rs:87-95`, `gles.rs:626-631`).
- **Change:** downscale the target and pass the effective `nsh.scale`, as the engine does; clamp
  the fill's scale consistently; if a hard cap remains, document it and warn once.
- **Acceptance:**
  - [ ] A 5000 px-tall effect renders (at reduced scale) instead of disappearing.
  - [ ] `nsh.scale` matches the pixels actually rendered.
- **Verify:** device fixture with a tall node and a scale-sensitive shader.

### P2-12 — Shader layer races: `from`, `active`, progress

- **Area:** `render/composition/gles.rs`, `render/shader/effects.rs`, `worker.rs`.
- **Status:** Confirmed code paths; frequency needs device testing.
- **Problem:**
  - `ShaderTransition` only arms `from` when a previous input exists (`gles.rs:498-511,
    587-602`); the first frame or a lost wake makes `from` alias the live input.
  - `Shared.active` is one atomic per composite, swapped once per layer (`effects.rs:292-299`);
    with two layers targeting the root, the last (hash-order) `false` disables both.
  - A layer update resets `view.progress` to the static `layer.progress` (`gles.rs:455-458`),
    clobbering an animated progress until the next slot.
- **Change:** capture the transition's `from` synchronously (as macOS does) or ignore the
  transition until it exists; make `active` a per-layer/per-composite count; keep animated
  progress in a slot that survives layer updates.
- **Acceptance:**
  - [ ] A `ShaderTransition` always shows the old picture, including the first change after mount.
  - [ ] A screen shader and a navigation transition on the root render together.
- **Verify:** device fixture that changes `ShaderTransition` immediately after mount and
  repeatedly; two-layer screen shader + page transition.

### P2-13 — EGL config requests ES2 while creating an ES3 context

- **Area:** `render/composition/readback.rs`.
- **Status:** Device-verify; plausible strict-driver failure.
- **Problem:** the EGL config requires `EGL_OPENGL_ES2_BIT` (`readback.rs:91-119`) but the context
  is created with `EGL_CONTEXT_CLIENT_VERSION = 3`. Some drivers require an ES3-capable config.
  If initialization fails, the compositor has no GPU and shader surfaces never present (only
  logged).
- **Change:** choose a config whose renderable type matches the context (ES3 where available, fall
  back to ES2 + `GL_VERSION` check), and report the failure to the app once.
- **Acceptance:**
  - [ ] The compositor initializes on the API 20/23/26 reference devices; a failure is visible in
    Hilog and (once) to the app.
- **Verify:** device startup on each API level with a shader fixture.

### P2-14 — Panics can cross the main TSFN callbacks; GL error-path leaks

- **Area:** `app/surface.rs`, `app/runtime.rs`, `render/composition/gles.rs`, `readback.rs`.
- **Status:** Confirmed (absence of guard); trigger requires a bug elsewhere.
- **Problem:** the two main TSFN callbacks (`surface.rs:721-727`, `runtime.rs:724-732`) call
  `State::dispatch`/`receive` directly; those contain `.unwrap()`/`.expect()` on locks and state,
  while ArkUI/executor/host callbacks are wrapped in `catch_unwind`. A panic unwinds through an
  `extern "C"` callback and aborts. The same gap exists on custom **draw** callbacks: measure and
  layout are wrapped in `callback()` (`render/node/layout.rs:23-40`), but
  `render/node/paint.rs:27-31`, `render/shadow.rs:78-93` and the wrapper draw in
  `interop/view.rs:127-140` call into drawing directly, and `Node::draw` reaches locked font/canvas
  state. `Target::new` returns on an incomplete framebuffer without
  deleting the texture/framebuffer (`gles.rs:69-113`); `TextureReader::drop` `mem::forget`s its GL
  owners when the context cannot be entered (`readback.rs:624-629`) without telemetry.
- **Change:** wrap both TSFN callbacks and the draw callbacks in `catch_unwind` and report the panic
  as an app error; fix the `Target::new` error-path deletion; log/quarantine counting for the
  `mem::forget` paths.
- **Acceptance:**
  - [ ] An induced panic in a UI message is logged and the surface closes, not the process.
  - [ ] No GL objects leak across 100 target recreations in a fault-injected build.
- **Verify:** host unit test for the guard; device fault injection only.

### P2-15 — NodeId-keyed state never shrinks and is scanned every frame

- **Area:** `render/mount.rs`, `render/animation.rs`.
- **Status:** Confirmed; the other backends use maps and remove entries.
- **Problem:** `Mounter.slots` is a dense `Vec<Option<Rc<Node>>>` grown to `id + 1` on every
  `Create` (`mount.rs:84,805-807`); deletion only clears the slot (`mount.rs:603-615`) and the
  vector is never truncated. The `Animator` keeps four more dense by-id vectors (`bindings`,
  `motion`, `scrolls`, `sticky` — `animation.rs:25-30`), clears entries without shrinking them, and
  every frame iterates all of `scrolls`/`motion` (`animation.rs:188-214`) while `write()` iterates
  every binding and sticky slot (`animation.rs:220-249`). `Mounter::environment`, `cancel_input`
  and `native_context` also scan `slots` (`mount.rs:151,191,224`). IDs advance by 2 per JS element
  (`nsh-js/src/dom.rs:830-834`), so a long session that creates and destroys 100k elements leaves
  ~200k slots (~tens of MB) and pays an O(max id) scan on every active frame and on those
  maintenance paths. macOS/Windows use `FxHashMap` and remove on Delete.
- **Change:** key the mounter and animator state by a map or a slab with free-list reuse (the core
  guarantees monotonic IDs today, but nothing should rely on the maximum), and compact the animator
  sets when they empty. At minimum, truncate trailing `None` slots and skip empty vectors.
- **Acceptance:**
  - [ ] After 100k create/delete cycles, an idle frame touches only active entries (counted).
  - [ ] Resident memory returns near the baseline after the deletions.
- **Verify:** host microbenchmark on the animator; device long-session trace with memory sampling.

### P2-16 — `Stack`, `Image` and `Input` children are not clipped

- **Area:** `render/node/style.rs`.
- **Status:** Confirmed.
- **Problem:** `clips_children()` clips only `overflow_hidden`, `Scroll` and non-full-screen modals
  (`style.rs:22-28`). macOS, Windows and Linux also clip `Image`, `Stack` and `Input`
  (`nsh-platform-macos/src/view.rs:211-217`, `nsh-platform-windows/src/view.rs:260-266`,
  `nsh-platform-linux/src/view.rs:187-193`). During a push the entering screen is translated to
  `x = width` and the one below to `-0.3 * width` (`navigation/stack.rs:324-339`); a sheet-sized
  stack can therefore paint its sliding page outside its bounds, and image/input children are not
  clipped to their rounded boxes.
- **Change:** match the other backends' kind list in `clips_children()`, keeping the modal sheet
  rule.
- **Acceptance:**
  - [ ] A constrained-size `NavStack` shows no page outside its bounds during push/pop.
  - [ ] Children of `Image`/`Input` clip to the rounded bounds.
- **Verify:** device screenshots of a sheet-sized stack mid-transition, and an image with an
  overflowing child.

### P2-17 — Inset box-shadows with zero blur are dropped

- **Area:** `render/decor.rs`.
- **Status:** Confirmed.
- **Problem:** `visible_layer` requires `(!inset || layer.blur.0 > 0.0)` (`decor.rs:235-238`), so a
  visible inset shadow with no blur — CSS `box-shadow: inset 0 0 0 Npx color`, the common focus
  ring — is never drawn. `BoxShadow::is_visible` only checks the color
  (`nsh-core/src/effects.rs:31-33`) and the style enables custom drawing for it
  (`node/style.rs:397-405`).
- **Change:** draw inset shadows regardless of blur (the painter handles the offset/spread); keep
  the existing fast path for empty layers.
- **Acceptance:**
  - [ ] An inset 0-blur ring shows in light and dark.
- **Verify:** device screenshot of a focus ring.

### P2-18 — Compositor walks the tree every UI frame; scenes rebuild every batch

- **Area:** `render/mount.rs`, `render/composition/scene.rs`, `render/arkui.rs`,
  `interop/view.rs`, `render/shader/mod.rs`.
- **Status:** Confirmed; performance-only but on a hot path.
- **Problem:** every mount batch calls `scene.invalidate()` unconditionally (`mount.rs:665-667`),
  and `Scene::build` re-snapshots the whole tree (including `capture_bounds`) and re-plans the
  worker (`composition/scene.rs:133-183`); every `flush_arkui` walks all composite owners and
  calls `update_wrapper → Composite::update → capture_bounds()` (`render/arkui.rs:115-121`,
  `interop/view.rs:493-506`, `composition/worker.rs:891-943`). `Shaders::frame` calls `in_sight`
  for every shader view on every UI frame, with native geometry queries per node and ancestor
  (`shader/mod.rs:208-252,290-322`). There is no dirty tracking, so a screen shader on the root
  walks the whole tree each batch and each shader view is re-tested each frame during any
  animation.
- **Change:** invalidate the scene per changed node; recompute a composite's capture bounds only
  when its layout or style changed (the mounter knows); cache `in_sight` per scroll/layout
  generation and skip unchanged views. Add counters/trace to prove the bound.
- **Acceptance:**
  - [ ] During scroll and animation, per-frame native queries scale with visible shaders, not all
    nodes.
  - [ ] A batch that changes one node does not re-plan every scene root.
- **Verify:** device trace on a shader/mask-heavy page; before/after counters in `nsh::perf`.

---

## P3 — small, low-risk

### P3-1 — Animated image frame durations need `api-23`

- **Area:** `media/image.rs:495-515`.
- **Change:** keep the per-frame times in the descriptor on the API-20 build where the binding
  allows it; otherwise document the uniform-frame fallback.
- **Acceptance:** a GIF with non-uniform delays plays with its own timings, or the limitation is
  documented.

### P3-2 — Signed-package/resource limits disagree; packages are consumed too early

- **Area:** `app/session.rs:33`, `app/trust.rs:18,149-165`, `app/surface.rs:867-891`.
- **Change:** one size limit for `openResource`/`openPackage` (64 MiB vs 256 MiB), and consume
  `NshPackage` only after the app starts, so a failed start can retry.
- **Acceptance:** a 100 MiB signed package opens; a failed start leaves the package usable.

### P3-3 — Notification listeners accumulate; `exitApp` is a no-op

- **Area:** `services/system.rs:50-53,150-152,175-185`, `NativeSystem.ets:81-87`,
  `render/mount.rs:1115-1117`.
- **Change:** remove a replaced listener's Rust request; document that `BackHandler.exitApp`
  cannot leave an embedded Ability (or call `terminateSelf` when the host opts in).
- **Acceptance:** repeated `Notifications.listen` keeps one request; `exitApp` behavior is
  documented and tested.

### P3-4 — Default strings and hardcoded colors/radii in NSH chrome

- **Area:** `services/dialogs.rs`, `navigation/stack.rs`, `render/node/style.rs`,
  `render/decor.rs`.
- **Change:** remove the `"OK"` fallback; take header/alert/modal colors and radii from the design
  tokens or the platform's system colors; keep only what the system style requires and document
  any exception (the project's "No styles in NSH" rule).
- **Acceptance:** no literal `0x........` colors or `16.0` radii outside a documented exception;
  light/dark screenshots match the token palette.

### P3-5 — `process.run` has no OHOS sandbox gate

- **Area:** `crates/nsh-core/src/runtime/process.rs`.
- **Change:** device-verify whether fork/exec is permitted; if not, answer
  `ERR_FEATURE_UNAVAILABLE_ON_PLATFORM` instead of a raw OS error.
- **Acceptance:** the promise rejects with the stable code on a sandboxed device.

### P3-6 — Host-side tests cover none of the compositor/mounter state machines

- **Area:** `crates/nsh-platform-ohos/tests/ohos/`.
- **Change:** add pure tests for the bounds/`nsh.size`/overflow math, snapshot alignment and alpha
  conversion, shader layer merge/progress/transition timing, and the mount deferred-op ordering
  (the existing tests cover geometry, blend, frame clock, scroll motion and native ownership only).
- **Acceptance:** the P0-3/P2-12 fixes land with tests that fail before them.

### P3-7 — Stale validation numbers in the OHOS docs

- **Area:** `docs/dev/ohos.md`.
- **Change:** update the host-test count (83 at this baseline) and the shader validation notes;
  move the "follow-up" table entries to this fix list once fixed.

### P3-8 — `pagePosition` ignores transforms and pins

- **Area:** `render/node/pointer.rs`, `render/node/transform.rs`.
- **Status:** Device-verify.
- **Problem:** hit testing composes the view's `input_transform` (transform origin, transition
  transform, sticky pin, navigation presentation) in `transform.rs:14-68,99-253`, but
  `pointer_positions` derives `pagePosition` from `layout_position_in_window` plus local
  coordinates (`pointer.rs:128-156`), and that native helper reports the layout position without
  translation/scale/rotation. Events from a transformed, animated or sticky-pinned node can
  therefore report a page position that disagrees with the point the engine hit.
- **Change:** derive `pagePosition` from the same composed matrix used for hit testing (or document
  that it is the layout position), and add a device check during transitions and on pinned sticky
  content.
- **Acceptance:**
  - [ ] `pagePosition` matches the visual point under a translated/rotated view and a pinned sticky
    header.
- **Verify:** device test logging `pagePosition` against screenshots.

### P3-9 — Stricter fatal mount policy than the other backends

- **Area:** `render/mount.rs`.
- **Status:** Confirmed divergence; currently unreachable with monotonic core IDs.
- **Problem:** a duplicate `Create` and an `Insert` whose child is unknown become fatal
  `Error::Tree` (`mount.rs:805-814,829-832`), and a `Tree` error closes the running app
  (`app/runtime.rs:395-405`). macOS and Windows skip/replace instead
  (`nsh-platform-macos/src/mount.rs:223-236,247-250`, `nsh-platform-windows/src/mount.rs:112-124,136-137`).
  `MountItem::Frame` is not ignored for `ROOT_ID`, while the other mounters return for it
  (`macos:299-303`, `windows:185-189`).
- **Change:** match the other mounters: warn and skip/replace recoverable staleness, keep a fatal
  error only for an unrecoverable native failure, and ignore a root `Frame`.
- **Acceptance:**
  - [ ] A stale/duplicate op is logged and the app keeps running.
  - [ ] A root `Frame` does not fight the viewport size.
- **Verify:** host test with a synthetic batch; device fault injection only.

### P3-10 — Mount parity leftovers

- **Area:** `render/mount.rs`, `render/node/lifecycle.rs`, `render/node/layout.rs`,
  `navigation/stack.rs`.
- **Status:** Confirmed.
- **Problem / change:**
  - **`InputAccessory`** is treated as an ordinary child (`mount.rs:824-893`; `Node::new` ignores
    the kind and its layout callbacks, `node/lifecycle.rs:140-156`), and nothing reads
    `InputConfig.accessory`/`accessory_id`. `nsh/ios`'s `InputAccessoryView` is iOS-only, so decide
    between attaching it above the keyboard (as macOS/Windows do) and not creating a stray view;
    document the choice.
  - **`Mounter::scroll_to`** ignores `x`, `y` and `animated` (`mount.rs:234-241`); the only sender
    passes zeros today (`app/runtime.rs:342-346`). Implement it or assert the zero contract.
  - **Large titles** are approximated by shifting children down by `52 + top_overlap` and shrinking
    the viewport (`node/layout.rs:58-77`, `navigation/stack.rs:119-129`), so content never scrolls
    under the header and the native frames used for hit testing differ from the frames core
    reports. Implement a collapsing large title (native frames following core's `onLayout`) or
    document that OHOS draws a fixed large-title bar.
- **Acceptance:**
  - [ ] The keyboard accessory appears above the keyboard or is documented unsupported.
  - [ ] `scrollTo` either scrolls or rejects a non-zero request.
  - [ ] A large title collapses under scroll, or the limitation is documented.
- **Verify:** device fixture for each; screenshot the large title mid-scroll.

---

## Device-verification checklist

Run these on ALN-AL00 (API 26) and, where possible, an API 20 device:

- [ ] Split bundle with two shared chunks, both `bundle.js` and `bundle.nshc` (P0-1).
- [ ] `capabilities.json` intersection with a host grant (P0-2).
- [ ] `maxSampleOffset` fill and a shadowed `ShaderEffect` (P0-3).
- [ ] Screen shader + `maskImage`/`plus-darker` descendant (P0-4).
- [ ] TabBar mount, selection, tint (P1-1).
- [ ] Status bar hide/show and fullscreen video bars (P1-2).
- [ ] Paste, caret, composition, autoCorrect, readonly selection (P1-3).
- [ ] Push/pop with `transitionShader` timing and stop/load teardown (P1-4).
- [ ] 60 Hz and 120 Hz pacing; background/foreground resume (P1-5).
- [ ] 50 identical shader cards compile count (P1-6).
- [ ] Action sheet presentation (P1-7).
- [ ] Failed dispatch and duplicate openUrl (P1-8).
- [ ] `os.platform()` value (P1-9).
- [ ] Backdrop shader (P1-10).
- [ ] Two layout-animation scopes in one batch; `Tabs` indicator (P1-11).
- [ ] Text `transition: { color }` (P1-12).
- [ ] Foreground blur, backdrop blur, shadow and image blur at density 1/2/3 (P1-13).
- [ ] Screen reader walkthrough of header/alert/video (P2-1).
- [ ] 20 app reloads on one surface: no XComponent/EGL growth (P2-6).
- [ ] Rotation + density change (P2-7).
- [ ] EGL init on API 20/23/26 (P2-13).
- [ ] Large/tall shader target (P2-11).
- [ ] 100k create/delete cycles: no memory growth, bounded per-frame scan (P2-15).
- [ ] Sheet-sized `NavStack` mid-push: no page painted outside (P2-16).
- [ ] Inset 0-blur focus ring (P2-17).
- [ ] Scroll on a mask/shader-heavy page: compositor work scales with visible nodes (P2-18).
- [ ] `pagePosition` under a transformed and a pinned view (P3-8).
- [ ] Stale/duplicate mount ops keep the app running (P3-9).
- [ ] Keyboard accessory and large title (P3-10).

## Suggested PR split

One change per PR, with the tests named above:

1. `cli: Record entry chunks in app.json for every target` (P0-1; touches `nsh-cli`,
   `nsh-bundler` tests, and the OHOS loader's manifest-only read).
2. `ohos: Enforce the app's declared capabilities beside the host grant` (P0-2).
3. `ohos: Size shader targets by the view box and capture overflow` (P0-3, P2-11).
4. `ohos: Keep shader layers alive in scene composition` (P0-4).
5. `ci: Build, lint and test the OHOS backend` (P0-5).
6. `core: Enable nsh/mobile's TabBar on OHOS` (P1-1) or remove the dead implementation.
7. `ohos: Manage window system bars by default and document the option` (P1-2).
8. `ohos: Forward paste, caret and composition events; read-only inputs` (P1-3).
9. `ohos: Take navigation snapshots and teardown off the UI thread` (P1-4).
10. `ohos: Follow the window's refresh rate and resume the frame clock` (P1-5).
11. `ohos: Cache compiled shader programs and cap fill surfaces` (P1-6).
12. `ohos: Present action sheets as sheets` (P1-7).
13. `ohos: One result per host command, with stable failure codes` (P1-8).
14. `runtime: Report "ohos" from os.platform and the updater` (P1-9).
15. `ohos: Capture a shader's backdrop` (P1-10).
16. `ohos: System-looking navigation, alerts and video chrome` (P2-1).
17. `ohos: Type the Rust-ArkTS system protocol` (P2-2).
18. `ohos: Read picked files off the UI thread` (P2-3).
19. `ohos: Budget IO per surface` (P2-4).
20. `engine: Share one network transport` (P2-5).
21. `ohos: Drain shader retirement before teardown` (P2-6).
22. `ohos: Track window size and density in the environment` (P2-7).
23. `ohos: Serve app UI sets; reserve nsh/* component names` (P2-8).
24. `docs: Correct the OHOS server and capability documentation` (P2-9, P2-10, P3-7).
25. `ohos: Follow each layout-animation scope` (P1-11).
26. `ohos: Transition text colors` (P1-12).
27. `ohos: Use ArkUI's blur and shadow units` (P1-13).
28. `ohos: Reclaim NodeId-keyed state` (P2-15).
29. `ohos: Clip Stack, Image and Input children` (P2-16).
30. `ohos: Draw inset shadows without blur` (P2-17).
31. `ohos: Rebuild scenes and composites only when dirty` (P2-18).
32. `ohos: Match the other backends' mount tolerance` (P3-8, P3-9, P3-10).
33. `build: Build the OHOS SDK against released ohos bindings` (P0-6; after the binding release or
    after P1-4 removes the snapshot consumer).

## What was checked and is sound

- The shader uniform/builtin layout matches `nsh-core` (32-byte block, `progress`, `overflow`);
  the GLSL ES translation is the core's and needs no OHOS-specific copy.
- Every `MountItem` and core `Command` variant is matched by the mounter; `TreeOrder` and
  `ProjectStates` are intentionally not requested; stale `TrackStates` and `Insert.before` for a
  sibling not yet inserted are tolerated (deferred updates; insert-last fallback). Its semantics
  differ for repeated `LayoutAnimation` items and text recolor (P1-11, P1-12), and its clipping and
  fatal-error policy differ from the other mounters (P2-16, P3-9).
- The render/mount pass confirmed the px/vp split for layout, position, transforms and hit testing,
  `ResponseRegion`/hitSlop mapping, the `HitTestMode` mapping, the monotonic frame clock, and the
  navigation/transform ordering. The exceptions are the blur/shadow units (P1-13) and the
  `pagePosition` derivation (P3-8).
- Core motion is reused where it exists: navigation push/pop and insets, hit areas, layout
  transitions/keyframes, the animation graph, scroll config/sticky/reports, the selection model,
  SVG, shader compilation and the image disk cache.
- Text is shaped and measured on the engine thread; the UI thread receives owned drawing commands.
- One engine per surface, one TSFN hop per commit, bounded IO admission, panic guards on ArkUI
  callbacks and executor jobs, and generation tokens for loads and shader completions.
- The dev server path is feature-gated and does not weaken release capability checks; signed
  packages use the shared verifier off the UI thread.
