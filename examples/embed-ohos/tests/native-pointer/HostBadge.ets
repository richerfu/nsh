import { ComponentContent, UIContext } from "@kit.ArkUI";
import { NshNativeProperty, NshSurface } from "libnsh_platform_ohos.so";

class BadgeParams {
  text: string;
  color: string;
  onTap: () => void;
  opacity: number;
  mouseProbe: string;
  onMouse?: (event: MouseEvent) => void;
  onAxis?: (event: AxisEvent) => void;
  constructor(
    text: string,
    color: string,
    onTap: () => void,
    opacity: number = 1,
    mouseProbe: string = "",
    onMouse?: (event: MouseEvent) => void,
    onAxis?: (event: AxisEvent) => void,
  ) {
    this.text = text;
    this.color = color;
    this.onTap = onTap;
    this.opacity = opacity;
    this.mouseProbe = mouseProbe;
    this.onMouse = onMouse;
    this.onAxis = onAxis;
  }
}

class TapData {
  count: number;
  constructor(count: number) {
    this.count = count;
  }
}

class MouseData {
  leftDown: number;
  leftUp: number;
  rightDown: number;
  rightUp: number;
  moves: number;
  constructor(leftDown: number, leftUp: number, rightDown: number, rightUp: number, moves: number) {
    this.leftDown = leftDown;
    this.leftUp = leftUp;
    this.rightDown = rightDown;
    this.rightUp = rightUp;
    this.moves = moves;
  }
}

@Builder
function badge(params: BadgeParams) {
  Text(params.text)
    .fontSize(16)
    .fontColor("#ffffff")
    .backgroundColor(params.color)
    .padding(10)
    .width("100%")
    .height("100%")
    .opacity(params.opacity)
    .id(params.mouseProbe ? `host-mouse-${params.mouseProbe}` : "host-badge")
    .onMouse(params.onMouse)
    .onAxisEvent(params.onAxis)
    .onClick(params.onTap);
}

/** A host-owned ArkUI widget implementing the same badge contract as iOS/Android. */
export class HostBadge {
  node: ComponentContent<BadgeParams>;
  private text: string = "Native badge";
  private color: string = "#8250df";
  private taps: number = 0;
  private onTap: () => void;
  private mouseProbe: string = "";
  private leftDown: number = 0;
  private leftUp: number = 0;
  private rightDown: number = 0;
  private rightUp: number = 0;
  private moves: number = 0;
  private onMouse: (event: MouseEvent) => void;
  private axes: number = 0;
  private onAxis: (event: AxisEvent) => void;
  private readonly context: UIContext;

  constructor(context: UIContext, handle: number, surface: NshSurface) {
    this.context = context;
    this.onTap = () => {
      this.taps++;
      surface.emitComponentEvent(handle, "tap", new TapData(this.taps));
    };
    this.onMouse = (event: MouseEvent) => {
      if (event.action === MouseAction.Move) {
        this.moves++;
      } else if (event.button === MouseButton.Left && event.action === MouseAction.Press) {
        this.leftDown++;
      } else if (event.button === MouseButton.Left && event.action === MouseAction.Release) {
        this.leftUp++;
      } else if (event.button === MouseButton.Right && event.action === MouseAction.Press) {
        this.rightDown++;
      } else if (event.button === MouseButton.Right && event.action === MouseAction.Release) {
        this.rightUp++;
      } else {
        return;
      }
      surface.emitComponentEvent(
        handle,
        "mouse",
        new MouseData(this.leftDown, this.leftUp, this.rightDown, this.rightUp, this.moves),
      );
    };
    this.onAxis = (event: AxisEvent) => {
      surface.emitComponentEvent(handle, "axis", new TapData(++this.axes));
      event.propagation();
    };
    this.node = new ComponentContent<BadgeParams>(
      context,
      wrapBuilder(badge),
      new BadgeParams(this.text, this.color, this.onTap),
    );
    surface.setComponentSize(handle, 240, 44);
  }

  updateProps(changes: NshNativeProperty[]): void {
    for (const change of changes) {
      if (change.name === "text") {
        this.text = change.removed ? "" : (change.value as string);
      }
      if (change.name === "color") {
        this.color = change.removed ? "#8250df" : (change.value as string);
      }
      if (change.name === "mouseProbe") {
        this.mouseProbe = change.removed ? "" : (change.value as string);
      }
    }
    this.node.update(this.params());
  }

  private params(opacity: number = 1): BadgeParams {
    return new BadgeParams(
      this.text,
      this.color,
      this.onTap,
      opacity,
      this.mouseProbe,
      this.mouseProbe ? this.onMouse : undefined,
      this.mouseProbe ? this.onAxis : undefined,
    );
  }

  command(name: string): void {
    if (name === "pulse" && !this.node.isDisposed()) {
      this.node.update(this.params(0.3));
      this.context.animateTo({ duration: 600 }, () => {
        this.node.update(this.params());
      });
    }
  }

  dispose(): void {
    if (!this.node.isDisposed()) {
      this.node.dispose();
    }
  }
}
