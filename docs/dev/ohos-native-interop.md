# Native component and subtree interop

Each NshSurface owns a native tree. WebView, editors, host widgets, video and
exported subtrees participate in its scroll clips, transforms, sibling order,
modals and disposal. The host still lays out its ContentSlots with ordinary
ArkUI containers.

## Owners and native edges

Ability owns `ContentRoot` and `NativeAttachment`, using the same ArkUI binding
source as NSH. ContentRoot retains its NodeContent, native root and attachment
listener on the UI thread. NativeAttachment retains an owned native container
and any borrowed SDK FrameNode/NodeContent until detachment succeeds. Target IDs
are monotonic; retirement permanently closes a target.

The retained Ability WebView plugin owns its ArkWeb controller and BuilderNode.
NSH's `@nsh/ohos` HAR owns SDK TextInput/TextArea controls, selectable-text menus and
texture-producing builders. NSH owns layout, editing policy, semantic values,
composition, input routing and checked media resources. Video uses an NSH-owned
C XComponent/native window and a native AVPlayer binding.

Business native components keep their existing per-surface factory contract.
The returned business ObjectRef stays retained for props, commands, events and
the host's disposal callback. Its FrameNode attaches through NativeAttachment;
the borrowing edge is removed before calling the business disposal callback.
The native shell does not send platform objects to workers.

## Callback-scoped attachment

| Event | Request | Response |
|---|---|---|
| `native-node-ready` | `ohos.node.NativeNodeReady { targetId, content }` | `ohos.node.Acknowledgement { accepted }` |
| `native-subtree-ready` | `ohos.node.NativeSubtreeReady { targetId, content, slot }` | The same acknowledgement |
| `native-node-retiring` | `ohos.node.NativeNodeRetiring { targetId }` | The same acknowledgement after detachment |

Decoding is pure. The named UI-thread callback explicitly calls
`ready.attach(&event)?` before acknowledging readiness. No borrowed FrameNode
becomes a worker-safe result. A subtree target retains its outer container and
inner C-owned subtree, removes the inner edge before the wrapper edge, and
restores its event receiver while that edge is active.

`NativeAttachment::dispatch` guards callback reentry. `with_child` returns a
fallible optional result. Unknown, retired and reentrant targets return errors.
A new SDK owner after UIContext recreation receives a fresh target; controller
IDs do not authorize reuse of a permanently retired attachment.

## Retirement and failures

Stop admission and retire event routes before releasing native users. Retire
attachments, runtime/overlay owners and host widgets before unmounting the
ContentRoot. Surface disposal during a native callback is deferred to a later UI
turn. Removing a ContentSlot alone preserves a retained surface's VM.

Controller fields release their strong node handles before recursive native-tree
disposal. Independently held children (Video controls and the development overlay)
are detached from both the native and Rust child lists before individual disposal.
Refresh takes its optional node before disposal and restores that owner on failure.
Surface teardown releases its viewport handle before ContentRoot unmounts the
root; a weak route restores it if native unmount needs retry. Unattached targets
and targets without a UIContext retire immediately rather than waiting for an SDK
batch that cannot run.

Ability's synchronous `onClosing` hook removes borrowed native edges before
Rust session destruction. Its revocable cleanup context expires at hook return;
asynchronous resource disposal cannot retain that capability. Each independent
owner gets its cleanup attempt. A failed detach retains the complete native and
SDK/business owner for retry, and final-drop failure retains that owner rather
than releasing a still-borrowed child. Errors are not successful retirement ACKs.

WebView keeps a single pending native owner until SDK disposal completes and
returns its result to the UI queue. The binding rejects `ArkUINode::dispose`
while another strong `Rc` owns that node, including attachment owners. WebView
shares one container handle through its UI owner and keeps only a weak route to
its parent after detachment. A shutdown-time request error permits
only another final native-disposal attempt; owners remain retained if that
operation still fails. Successful SDK replacement clears old context targets
only while its returned target is still current.

Video retains each local file or private Ability-cache descriptor until native
ReleaseSync succeeds, and releases its player off the UI thread. Unrestricted
remote sources use AVPlayer URL streaming; restricted URLs stream to disk with
Rust redirect checks and the system proxy. Filesystem assets and `file://` sources
keep their original descriptors. HAP assets use bounded raw-file reads because
the pinned binding omits the descriptor's HAP offset. Cached files alone are
removed on retirement; original asset/local files remain in their stores. A strong reply route keeps its UI
queue alive until successful release acknowledges retirement; only then may the
UI release the XComponent/native-window owner. Release acknowledgements are
handled even after the engine has stopped. The retiring UI owner keeps its
parent alive through release acknowledgement, detaches its independently held
controls before disposing them, and finally disposes the parent if the NSH node
has gone away. SDK object retirement and native player retirement have distinct
completion barriers.

API-23 action descriptions use immutable process-lifetime C strings because
ArkUI shallow-copies their pointers without a final-copy notification. Distinct
labels share a bounded pool: 4 MiB of string data including terminators and
16,384 labels. Invalid NULs or exhaustion leave the pool unchanged; provider
snapshots fall back to the static action names. Custom actions use at most 16
entries. This bounds string payloads, not allocator metadata or process RSS.

## Composition and input

A texture producer in the NSH HAR exports the live subtree to a NativeImage
surface. NSH's compositor worker owns EGL/GLES state, sampling/readback and
ordered filters/blending through `glow` and `khronos-egl`. Bindings expose native
ownership and primitive SDK calls. GPU initialization reports its texture ID
asynchronously; the UI installs the producer on the subsequent ready turn. GPU
contexts are created, used and destroyed on their worker thread; native listeners use weak integer-token routing so late
callbacks cannot dereference released user data.

Each surface keeps its existing limits: 128 exported layers, eight megapixels
of source frames and four megapixels per source/filtered image. Plans and ready
messages coalesce by node lifetime, and equal source bytes suppress duplicate
publication. The current visible tree determines scene activation on every dirty
turn. The last scene trigger's removal clears published frames, drops exports
that are no longer needed and restores the native scroll tree. Native builders
retire through the existing attachment acknowledgement; local foreground-filter
exports can continue independently. Modal and scroll chrome exports retain their
own ordering and ownership. CSS filter radii use half the physical radius as
Gaussian sigma, with capture bounds padded by three sigma.

Typography stays on the engine thread. Immutable text metrics/selection geometry
cross with a single-use RGBA transfer; after UI upload, only the UI bitmap holds
the raster pixels. Long paragraphs use adaptive raster scale. The existing binding
keeps native Typography objects thread-confined, so zoomed text currently scales
that bitmap. Selection cache admission and local raster limits preserve engine
lifetime. System password TextInput owns every secure field, including requests
that also set `multiline`, within the API-20 editor contract.

Fetch, image and custom WebView subresource requests use bounded IO lanes.
Their messages contain owned bytes/metadata, never SDK objects. Image presentation
uses retained PixelMaps/DrawableDescriptors from raw encoded bytes, eliminating
base64 transfer strings.

| Input | Destination |
|---|---|
| Touchscreen | An owned API-15 native input clone enters the C subtree |
| Primary mouse-generated touch | Callback-scoped SDK TouchEvent enters the DISPLAY BuilderNode |
| Raw mouse and axis | ArkUI delivers its own events to live native controls |

Builder origins come from `getPositionToWindow()`, so nested parents do not
shift mouse-generated touch coordinates. Coordinate conversion is scoped to synchronous SDK posting and restored before
return. A mouse owner retains plain numeric Cancel data, not the original SDK
event. Retirement detaches native edges synchronously and defers SDK builder
release while a post is active. It never invokes an expired cleanup context.

## WebView runtime configuration

Register navigation callbacks, JS proxy methods, custom protocols and target
subscriptions before the first native creation. The controller ID scopes
business declarations; process-unique native tags reject callbacks from a
retired controller. No extra owner-key or controller generation is required.

CreateRequest owns the initial JavaScript access, UA and document-start scripts.
Existing-controller changes await document options, native settings and the UA
setter before loading a source. Same-source changes update the existing
controller and preserve history. Clearing caller initialization restores NSH's
bridge-only script; clearing UA calls the existing setter with an empty string.
The HTML base URL changes before `load_html`. Applied props advance only after
successful configuration, allowing failed updates to retry.

## Verification

The fixtures cover independent surfaces, nested scrolling/clipping,
transforms/overlap, modals, source replacement, host/plugin disposal, texture
pixels/input, controlled editing and media controls. See the
[current OHOS verification record](ohos.md#verification) and the
[composition fixture](../../examples/embed-ohos/tests/visual-composition/README.md).
Production host tests substitute platform effects; native builds and import
checks do not establish ArkWeb, IME, GPU pixels or AVPlayer runtime behavior.
Actual API-20/23 execution, full IME/transform/failure matrices and QEMU reader
coverage remain separate acceptance work.
