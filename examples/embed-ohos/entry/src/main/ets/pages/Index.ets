import { DefaultXComponent } from "@ohos-rs/ability";
import { NshSurface, NshTrust, onBackPressIntercept, getCapabilities } from "libnsh_platform_ohos.so";
import { common } from "@kit.AbilityKit";
import { util } from "@kit.ArkTS";
import { HostBadge } from "../components/HostBadge";

@Entry
@Component
struct Index {
  private left: NshSurface = new NshSurface();
  private right?: NshSurface = new NshSurface();
  @State visible: boolean = true;
  @State split: boolean = true;
  @State swapped: boolean = false;
  @State narrow: boolean = false;
  @State leftVisible: boolean = true;
  @State status: string = "";
  @State checks: string = "";
  @State pluginRoot: boolean = true;

  private overlayChecks(): void {
    const source = (name: string): string =>
      'import { mount } from "nsh"; import { Alert } from "nsh/base"; ' +
      'mount(() => null); Alert.alert("Overlay ' + name + '", "System back routing").then(i=>console.log("overlay:' + name + ':"+i));';
    this.left.loadSource(source("left"));
    this.right?.loadSource(source("right"));
    setTimeout(() => { this.left.activate(); this.checks = "overlay-first:" + onBackPressIntercept(); }, 1500);
    setTimeout(() => { this.left.activate(); this.checks = "overlay-second:" + onBackPressIntercept(); }, 3000);
  }


  aboutToAppear(): void {
    this.prepare(this.left);
    this.load(this.left);
    if (this.right !== undefined) {
      this.prepare(this.right);
      this.load(this.right);
    }
  }

  aboutToDisappear(): void {
    this.left.dispose();
    this.right?.dispose();
  }

  private load(surface: NshSurface): void {
    surface.setErrorHandler((_message: string) => this.refresh());
    surface
      .loadResource("bundle.js")
      .then(() => this.refresh())
      .catch(() => this.refresh());
  }

  private prepare(surface: NshSurface): void {
    surface.registerComponent(
      "embed.badge",
      (handle: number) => new HostBadge(this.getUIContext(), handle, surface),
    );
  }

  private async packageChecks(): Promise<void> {
    try {
      const context = this.getUIContext().getHostContext() as common.UIAbilityContext;
      const bytes = await context.resourceManager.getRawFileContent("package-root.pub");
      const root = util.TextDecoder.create("utf-8").decodeToString(bytes).trim();
      const directory = context.filesDir + "/nsh-trust-capabilities-" + Date.now();
      const trust = await NshTrust.open(directory, [root]);
      const pkg = await trust.openResource("package2.nshpkg");
      if (pkg.info.build !== "2") { throw new Error("wrong package metadata"); }
      this.left.runPackage(pkg);
      let refused = 0;
      try { this.left.runPackage(pkg); } catch (_) { refused++; }
      try { await trust.openResource("package1.nshpkg"); } catch (_) { refused++; }
      const reopened = await NshTrust.open(directory, [root]);
      try { await reopened.openResource("package1.nshpkg"); } catch (_) { refused++; }
      const untrusted = await NshTrust.open(context.filesDir + "/nsh-untrusted", []);
      try { await untrusted.openResource("package2.nshpkg"); } catch (_) { refused++; }
      try { await trust.openResource("package-tampered.nshpkg"); } catch (_) { refused++; }
      const revocations = util.TextDecoder.create("utf-8").decodeToString(
        await context.resourceManager.getRawFileContent("package-revocations.json"));
      await reopened.updateRevocations(revocations);
      try { await trust.openResource("package2.nshpkg"); } catch (_) { refused++; }
      try { await reopened.updateRevocations(revocations); } catch (_) { refused++; }
      const persisted = await NshTrust.open(directory, [root]);
      try { await persisted.openResource("package2.nshpkg"); } catch (_) { refused++; }
      this.checks = "packages:" + refused + ":" + pkg.isConsumed;
      this.refresh();
    } catch (error) {
      this.checks = "packages:error:" + (error as Error).message;
    }
  }

  private refresh(): void {
    setTimeout(() => {
      this.status =
        "left:" +
        (this.left.isDisposed
          ? "disposed"
          : this.left.lastError || (this.left.isRunning ? "running" : "empty")) +
        " | right:" +
        (this.right === undefined
          ? "absent"
          : this.right.lastError || (this.right.isRunning ? "running" : "empty"));
    }, 300);
  }

  private race(): void {
    this.checks = "race:pending";
    const stale = this.left.loadResource("bundle.js");
    this.left.stop();
    const current = this.left.loadResource("bundle.js");
    stale
      .then(() => {
        this.checks = "race:unexpected";
      })
      .catch(() => {
        current
          .then(() => {
            this.checks = "race:cancelled";
            this.refresh();
          })
          .catch(() => {
            this.checks = "race:failed";
          });
      });
  }

  private invalid(): void {
    let rejected = 0;
    try {
      new NshSurface({ memoryBytes: 1 });
    } catch (_) {
      rejected++;
    }
    try {
      this.left.loadResource("../bundle.js");
    } catch (_) {
      rejected++;
    }
    try {
      this.left.setEnvironment({ fontScale: 0 });
    } catch (_) {
      rejected++;
    }
    this.checks = "invalid:" + rejected;
  }

  build() {
    Stack() {
      // Ability's UI plugin root is shared by the host. It owns no NSH engine.
      if (this.pluginRoot) {
        DefaultXComponent({ moduleName: "nsh_platform_ohos" }).width("100%").height("100%")
      }
      Column() {
        Flex({ wrap: FlexWrap.Wrap }) {
          Button("Plugin context").onClick(() => { this.pluginRoot = !this.pluginRoot; })
            Button(this.visible ? "Unmount" : "Mount")
            .onClick(() => {
              this.visible = !this.visible;
            })
          Button("Single")
            .onClick(() => {
              this.split = false;
              this.right?.dispose();
              this.right = undefined;
              this.refresh();
            })
          Button("Split")
            .onClick(() => {
              if (!this.right) {
                this.right = new NshSurface();
                this.prepare(this.right);
                this.load(this.right);
              }
              this.split = true;
            })
          Button("Swap")
            .onClick(() => {
              this.swapped = !this.swapped;
            })
          Button("Resize")
            .onClick(() => {
              this.narrow = !this.narrow;
            })
          Button("Stop left")
            .onClick(() => {
              this.left.stop();
              this.refresh();
            })
          Button("Reload left")
            .onClick(() => {
              this.load(this.left);
            })
          Button("Missing left")
            .onClick(() => {
              this.left.loadResource("missing.js").catch(() => this.refresh());
            })
          Button("Dispose left")
            .onClick(() => {
              this.left.dispose();
              this.left.dispose();
              this.leftVisible = false;
              this.refresh();
            })
          Button("Recreate left")
            .onClick(() => {
              if (this.left.isDisposed) {
                this.left = new NshSurface();
                this.load(this.left);
                this.leftVisible = true;
                this.prepare(this.left);
              }
            })
          Button("Race")
            .onClick(() => {
              this.race();
            })
          Button("Invalid")
            .onClick(() => {
              this.invalid();
            })
          Button("Disposed checks")
            .onClick(() => {
              let rejected = 0;
              try {
                this.left.loadSource("");
              } catch (_) {
                rejected++;
              }
              try {
                this.left.stop();
              } catch (_) {
                rejected++;
              }
              try {
                this.left.setEnvironment({ fontScale: 1 });
              } catch (_) {
                rejected++;
              }
              this.checks = "disposed:" + rejected;
            })
          Button("Source left")
            .onClick(() => {
              this.left.loadSource(
                "import { mount, Text } from 'nsh'; mount(() => Text({ testID: 'source-loaded', children: 'Source loaded' }));",
              );
              this.refresh();
            })
          Button("Capabilities").onClick(() => {
            const caps = getCapabilities();
            this.checks = `api:${caps.apiLevel}:min:${caps.minimumApiLevel}:23:${caps.api23}:26:${caps.api26}`;
          })
          Button("Status")
            .onClick(() => {
              this.refresh();
            })
          Button("Overlay checks").onClick(() => { this.overlayChecks(); })
          Button("Connect dev").onClick(() => { this.left.connectDevServer("ws://127.0.0.1:8790/ws"); })
          Button("Disconnect dev").onClick(() => { this.left.disconnectDevServer(); })
          Button("Package checks").onClick(() => { this.packageChecks(); })
          Button("Native back").onClick(() => { this.left.activate(); this.checks = "back:" + onBackPressIntercept(); })
        }.margin(8)
        Text(this.status)
          .id("surface-status")
          .fontSize(12)
        Text(this.checks)
          .id("surface-checks")
          .fontSize(12)
        if (this.visible) {
          Flex({ direction: this.swapped ? FlexDirection.RowReverse : FlexDirection.Row }) {
            Column() {
              Text("Left instance")
              Stack() {
                if (this.leftVisible) {
                  ContentSlot(this.left.content);
                }
              }
              .width("100%")
              .layoutWeight(1)
            }
            .width(this.split ? (this.narrow ? "35%" : "50%") : "100%")
            .height("100%")
            if (this.split && this.right) {
              Column() {
                Text("Right instance")
                Stack() {
                  ContentSlot(this.right.content)
                }
                .width("100%")
                .layoutWeight(1)
              }
              .width(this.narrow ? "65%" : "50%")
              .height("100%");
            }
          }
          .width("100%")
          .layoutWeight(1);
        }
      }
      .width("100%")
      .height("100%")
      .backgroundColor("#f4f5f7")
    }
    .width("100%")
    .height("100%");
  }
}
