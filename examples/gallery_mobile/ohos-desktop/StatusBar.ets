import { common } from '@kit.AbilityKit';

@Entry
@Component
struct StatusBar {
  build() {
    Column({ space: 12 }) {
      Text('Jadix Gallery').fontSize(18).fontWeight(FontWeight.Bold)
      Button('打开展示廊').onClick(() => {
        const context = this.getUIContext().getHostContext() as common.UIExtensionContext;
        context.startAbility({ bundleName: 'org.jadix.gallery', abilityName: 'EntryAbility' });
      })
    }.padding(20).width('100%').height('100%')
  }
}
