import { createApplication, Box, Button, Text } from '@jadix/shell';
const app = createApplication();
const rows = (globalThis as { scaleRows?: number }).scaleRows ?? 32;
let count = 0;
function rowText(index: number) {
  const shifting = index === 0 && (globalThis as {searchOffsets?:boolean}).searchOffsets;
  const prefix = shifting ? 'x'.repeat(count % 7) : '';
  const suffix = shifting ? 'x'.repeat(6 - count % 7) : '';
  const extra = index === 0 && count % 2 === 1 && (globalThis as {growingRows?:boolean}).growingRows ? '\nextra 中文 🦀\nthird line' : '';
  return `${prefix}Row ${index}: ${index === 0 ? count : 0}${suffix}${extra}`;
}
const activate = app.event('Button', 'activate', () => {
  count++;
  if ((globalThis as {leafPatches?:boolean}).leafPatches) app.patch([{path:[1],props:{text:rowText(0)}}]);
  else render();
});
function render() {
  const children = [<Button key="advance" label="Advance" events={{ activate }} style={{height:24, shrink:0}} />];
  for (let i = 0; i < rows; i++) children.push(<Text key={`row-${i}`} text={rowText(i)} style={(globalThis as {growingRows?:boolean}).growingRows ? {text_overflow:"wrap",shrink:0} : {height:20, shrink:0}} />);
  app.mount(<Box style={{axis:'column', width:320}} children={children}/>);
}
render();
