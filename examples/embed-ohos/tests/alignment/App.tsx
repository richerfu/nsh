import { mount, signal, View, Text, Input, type YNode } from "nsh";
import { LayoutAnimation } from "nsh/animated";
import { keymap } from "nsh/keymap";
import { announce, setAccessibilityFocus } from "nsh/accessibility";
import { createNavStack, NavPage, NavStack } from "nsh/base";

function App() {
  const nav = createNavStack([{ page: "main" }]);
  const large = signal(false);
  const shown = signal(true);
  const keys = signal(0);
  const inputKey = signal("none");
  const hidden = signal(false);
  let focus!: YNode;
  keymap.bind("ctrl+k", () => keys.update(v => v + 1), { when: "always" });
  const change = () => {
    LayoutAnimation.configureNext({
      duration: 2200,
      create: { type: "linear", property: "opacity" },
      update: { type: "linear" },
      delete: { type: "linear", property: "opacity" },
    });
    large.update(v => !v);
    shown.update(v => !v);
  };
  const button = (id: string, title: string, press: () => void) =>
    <View testID={id} onPress={press} style={{ height: 36, backgroundColor: "#dbeafe", padding: 4 }}><Text>{title}</Text></View>;
  return <NavStack state={nav} style={{ flex: 1 }}>
    {() => <NavPage headerVisible={false} statusBar={{ hidden: hidden(), animated: true, foreground: "dark" }}>
      <View style={{ flex: 1, padding: 8, gap: 5, backgroundColor: "white" }}>
        {button("layout-toggle", "Animate layout / removal", change)}
        {button("status-toggle", "Toggle status bar", () => hidden.update(v => !v))}
        {button("accessibility-announce", "Announce", () => announce("Native Shell accessibility"))}
        {button("accessibility-focus", "Focus accessible target", () => setAccessibilityFocus(focus))}
        <Text testID={`keys:${keys()}`}>Ctrl+K count: {keys()}</Text>
        <Input testID="keyboard-editor" placeholder="Keyboard fixture" onKeyPress={e => inputKey.set(e.key)} />
        <Text testID={`input-key:${inputKey()}`}>Input key: {inputKey()}</Text>
        <View testID="layout-target" style={{ width: large() ? 240 : 80, height: 34, backgroundColor: "#ef4444" }} />
        <View testID="transition-target" style={{ width: 100, height: 34, backgroundColor: large() ? "#2563eb" : "#f97316", opacity: large() ? 0.4 : 1,
          transform: [{ translateX: large() ? 80 : 0 }], transitionProperty: "all", transitionDuration: 2200, transitionTimingFunction: "linear" }} />
        <If when={shown()}><View testID="presence-target" style={{ height: 34, backgroundColor: "#16a34a" }} /></If>
        <View testID="keyframe-target" style={{ width: 40, height: 40, backgroundColor: "#7c3aed", animationName: "spin", animationDuration: 2000, animationIterationCount: "infinite", animationTimingFunction: "linear" }} />
        <View ref={focus} testID="accessibility-target" accessible role="button" accessibilityLabel="Alignment target" style={{ height: 34 }}><Text>Accessible target</Text></View>
      </View>
    </NavPage>}
  </NavStack>;
}
mount(App);
