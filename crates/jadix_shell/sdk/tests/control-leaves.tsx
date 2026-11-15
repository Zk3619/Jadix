import { createApplication, Box, Text, Checkbox, Switch, Select } from '@jadix/shell';
const app=createApplication();
const rows=(globalThis as {scaleRows?:number}).scaleRows??1024;
const partial=(globalThis as {leafPatches?:boolean}).leafPatches??false;
const controlRows=(globalThis as {controlRows?:boolean}).controlRows??false;
let checked=false,switched=false,selected='one';
const options=[{value:'one',label:'One',enabled:true},{value:'two',label:'Two',enabled:true}];
const checkbox=app.event('Checkbox','toggle',event=>{checked=event.checked;if(partial)app.patch([{path:[0],props:{label:'Keep enabled',checked,disabled:false}}]);else render();});
const toggle=app.event('Switch','toggle',event=>{switched=event.checked;if(partial)app.patch([{path:[1],props:{label:'Keep enabled',checked:switched,disabled:false}}]);else render();});
const select=app.event('Select','select',event=>{selected=event.value;if(partial)app.patch([{path:[2],props:{options,value:selected,placeholder:'Choose',disabled:false}}]);else render();});
function render(){
 const children=[
  <Checkbox key="check" label="Keep enabled" checked={checked} events={{toggle:checkbox}} style={{height:16,shrink:0}}/>,
  <Switch key="switch" label="Keep enabled" checked={switched} events={{toggle}} style={{height:16,shrink:0}}/>,
  <Select key="select" options={options} value={selected} placeholder="Choose" events={{select}} style={{height:16,shrink:0}}/>,
 ];
 for(let i=0;i<rows;i++)children.push(controlRows
  ? <Select key={`row-${i}`} options={options} value="one" style={{height:16,shrink:0}}/>
  : <Text key={`row-${i}`} text={`Business ${i}`} style={{height:16,shrink:0}}/>);
 app.mount(<Box style={{axis:'column',width:320}}>{children}</Box>);
}
render();
