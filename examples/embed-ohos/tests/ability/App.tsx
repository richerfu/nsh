import { mount, signal, Text, View } from "nsh";
import { Clipboard } from "nsh/clipboard";
import { Platform, PixelRatio } from "nsh/device";
import { Image } from "nsh/image";
import pixel from "./pixel.png";

function App() {
  const value = signal("surface");
  const result = signal("idle");
  const has = signal("idle");
  const image = signal("loading");
  const copy = () => {
    Clipboard.setString(value());
    result.set("pending");
    Clipboard.getString().then(text => result.set(text));
    Clipboard.hasString().then(value => has.set(String(value)));
  };
  return <View style={{ flex: 1, padding: 16, gap: 12, backgroundColor: "#f3f4f6" }}>
    <Text testID={`reply:${value()}:${result()}:${has()}:${image()}`}>{`${value()} → ${result()} (${has()})`}</Text>
    <Text testID={`platform:${Platform.OS}:${Platform.Version}:${PixelRatio.getFontScale()}`}>{`${Platform.OS} API ${Platform.Version}`}</Text>
    <View testID="alpha" onPress={() => value.set("alpha")} style={{ height: 52, backgroundColor: "#bfdbfe" }}><Text>Alpha</Text></View>
    <View testID="beta" onPress={() => value.set("beta")} style={{ height: 52, backgroundColor: "#bbf7d0" }}><Text>Beta</Text></View>
    <View testID="copy" onPress={copy} style={{ height: 52, backgroundColor: "#fde68a" }}><Text>Copy and read through Ability</Text></View>
    <Image src={pixel} testID="asset" onLoad={() => image.set("loaded")} onError={event => image.set(`error:${event.message}`)} style={{ width: 64, height: 64 }}/>
  </View>;
}

mount(App);
