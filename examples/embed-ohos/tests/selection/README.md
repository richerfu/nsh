# OHOS text selection fixture

This fixture covers the API-20 selection path on ordinary rasterized Text.
The engine supplies owned grapheme/range geometry; the standard Ability
ArkUI plugin supplies the native menu. No fixture-specific ArkTS renderer
or NSH plugin is needed.

Build the native SDK first, then assemble the acceptance HAP:

```sh
NSH_CLIPBOARD_E2E=1 NSH_PROFILE=selection-api26 \
NSH_EXAMPLE=examples/embed-ohos/tests/selection \
bash examples/embed-ohos/build.sh
```

`NSH_CLIPBOARD_E2E=1` temporarily adds the `READ_PASTEBOARD` declaration,
reason and used scene for the read-back assertion. The script restores the
ordinary example manifest on exit. Sign the HAP with a development profile
whose ACL allows `ohos.permission.READ_PASTEBOARD`, then install it on the
windowed MateBook Pro DevEco emulator. Obtain its
`applicationInfo.accessTokenId` with `hdc -t <emulator> shell bm dump -n com.example.nsh`
and grant that test application's user permission:

```sh
hdc -t 127.0.0.1:10090 shell atm perm -g -i <token-id> -p ohos.permission.READ_PASTEBOARD
uv run --with pillow python examples/embed-ohos/tests/check-selection.py \
  --target 127.0.0.1:10090 --scale 1.9
```

Use the current emulator HDC target and display density; the values above
match the MateBook Pro API-26 profile. The suite verifies the emulator's
product and 2-in-1 device type before injecting input. It never signs,
installs or grants permissions itself. Ordinary interaction and rendering
acceptance use this DevEco emulator; QEMU is reserved for accessibility.

The suite checks native word Copy, dismissal/reopening, a trailing-handle
extension, Select All including emoji/combining text/Chinese, editor-focus
cleanup and text removal/recreation. Screenshots and the latest tree go to
`target/ohos/selection-regression/`.

The [edge-scroll fixture](../selection-scroll/README.md) also checks held-handle
scrolling in both directions and offscreen range copying on MateBook.
TODO: transformed/scrolled
selection, toggling `selectable`, leading handles and crossing, RTL and multiline/ellipsis native matrices, inline attachments,
keyboard shortcuts, screen-reader selection, both simultaneous surfaces,
actual API-20/23 systems, and native memory/frame measurements. Portable
tests cover grapheme boundaries, UTF-16 ranges, RTL edges, clipping, attachment
omission, ligature highlight merging, resource limits and menu generations.
