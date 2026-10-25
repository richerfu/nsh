# Selection edge-scroll regression

This fixture checks held text-selection scrolling on the windowed MateBook Pro
DevEco emulator. QEMU is reserved for accessibility acceptance.

Build the OHOS native library into `target/ohos/selection-scroll`, then assemble
the fixture with the temporary clipboard read-back permission:

```sh
NSH_CLIPBOARD_E2E=1 NSH_PROFILE=selection-scroll \
NSH_EXAMPLE=examples/embed-ohos/tests/selection-scroll \
bash examples/embed-ohos/build.sh
```

Sign/install the HAP with the READ_PASTEBOARD ACL and grant the test application's
permission as described in [the selection fixture](../selection/README.md). Run:

```sh
uv run --with pillow python examples/embed-ohos/tests/check-selection-scroll.py \
  --target 127.0.0.1:10090 --scale 1.9
```

The suite verifies the DevEco 2-in-1 target before injecting input. It selects a
word, drags a handle to an edge, and holds it there with `uinput`. Two snapshots
after movement finishes must show continued scrolling. Both directions stop
after release; Copy includes the final line with nonzero top/bottom content insets. It also checks
text retirement during a held drag and restored native scroll interaction. Evidence is retained in `target/ohos/selection-scroll-regression`.

TODO: horizontal and nested scrolling, zoom/RTL/transforms, a second contact,
background/detachment during a held drag, and memory/frame
profiling. Pure tests cover edge velocity, short viewports, invalid values,
content bounds, bounded frame time after suspension and yielding at rounded
native limits.
