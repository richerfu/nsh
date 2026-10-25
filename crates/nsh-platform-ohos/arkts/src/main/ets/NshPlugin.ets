import deviceInfo from "@ohos.deviceInfo";
import {
  BridgeCallContext, BridgeContextRequirement, BridgeLifecycleEvent, BridgePluginContext,
  BridgePluginHookContext, BridgeTypedValue, MainThreadSyncBridgePlugin, expectBridgeRequestType,
} from "@ohos-rs/ability";
import { NativeEditors, EditorSettings, EditorCommand } from "./NativeEditors";
import { NativeViews, Settings } from "./NativeViews";

interface Batch {
  editors: EditorSettings[];
  commands: EditorCommand[];
  removeEditors: number[];
  views: Settings[];
  removeViews: number[];
}

/** Private SDK fragments owned by one NativeAbility plugin instance. */
export class NshPlugin implements MainThreadSyncBridgePlugin {
  readonly id: string = "nsh.arkui";
  readonly execution: "sync-main-thread" = "sync-main-thread";
  readonly requires: BridgeContextRequirement[] = ["ability"];
  private context?: BridgePluginContext;
  private editors: NativeEditors = new NativeEditors();
  private views: NativeViews = new NativeViews();
  attachContext(context: BridgePluginContext): void { this.context = context; }
  onInstall(context: BridgePluginHookContext): void {
    context.invokeNativeSync("device-info", "nsh.arkui.DeviceInfo", "ohos.node.Acknowledgement", {
      deviceType: deviceInfo.deviceType,
      cacheDir: context.abilityContext.cacheDir,
    });
  }
  invokeSync(action: string, payload: BridgeTypedValue, call: BridgeCallContext): BridgeTypedValue {
    this.requireUI(call);
    expectBridgeRequestType(payload, this.id, action, "nsh.arkui.Batch");
    if (action !== "apply") { throw new Error("Unsupported NSH SDK operation"); }
    const batch = payload.value as Batch;
    this.editors.remove(batch.removeEditors, this.context!);
    this.requireUI(call);
    this.views.remove(batch.removeViews, this.context!);
    this.requireUI(call);
    this.editors.update(batch.editors, batch.commands, this.context!);
    this.requireUI(call);
    this.views.update(batch.views, this.context!);
    return { typeName: "ohos.node.Acknowledgement", value: { accepted: true } };
  }
  private requireUI(call: BridgeCallContext): void {
    if (!call.isActive() || !this.context?.isActive()) { throw new Error("NSH SDK session is inactive"); }
    // Installation needs only the Ability; SDK actions need the current attached UIContext.
    call.getUIContext();
  }
  onClosing(context: BridgePluginHookContext): void {
    try { this.views.dispose(context); } finally { this.editors.dispose(context); }
  }
  async onLifecycle(event: BridgeLifecycleEvent, context: BridgePluginHookContext): Promise<void> {
    if (event.kind === "ui-context-destroy") { await this.onDispose(context); }
  }
  async onDispose(context: BridgePluginHookContext): Promise<void> {
    try { this.onClosing(context); } finally { await this.views.settled(); }
  }
}
