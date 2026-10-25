import { Scroll, Text, View, mount, signal, type YNode } from "nsh";
import { Clipboard } from "nsh/clipboard";

function App() {
  let scroll!: YNode;
  const offset = signal(0), copied = signal("unset"), shown = signal(true), retire = signal(false);
  const text = Array.from({ length: 40 }, (_, i) => `Line ${String(i).padStart(2, "0")} alpha bravo charlie.`).join("\n");
  return <View style={{ flex: 1, padding: 12, gap: 8, backgroundColor: "white" }}>
    <View testID="read-copy" onPress={async () => copied.set(await Clipboard.readText())} style={{ padding: 8, backgroundColor: "#dbeafe" }}><Text>Read clipboard</Text></View>
    <View testID="remove-text" onPress={() => shown.set(!shown())} style={{ padding: 8, backgroundColor: "#dbeafe" }}><Text>Remove / restore text</Text></View>
    <View testID="jump-scroll" onPress={() => scroll.scrollTo({ y: 600, animated: false })} style={{ padding: 8, backgroundColor: "#dbeafe" }}><Text>Jump to 600</Text></View>
    <View testID="retire-on-scroll" onPress={() => retire.set(true)} style={{ padding: 8, backgroundColor: "#dbeafe" }}><Text>Retire on next scroll</Text></View>
    <Text testID={`offset:${Math.round(offset())}`}>{`Scroll offset: ${Math.round(offset())}`}</Text>
    <Scroll ref={scroll} testID="selection-scroll" contentInset={{ top: 12, bottom: 16 }} onScroll={e => { offset.set(e.offset.y); if (retire() && e.offset.y > 1) { retire.set(false); shown.set(false); } }} style={{ height: 140, backgroundColor: "#f1f5f9" }}>
      <If when={shown()}><Text testID="scroll-text" selectable style={{ selectionColor: "#4499ff66", fontSize: 18, lineHeight: 24 }}>{text}</Text></If>
    </Scroll>
    <Text testID={`copied:${copied()}`}>{`Copied: ${copied()}`}</Text>
  </View>;
}
mount(App);
