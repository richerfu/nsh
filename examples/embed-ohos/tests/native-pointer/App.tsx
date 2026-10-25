import { mount, signal, Text, View } from "nsh";
import { nativeComponent } from "nsh/native";

interface MouseCounts {
  leftDown: number;
  leftUp: number;
  rightDown: number;
  rightUp: number;
  moves: number;
}

const Badge = nativeComponent<
  { text: string; mouseProbe: string },
  { mouse: MouseCounts; tap: { count: number }; axis: { count: number } }
>("embed.badge", { props: ["text", "mouseProbe"], events: ["mouse", "tap", "axis"] });

function Probe(props: { name: string; composed: boolean }) {
  const counts = signal<MouseCounts>({ leftDown: 0, leftUp: 0, rightDown: 0, rightUp: 0, moves: 0 });
  const taps = signal(0);
  const axes = signal(0);
  return <View style={{ gap: 8 }}>
    <Text>{props.name}</Text>
    <View style={{ width: 240, height: 44, filter: props.composed ? "grayscale(1)" : "none" }}>
      <Badge text={`Native ${props.name}`} mouseProbe={props.name}
        onMouse={event => counts.set(event)} onTap={event => taps.set(event.count)}
        onAxis={event => axes.set(event.count)} />
    </View>
    <Text testID={`mouse-state:${props.name}:${counts().leftDown}:${counts().leftUp}:${counts().rightDown}:${counts().rightUp}:${counts().moves}:${taps()}:${axes()}`}>
      {`Left ${counts().leftDown}/${counts().leftUp}, right ${counts().rightDown}/${counts().rightUp}, moves ${counts().moves}, clicks ${taps()}, axes ${axes()}`}
    </Text>
  </View>;
}

function App() {
  return <View style={{ flex: 1, padding: 16, gap: 20, backgroundColor: "white" }}>
    <Text>Native SDK mouse callbacks</Text>
    <Probe name="plain" composed={false} />
    <Probe name="export" composed={true} />
  </View>;
}

mount(App);
