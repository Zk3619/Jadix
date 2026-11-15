import { createApplication, Box, Scroll, Text, Input } from '@jadix/shell';
const app = createApplication();
let draft = 'draft';
const edit = app.event('Input', 'edit', value => { draft = value.value; render(); });
function render() {
  app.mount(<Box style={{axis:'column'}}>
    <Scroll key="scroll" style={{height:48,shrink:0}} children={[
      <Text key="text" text={'wrap 中文 🦀 words '.repeat(5)} style={{text_overflow:'wrap',shrink:0}} />
    ]} />
    <Input key="draft" value={draft} events={{edit}} style={{height:16,shrink:0}} />
  </Box>);
}
render();
