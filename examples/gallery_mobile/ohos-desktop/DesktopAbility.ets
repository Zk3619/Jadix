import { window } from '@kit.ArkUI';
import { image } from '@kit.ImageKit';
import { statusBarManager } from '@kit.StatusBarExtensionKit';
import EntryAbility from './EntryAbility';

export default class DesktopAbility extends EntryAbility {
  private trayInstalled: boolean = false;

  onWindowStageCreate(stage: window.WindowStage): void {
    super.onWindowStageCreate(stage);
    this.installTray().catch((error: Error) => console.error(`Gallery status bar: ${error.message}`));
  }

  private async installTray(): Promise<void> {
    if (!canIUse('SystemCapability.PCService.StatusBarManager') || this.trayInstalled) {
      return;
    }
    const data = await this.context.resourceManager.getMediaContent($r('app.media.app_icon'));
    const source = image.createImageSource(data.buffer);
    const icon = await source.createPixelMap({ desiredSize: { width: 24, height: 24 } });
    statusBarManager.addToStatusBar(this.context, {
      icons: { white: icon, black: icon },
      quickOperation: { title: 'Jadix Gallery', height: 140, abilityName: 'GalleryStatusBarAbility' },
      statusBarGroupMenu: [[{ title: '打开 Gallery', menuAction: { abilityName: 'EntryAbility' } }]],
      hoverTips: 'Jadix Gallery'
    });
    this.trayInstalled = true;
    source.release();
  }

  async onDestroy(): Promise<void> {
    if (this.trayInstalled) {
      try {
        statusBarManager.removeFromStatusBar(this.context);
      } catch (error) {
        console.error(`Gallery status bar cleanup: ${error}`);
      }
    }
    await super.onDestroy();
  }
}
