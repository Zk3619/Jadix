import { createApplication, Box, Text, Button, Input, Textarea } from '@jadix/shell';
const app = createApplication();
const rows = (globalThis as {scaleRows?:number}).scaleRows ?? 32;
const partial = (globalThis as {leafPatches?:boolean}).leafPatches ?? false;
const fieldRows = (globalThis as {fieldRows?:boolean}).fieldRows ?? false;
let draft = 'draft', notes = 'notes';
function inputProps() { return {value:draft,placeholder:'Name',maxLength:32}; }
function notesProps() { return {value:notes,placeholder:'Notes',maxLength:128}; }
const edit = app.event('Input','edit', v => { draft=v.value; if(partial) app.patch([{path:[0],props:inputProps()}]); else render(); });
const editNotes = app.event('Textarea','edit', v => { notes=v.value; if(partial) app.patch([{path:[1],props:notesProps()}]); else render(); });
const replace = app.event('Button','activate', () => { draft='SERVER 中文 🦀'; if(partial) app.patch([{path:[0],props:inputProps()}]); else render(); });
function render() {
  const children = [<Input key="draft" {...inputProps()} events={{edit}} style={{height:16,shrink:0}} />,
    <Textarea key="notes" {...notesProps()} events={{edit:editNotes}} style={{height:48,shrink:0}} />,
    <Button key="replace" label="Replace" events={{activate:replace}} style={{height:16,shrink:0}} />];
  for(let i=0;i<rows;i++) children.push(fieldRows
    ? <Input key={`row-${i}`} value={`Row ${i}: business`} style={{height:16,shrink:0}} />
    : <Text key={`row-${i}`} text={`Row ${i}: business`} style={{height:16,shrink:0}} />);
  app.mount(<Box style={{axis:'column',width:320}} children={children}/>);
}
render();
