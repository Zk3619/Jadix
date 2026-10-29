import { LazyPlugin, NativeAbility } from '@ohos-rs/ability';
import { AppControlPlugin } from '@ohos-rs/ability-plugin-app-control';
import { WindowPlugin } from '@ohos-rs/ability-plugin-window';
import { ClipboardPlugin } from '@ohos-rs/ability-plugin-clipboard';
import { FilesPlugin } from '@ohos-rs/ability-plugin-files';
import { PermissionPlugin } from '@ohos-rs/ability-plugin-permission';
import { UrlPlugin } from '@ohos-rs/ability-plugin-url';

export default class EntryAbility extends NativeAbility {
  public moduleName: string = 'gallery_mobile';
  public defaultPage: boolean = true;
  public bridgePlugins = [
    new LazyPlugin(() => new AppControlPlugin()),
    new LazyPlugin(() => new WindowPlugin()),
    new LazyPlugin(() => new ClipboardPlugin({
      metadataMimeType: 'application/x-jadix-text-metadata-v1'
    })),
    new LazyPlugin(() => new FilesPlugin()),
    new LazyPlugin(() => new PermissionPlugin()),
    new LazyPlugin(() => new UrlPlugin())
  ];
}
