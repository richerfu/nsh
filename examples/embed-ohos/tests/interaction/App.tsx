import { Scroll, Text, View, mount, signal, type YNode } from "nsh";

function App() {
  const touch = signal("none"), zoom = signal(1), samples = signal(0), offset = signal(0);
  let zoomer!: YNode, snap!: YNode, center!: YNode;
  const button = (id: string, label: string, action: () => void) =>
    <View testID={id} onPress={action} style={{ height: 32, flex: 1, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  const report = (name: string, x: number, y: number) => touch.set(`${name}:${Math.round(x)}:${Math.round(y)}`);
  return <View style={{ flex: 1, padding: 12, gap: 8, backgroundColor: "white" }}>
    <Text testID={`hit:${touch()}`}>{touch()}</Text>
    <View style={{ height: 100, paddingLeft: 70, paddingTop: 20 }}>
      <View testID="rotated-pad" onTouchEnd={e => report("rotated", e.locationX, e.locationY)}
        style={{ width: 100, height: 60, backgroundColor: "#ef4444", transform: [{ rotate: "30deg" }, { scale: 1.2 }] }} />
      <View testID="clip-box" hitSlop={{ right: 40 }} style={{ position: "absolute", left: 260, top: 20, width: 100, height: 60, overflow: "hidden", backgroundColor: "#f3f4f6" }}>
        <View onTouchEnd={e => report("clip", e.locationX, e.locationY)} style={{ width: 100, height: 60, backgroundColor: "#a855f7", transform: [{ translateX: 60 }] }} />
      </View>
    </View>
    <View style={{ flexDirection: "row", gap: 8 }}>
      {button("zoom-animated", "Animate zoom", () => { samples.set(0); zoomer.zoomTo({ scale: 2, x: 100, y: 50, animated: true }); })}
      {button("zoom-reset", "Reset zoom", () => zoomer.zoomTo({ scale: 1, animated: false }))}
      {button("snap-reset", "Reset snap", () => snap.scrollTo({ y: 0, animated: false }))}
      {button("snap-jump", "Animate to 500", () => snap.scrollTo({ y: 500, animated: true }))}
      {button("center-zoom", "Center at 0.5", () => center.zoomTo({ scale: 0.5, animated: false }))}
    </View>
    <Text testID={`zoom:${zoom().toFixed(2)}:${samples()}`}>{`Scale ${zoom().toFixed(2)}, samples ${samples()}`}</Text>
    <Scroll ref={zoomer} testID="zoom-scroll" minimumZoomScale={1} maximumZoomScale={3}
      onScroll={e => { if (Math.abs(e.zoomScale - zoom()) > 0.001) { samples.set(samples() + 1); zoom.set(e.zoomScale); } }}
      style={{ height: 100, backgroundColor: "#fef3c7" }}>
      <View style={{ width: 400, height: 300, paddingLeft: 70, paddingTop: 20 }}>
        <View testID="zoomed-pad" onTouchEnd={e => report("zoomed", e.locationX, e.locationY)} style={{ width: 100, height: 60, backgroundColor: "#22c55e" }} />
      </View>
    </Scroll>
    <Text testID={`snap:${Math.round(offset())}`}>{`Snap offset ${Math.round(offset())}`}</Text>
    <Scroll ref={snap} testID="snap-scroll" snapToInterval={100} disableIntervalMomentum={true} decelerationRate="fast"
      onScroll={e => offset.set(e.offset.y)} style={{ height: 100, backgroundColor: "#e0e7ff" }}>
      <Each of={Array.from({ length: 20 }, (_, i) => i)}>{i => <Text style={{ height: 50 }}>{`Snap row ${i()}`}</Text>}</Each>
    </Scroll>
    <Scroll ref={center} testID="center-scroll" centerContent={true} minimumZoomScale={0.5} maximumZoomScale={2} style={{ height: 80, backgroundColor: "#f3f4f6" }}>
      <View testID="centered-pad" onTouchEnd={e => report("centered", e.locationX, e.locationY)} style={{ width: 80, height: 40, backgroundColor: "#3b82f6" }} />
    </Scroll>
  </View>;
}
mount(App);
