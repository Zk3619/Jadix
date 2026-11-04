import { Box, Text, Button, Input } from '@jadix/shell';
export default <Box style={{ width: 320, gap: 8 }}>
  <Text text="Portable Jadix" />
  <Button label="Continue" events={{ activate: { session: '1', sequence: '2' } }} />
  <Input value="" placeholder="Name" />
</Box>;
