import { createApplication, Box, Text, Button } from '@jadix/shell';
const app=createApplication();let phase=0;
const activate=app.event('Button','activate',()=>{phase++;render()});
function render(){
 const children=[<Button key="advance" label="Advance" events={{activate}}/>];
 for(const name of (phase%2 ? ['B','A']:['A','B']))children.push(<Text key={name} text={`${name}:${phase}`} />);
 if(phase%2)children.push(<Text key="temporary" text="Temporary"/>);
 app.mount(<Box style={{gap:2}} children={children}/>);
}
render();
