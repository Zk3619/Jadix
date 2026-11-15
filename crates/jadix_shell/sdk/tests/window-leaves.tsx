import { createApplication, Box, Checkbox, Switch, Text } from '@jadix/shell';

const app = createApplication();
const rows = (globalThis as { scaleRows?: number }).scaleRows ?? 32;
const partial = (globalThis as { leafPatches?: boolean }).leafPatches ?? true;
let checked = false;
let switched = false;
const checkbox = app.event('Checkbox', 'toggle', event => {
  checked = event.checked;
  if (partial) app.patch([{ path: [0], props: { label: 'Checkbox:' + checked, checked, disabled: false } }]);
  else render();
});
const toggle = app.event('Switch', 'toggle', event => {
  switched = event.checked;
  if (partial) app.patch([{ path: [1], props: { label: 'Switch:' + switched, checked: switched, disabled: false } }]);
  else render();
});

function render() {
  const children = [
    <Checkbox key="checkbox" label={'Checkbox:' + checked} checked={checked} events={{ toggle: checkbox }} style={{ height: 32, shrink: 0 }} />,
    <Switch key="switch" label={'Switch:' + switched} checked={switched} events={{ toggle }} style={{ height: 32, shrink: 0 }} />,
  ];
  for (let i = 0; i < rows; i++) children.push(<Text key={'row-' + i} text={'Business ' + i} style={{ height: 16, shrink: 0 }} />);
  app.mount(<Box style={{ axis: 'column', width: 320 }} children={children} />);
}
render();
