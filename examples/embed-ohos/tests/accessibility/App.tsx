import { Scroll, Text, View, mount, signal, type YNode } from "nsh";
import { announce, setAccessibilityFocus } from "nsh/accessibility";

function App() {
  const checked = signal(false), count = signal(5), event = signal("none");
  let choice!: YNode;
  const button = (id: string, label: string, action: () => void) =>
    <View testID={id} accessible role="button" accessibilityLabel={label} onPress={action}
      style={{ minHeight: 44, padding: 8, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  return <Scroll style={{ flex: 1, backgroundColor: "white" }}>
    <View style={{ padding: 12, gap: 10 }}>
      <Text role="header" style={{ fontSize: 22 }}>Accessibility acceptance</Text>
      <Text testID={`a11y-event:${event()}`}>{`Last action: ${event()}`}</Text>
      <View ref={choice} testID="a11y-choice" accessible role="checkbox"
        accessibilityLabel="Example choice" accessibilityHint="Double tap to toggle"
        accessibilityState={{ checked: checked(), selected: checked() }}
        accessibilityActions={[{ name: "toggle", label: "Toggle choice" }, { name: "activate" }]}
        onPress={() => { checked.set(!checked()); event.set("press"); }}
        onAccessibilityAction={value => { checked.set(!checked()); event.set(value.actionName); }}
        style={{ minHeight: 48, padding: 10, backgroundColor: checked() ? "#bbf7d0" : "#e5e7eb" }}>
        <Text>{checked() ? "Checked" : "Unchecked"}</Text>
      </View>
      <View testID="a11y-range" accessible role="adjustable" accessibilityLabel="Volume"
        accessibilityValue={{ min: 0, max: 10, now: count() }}
        accessibilityActions={[{ name: "increment", label: "Increase volume" }, { name: "decrement", label: "Decrease volume" }]}
        onAccessibilityAction={value => { count.set(Math.max(0, Math.min(10, count() + (value.actionName === "increment" ? 1 : -1)))); event.set(value.actionName); }}
        style={{ minHeight: 48, padding: 10, backgroundColor: "#eef2ff" }}><Text>{`Volume ${count()}`}</Text></View>
      {button("a11y-polite", "Update polite region", () => count.set((count() + 1) % 11))}
      <Text testID={`a11y-live:${count()}`} ohos={{ accessibilityLiveRegion: "polite" }}>{`Current volume ${count()}`}</Text>
      {button("a11y-assertive", "Update assertive region", () => event.set(`urgent-${Date.now()}`))}
      <Text ohos={{ accessibilityLiveRegion: "assertive" }}>{`Latest event ${event()}`}</Text>
      {button("a11y-focus", "Focus the checkbox", () => setAccessibilityFocus(choice))}
      {button("a11y-announce", "Announce a message", () => announce("Native Shell announcement"))}
      <View accessibilityElementsHidden><Text>Hidden text must not receive accessibility focus</Text></View>
    </View>
  </Scroll>;
}
mount(App);
