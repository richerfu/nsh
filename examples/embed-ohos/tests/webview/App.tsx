import { mount, signal, View, Text, Scroll, type YNode } from "nsh";
import { Modal } from "nsh/ui/mobile";
import { WebView } from "nsh/webview";
function App() {
  const loads = signal(0), geometry = signal(false), modal = signal(false), overlays = signal(0);
  let web!: YNode;
  const messages = signal("none"), state = signal("idle"), shown = signal(true);
  const html = '<html><head><meta name="viewport" content="width=device-width,initial-scale=1"><title>NSH native WebView</title></head><body style="margin:0;background:#bbf7d0"><button id="send" style="height:60px;width:100%" onclick="window.ReactNativeWebView.postMessage(\'page-tap\')">Send native message</button><div style="height:450px">Native WebView content</div></body></html>';
  const source = signal<{ html: string } | { uri: string }>({ html });
  const button = (id: string, label: string, press: () => void) => <View testID={id} onPress={press} style={{ height: 36, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  const media = () => <WebView ref={web} testID="embedded-web" source={source()} style={{ transform: geometry() ? [{ translateX: 30 }, { rotate: 8 }, { scale: 0.85 }] : [], height: 180, margin: 8, borderRadius: 12, overflow: "hidden" }}
          injectedJavaScriptBeforeContentLoaded='window.addEventListener("message",e=>window.ReactNativeWebView.postMessage("received:"+e.data))'
          onLoadStart={() => state.set("loading")} onLoad={() => { state.set("loaded"); loads.update(v => v + 1); }}
          onError={e => state.set(`error:${e.message}`)} onMessage={e => messages.set(e.data)} />;
  return <View style={{ flex: 1, padding: 8, gap: 6, backgroundColor: "white" }}>
    <Text testID={`media-loads:${loads()}`}>{loads()}</Text>
    <Text testID={`web-state:${state()}`}>{state()}</Text>
    <Text testID={`web-message:${messages()}`}>{messages()}</Text>
    {button("web-inject", "Inject message", () => web.injectJavaScript('window.ReactNativeWebView.postMessage("injected")'))}
    {button("web-post", "Post to page", () => web.postMessage("host-message"))}
    {button("web-reload", "Reload", () => web.reload())}
    {button("web-toggle", "Unmount / remount", () => shown.update(v => !v))}
    {button("web-asset", "Load scoped HTML asset", () => source.set({ uri: "asset:/native web.html" }))}
    <View style={{ flexDirection: "row", gap: 6 }}>
      {button("media-geometry", "Transform media", () => geometry.update(v => !v))}
      {button("media-modal-open", "Open media modal", () => modal.set(true))}
    </View>
    <Text testID={`media-overlays:${overlays()}`}>{overlays()}</Text>
    <Scroll testID="media-scroll" style={{ flex: 1, backgroundColor: "#f1f5f9" }}>
      <View style={{ height: 60 }}><Text>Native sibling above the web view</Text></View>
      <If when={shown() && !modal()}>
        <View style={{ height: 208 }}>
          {media()}
          <View testID="media-overlap" onPress={() => overlays.update(v => v + 1)} style={{ position: "absolute", left: 30, top: 75, width: 130, height: 40, backgroundColor: "#2563eb" }}><Text style={{ color: "white" }}>Native overlap</Text></View>
        </View>
      </If>
      <View testID="web-overlay" style={{ height: 40, backgroundColor: "#ef4444" }}><Text>Native sibling below the web view</Text></View>
      <View style={{ height: 500 }}><Text>Scroll and clip the embedded web view</Text></View>
    </Scroll>
    <If when={modal()}>
      <Modal visible={true} animationType="none" onRequestClose={() => modal.set(false)}>
        <View testID="media-modal" style={{ flex: 1, padding: 30, backgroundColor: "white" }}>
          {button("media-modal-close", "Close media modal", () => modal.set(false))}
          {media()}
        </View>
      </Modal>
    </If>
  </View>;
}
mount(App);
