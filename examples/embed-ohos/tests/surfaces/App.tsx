import { mount, signal, Text, View, type YNode } from "nsh";
import { nativeComponent } from "nsh/native";

const Badge = nativeComponent<{ text: string }, { tap: { count: number } }>("embed.badge", {
  props: ["text"], events: ["tap"],
});

// The native hierarchy exposes state and viewport size to the device regression.
function App() {
  const count = signal(0);
  const size = signal("pending");
  const taps = signal(0);
  let badge: YNode | undefined;
  return <View
    testID={`viewport:${size()}`}
    onLayout={event => size.set(`${Math.round(event.size.width)}:${Math.round(event.size.height)}`)}
    style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "#eef2ff" }}>
    <Text testID={`count:${count()}`} style={{ fontSize: 28 }}>{`Count ${count()}`}</Text>
    <View testID="increment" onPress={() => count.set(count() + 1)}
      style={{ height: 64, backgroundColor: "#bfdbfe", justifyContent: "center" }}>
      <Text>Increment this surface</Text>
    </View>
    <Text>{`Viewport ${size()}`}</Text>
    <Badge text={`Native taps ${taps()}`} ref={node => badge = node}
      onTap={event => { taps.set(event.count); badge?.command("pulse"); }} />
    <Text testID={`native-count:${taps()}`}>{`Native event ${taps()}`}</Text>
  </View>;
}

mount(App);
