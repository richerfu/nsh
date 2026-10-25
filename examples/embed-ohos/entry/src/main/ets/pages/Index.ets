import { NshHostReply, NshSurface, onBackPressIntercept, getCapabilities } from "@nsh/ohos";
import { HostBadge } from "../components/HostBadge";

class LaunchParams {
  user: string;
  constructor(user: string) {
    this.user = user;
  }
}

class TimeReply {
  time: string;
  constructor(time: string) {
    this.time = time;
  }
}

class HostMessage {
  text: string;
  constructor(text: string) {
    this.text = text;
  }
}

const BRAND_THEME: string =
  '{"cssVars":{"theme":{"radius":"1rem"},"light":{"primary":"#e11d48","primary-foreground":"#ffffff"},"dark":{"primary":"#fb7185","primary-foreground":"#1c1917"}}}';

@Entry
@Component
struct Index {
  private left: NshSurface = new NshSurface();
  private right?: NshSurface;
  private branded: boolean = false;
  private note: string = "";
  @State split: boolean = false;
  @State status: string = "";

  aboutToAppear(): void {
    this.prepare(this.left);
    this.load(this.left);
  }

  aboutToDisappear(): void {
    this.left.dispose();
    this.right?.dispose();
  }

  onBackPress(): boolean {
    return onBackPressIntercept();
  }

  private prepare(surface: NshSurface): void {
    surface.registerComponent(
      "embed.badge",
      (handle: number) => new HostBadge(this.getUIContext(), handle, surface),
    );
    surface.setParams(new LaunchParams("Ada"));
    surface.handle("time", (_data, reply: NshHostReply) => {
      reply.resolve(new TimeReply(new Date().toLocaleTimeString()));
    });
    surface.handle("tapped", (data, _reply: NshHostReply) => {
      const record = data as Record<string, Object>;
      const count = record ? record["count"] : 0;
      this.note = `the app posted tapped: ${count}`;
      this.refresh();
    });
    if (this.branded) {
      surface.setThemeOverrides(BRAND_THEME);
    }
  }

  private load(surface: NshSurface): void {
    surface.setErrorHandler((_message: string) => this.refresh());
    surface
      .loadResource("bundle.js")
      .then(() => this.refresh())
      .catch(() => this.refresh());
  }

  private sendMessage(): void {
    const msg = new HostMessage(`Hello from OHOS at ${new Date().toLocaleTimeString()}`);
    this.left.send("message", msg);
    this.right?.send("message", msg);
  }

  private toggleBrand(): void {
    this.branded = !this.branded;
    const theme = this.branded ? BRAND_THEME : undefined;
    this.left.setThemeOverrides(theme);
    this.right?.setThemeOverrides(theme);
  }

  private refresh(): void {
    setTimeout(() => {
      const caps = getCapabilities();
      const base =
        `API ${caps.apiLevel} | left: ` +
        (this.left.isDisposed
          ? "disposed"
          : this.left.lastError || (this.left.isRunning ? "running" : "empty")) +
        " | right: " +
        (this.right === undefined
          ? "absent"
          : this.right.lastError || (this.right.isRunning ? "running" : "empty"));
      this.status = this.note ? `${base} | ${this.note}` : base;
    }, 150);
  }

  build() {
    Column() {
      Flex({ wrap: FlexWrap.Wrap }) {
        Button("Reload")
          .onClick(() => {
            this.load(this.left);
            if (this.right) {
              this.load(this.right);
            }
          })
        Button(this.split ? "Single" : "Split")
          .onClick(() => {
            if (this.split) {
              this.split = false;
              this.right?.dispose();
              this.right = undefined;
              this.refresh();
            } else {
              this.right = new NshSurface();
              this.prepare(this.right);
              this.load(this.right);
              this.split = true;
            }
          })
        Button("Send a message")
          .onClick(() => {
            this.sendMessage();
          })
        Button("Brand")
          .onClick(() => {
            this.toggleBrand();
          })
        Button("Stop")
          .onClick(() => {
            this.left.stop();
            this.right?.stop();
            this.refresh();
          })
        Button("Connect dev")
          .onClick(() => {
            try {
              this.left.connectDevServer("ws://127.0.0.1:8790/ws");
            } catch {
              // Reported via setErrorHandler / lastError.
            }
            this.refresh();
          })
        Button("Disconnect dev")
          .onClick(() => {
            try {
              this.left.disconnectDevServer();
            } catch {
              // Ignored if surface is stopped/disposed.
            }
            this.refresh();
          })
      }.margin(8)
      Text(this.status)
        .id("surface-status")
        .fontSize(12)
        .margin({ bottom: 4 })
      Flex({ direction: FlexDirection.Row }) {
        Column() {
          ContentSlot(this.left.content)
        }
        .width(this.split ? "50%" : "100%")
        .height("100%")
        if (this.split && this.right) {
          Column() {
            ContentSlot(this.right.content)
          }
          .width("50%")
          .height("100%")
        }
      }
      .width("100%")
      .layoutWeight(1)
    }
    .width("100%")
    .height("100%")
    .backgroundColor("#f4f5f7")
  }
}
