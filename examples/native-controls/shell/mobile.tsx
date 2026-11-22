import { Box, Button, Input, Text, createApplication } from '@jadix/shell';

const app = createApplication();
let count = 0;
let value = '';
const edit = app.event('Input', 'edit', event => { value = event.value; render(); });
const activate = app.event('Button', 'activate', () => { count++; render(); });

app.persistence(() => ({ version: 1, count, value }), state => {
  if (!state || typeof state !== 'object' || Array.isArray(state) || state.version !== 1 ||
      typeof state.count !== 'number' || !Number.isSafeInteger(state.count) || state.count < 0 ||
      typeof state.value !== 'string') throw new TypeError('invalid application checkpoint');
  count = state.count;
  value = state.value;
  render();
});

function render() {
  const editor = <Input key="editor" value={value} placeholder="中文 😀 العربية" events={{ edit }} style={{ height: 48 }} />;
  const button = <Button key="increment" label={`Count ${count}`} events={{ activate }} style={{ height: 48 }} />;
  app.mount(<Box style={{ padding: [20, 20, 20, 20], gap: 12 }}>
    <Text text="Jadix Shell · native SDK" style={{ height: 36 }} />
    {count % 2 ? [button, editor] : [editor, button]}
    <Text text={`Value: ${value}`} style={{ height: 48 }} />
  </Box>);
}
render();
