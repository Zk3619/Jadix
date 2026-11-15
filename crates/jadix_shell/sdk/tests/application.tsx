import { createApplication, Box, Text, Button, Input, Textarea } from '@jadix/shell';
const app = createApplication();
let count = 0;
let draft = '';
let notes = '';
const activate = app.event('Button', 'activate', () => { count++; render(); });
const edit = app.event('Input', 'edit', value => { draft = value.value; render(); });
const editNotes = app.event('Textarea', 'edit', value => { notes = value.value; render(); });
function render() {
  app.mount(<Box style={{ axis: 'column', width: 320, height: 200, background: [20, 24, 40, 255] }}>
    <Text key="counter" text={`Count: ${count}`} style={{ foreground: [226, 232, 240, 255] }} />
    <Button key="increment" label="Increment" events={{ activate }} style={{ foreground: [226, 232, 240, 255] }} />
    <Input key="draft" value={draft} placeholder="Name" events={{ edit }} style={{ height: 32, foreground: [226, 232, 240, 255] }} />
    <Textarea key="notes" value={notes} placeholder="Notes" events={{ edit: editNotes }} style={{ height: 64, foreground: [226, 232, 240, 255] }} />
    <Text key="multiline" text="Multiple lines with a bounded height and a long tail" style={{ width: 180, height: 36, max_lines: 2, foreground: [226, 232, 240, 255] }} />
  </Box>);
}
render();
