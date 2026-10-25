import { mount, signal, View, Text } from "nsh";
import { Image } from "nsh/image";
import pixel from "./pixel.png";
function App() {
  const green = signal(true), shown = signal(true), loaded = signal(0);
  return <View style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "white" }}>
    <Text testID={`images-loaded:${loaded()}`}>{loaded()}</Text>
    <View testID="image-tint-toggle" onPress={() => green.update(v => !v)} style={{ height: 40, backgroundColor: "#dbeafe" }}><Text>Toggle native tint</Text></View>
    <View testID="image-remount" onPress={() => { shown.set(false); setTimeout(() => shown.set(true), 50); }} style={{ height: 40, backgroundColor: "#dbeafe" }}><Text>Restart image fade</Text></View>
    <If when={shown()}>
      <Image testID="image-fade" src={pixel} fadeDuration={2200} onLoad={() => loaded.update(v => v + 1)} style={{ width: 120, height: 120, borderRadius: 20, tintColor: green() ? "#22c55e" : "#2563eb" }} />
    </If>
    <Image testID="image-blur" src={pixel} blurRadius={6} style={{ width: 120, height: 120, borderRadius: 20, tintColor: "#ef4444" }} />
  </View>;
}
mount(App);
