import { DefaultXComponent } from "@ohos-rs/ability";
import { NshSurface } from "libnsh_platform_ohos.so";
import pasteboard from "@ohos.pasteboard";

@Entry
@Component
struct Index {
  private left: NshSurface = new NshSurface();
  private right: NshSurface = new NshSurface();
  @State leftPresent: boolean = true;
  @State leftMounted: boolean = true;
  @State narrow: boolean = false;
  @State leftError: string = '""';
  @State rightError: string = '""';
  @State leftState: string = "pending";
  @State rightState: string = "pending";
  @State clipboard: string = '""';
  @State clipboardState: string = "idle";
  @State clipboardReads: number = 0;
  @State hostError: string = "";

  aboutToAppear(): void {
    this.connect(this.left, "left");
    this.connect(this.right, "right");
    this.inspect();
  }

  aboutToDisappear(): void {
    this.left.dispose();
    this.right.dispose();
  }

  private connect(surface: NshSurface, side: string): void {
    surface.setErrorHandler((_message: string): void => this.inspect());
    surface.connectDevServer(`ws://127.0.0.1:8797/ws?side=${side}`);
  }

  private inspect(): void {
    this.leftState = this.left.isDisposed ? "disposed" : this.left.isRunning ? "running" : "empty";
    this.rightState = this.right.isDisposed
      ? "disposed"
      : this.right.isRunning
        ? "running"
        : "empty";
    this.leftError = JSON.stringify(this.left.isDisposed ? "" : this.left.lastError);
    this.rightError = JSON.stringify(this.right.isDisposed ? "" : this.right.lastError);
  }

  private async readClipboard(): Promise<void> {
    this.clipboardState = "reading";
    try {
      const data = await pasteboard.getSystemPasteboard().getData();
      this.clipboard = JSON.stringify(data.getPrimaryText() ?? "");
      this.clipboardState = "ok";
    } catch (error) {
      this.clipboardState = `error:${String(error)}`;
    } finally {
      this.clipboardReads++;
    }
  }

  private async seedClipboard(): Promise<void> {
    this.clipboardState = "writing";
    try {
      await pasteboard
        .getSystemPasteboard()
        .setData(pasteboard.createPlainTextData("dev-overlay seed 😀\n"));
      this.clipboardState = "seeded";
    } catch (error) {
      this.clipboardState = `error:${String(error)}`;
    }
  }

  private disposeLeft(): void {
    this.left.dispose();
    this.leftPresent = false;
    this.leftMounted = false;
    this.inspect();
  }

  private recreateLeft(): void {
    if (!this.left.isDisposed) {
      this.hostError = "left must be disposed before recreation";
      return;
    }
    this.left = new NshSurface();
    this.connect(this.left, "left");
    this.leftPresent = true;
    this.leftMounted = true;
    this.inspect();
  }

  build() {
    Stack() {
      DefaultXComponent({ moduleName: "nsh_platform_ohos" })
        .width("100%")
        .height("100%")
      Column() {
        Flex({ wrap: FlexWrap.Wrap }) {
          Button("Inspect")
            .id("dev-inspect")
            .onClick(() => this.inspect())
          Button("Resize")
            .id("dev-resize")
            .onClick(() => (this.narrow = !this.narrow))
          Button(this.leftMounted ? "Unmount left" : "Mount left")
            .id("dev-mount-left")
            .onClick(() => (this.leftMounted = !this.leftMounted))
          Button("Dispose left")
            .id("dev-dispose-left")
            .onClick(() => this.disposeLeft())
          Button("Recreate left")
            .id("dev-recreate-left")
            .onClick(() => this.recreateLeft())
          Button("Read clipboard")
            .id("dev-read-clipboard")
            .onClick(() => this.readClipboard())
          Button("Seed clipboard")
            .id("dev-seed-clipboard")
            .onClick(() => this.seedClipboard())
        }.margin(8)
        Text(this.leftState)
          .id("dev-state-left")
          .fontSize(12)
        Text(this.rightState)
          .id("dev-state-right")
          .fontSize(12)
        Text(this.leftError)
          .id("dev-last-error-left")
          .maxLines(1)
          .fontSize(10)
        Text(this.rightError)
          .id("dev-last-error-right")
          .maxLines(1)
          .fontSize(10)
        Text(this.clipboardState)
          .id("dev-clipboard-state")
          .fontSize(12)
        Text(String(this.clipboardReads))
          .id("dev-clipboard-revision")
          .fontSize(10)
        Text(this.clipboard)
          .id("dev-clipboard-text")
          .maxLines(1)
          .fontSize(10)
        Text(this.hostError)
          .id("dev-host-error")
          .maxLines(1)
          .fontSize(10)
        Row() {
          Stack() {
            if (this.leftPresent && this.leftMounted) {
              ContentSlot(this.left.content);
            }
          }
          .id("dev-slot-left")
          .width(this.narrow ? "35%" : "50%")
          .height("100%")
          Stack() {
            ContentSlot(this.right.content)
          }
          .id("dev-slot-right")
          .width(this.narrow ? "65%" : "50%")
          .height("100%")
        }
        .width("100%")
        .layoutWeight(1)
      }
      .width("100%")
      .height("100%")
      .backgroundColor("#f4f5f7")
    }
    .width("100%")
    .height("100%");
  }
}
