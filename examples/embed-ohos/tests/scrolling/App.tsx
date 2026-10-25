import { mount, signal, View, Text, Scroll, type YNode } from "nsh";
import { fontScale } from "nsh/device";
import { List } from "nsh/list";
const refresh = signal(0), touches = signal("none"), zoom = signal(1), offset = signal(0), refreshing = signal(false);
function App() {
  let scroll!: YNode;
  let zoomer!: YNode;
  let list!: YNode;
  const button = (id: string, label: string, tap: () => void) => <View testID={id} onPress={tap} style={{ height: 36, flex: 1, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  return <View style={{ flex: 1, padding: 12, gap: 6, backgroundColor: "white" }}>
    <Text testID={`scroll-state:${Math.round(offset())}:${refresh()}:${refreshing()}:${zoom()}`}>{`offset ${Math.round(offset())}, refresh ${refresh()}, zoom ${zoom()}`}</Text>
    <Text testID={`font:${fontScale()}`}>{`Font scale ${fontScale()}`}</Text>
    <View testID="touch-pad" onTouchStart={() => touches.set("start")} onTouchMove={() => touches.set("move")} onTouchEnd={() => touches.set("end")} onTouchCancel={() => touches.set("cancel")} style={{ height: 36, backgroundColor: "#bbf7d0" }}><Text>Touch path</Text></View>
    <Text testID={`touch:${touches()}`}>{touches()}</Text>
    <View style={{ flexDirection: "row", gap: 8 }}>
    {button("scroll-jump", "Scroll to 300", () => scroll.scrollTo({ y: 300, animated: false }))}
    {button("zoom-double", "Zoom to 2", () => zoomer.zoomTo({ scale: 2, x: 50, y: 50, animated: false }))}
    {button("zoom-reset", "Reset zoom", () => zoomer.zoomTo({ scale: 1, animated: false }))}
    {button("refresh-finish", "Finish refresh", () => refreshing.set(false))}
    {button("sticky-jump", "Sticky 300", () => list.scrollToOffset({ offset: 300, animated: false }))}
    </View>
    <Scroll ref={scroll} testID="native-scroll" onScroll={e => offset.set(e.offset.y)} onRefresh={() => { refresh.set(refresh() + 1); refreshing.set(true); }} refreshing={refreshing()} snapToInterval={100} style={{ height: 90, backgroundColor: "#eef2ff" }}>
      <Each of={Array.from({ length: 15 }, (_, i) => i)}>{i => <Text testID={`scroll-row:${i()}`} style={{ height: 50 }}>{`Row ${i()}`}</Text>}</Each>
    </Scroll>
    <Scroll ref={zoomer} testID="zoom-scroll" minimumZoomScale={1} maximumZoomScale={3} onScroll={e => zoom.set(e.zoomScale)} style={{ height: 90, backgroundColor: "#fef3c7" }}>
      <View style={{ width: 400, height: 300, gap: 12 }}><Text style={{ fontSize: 24 }}>Zoom both axes</Text><View style={{ width: 100, height: 100, backgroundColor: "#ef4444" }} /></View>
    </Scroll>
    <List ref={list} testID="sticky-list" each={Array.from({ length: 30 }, (_, i) => i)} stickyHeaderIndices={[0, 5, 10]} estimatedItemSize={44} style={{ flex: 1, minHeight: 80 }}>
      {i => <View testID={`sticky-row:${i()}`} style={{ height: 44, backgroundColor: i() % 5 === 0 ? "#a7f3d0" : "#f3f4f6" }}><Text>{`Sticky row ${i()}`}</Text></View>}
    </List>
  </View>;
}
mount(App);
