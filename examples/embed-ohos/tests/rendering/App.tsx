import { mount, signal, Text, View } from "nsh";
import { Animated, createNativeValue } from "nsh/animated";
import { Canvas } from "nsh/canvas";
const amount = createNativeValue(0);
const state = signal("idle");
const image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGP4z8DwH4QZYAwAR8oH+WdZbrcAAAAASUVORK5CYII=";
function App() {
  const color = amount.interpolate({ inputRange: [0, 1], outputRange: ["#ff0000", "#0000ff"] });
  return <View style={{ flex: 1, padding: 16, gap: 14, backgroundColor: "white" }}>
    <View testID="start-animation" onPress={() => { amount.set(0); state.set("running"); Animated.timing(amount, { toValue: 1, duration: 500 }).start().then(done => state.set(done ? "done" : "stopped")); }} style={{ height: 44, backgroundColor: "#dbeafe" }}><Text>Animate text and Canvas</Text></View>
    <Text testID={`animation:${state()}`}>{state()}</Text>
    <Text testID="tinted-text" style={{ color, fontSize: 28 }}>Native color animation</Text>
    <View style={{ flexDirection: "row", gap: 12 }}>
      <View testID="linear-gradient" style={{ width: 100, height: 60, borderRadius: 12, backgroundImage: "linear-gradient(90deg, red, blue)", outlineWidth: 2, outlineColor: "#00aa00", outlineOffset: 3 }}/>
      <View testID="radial-gradient" style={{ width: 100, height: 60, backgroundImage: "radial-gradient(ellipse, red, blue)" }}/>
      <View testID="conic-gradient" style={{ width: 100, height: 60, backgroundImage: "conic-gradient(red, lime, blue, red)" }}/>
    </View>
    <View testID="filtered" style={{ width: 100, height: 44, backgroundColor: "red", filter: "grayscale(1)" }}/>
    <Canvas testID="animated-canvas" style={{ width: 300, height: 160, backgroundColor: "#eef2ff" }} draw={ctx => {
      ctx.fillStyle = color; ctx.fillRect(0, 0, amount.interpolate({ inputRange: [0, 1], outputRange: [10, 140] }), 40);
      const radial = ctx.createRadialGradient(190, 20, 0, 190, 20, 25); radial.addColorStop(0, "white"); radial.addColorStop(1, "black"); ctx.fillStyle = radial; ctx.fillRect(160, 0, 60, 45);
      ctx.drawImage(image, 230, 0, 40, 40);
      ctx.beginPath(); ctx.roundRect(10, 60, 100, 60, 15); ctx.lineWidth = 3; ctx.setLineDash([8, 4]); ctx.strokeStyle = "#007744"; ctx.stroke();
      const sweep = ctx.createConicGradient(Math.PI / 2, 180, 90); sweep.addColorStop(0, "red"); sweep.addColorStop(0.5, "blue"); sweep.addColorStop(1, "red"); ctx.fillStyle = sweep; ctx.fillRect(140, 60, 80, 65);
      ctx.font = "18px sans-serif"; ctx.fillStyle = "#333333"; ctx.fillText("Native Canvas", 10, 150);
    }}/>
  </View>;
}
mount(App);
