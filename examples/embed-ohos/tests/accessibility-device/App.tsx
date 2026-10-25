import { Input, Text, View, mount, signal } from "nsh";
import { AccessibilityInfo } from "nsh/accessibility";
import { Video } from "nsh/video";
import { WebView } from "nsh/webview";

function App() {
  const mode = signal("input"), value = signal(""), state = signal("idle");
  const button = (id: string, label: string, press: () => void) =>
    <View testID={id} accessible accessibilityRole="button" accessibilityLabel={label}
      onPress={press} style={{ minHeight: 36, padding: 6, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  const html = '<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1"></head><body><h1>Accessible web document</h1><button onclick="window.ReactNativeWebView.postMessage(\'web-activated\')">Web action</button><label>Web editor<input aria-label="Web editor" /></label><p>Web document end</p></body></html>';
  return <View style={{ flex: 1, padding: 8, gap: 6, backgroundColor: "white" }}>
    <View style={{ flexDirection: "row", gap: 6 }}>
      {button("device-mode-input", "Input fixture", () => { mode.set("input"); state.set("idle"); })}
      {button("device-mode-web", "Web fixture", () => { mode.set("web"); state.set("idle"); })}
      {button("device-mode-video", "Video fixture", () => { mode.set("video"); state.set("idle"); })}
    </View>
    <Text testID={`device-mode:${mode()}`}>{`Mode ${mode()}`}</Text>
    <Text testID={`device-state:${state()}`}>{`State ${state()}`}</Text>
    <If when={mode() === "input"}>
      <Input testID="device-editor" accessibilityLabel="Native editor" placeholder="Native editor"
        value={value()} onChangeText={text => { value.set(text); state.set(`text:${text}`); }}
        onFocus={() => state.set("editor-focused")} style={{ minHeight: 48, borderWidth: 1, padding: 8 }} />
      <Input testID="device-multiline" accessibilityLabel="Multiline editor" placeholder="Multiline editor"
        multiline onChangeText={text => state.set(`multiline:${text}`)} style={{ height: 70, borderWidth: 1, padding: 8 }} />
    </If>
    <If when={mode() === "web"}>
      <WebView testID="device-web" source={{ html }} style={{ flex: 1 }}
        onLoad={() => state.set("web-loaded")} onMessage={event => state.set(event.data)}
        onError={event => state.set(`error:${event.message}`)} />
    </If>
    <If when={mode() === "video"}>
      <Video testID="device-video" source="asset:/sample.mp4" controls paused style={{ flex: 1 }}
        onLoad={() => state.set("video-loaded")} onProgress={event => state.set(`video-time:${Math.floor(event.currentTime)}`)}
        onError={event => state.set(`error:${event.message}`)} />
    </If>
    {button("device-announcement", "Explicit announcement", () => AccessibilityInfo.announceForAccessibility("Native Shell device announcement"))}
  </View>;
}
mount(App);
