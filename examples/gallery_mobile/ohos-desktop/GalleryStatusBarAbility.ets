import { Want, UIExtensionContentSession } from '@kit.AbilityKit';
import { StatusBarViewExtensionAbility } from '@kit.StatusBarExtensionKit';

export default class GalleryStatusBarAbility extends StatusBarViewExtensionAbility {
  onSessionCreate(want: Want, session: UIExtensionContentSession): void {
    session.loadContent('pages/StatusBar');
  }
}
