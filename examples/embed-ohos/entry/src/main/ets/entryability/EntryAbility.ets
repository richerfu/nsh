import { UIAbility, Want } from "@kit.AbilityKit";
import { window } from "@kit.ArkUI";

import { onNotificationWant } from "@nsh/ohos";

export default class EntryAbility extends UIAbility {
  onCreate(want: Want): void { onNotificationWant(want); }
  onNewWant(want: Want): void { onNotificationWant(want); }
  onWindowStageCreate(windowStage: window.WindowStage): void {
    windowStage.loadContent("pages/Index");
  }
}
