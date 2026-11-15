import { createApplication, Box, Text, Button, Input, Textarea, Scroll, Image, Select, Checkbox, Switch, DialogContent, PopoverContent } from '@jadix/shell';
const app=createApplication();
let selected='a', checked=false, switched=false, dialog=false, popover=false, removed=false, status='Ready';
const select=app.event('Select','select',event=>{selected=event.value;status=`Selected:${selected}`;render()});
const toggle=app.event('Checkbox','toggle',event=>{checked=event.checked;render()});
const switchToggle=app.event('Switch','toggle',event=>{switched=event.checked;render()});
const scroll=app.event('Scroll','scroll',event=>{status=`Scroll:${event.x},${event.y}`;render()});
const wheel=app.event('Scroll','wheel',event=>{status=`Wheel:${event.deltaX},${event.deltaY}`;render()});
const showDialog=app.event('Button','activate',()=>{dialog=true;render()});
const showPopover=app.event('Button','activate',()=>{popover=true;render()});
const remove=app.event('Button','activate',()=>{removed=true;render()});
const dismissDialog=app.event('DialogContent','dismiss',event=>{dialog=false;status=`Dialog:${event.backdrop}`;render()});
const dismissPopover=app.event('PopoverContent','dismiss',event=>{popover=false;status=`Popover:${event.backdrop}`;render()});
function render(){const children=[
 <Text key="status" text={status} style={{height:20}}/>,
 <Button key="anchor" label="Show popover" events={{activate:showPopover}} style={{width:160,height:28}}/>,
 <Button key="dialog-trigger" label="Show dialog" events={{activate:showDialog}} style={{width:160,height:28}}/>,
 <Select key="selection" value={selected} options={[{value:'a',label:'Alpha',enabled:true},{value:'b',label:'Beta',enabled:true},{value:'c',label:'Disabled',enabled:false}]} events={{select}} style={{width:180,height:40}}/>,
 <Checkbox key="checkbox" label="Check" checked={checked} events={{toggle}} style={{height:24}}/>,
 <Switch key="switch" label="Switch" checked={switched} events={{toggle:switchToggle}} style={{height:24}}/>,
 <Image key="image" src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAYAAACp8Z5+AAAAFUlEQVR4nGP8z8DwnwEJMCFziBMAAIPRAgYEvCRHAAAAAElFTkSuQmCC" alt="Red tile" style={{width:24,height:24}}/>,
 <Scroll key="scroll" events={{scroll,wheel}} style={{width:220,height:45}} children={[<Text text="Scroll document" style={{height:180,shrink:0}}/>]}/>,
 <Input key="input" value="abc אבג" style={{height:32}}/>,
 <Textarea key="textarea" value={"First\nSecond"} style={{height:48}}/>,
 <Text key="rich" text="A long label with 中文 and emoji 😀" style={{width:140,height:22,text_overflow:'ellipsis',max_lines:1,inline_styles:[{range:[2,6],foreground:[0,230,100,255],background:[30,40,70,255],bold:true,italic:true,underline:true}]}}/>,
 ]; if(!removed)children.push(
 <PopoverContent key="popover" anchor="anchor" open={popover} placement="right-end" gap={6} events={{dismiss:dismissPopover}} style={{width:140,height:50,background:[40,80,120,255]}} children={[<Text text="Popover panel"/>]}/>,
 <DialogContent key="dialog" open={dialog} close_on_overlay_click={true} events={{dismiss:dismissDialog}} style={{width:200,height:100,background:[70,80,100,255]}}><Text text="Dialog panel with a long ellipsized caption 中文 😀" style={{width:180,height:22,text_overflow:'ellipsis',max_lines:1}}/><Button label="Remove overlays" events={{activate:remove}} style={{height:30}}/></DialogContent>);
app.mount(<Box style={{width:600,height:440,background:[20,24,40,255],foreground:[240,240,240,255],gap:4}} children={children}/>)}
render();
