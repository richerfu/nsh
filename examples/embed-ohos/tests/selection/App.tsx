import { Input, Text, View, mount, signal } from "nsh";
import { Clipboard } from "nsh/clipboard";

function App() {
  const copy = signal("unset"), reads = signal(0), shown = signal(true), focused = signal(false);
  const button = (id: string, label: string, action: () => void) =>
    <View testID={id} onPress={action} style={{ padding: 5, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  return <View style={{ flex: 1, padding: 8, gap: 6, backgroundColor: "white" }}>
    <View style={{ flexDirection: "row", gap: 8 }}>
      {button("read-copy", "Read clipboard", async () => { copy.set(await Clipboard.getString()); reads.set(reads() + 1); })}
      {button("replace-text", "Remove / restore text", () => shown.set(!shown()))}
    </View>
    <Text testID={`selection-focus:${focused()}`}>Long press and drag a handle.</Text>
    <If when={shown()}>
      <Text testID="select-words" selectable selectionColor="#4499ff66" style={{ fontSize: 18, lineHeight: 24 }}>
        Alpha bravo Charlie. 👩🏽‍💻 Café 中文测试. A second line wraps for selection.
      </Text>
      <Text testID="select-rtl" selectable style={{ fontSize: 18, writingDirection: "rtl" }}>שלום עולם مرحبا</Text>
      <Text testID="select-plain">This ordinary text remains unselectable.</Text>
    </If>
    <Input testID="selection-editor" placeholder="Focus clears text selection" onFocus={() => focused.set(true)} onBlur={() => focused.set(false)} style={{ minHeight: 32, borderWidth: 1 }} />
    <Text testID={`copied:${reads()}:${copy()}`}>{`Copied: ${copy()}`}</Text>
  </View>;
}
mount(App);
