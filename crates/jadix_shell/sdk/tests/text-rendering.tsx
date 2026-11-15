import { createApplication, Box, Text } from '@jadix/shell';
import type { NodeStyle } from '@jadix/shell';
declare global { var textSpec: { text: string; style: NodeStyle } | undefined; }
const spec = globalThis.textSpec ?? { text: 'AAAA\nBBBB\nCCCC', style: { max_lines: 2, text_overflow: 'ellipsis' } };
createApplication().mount(<Box style={{ width: 240, height: 120, background: [0, 0, 0, 255] }} children={[
  <Text key="caption" text={spec.text} style={{ width: 220, font_size: 20, font_family: 'monospace', foreground: [255, 255, 255, 255], ...spec.style }} />
]} />);
