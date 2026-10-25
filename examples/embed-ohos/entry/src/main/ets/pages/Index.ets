import { NshSurface, onBackPressIntercept, getCapabilities } from "@nsh/ohos";
import { HostBadge } from "../components/HostBadge";

@Entry
@Component
struct Index {
  private left: NshSurface = new NshSurface();
  private right?: NshSurface;
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
  }

  private load(surface: NshSurface): void {
    surface.setErrorHandler((_message: string) => this.refresh());
    surface
      .loadResource("bundle.js")
      .then(() => this.refresh())
      .catch(() => this.refresh());
  }

  private refresh(): void {
    setTimeout(() => {
      const caps = getCapabilities();
      this.status =
        `API ${caps.apiLevel} | left: ` +
        (this.left.isDisposed
          ? "disposed"
          : this.left.lastError || (this.left.isRunning ? "running" : "empty")) +
        " | right: " +
        (this.right === undefined
          ? "absent"
          : this.right.lastError || (this.right.isRunning ? "running" : "empty"));
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
        Button("Stop")
          .onClick(() => {
            this.left.stop();
            this.right?.stop();
            this.refresh();
          })
        Button("Connect dev")
          .onClick(() => {
            this.left.connectDevServer("ws://127.0.0.1:8790/ws");
          })
        Button("Disconnect dev")
          .onClick(() => {
            this.left.disconnectDevServer();
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
