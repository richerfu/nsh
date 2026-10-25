# Native accessibility regression fixture

This fixture exercises the system screen reader with native TextInput/TextArea,
WebView document nodes and Video controls. Each embedding surface keeps its own
mode and event state. The video is a local raw resource; WebView uses an inline
document and reports its button action through the native message bridge.

Build the fixture with a previously built API-26 SDK from the embedding example:

```sh
NSH_PROFILE=debug-api-26 \
  NSH_EXAMPLE=examples/embed-ohos/tests/accessibility-device ./build.sh
```

Use the project's device development signature and an explicitly selected HDC
target when installing the resulting signed HAP. Record the original screen
reader setting, enable the system reader, open Native Shell and activate the
host's **Single** button. Restore the recorded setting when the run finishes.

The **Input fixture**, **Web fixture** and **Video fixture** buttons switch the
current surface. Touch exploration sets accessibility focus, a fast horizontal
swipe moves between semantic elements, and a double tap activates the focused
element. The mode and event state are exposed as `device-mode:*` and
`device-state:*` test IDs for layout assertions.

- Input: move between the native single-line and multiline editors in both
  directions, activate each editor, commit text through the IME and compare the
  native editor text with the `device-state:text:*` or
  `device-state:multiline:*` value. Commit an IME candidate before asserting the
  app value; composition text can remain in the keyboard until confirmation.
- WebView: explore **Web action**, move to the following DOM label and back,
  then activate the button. The expected event state is `web-activated`.
- Video: explore **Play**, move to the native progress Slider and back, activate
  Play and observe progress, then activate Pause and observe the Play label and
  stable progress value.
- Explicit announcement: activate the announcement button and inspect the
  system event separately from audible speech and interruption ordering.

Keep the system reader's focus/action log and before/after layout snapshots with
each result. Mark unexecuted combinations and audible speech checks as TODO.
The QEMU acceptance service and its direction-query diagnostic have their own
runner, `../check-qemu-accessibility.py`.
