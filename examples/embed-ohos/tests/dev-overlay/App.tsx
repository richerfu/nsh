import { mount, signal, Text, View } from "nsh";

function App() {
  const presses = signal(0);
  return <View testID="dev-underlay" onPress={() => presses.set(presses() + 1)}
    style={{ flex: 1, backgroundColor: "#dbeafe", padding: 20, justifyContent: "center" }}>
    <Text testID={`dev-underlay-count:${presses()}`} style={{ fontSize: 24 }}>
      {`Underlying presses ${presses()}`}
    </Text>
  </View>;
}

mount(App);
