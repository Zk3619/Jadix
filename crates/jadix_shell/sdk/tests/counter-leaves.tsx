import { createApplication, Box, Text, Button } from '@jadix/shell';
const app = createApplication();
const businessRows = (globalThis as {businessRows?:number}).businessRows ?? 0;
let count = 0;
const line = {height:16,shrink:0,font_size:16,foreground:[255,255,255,255] as [number,number,number,number],background:[0,0,0,255] as [number,number,number,number]};
const press = app.event('Button','activate', () => {
  count++;
  app.patch([{path:[0],props:{text:`Counter ${count}`}}]);
});
const children = [<Text key="counter" text="Counter 0" style={line}/>,
  <Button key="increment" label="Increment" events={{activate:press}} style={line}/>,
  <Text key="unicode" text="Unicode: 中文 🦀 é" style={line}/>];
for (let i=0;i<businessRows;i++) children.push(<Text key={`row-${i}`} text={`Row ${i}: business`} style={line}/>);
app.mount(<Box style={{axis:'column',width:640,height:384,background:[0,0,0,255]}} children={children}/>);
