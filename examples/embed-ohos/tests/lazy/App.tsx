import { mount, signal, View, Text } from "nsh";
function App() {
  const result = signal("idle");
  return <View style={{ flex: 1, padding: 16, backgroundColor: "white" }}>
    <View testID="lazy-load" onPress={async () => {
      try { const { message } = await import("./Detail"); result.set(message); }
      catch (error) { result.set(`error:${String(error)}`); }
    }} style={{ height: 44, backgroundColor: "#dbeafe" }}><Text>Load split chunk</Text></View>
    <Text testID={`lazy:${result()}`}>{result()}</Text>
  </View>;
}
mount(App);
