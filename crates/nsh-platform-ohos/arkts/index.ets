import { LazyPlugin } from "@ohos-rs/ability";
import { NshPlugin } from "./src/main/ets/NshPlugin";

/** Registers Native Shell's instance-owned SDK editor, menu and texture fragments. */
export function nativeShellPlugins(): LazyPlugin[] {
  return [new LazyPlugin(() => new NshPlugin())];
}
