# Native widget mouse callbacks

Build the embedding example from the repository root:

```sh
NSH_EXAMPLE=examples/embed-ohos/tests/native-pointer NSH_NATIVE_POINTER_E2E=1 \
  examples/embed-ohos/build.sh
```

Sign and install the HAP on the windowed MateBook emulator, then run:

```sh
python examples/embed-ohos/tests/check-native-pointer.py \
  --target 127.0.0.1:10090 --artifacts target/ohos/native-pointer
```

The build flag temporarily copies this fixture's HostBadge into the host and
restores the ordinary sample when the build exits. The fixture badge opts into
SDK `onMouse` and propagating `onAxisEvent` probes. Each surface has an ordinary
badge and a grayscale texture export containing the same native widget. Plain
counters distinguish left/right press and release, pointer movement, native
clicks and SDK axis callbacks. Identical wheel commands compare each export's
axis delta with the ordinary widget on the same surface. The 16 cases cover four
widgets with hover, left click, right click and wheel input. The script records all
cases before reporting differences and checks that events remain isolated to the
selected widget and surface.

The badge's default behavior and the ordinary surfaces fixture remain unchanged.
The script selects only its explicit device target and never builds, signs or
installs an app.

TODO: physical modifier keys, dragging, cancellation, simultaneous buttons and
execution on actual API-20/23 systems.
