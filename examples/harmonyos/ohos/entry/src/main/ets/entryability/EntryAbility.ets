import { LazyPlugin, NativeAbility } from "@ohos-rs/ability";
import { AppControlPlugin } from "@ohos-rs/ability-plugin-app-control";

export default class EntryAbility extends NativeAbility {
  public moduleName: string = "harmonyos_host";
  public defaultPage: boolean = true;
  public bridgePlugins = [new LazyPlugin(() => new AppControlPlugin())];
}
