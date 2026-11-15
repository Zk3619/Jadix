import { createApplication, Box, Text, Button, Input, Textarea } from '@jadix/shell';
const app = createApplication();
const partial = (globalThis as {leafPatches?:boolean}).leafPatches ?? false;
let draft='', notes='', limit=5;
const inputProps=()=>({value:draft,maxLength:limit});
const notesProps=()=>({value:notes,maxLength:4});
const edit = app.event('Input','edit',v=>{draft=v.value;if(partial)app.patch([{path:[0],props:inputProps()}]);else render();});
const editNotes = app.event('Textarea','edit',v=>{notes=v.value;if(partial)app.patch([{path:[1],props:notesProps()}]);else render();});
const tighten=app.event('Button','activate',()=>{limit=2;if(partial)app.patch([{path:[0],props:inputProps()},{path:[3],props:{text:`Limit:${limit}`}}]);else render();});
function render(){app.mount(<Box style={{axis:'column',width:320,height:240}}>
  <Input key="draft" {...inputProps()} events={{edit}} style={{height:36,foreground:[20,24,40,255]}} />
  <Textarea key="notes" {...notesProps()} events={{edit:editNotes}} style={{height:108,foreground:[20,24,40,255]}} />
  <Button key="tighten" label="Limit2" events={{activate:tighten}} style={{height:32}} />
  <Text key="status" text={`Limit:${limit}`} />
</Box>);}
render();
