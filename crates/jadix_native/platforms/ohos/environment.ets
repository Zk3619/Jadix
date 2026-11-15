import { common, Configuration, EnvironmentCallback } from '@kit.AbilityKit';

export function observeJadixConfiguration(context: common.UIAbilityContext,
  publish: (configuration: string) => void): () => void {
  const application = context.getApplicationContext();
  let closed = false;
  const callback: EnvironmentCallback = {
    onConfigurationUpdated(configuration: Configuration): void {
      if (!closed) publish(JSON.stringify(configuration));
    },
    onMemoryLevel(_level: number): void {}
  };
  const identity = application.on('environment', callback);
  try {
    publish(JSON.stringify(context.config));
  } catch (error) {
    application.off('environment', identity);
    throw error;
  }
  return (): void => {
    if (!closed) {
      closed = true;
      application.off('environment', identity);
    }
  };
}
