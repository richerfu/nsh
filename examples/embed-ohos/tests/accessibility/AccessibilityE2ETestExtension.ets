import {
  AccessibilityElement,
  AccessibilityEvent,
  AccessibilityExtensionAbility,
  FocusDirection,
} from "@kit.AccessibilityKit";
import { hilog } from "@kit.PerformanceAnalysisKit";

const LOG_DOMAIN: number = 0x0000;
const LOG_TAG: string = "NshA11yE2E";

/**
 * Small screen-reader-style service used only by the QEMU acceptance suite.
 *
 * The public OpenHarmony QEMU image exposes AccessibilityManagerService but
 * does not bundle a user-facing screen reader. This extension drives the same
 * focus and performAction path without replacing or patching the image.
 */
export default class AccessibilityE2ETestExtension extends AccessibilityExtensionAbility {
  private focusedElement: AccessibilityElement | undefined = undefined;

  onConnect(): void {
    hilog.info(LOG_DOMAIN, LOG_TAG, "connected");
  }

  onDisconnect(): void {
    hilog.info(LOG_DOMAIN, LOG_TAG, "disconnected");
    this.focusedElement = undefined;
  }

  onAccessibilityEvent(event: AccessibilityEvent): void {
    const eventType: string = `${event.eventType}`;
    hilog.info(LOG_DOMAIN, LOG_TAG, "event=%{public}s announcement=%{public}s element=%{public}d",
      eventType, event.textAnnouncedForAccessibility ?? "", event.elementId ?? -1);
    switch (eventType) {
      case "right":
        this.moveFocus("forward");
        break;
      case "left":
        this.moveFocus("backward");
        break;
      case "rightThenLeft":
        this.performRangeAction("scrollForward");
        break;
      case "leftThenRight":
        this.performRangeAction("scrollBackward");
        break;
      case "down":
        this.performFocusedAction("click");
        break;
      case "up":
        this.performFocusedAction("executeCustomAction");
        break;
      case "hoverEnter":
        if (event.target !== undefined) {
          this.focusHoveredTarget(event.target);
        }
        break;
      case "requestFocusForAccessibility":
        if (event.elementId !== undefined) {
          this.focusRequestedElement(event.elementId).catch((error: Error): void => {
            hilog.error(LOG_DOMAIN, LOG_TAG, "requested focus failed: %{public}s", error.message);
          });
        }
        break;
      default:
        break;
    }
  }

  private async focusRequestedElement(id: number): Promise<void> {
    const root = await this.context.getWindowRootElement();
    const target = await this.findById(root, id, 0);
    if (target === undefined) { throw new Error(`Requested element ${id} was not found`); }
    await target.performAction("accessibilityFocus");
    this.focusedElement = target;
    hilog.info(LOG_DOMAIN, LOG_TAG, "requested-focus component=%{public}d label=%{public}s",
      id, await target.attributeValue("accessibilityText"));
  }

  private async findById(element: AccessibilityElement, id: number, depth: number): Promise<AccessibilityElement | undefined> {
    if (depth > 32) { return undefined; }
    if (await element.attributeValue("componentId") === id) { return element; }
    for (const child of await element.attributeValue("children")) {
      const found = await this.findById(child, id, depth + 1);
      if (found !== undefined) { return found; }
    }
    return undefined;
  }

  private moveFocus(direction: FocusDirection): void {
    this.moveFocusAsync(direction).catch((error: Error): void => {
      hilog.error(LOG_DOMAIN, LOG_TAG, "moveFocus failed: %{public}s", error.message);
    });
  }

  private async moveFocusAsync(direction: FocusDirection): Promise<void> {
    let current: AccessibilityElement;
    try {
      current = await this.context.getFocusElement(true);
    } catch {
      current = await this.context.getWindowRootElement();
    }
    const source: number = await current.attributeValue("componentId");
    let next: AccessibilityElement = current;
    let found: boolean = false;
    for (let step = 0; step < 32; step++) {
      next = await current.findElement("focusDirection", direction);
      if (await next.attributeValue("componentId") === await current.attributeValue("componentId")) {
        throw new Error("Focus traversal returned the same element");
      }
      const label = await next.attributeValue("accessibilityText");
      const actions = await next.actionNames();
      const type = await next.attributeValue("componentType");
      hilog.info(LOG_DOMAIN, LOG_TAG, "traverse component=%{public}d type=%{public}s label=%{public}s actions=%{public}s",
        await next.attributeValue("componentId"), type, label, actions.join(","));
      // Provider boundaries may return a transparent native container first.
      if (type !== "root" && (label.length > 0 || actions.some(action => action === "click" || action === "longClick" || action === "scrollForward" || action === "executeCustomAction"))) {
        found = true;
        break;
      }
      await next.performAction("accessibilityFocus");
      current = next;
    }
    if (!found) { throw new Error("Focus traversal found no semantic element"); }
    await next.performAction("accessibilityFocus");
    this.focusedElement = next;
    hilog.info(LOG_DOMAIN, LOG_TAG, "moved-focus from=%{public}d to=%{public}d type=%{public}s label=%{public}s",
      source, await next.attributeValue("componentId"),
      await next.attributeValue("componentType"), await next.attributeValue("accessibilityText"));
  }

  private focusHoveredTarget(target: AccessibilityElement): void {
    this.focusHoveredTargetAsync(target).catch((error: Error): void => {
      hilog.error(LOG_DOMAIN, LOG_TAG, "focusHoveredTarget failed: %{public}s", error.message);
    });
  }

  private async focusHoveredTargetAsync(target: AccessibilityElement): Promise<void> {
    const actionable: AccessibilityElement | undefined = await this.findActionable(target, 0);
    if (actionable === undefined) {
      throw new Error("hovered accessibility subtree has no actionable element");
    }
    await actionable.performAction("accessibilityFocus");
    this.focusedElement = actionable;
    const actions: string[] = await actionable.actionNames();
    const componentId: number = await actionable.attributeValue("componentId");
    const checkable: boolean = await actionable.attributeValue("checkable");
    let accessibilityText: string = "";
    try {
      accessibilityText = await actionable.attributeValue("accessibilityText");
    } catch {}
    hilog.info(
      LOG_DOMAIN,
      LOG_TAG,
      "focused a11y=%{public}s actions=%{public}s component=%{public}d checkable=%{public}s",
      accessibilityText,
      actions.join(","),
      componentId,
      `${checkable}`,
    );
    if (await actionable.attributeValue("componentType") === "slider") {
      hilog.info(LOG_DOMAIN, LOG_TAG, "range min=%{public}d max=%{public}d now=%{public}d",
        await actionable.attributeValue("valueMin"), await actionable.attributeValue("valueMax"),
        await actionable.attributeValue("valueNow"));
    }
  }

  private async findActionable(
    element: AccessibilityElement,
    depth: number,
  ): Promise<AccessibilityElement | undefined> {
    if (depth > 8) {
      return undefined;
    }
    try {
      const actions: string[] = await element.actionNames();
      const id: number = await element.attributeValue("componentId");
      const type: string = await element.attributeValue("componentType");
      hilog.info(LOG_DOMAIN, LOG_TAG, "candidate component=%{public}d type=%{public}s actions=%{public}s", id, type, actions.join(","));
      if (actions.includes("click") || actions.includes("executeCustomAction") || actions.includes("scrollForward")) {
        return element;
      }
    } catch {}

    let children: AccessibilityElement[];
    try {
      children = await element.attributeValue("children");
    } catch {
      return undefined;
    }
    for (const child of children) {
      const target: AccessibilityElement | undefined = await this.findActionable(child, depth + 1);
      if (target !== undefined) {
        return target;
      }
    }
    return undefined;
  }

  private performRangeAction(action: "scrollForward" | "scrollBackward"): void {
    this.performRangeActionAsync(action).catch((error: Error): void => {
      hilog.error(LOG_DOMAIN, LOG_TAG, "range failed: %{public}s", error.message);
    });
  }
  private async performRangeActionAsync(action: "scrollForward" | "scrollBackward"): Promise<void> {
    const element = await this.context.getFocusElement(true);
    await element.performAction(action);
    hilog.info(LOG_DOMAIN, LOG_TAG, "performed=%{public}s", action);
  }
  private performFocusedAction(action: "click" | "executeCustomAction"): void {
    this.performFocusedActionAsync(action).catch((error: Error): void => {
      hilog.error(LOG_DOMAIN, LOG_TAG, "%{public}s failed: %{public}s", action, error.message);
    });
  }

  private async performFocusedActionAsync(action: "click" | "executeCustomAction"): Promise<void> {
    let element: AccessibilityElement;
    try {
      element = await this.context.getFocusElement(true);
    } catch {
      if (this.focusedElement === undefined) {
        throw new Error("no accessibility-focused element");
      }
      element = this.focusedElement;
    }
    const actions: string[] = await element.actionNames();
    if (action !== "executeCustomAction" && !actions.includes(action)) {
      throw new Error(`focused element does not support ${action}`);
    }
    if (action === "executeCustomAction") {
      await element.performAction(action, { customAction: "Toggle choice" });
    } else {
      await element.performAction(action);
    }
    hilog.info(LOG_DOMAIN, LOG_TAG, "performed=%{public}s", action);
  }
}
