import { initLogging } from "libnsh_platform_ohos.so";
import { NativeAbility, LazyPlugin } from "@ohos-rs/ability";
import { ResourcePlugin } from "@ohos-rs/ability-plugin-resource";
import { AccessibilityPlugin } from "@ohos-rs/ability-plugin-accessibility";
import { nativeShellPlugins } from "@nsh/ohos";
import { ClipboardPlugin } from "@ohos-rs/ability-plugin-clipboard";
import { WindowPlugin } from "@ohos-rs/ability-plugin-window";
import { WebviewPlugin } from "@ohos-rs/ability-plugin-webview";
import { UrlPlugin } from "@ohos-rs/ability-plugin-url";
import { window } from "@kit.ArkUI";

export default class EntryAbility extends NativeAbility {
  public moduleName: string = "nsh_platform_ohos";
  public defaultPage: boolean = false;
  public bridgePlugins = [
    new LazyPlugin(() => new ResourcePlugin()),
    new LazyPlugin(() => new AccessibilityPlugin()),
    ...nativeShellPlugins(),
    new LazyPlugin(() => new ClipboardPlugin({ metadataMimeType: "application/x-nsh-clipboard" })),
    new LazyPlugin(() => new WindowPlugin()),
    new LazyPlugin(() => new WebviewPlugin()),
    new LazyPlugin(() => new UrlPlugin()),
  ];

  protected async loadWindowStageContent(stage: window.WindowStage): Promise<void> {
    initLogging();
    await stage.loadContent("pages/Index");
  }
}
