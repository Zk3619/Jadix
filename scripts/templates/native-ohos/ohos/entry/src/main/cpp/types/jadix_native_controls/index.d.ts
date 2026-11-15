export const nativeMount: (content: Object, width: number, height: number, density: number, notify: () => void) => string;
export const nativePoll: (identity: string) => string;
export const nativeInspect: (identity: string, includeText: boolean) => string;
export const nativeResize: (identity: string, width: number, height: number, density: number, environment: number) => void;
export const nativeUnmount: (identity: string) => void;
