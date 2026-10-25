# Secure multiline input

Build the embedding example with
`NSH_EXAMPLE=examples/embed-ohos/tests/secure-input`, sign it and install it
on the windowed MateBook emulator. Run from the repository root:

```sh
python3 examples/embed-ohos/tests/check-secure-input.py \
  --target 127.0.0.1:10090 --artifacts target/ohos/secure-native-regression
```

The API-26 MateBook run passed UTF-16 emoji selection/deletion, hardware input,
newline/submit behavior, keyboard paste, the system PasteButton menu, password
Copy/Cut preserving text and clipboard, maxLength, read-only state, independent
surfaces and five focused removal/recreation cycles. The fixture exposes lengths
and selection offsets for assertions; passwords never appear in UI text. Wait
for the requested focus state before issuing a hardware shortcut.

The editor uses Native Shell's SDK menu fragment and the Ability clipboard plugin. PasteButton
grants foreground clipboard access according to system policy. Keyboard paste can
receive a single-use grant; it does not imply permission for unrelated reads.
The fixture also switches an edited value between single-line and multiline secure input, then edits each replacement. This covers inheriting a nonzero native edit count. The script never signs, installs or selects another target.

TODO: actual API-20/23 execution, full software-IME composition, autofill,
scrolling/RTL, focus/foreground interruption and empty-clipboard native cases.
Secure-field accessibility, selection and audible speech require the QEMU a11y
suite; this fixture does not validate accessibility on the MateBook emulator.
