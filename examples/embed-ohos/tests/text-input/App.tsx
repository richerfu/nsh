import { Input, Scroll, Text, View, mount, signal } from "nsh";

function App() {
  const policy = signal<"never" | "always" | "handled">("never");
  const focused = signal(false), presses = signal(0), checked = signal(false);
  const pressIn = signal(0), pressOut = signal(0);
  const underlineOffset = signal(4);
  const choice = (value: "never" | "always" | "handled") =>
    <View testID={`policy-${value}`} onPress={() => policy.set(value)} style={{ flex: 1, height: 32, backgroundColor: "#dbeafe" }}><Text>{value}</Text></View>;
  return <View style={{ flex: 1, padding: 12, gap: 8, backgroundColor: "white" }}>
    <Text testID={`input:${policy()}:${focused()}:${presses()}`}>{`${policy()}, focus ${focused()}, presses ${presses()}`}</Text>
    <Text testID={`feedback:${pressIn()}:${pressOut()}`}>{`Feedback ${pressIn()}:${pressOut()}`}</Text>
    <View style={{ flexDirection: "row", gap: 8 }}>{choice("never")}{choice("always")}{choice("handled")}</View>
    <Scroll testID="input-scroll" keyboardShouldPersistTaps={policy()} keyboardDismissMode="on-drag" style={{ height: 180, backgroundColor: "#f3f4f6" }}>
      <Input testID="editor" placeholder="Native editor" onFocus={() => focused.set(true)} onBlur={() => focused.set(false)} style={{ height: 40, borderWidth: 1 }} />
      <View testID="inside-button" onPressDown={() => pressIn.set(pressIn() + 1)} onPressUp={() => pressOut.set(pressOut() + 1)} onPress={() => presses.set(presses() + 1)} style={{ height: 48, backgroundColor: "#bbf7d0" }}><Text>Handled press</Text></View>
      <View testID="blank-pad" style={{ height: 80 }} />
      <View style={{ height: 180 }} />
    </Scroll>
    <Text testID="decoration-solid" style={{ fontSize: 24, lineHeight: 42, textDecorationLine: "underline", textDecorationThickness: 3, textUnderlineOffset: underlineOffset(), textDecorationColor: "#ef4444" }}>Decorated text</Text>
    <Text testID="decoration-wave" style={{ fontSize: 24, lineHeight: 42, textDecorationLine: "underline", textDecorationStyle: "wavy", textDecorationThickness: 2, textUnderlineOffset: 2, textDecorationColor: "#3b82f6" }}>Wave underline</Text>
    <View testID={`decoration-offset:${underlineOffset()}`} onPress={() => underlineOffset.set(8)} style={{ height: 32, backgroundColor: "#dbeafe" }}><Text>Move underline</Text></View>
    <View testID="a11y-checkbox" accessible={true} role="checkbox" accessibilityLabel="Example choice" accessibilityHint="Toggles the choice"
      accessibilityState={{ checked: checked(), selected: checked() }} onPress={() => checked.set(!checked())}
      style={{ height: 44, backgroundColor: checked() ? "#bbf7d0" : "#e5e7eb" }}><Text>{checked() ? "Checked" : "Unchecked"}</Text></View>
  </View>;
}
mount(App);
