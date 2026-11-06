import { NshSurface } from "@nsh/ohos";
import { HostBadge } from "../components/HostBadge";

interface HostMessage {
  text: string;
}

@Entry
@Component
struct Index {
  private surface?: NshSurface;
  @State private ready: boolean = false;

  aboutToAppear(): void {
    const context = this.getUIContext();
    const surface = new NshSurface(context);
    this.surface = surface;
    surface.registerComponent("embed.badge", (handle: number) => new HostBadge(context, handle, surface));
    surface.setParams({ user: "OHOS" });
    surface.handle("time", (_data, reply) => {
      surface.send("message", { text: "Time requested from the native host" });
      reply.resolve({ time: new Date().toLocaleTimeString() });
    });
    surface.handle("tapped", (_data, _reply) => {
      surface.send("message", { text: "The native host received your tap" });
    });
    surface.loadResource("bundle.js");
    this.ready = true;
  }

  aboutToDisappear(): void {
    this.ready = false;
    this.surface?.dispose();
    this.surface = undefined;
  }

  onBackPress(): boolean {
    return this.surface?.onBackPress() ?? false;
  }

  build() {
    Column() {
      Row({ space: 12 }) {
        Text("Native Shell").fontSize(16).fontWeight(FontWeight.Medium)
        TextInput({ placeholder: "Host input" }).layoutWeight(1).height(36)
        Button("Send").onClick(() => {
          const message: HostMessage = { text: "Hello from the native toolbar" };
          this.surface?.send("message", message);
        })
      }.padding(12).width("100%")
      if (this.ready && this.surface) {
        Column() {
          ContentSlot(this.surface.content)
        }.width("100%").layoutWeight(1)
      }
    }
    .width("100%")
    .height("100%")
  }
}
