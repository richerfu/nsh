import {
  AbilityConstant,
  AbilityLifecycleCallback,
  ApplicationStateChangeCallback,
  Configuration,
  EnvironmentCallback,
  UIAbility,
  application,
  common,
} from "@kit.AbilityKit";
import { accessibility } from "@kit.AccessibilityKit";
import { NodeContent, window } from "@kit.ArkUI";
import { FrameNode } from "@ohos.arkui.node";
import { UIContext } from "@ohos.arkui.UIContext";
import deviceInfo from "@ohos.deviceInfo";
import {
  EditorEventPayload,
  EditorReceipt,
  MenuActionPayload,
  NshRuntimeContext,
  WebEventPayload,
} from "./NativeEditors";
import { Batch, NshPlugin } from "./NshPlugin";

interface AvoidRectPayload {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface AvoidAreaPayload {
  visible: boolean;
  leftRect: AvoidRectPayload;
  topRect: AvoidRectPayload;
  rightRect: AvoidRectPayload;
  bottomRect: AvoidRectPayload;
}

interface HostInitPayload {
  sdkApiVersion: number;
  deviceType: string;
  cacheDir: string;
  colorMode: number;
  fontSizeScale: number;
  language: string;
  screenReader: boolean;
  reduceMotion?: boolean;
}

interface HostCommandPayload {
  kind: string;
  enabled?: boolean;
  lightContent?: boolean;
  animated?: boolean;
  text?: string;
  assertive?: boolean;
  announcement?: string;
  elementId?: number;
  openId?: number;
  url?: string;
}

interface NshNativeModule {
  nshRegisterBootstrap(sdkApiVersion: number, bootstrap: () => void): void;
  nshInitHost(
    init: HostInitPayload,
    resourceManager: object,
    onCommand: (command: HostCommandPayload) => void,
  ): void;
  nshDestroyHost(): void;
  nshSetForeground(foreground: boolean): void;
  nshUpdateConfig(
    colorMode: number,
    fontSizeScale: number,
    language: string,
    deviceType?: string,
  ): void;
  nshUpdateAccessibility(screenReader?: boolean, reduceMotion?: boolean): void;
  nshAttachUi(
    renderOwner: string,
    platformWindowId: number | undefined,
    applyBatch: (batch: Batch) => void,
  ): void;
  nshDetachUi(renderOwner: string): void;
  nshUpdateWindow(
    renderOwner: string,
    platformWindowId: number,
    visual: AvoidAreaPayload[],
    keyboard: AvoidAreaPayload,
  ): void;
  nshOpenUrlResult(openId: number, opened: boolean): void;
  nshReportHostError(message: string): void;
  nshAttachNativeNode(targetId: number, content: FrameNode): void;
  nshAttachNativeSubtree(targetId: number, content: FrameNode, slot: NodeContent): void;
  nshRetireNativeNode(targetId: number): void;
  nshEditorEvent(targetId: number, event: EditorEventPayload): EditorReceipt;
  nshViewAction(action: MenuActionPayload): boolean;
  nshWebControllerAttached(targetId: number, nativeTag: string): boolean;
  nshWebNavigate(targetId: number, url: string): boolean;
  nshWebEvent(event: WebEventPayload): boolean;
  disposeAllRenders(): void;
  onBackPressIntercept(): boolean;
}

let cachedNativeModule: NshNativeModule | undefined = undefined;

function getNativeModule(): NshNativeModule {
  if (cachedNativeModule !== undefined) {
    return cachedNativeModule;
  }
  const libraryName = "libnsh_platform_ohos.so";
  let loaded: ESObject;
  try {
    loaded = loadNativeModule(libraryName);
  } catch (error) {
    throw new Error(`${libraryName} failed to load: ${String(error)}`);
  }
  const candidate: ESObject =
    typeof loaded?.nshInitHost === "function" ? loaded : loaded?.default;
  const mod = candidate as NshNativeModule;
  if (!mod || typeof mod.nshInitHost !== "function") {
    throw new Error(`${libraryName} does not export the Native Shell OHOS host ABI`);
  }
  cachedNativeModule = mod;
  return mod;
}

function toAvoidRect(rect: window.Rect): AvoidRectPayload {
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
  };
}

function toAvoidArea(area: window.AvoidArea): AvoidAreaPayload {
  return {
    visible: area.visible,
    leftRect: toAvoidRect(area.leftRect),
    topRect: toAvoidRect(area.topRect),
    rightRect: toAvoidRect(area.rightRect),
    bottomRect: toAvoidRect(area.bottomRect),
  };
}

function emptyAvoidArea(): AvoidAreaPayload {
  const zero: AvoidRectPayload = { left: 0, top: 0, width: 0, height: 0 };
  return {
    visible: false,
    leftRect: zero,
    topRect: zero,
    rightRect: zero,
    bottomRect: zero,
  };
}

function safeAvoidArea(win: window.Window, type: window.AvoidAreaType): AvoidAreaPayload {
  try {
    return toAvoidArea(win.getWindowAvoidArea(type));
  } catch {
    return emptyAvoidArea();
  }
}

let nextRenderOwnerSequence = 1;

class ComponentRuntimeContext implements NshRuntimeContext {
  private readonly ui: UIContext;
  private readonly mod: NshNativeModule;
  private active: boolean = true;

  constructor(ui: UIContext, mod: NshNativeModule) {
    this.ui = ui;
    this.mod = mod;
  }

  deactivate(): void {
    this.active = false;
  }

  isActive(): boolean {
    return this.active;
  }

  getUIContext(): UIContext {
    return this.ui;
  }

  attachNativeNode(targetId: number, content: FrameNode | null): boolean {
    if (!this.active || content === null) {
      return false;
    }
    try {
      this.mod.nshAttachNativeNode(targetId, content);
      return true;
    } catch (error) {
      console.error(`[NSH] attachNativeNode(${targetId}) failed: ${String(error)}`);
      return false;
    }
  }

  attachNativeSubtree(targetId: number, content: FrameNode | null, slot: NodeContent): boolean {
    if (!this.active || content === null) {
      return false;
    }
    try {
      this.mod.nshAttachNativeSubtree(targetId, content, slot);
      return true;
    } catch (error) {
      console.error(`[NSH] attachNativeSubtree(${targetId}) failed: ${String(error)}`);
      return false;
    }
  }

  retireNativeNode(targetId: number): boolean {
    try {
      this.mod.nshRetireNativeNode(targetId);
      return true;
    } catch (error) {
      console.error(`[NSH] retireNativeNode(${targetId}) failed: ${String(error)}`);
      return false;
    }
  }

  editorEvent(targetId: number, event: EditorEventPayload): EditorReceipt {
    if (!this.active) {
      return { accepted: false, count: 0 };
    }
    try {
      return this.mod.nshEditorEvent(targetId, event);
    } catch {
      return { accepted: false, count: 0 };
    }
  }

  viewAction(action: MenuActionPayload): boolean {
    if (!this.active) {
      return false;
    }
    try {
      return this.mod.nshViewAction(action);
    } catch {
      return false;
    }
  }

  webControllerAttached(targetId: number, nativeTag: string): boolean {
    if (!this.active) {
      return false;
    }
    try {
      return this.mod.nshWebControllerAttached(targetId, nativeTag);
    } catch {
      return false;
    }
  }

  webNavigate(targetId: number, url: string): boolean {
    if (!this.active) {
      return true;
    }
    try {
      return this.mod.nshWebNavigate(targetId, url);
    } catch {
      return true;
    }
  }

  webEvent(event: WebEventPayload): boolean {
    if (!this.active) {
      return false;
    }
    try {
      return this.mod.nshWebEvent(event);
    } catch {
      return false;
    }
  }
}

class HostEnvironmentListener extends EnvironmentCallback {
  private readonly owner: NshHostRuntime;
  constructor(owner: NshHostRuntime) {
    super();
    this.owner = owner;
  }
  onConfigurationUpdated(config: Configuration): void {
    this.owner.onConfigurationUpdated(config);
  }
  onMemoryLevel(_level: AbilityConstant.MemoryLevel): void {}
}

class HostStateChangeListener extends ApplicationStateChangeCallback {
  private readonly owner: NshHostRuntime;
  constructor(owner: NshHostRuntime) {
    super();
    this.owner = owner;
  }
  onApplicationForeground(): void {
    this.owner.onForegroundChanged(true);
  }
  onApplicationBackground(): void {
    this.owner.onForegroundChanged(false);
  }
}

class HostLifecycleListener extends AbilityLifecycleCallback {
  private readonly owner: NshHostRuntime;
  constructor(owner: NshHostRuntime) {
    super();
    this.owner = owner;
  }
  onAbilityCreate(ability: UIAbility): void {
    this.owner.onAbilityCaptured(ability.context);
  }
  onWindowStageCreate(ability: UIAbility, windowStage: window.WindowStage): void {
    this.owner.onWindowStageCaptured(ability.context, windowStage);
  }
  onWindowStageActive(_ability: UIAbility, _windowStage: window.WindowStage): void {
    this.owner.onForegroundChanged(true);
  }
  onWindowStageInactive(_ability: UIAbility, _windowStage: window.WindowStage): void {}
  onWindowStageDestroy(_ability: UIAbility, windowStage: window.WindowStage): void {
    this.owner.onWindowStageDestroyed(windowStage);
  }
  onAbilityDestroy(ability: UIAbility): void {
    this.owner.onAbilityDestroyed(ability.context);
  }
  onAbilityForeground(_ability: UIAbility): void {
    this.owner.onForegroundChanged(true);
  }
  onAbilityBackground(_ability: UIAbility): void {
    this.owner.onForegroundChanged(false);
  }
  onAbilityContinue(_ability: UIAbility): void {}
}

class NshHostRuntime {
  private mod?: NshNativeModule;
  private hostInitialized: boolean = false;
  private appContext?: common.ApplicationContext;
  private abilityContext?: common.UIAbilityContext;
  private observedWindowStage?: window.WindowStage;
  private mainWindow?: window.Window;
  private uiContext?: UIContext;
  private renderOwner: string = "";
  private windowId: number = 0;
  private runtimeContext?: ComponentRuntimeContext;
  private plugin: NshPlugin = new NshPlugin();
  private envCallbackId?: number;
  private lifecycleCallbackId?: number;
  private stateListenerRegistered: boolean = false;
  private stateListener?: HostStateChangeListener;
  private screenReaderHandler?: (enabled: boolean) => void;
  private reduceMotionHandler?: (enabled: boolean) => void;
  private windowResolving: boolean = false;

  private readonly onWindowStageEvent = (event: window.WindowStageEventType): void => {
    const mod = this.mod;
    if (!mod) {
      return;
    }
    if (
      event === window.WindowStageEventType.SHOWN ||
      event === window.WindowStageEventType.ACTIVE ||
      event === window.WindowStageEventType.RESUMED
    ) {
      mod.nshSetForeground(true);
    } else if (
      event === window.WindowStageEventType.INACTIVE ||
      event === window.WindowStageEventType.PAUSED ||
      event === window.WindowStageEventType.HIDDEN
    ) {
      mod.nshSetForeground(false);
    }
  };

  private readonly onAvoidAreaChange = (_data: window.AvoidAreaOptions): void => {
    this.syncWindowMetrics();
  };

  private readonly onWindowSizeChange = (_size: window.Size): void => {
    this.syncWindowMetrics();
  };

  private readonly onKeyboardHeightChange = (_height: number): void => {
    this.syncWindowMetrics();
  };

  registerBootstrap(): void {
    const mod = getNativeModule();
    this.mod = mod;
    mod.nshRegisterBootstrap(deviceInfo.sdkApiVersion, (): void => {
      this.ensureReady();
    });
  }

  ensureReady(): void {
    const mod = this.mod ?? getNativeModule();
    this.mod = mod;

    let resolvedUi: UIContext | undefined = this.uiContext;
    if (resolvedUi !== undefined) {
      try {
        if (!resolvedUi.isAvailable()) {
          resolvedUi = undefined;
        }
      } catch {
        resolvedUi = undefined;
      }
    }

    let resolvedAbility: common.UIAbilityContext | undefined = this.abilityContext;
    let resolvedWindow: window.Window | undefined = this.mainWindow;

    if (resolvedAbility === undefined || resolvedWindow === undefined || resolvedUi === undefined) {
      try {
        const ctx = getContext() as common.UIAbilityContext | undefined;
        if (ctx !== undefined && typeof ctx.cacheDir === "string") {
          resolvedAbility = ctx;
          if (ctx.windowStage !== undefined) {
            this.observeWindowStage(ctx.windowStage);
            resolvedWindow = ctx.windowStage.getMainWindowSync();
            resolvedUi = resolvedWindow.getUIContext();
          }
        }
      } catch {}
    }

    if (resolvedUi === undefined) {
      try {
        const resolved = UIContext.resolveUIContext();
        if (resolved !== undefined && resolved.isAvailable()) {
          resolvedUi = resolved;
        }
      } catch {}
    }

    if (resolvedUi === undefined) {
      try {
        const all = UIContext.getAllUIContexts();
        if (all.length > 0 && all[0].isAvailable()) {
          resolvedUi = all[0];
        }
      } catch {}
    }

    if (resolvedUi !== undefined && resolvedAbility === undefined) {
      try {
        const hostCtx = resolvedUi.getHostContext() as common.UIAbilityContext | undefined;
        if (hostCtx !== undefined && typeof hostCtx.cacheDir === "string") {
          resolvedAbility = hostCtx;
          if (hostCtx.windowStage !== undefined) {
            this.observeWindowStage(hostCtx.windowStage);
          }
        }
      } catch {}
    }

    if (resolvedWindow === undefined) {
      if (resolvedAbility !== undefined && resolvedAbility.windowStage !== undefined) {
        try {
          resolvedWindow = resolvedAbility.windowStage.getMainWindowSync();
        } catch {}
      }
      if (resolvedWindow === undefined && resolvedUi !== undefined) {
        try {
          const windowName = resolvedUi.getWindowName();
          if (windowName !== undefined && windowName.length > 0) {
            resolvedWindow = window.findWindow(windowName);
          }
        } catch {}
      }
    }

    if (resolvedAbility !== undefined) {
      this.abilityContext = resolvedAbility;
    }

    let appCtx = this.appContext;
    if (appCtx === undefined) {
      try {
        appCtx =
          resolvedAbility !== undefined
            ? resolvedAbility.getApplicationContext()
            : application.getApplicationContext();
        this.appContext = appCtx;
      } catch {}
    }

    const baseCtx: common.Context | undefined = resolvedAbility ?? appCtx;
    if (!this.hostInitialized && baseCtx !== undefined) {
      let screenReader = false;
      try {
        screenReader = accessibility.isScreenReaderOpenSync();
      } catch {}

      let reduceMotion: boolean | undefined = undefined;
      if (deviceInfo.sdkApiVersion >= 23) {
        try {
          reduceMotion = accessibility.isAnimationReduceEnabledSync();
        } catch {}
      }

      const config = resolvedAbility?.config;
      mod.nshInitHost(
        {
          sdkApiVersion: deviceInfo.sdkApiVersion,
          deviceType: deviceInfo.deviceType ?? "phone",
          cacheDir: baseCtx.cacheDir ?? "",
          colorMode: config?.colorMode ?? -1,
          fontSizeScale: config?.fontSizeScale ?? 1.0,
          language: config?.language ?? "",
          screenReader,
          reduceMotion,
        },
        baseCtx.resourceManager,
        (command: HostCommandPayload): void => {
          this.handleHostCommand(command);
        },
      );
      this.hostInitialized = true;
      this.subscribeProcessListeners(appCtx);
      this.subscribeAccessibility();
    } else if (this.hostInitialized && resolvedAbility?.config !== undefined) {
      const cfg = resolvedAbility.config;
      try {
        mod.nshUpdateConfig(
          cfg.colorMode ?? -1,
          cfg.fontSizeScale ?? 1.0,
          cfg.language ?? "",
          deviceInfo.deviceType,
        );
      } catch {}
    }

    if (resolvedUi !== undefined) {
      this.attachUiAndWindow(resolvedUi, resolvedWindow);
    } else if (!this.windowResolving && baseCtx !== undefined) {
      this.windowResolving = true;
      window
        .getLastWindow(baseCtx)
        .then((win: window.Window): void => {
          this.windowResolving = false;
          try {
            const ui = win.getUIContext();
            const hostCtx = ui.getHostContext() as common.UIAbilityContext | undefined;
            if (hostCtx !== undefined) {
              this.onAbilityCaptured(hostCtx);
            }
            this.attachUiAndWindow(ui, win);
          } catch {}
        })
        .catch((): void => {
          this.windowResolving = false;
        });
    }
  }

  onAbilityCaptured(context: common.UIAbilityContext): void {
    this.abilityContext = context;
    if (this.hostInitialized && context.config !== undefined) {
      this.onConfigurationUpdated(context.config);
    }
  }

  onWindowStageCaptured(context: common.UIAbilityContext, stage: window.WindowStage): void {
    this.abilityContext = context;
    this.observeWindowStage(stage);
    try {
      const win = stage.getMainWindowSync();
      const ui = win.getUIContext();
      this.attachUiAndWindow(ui, win);
    } catch {}
  }

  onWindowStageDestroyed(stage: window.WindowStage): void {
    if (this.observedWindowStage === stage) {
      try {
        stage.off("windowStageEvent", this.onWindowStageEvent);
      } catch {}
      this.observedWindowStage = undefined;
    }
    this.detachUi();
    this.mod?.disposeAllRenders();
  }

  onAbilityDestroyed(context: common.UIAbilityContext): void {
    if (this.abilityContext === context) {
      this.abilityContext = undefined;
    }
    this.detachUi();
  }

  onForegroundChanged(foreground: boolean): void {
    this.mod?.nshSetForeground(foreground);
  }

  onConfigurationUpdated(newConfig: Configuration): void {
    this.mod?.nshUpdateConfig(
      newConfig.colorMode ?? -1,
      newConfig.fontSizeScale ?? 1.0,
      newConfig.language ?? "",
      deviceInfo.deviceType,
    );
  }

  private observeWindowStage(stage: window.WindowStage): void {
    if (this.observedWindowStage === stage) {
      return;
    }
    if (this.observedWindowStage !== undefined) {
      try {
        this.observedWindowStage.off("windowStageEvent", this.onWindowStageEvent);
      } catch {}
    }
    this.observedWindowStage = stage;
    try {
      stage.on("windowStageEvent", this.onWindowStageEvent);
    } catch {}
  }

  private attachUiAndWindow(ui: UIContext, win: window.Window | undefined): void {
    const mod = this.mod;
    if (!mod || !this.hostInitialized) {
      return;
    }
    if (this.renderOwner && this.uiContext === ui) {
      if (win !== undefined && this.mainWindow !== win) {
        this.bindWindowListeners(win);
        this.syncWindowMetrics();
      }
      return;
    }
    if (this.renderOwner) {
      this.detachUi();
    }

    const renderOwner = `nsh-ui-${nextRenderOwnerSequence++}`;
    this.renderOwner = renderOwner;
    this.uiContext = ui;

    let resolvedWin = win;
    if (resolvedWin === undefined) {
      try {
        const windowName = ui.getWindowName();
        if (windowName) {
          resolvedWin = window.findWindow(windowName);
        }
      } catch {}
    }

    let windowId = 0;
    if (resolvedWin !== undefined) {
      try {
        windowId = resolvedWin.getWindowProperties().id;
      } catch {}
    }
    this.windowId = windowId;

    const runtimeCtx = new ComponentRuntimeContext(ui, mod);
    this.runtimeContext = runtimeCtx;

    mod.nshAttachUi(renderOwner, windowId, (batch: Batch): void => {
      this.plugin.apply(batch, runtimeCtx);
    });

    if (resolvedWin !== undefined) {
      this.bindWindowListeners(resolvedWin);
      this.syncWindowMetrics();
    }
  }

  private bindWindowListeners(win: window.Window): void {
    if (this.mainWindow === win) {
      return;
    }
    if (this.mainWindow !== undefined) {
      try {
        this.mainWindow.off("avoidAreaChange", this.onAvoidAreaChange);
      } catch {}
      try {
        this.mainWindow.off("windowSizeChange", this.onWindowSizeChange);
      } catch {}
      try {
        this.mainWindow.off("keyboardHeightChange", this.onKeyboardHeightChange);
      } catch {}
    }
    this.mainWindow = win;
    try {
      this.windowId = win.getWindowProperties().id;
    } catch {}
    try {
      win.on("avoidAreaChange", this.onAvoidAreaChange);
    } catch {}
    try {
      win.on("windowSizeChange", this.onWindowSizeChange);
    } catch {}
    try {
      win.on("keyboardHeightChange", this.onKeyboardHeightChange);
    } catch {}
  }

  private detachUi(): void {
    const win = this.mainWindow;
    this.mainWindow = undefined;
    if (win !== undefined) {
      try {
        win.off("avoidAreaChange", this.onAvoidAreaChange);
      } catch {}
      try {
        win.off("windowSizeChange", this.onWindowSizeChange);
      } catch {}
      try {
        win.off("keyboardHeightChange", this.onKeyboardHeightChange);
      } catch {}
    }

    const runtimeCtx = this.runtimeContext;
    this.runtimeContext = undefined;
    this.uiContext = undefined;
    if (runtimeCtx !== undefined) {
      try {
        this.plugin.dispose(runtimeCtx);
      } catch {}
      runtimeCtx.deactivate();
      void this.plugin.settled();
    }

    if (this.mod !== undefined && this.renderOwner) {
      try {
        this.mod.nshDetachUi(this.renderOwner);
      } catch {}
    }
    this.renderOwner = "";
  }

  private syncWindowMetrics(): void {
    const mod = this.mod;
    const win = this.mainWindow;
    const owner = this.renderOwner;
    if (!mod || !win || !owner) {
      return;
    }
    const visual: AvoidAreaPayload[] = [
      safeAvoidArea(win, window.AvoidAreaType.TYPE_SYSTEM),
      safeAvoidArea(win, window.AvoidAreaType.TYPE_CUTOUT),
      safeAvoidArea(win, window.AvoidAreaType.TYPE_NAVIGATION_INDICATOR),
    ];
    const keyboard = safeAvoidArea(win, window.AvoidAreaType.TYPE_KEYBOARD);
    try {
      mod.nshUpdateWindow(owner, this.windowId, visual, keyboard);
    } catch {}
  }

  private subscribeProcessListeners(appCtx: common.ApplicationContext | undefined): void {
    if (!appCtx) {
      return;
    }
    if (this.envCallbackId === undefined) {
      try {
        this.envCallbackId = appCtx.on("environment", new HostEnvironmentListener(this));
      } catch {}
    }
    if (this.lifecycleCallbackId === undefined) {
      try {
        this.lifecycleCallbackId = appCtx.on("abilityLifecycle", new HostLifecycleListener(this));
      } catch {}
    }
    if (!this.stateListenerRegistered) {
      try {
        const listener = new HostStateChangeListener(this);
        appCtx.on("applicationStateChange", listener);
        this.stateListener = listener;
        this.stateListenerRegistered = true;
      } catch {}
    }
  }

  private subscribeAccessibility(): void {
    const mod = this.mod;
    if (!mod) {
      return;
    }
    if (this.screenReaderHandler === undefined) {
      const srHandler = (enabled: boolean): void => {
        try {
          mod.nshUpdateAccessibility(enabled, undefined);
        } catch {}
      };
      try {
        accessibility.on("screenReaderStateChange", srHandler);
        this.screenReaderHandler = srHandler;
      } catch {}
    }

    if (deviceInfo.sdkApiVersion >= 23 && this.reduceMotionHandler === undefined) {
      const rmHandler = (enabled: boolean): void => {
        try {
          mod.nshUpdateAccessibility(undefined, enabled);
        } catch {}
      };
      try {
        accessibility.onAnimationReduceStateChange(rmHandler);
        this.reduceMotionHandler = rmHandler;
      } catch {}
    }
  }

  private bundleName(): string {
    return (
      this.abilityContext?.abilityInfo?.bundleName ??
      this.appContext?.applicationInfo?.name ??
      ""
    );
  }

  private handleHostCommand(command: HostCommandPayload): void {
    const mod = this.mod;
    if (!mod) {
      return;
    }
    if (command.kind === "status-bar") {
      const win = this.mainWindow;
      if (!win) {
        return;
      }
      const enabled = command.enabled ?? true;
      const animated = command.animated ?? false;
      const lightContent = command.lightContent ?? false;
      win
        .setSpecificSystemBarEnabled("status", enabled, animated)
        .then((): Promise<void> => {
          return win.setWindowSystemBarProperties({
            statusBarContentColor: lightContent ? "#FFFFFFFF" : "#FF000000",
          });
        })
        .catch((error: Error): void => {
          mod.nshReportHostError(`OHOS status bar update failed: ${String(error)}`);
        });
      return;
    }

    if (command.kind === "announce") {
      const text = command.text ?? "";
      if (!text) {
        return;
      }
      accessibility
        .sendAccessibilityEvent({
          type: "announceForAccessibility",
          bundleName: this.bundleName(),
          triggerAction: "common",
          textAnnouncedForAccessibility: text,
        })
        .catch((): void => {});
      return;
    }

    if (command.kind === "accessibility-event") {
      const announcement = command.announcement ?? undefined;
      const elementId = command.elementId;
      accessibility
        .sendAccessibilityEvent({
          type: announcement ? "announceForAccessibility" : "requestFocusForAccessibility",
          bundleName: this.bundleName(),
          triggerAction: "common",
          textAnnouncedForAccessibility: announcement,
          customId: elementId !== undefined && elementId !== null ? String(elementId) : undefined,
        })
        .catch((): void => {});
      return;
    }

    if (command.kind === "open-url") {
      const openId = command.openId ?? 0;
      const url = command.url ?? "";
      const abilityCtx = this.abilityContext;
      if (!url || !/^[A-Za-z][A-Za-z0-9+.-]*:\S+$/.test(url) || !abilityCtx) {
        mod.nshOpenUrlResult(openId, false);
        return;
      }
      abilityCtx
        .openLink(url)
        .then((): void => {
          mod.nshOpenUrlResult(openId, true);
        })
        .catch((): void => {
          abilityCtx
            .startAbility({
              action: "ohos.want.action.viewData",
              uri: url,
            })
            .then((): void => {
              mod.nshOpenUrlResult(openId, true);
            })
            .catch((): void => {
              mod.nshOpenUrlResult(openId, false);
            });
        });
    }
  }
}

const hostRuntime = new NshHostRuntime();
hostRuntime.registerBootstrap();
