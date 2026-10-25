import { mount, signal, View, Text, Scroll, type YNode } from "nsh";
import { Dialog } from "nsh/base";
import { Video } from "nsh/video";
import sample from "./sample.mp4";
function App() {
  const loads = signal(0), geometry = signal(false), modal = signal(false), overlays = signal(0);
  let video!: YNode;
  const source = signal(sample);
  let httpStep = 0;
  const httpTest = () => {
    httpStep++;
    source.set(`http://127.0.0.1:18769/${httpStep === 1 ? "redirect" : httpStep === 2 ? "oversized" : "slow"}`);
    if (httpStep >= 3) setTimeout(() => source.set(sample), 100);
  };
  const state = signal("idle"), progress = signal(0), paused = signal(false), shown = signal(true), controls = signal(false), volume = signal(1);
  const button = (id: string, label: string, press: () => void) => <View testID={id} onPress={press} style={{ height: 32, backgroundColor: "#dbeafe" }}><Text>{label}</Text></View>;
  const media = () => <Video ref={video} src={source()} paused={paused()} controls={controls()} volume={volume()} repeat={true}
          testID="embedded-video" style={{ transform: geometry() ? [{ translateX: 30 }, { rotate: 8 }, { scale: 0.85 }] : [], height: 180, marginVertical: 8, overflow: "hidden", borderRadius: 12 }}
          onLoad={() => { state.set("loaded"); loads.update(v => v + 1); }} onProgress={e => progress.set(Math.floor(e.currentTime))}
          onError={e => state.set(`error:${e.message}`)} onEnd={() => { if (!state().startsWith("error:")) state.set("ended"); }} />;
  return <View style={{ flex: 1, gap: 6, padding: 8, backgroundColor: "white" }}>
    <Text testID={`media-loads:${loads()}`}>{loads()}</Text>
    <Text testID={`video-state:${state()}`}>{state()}</Text>
    <Text testID={`video-time:${progress()}`}>{progress()}</Text>
    {button("video-pause", "Pause / play", () => paused.update(v => !v))}
    {button("video-seek", "Seek 4 seconds", () => video.seek(4))}
    {button("video-controls", "Toggle native controls", () => controls.update(v => !v))}
    {button("video-volume", "Toggle mute", () => volume.update(v => v === 0 ? 1 : 0))}
    {button("video-toggle", "Unmount / remount", () => shown.update(v => !v))}
    <View style={{ flexDirection: "row", gap: 6 }}>
      {button("media-geometry", "Transform media", () => geometry.update(v => !v))}
      {button("media-modal-open", "Open media modal", () => modal.set(true))}
      {button("video-http", "HTTP tests", httpTest)}
    </View>
    <Text testID={`media-overlays:${overlays()}`}>{overlays()}</Text>
    <Scroll testID="media-scroll" style={{ flex: 1 }}>
      <Text>Native sibling above</Text>
      <If when={shown() && !modal()}>
        <View style={{ height: 208 }}>
          {media()}
          <View testID="media-overlap" onPress={() => overlays.update(v => v + 1)} style={{ position: "absolute", left: 30, top: 75, width: 130, height: 40, backgroundColor: "#2563eb" }}><Text style={{ color: "white" }}>Native overlap</Text></View>
        </View>
      </If>
      <View style={{ height: 40, backgroundColor: "#ef4444" }}><Text>Native sibling below</Text></View>
      <View style={{ height: 500 }} />
    </Scroll>
    <If when={modal()}>
      <Dialog open={true} animation="none" onOpenChange={open => { if (!open) modal.set(false); }}>
        <View testID="media-modal" style={{ flex: 1, padding: 30, backgroundColor: "white" }}>
          {button("media-modal-close", "Close media modal", () => modal.set(false))}
          {media()}
        </View>
      </Dialog>
    </If>
  </View>;
}
mount(App);
