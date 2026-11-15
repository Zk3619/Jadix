import { createApplication, Box, Text, Button, Input, Textarea } from '@jadix/shell';
const app = createApplication();
let draft = '';
let notes = '';
let limit = 5;
const edit = app.event('Input', 'edit', value => { draft = value.value; render(); });
const editNotes = app.event('Textarea', 'edit', value => { notes = value.value; render(); });
const tighten = app.event('Button', 'activate', () => { limit = 2; render(); });
function render() {
  app.mount(<Box style={{ axis: 'column', width: 320, height: 240 }}>
    <Input key="draft" value={draft} maxLength={limit} events={{ edit }} style={{ height: 36, foreground: [20, 24, 40, 255] }} />
    <Textarea key="notes" value={notes} maxLength={4} events={{ edit: editNotes }} style={{ height: 108, foreground: [20, 24, 40, 255] }} />
    <Button key="tighten" label="Limit2" events={{ activate: tighten }} style={{ height: 32 }} />
    <Text key="status" text={`Limit:${limit}`} />
  </Box>);
}
render();
