# Live subtree composition

Build the embedding example with
`NSH_EXAMPLE=examples/embed-ohos/tests/visual-composition`, sign it and install it
on the windowed MateBook emulator. Run from the repository root:

```sh
uv run --with pillow python examples/embed-ohos/tests/check-composition.py \
  --target 127.0.0.1:10090 --artifacts target/ohos/composition-regression
```

The pixel assertions cover colored shadows from disconnected opaque children,
transparent holes, nested filter exports, negative child overflow, a circular
Canvas source, grayscale after the shadow, chained backdrop filters, opaque and
semitransparent plus-darker, dynamic background and child movement, foreground
outline ordering, native touch scrolling, independent surfaces and five export
cycles. MMI mouse clicks assert exactly one counter increment on the target
surface, MMI wheel input preserves the other surface's scroll offset, and a held
mouse button spans engine retirement/recreation without affecting the new owner.
The existing button label exposes its pressed state so retirement starts only
after the left surface has received the held DOWN.
The script always releases that button, including when an assertion fails.
Pixel checks read the screen after real native texture export and filtering. The
script never signs, installs, or selects a different device.

The complete pixel/touch/scroll/export-cycle suite passed on the windowed API-26
MateBook emulator on 2026-10-20.

TODO: full transformed and rounded group clipping, nested backdrop roots, modal
composition, resizing, native WebView/Video/editor descendants, physical
mouse/axis/multi-touch, native GPU/memory profiling and actual API-20/23 execution.
