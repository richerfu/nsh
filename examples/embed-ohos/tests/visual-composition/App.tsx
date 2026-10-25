import { Scroll, Text, View, mount, signal } from "nsh";
import { Canvas } from "nsh/canvas";

function App() {
  const shifted = signal(false);
  const visible = signal(true);
  const background = signal(false);
  const composed = signal(false);
  const offset = signal(0);
  const presses = signal(0);
  const pressed = signal(false);
  return <View style={{ flex: 1, padding: 12, gap: 8, backgroundColor: "white" }}>
    <Text>Live native subtree composition</Text>
    <Scroll testID="composition-scroll" onScroll={event => offset.set(Math.round(event.y))} style={{ position: "absolute", left: 260, top: 50, width: 140, height: 180, backgroundColor: "#ffeecc" }}>
      <View style={{ height: 600, width: 140, backgroundColor: "#88ccaa" }}><Text>Native scroll chrome</Text></View>
    </Scroll>
    <Text testID={`composition-offset:${offset()}`} style={{ position: "absolute", left: 260, top: 240 }}>Scroll {offset()}</Text>
    <Text testID={`composition-presses:${presses()}`} style={{ position: "absolute", left: 260, top: 260 }}>Presses {presses()}</Text>
    <View testID="composition-outline" style={{ position: "absolute", left: 260, top: 280, width: 100, height: 80, outlineWidth: 4, outlineOffset: -4, outlineColor: "black" }}>
      <View style={{ flex: 1, backgroundColor: "white" }} />
    </View>
    <If when={visible()}>
      <View testID="alpha-shadow" style={{ width: 220, height: 100, filter: "drop-shadow(12px 8px 0px blue)" }}>
        <View style={{ position: "absolute", left: -12, top: -10, width: 20, height: 20, backgroundColor: "#aa00ff" }} />
        <View testID="alpha-child" style={{ width: 40, height: 40, backgroundColor: "red", filter: "drop-shadow(4px 4px 0px yellow)", marginLeft: shifted() ? 60 : 0 }} />
        <View style={{ width: 40, height: 30, backgroundColor: "#00aa00", marginLeft: 100 }} />
      </View>
      <Canvas testID="blurred-shadow" style={{ width: 220, height: 80, filter: "drop-shadow(8px 6px 3px #0000ff) grayscale(1)" }} draw={ctx => {
        ctx.fillStyle = "red"; ctx.beginPath(); ctx.arc(40, 35, 18, 0, Math.PI*2); ctx.fill();
      }} />
    </If>
    <View testID="backdrop-chain" style={{ width: 220, height: 60, backgroundColor: background() ? "#336699" : "#cc9933" }}>
      <View style={{ position: "absolute", left: 0, top: 0, width: 80, height: 60, backdropFilter: composed() ? "invert(1) brightness(0.5)" : "none" }} />
      <View style={{ position: "absolute", left: 120, top: 0, width: 60, height: 60, backgroundColor: "#6680e6", mixBlendMode: composed() ? "plus-darker" : "normal" }} />
      <View style={{ position: "absolute", left: 180, top: 0, width: 40, height: 60, backgroundColor: "#6680e680", mixBlendMode: composed() ? "plus-darker" : "normal" }} />
    </View>
    <View testID={`composition-scene:${composed()}`} onPress={() => composed.set(!composed())} style={{ height: 24, backgroundColor: "#dbeafe" }}><Text>Toggle backdrop composition</Text></View>
    <View testID={`composition-background:${background()}`} onPressIn={() => pressed.set(true)} onPressOut={() => pressed.set(false)} onPress={() => { presses.set(presses() + 1); background.set(!background()); }} style={{ height: 24, backgroundColor: "#dbeafe" }}><Text testID={`composition-pressed:${pressed()}`}>Change backdrop source</Text></View>
    <View testID={`composition-shift:${shifted()}`} onPress={() => shifted.set(!shifted())} style={{ height: 24, backgroundColor: "#dbeafe" }}><Text>Move source child</Text></View>
    <View testID="composition-retire" onPress={() => visible.set(false)} style={{ height: 24, backgroundColor: "#dbeafe" }}><Text>Retire exports</Text></View>
    <View testID="composition-remount" onPress={() => visible.set(true)} style={{ height: 24, backgroundColor: "#dbeafe" }}><Text>Recreate exports</Text></View>
  </View>;
}
mount(App);
