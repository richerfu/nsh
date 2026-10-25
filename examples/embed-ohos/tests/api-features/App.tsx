import { Input, Text, View, mount, signal } from "nsh";

function App() {
  const angle = signal(120), checked = signal(false), live = signal(0), action = signal("none");
  return <View style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "white" }}>
    <View testID="hue-host" onPress={() => angle.set(angle() === 120 ? 0 : 120)}
      style={{ width: 180, height: 72, filter: `hue-rotate(${angle()}deg)` }}>
      <View testID="hue-child" style={{ width: 180, height: 72, backgroundColor: "#ff0000" }} />
    </View>
    <Text testID={`hue:${angle()}`}>{`Hue ${angle()}`}</Text>
    <View testID="layered-shadow" style={{ margin: 12, width: 160, height: 48, backgroundColor: "white", boxShadow: "8px 0 0 0 #ff0000, 0 8px 0 0 #0000ff" }} />
    <View testID="inset-shadow" style={{ width: 180, height: 60, backgroundColor: "white", borderRadius: 8, boxShadow: "inset 8px 0 0 0 #0000ff" }} />
    <Input testID="caps-editor" autoCapitalize="words" placeholder="Native capitalization" style={{ height: 42, borderWidth: 1 }} />
    <Input testID="decimal-editor" multiline keyboardType="decimal-pad" placeholder="Multiline decimals" style={{ height: 54, borderWidth: 1 }} />
    <View testID="feature-choice" accessible role="checkbox" accessibilityLabel="Feature choice"
      accessibilityState={{ checked: checked(), selected: checked() }}
      accessibilityActions={[{ name: "toggle", label: "Toggle choice" }]}
      onAccessibilityAction={event => { action.set(event.actionName); checked.set(!checked()); }}
      onPress={() => checked.set(!checked())} style={{ height: 44, backgroundColor: checked() ? "#bbf7d0" : "#e5e7eb" }}>
      <Text>{checked() ? "Checked" : "Unchecked"}</Text>
    </View>
    <Text testID={`action:${action()}`}>{action()}</Text>
    <View testID="live-increment" onPress={() => live.set(live() + 1)} style={{ height: 40, backgroundColor: "#dbeafe" }}><Text>Update live region</Text></View>
    <Text testID={`live:${live()}`} ohos={{ accessibilityLiveRegion: "polite" }}>{`Live count ${live()}`}</Text>
    <View testID="provider-range" accessible role="adjustable" accessibilityLabel="Level"
      accessibilityValue={{ min: 0, max: 10, now: live() }} accessibilityActions={[{ name: "increment" }, { name: "decrement" }]}
      onAccessibilityAction={event => live.set(live() + (event.actionName === "increment" ? 1 : -1))}
      style={{ height: 32, backgroundColor: "#eef2ff" }}><Text>Adjustable level</Text></View>
  </View>;
}
mount(App);
