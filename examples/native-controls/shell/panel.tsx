import { Box, Text, Button, Input, PopoverContent, type Handle } from '@jadix/shell';

let draft = '中文😀';
let count = 0;
let dismissed = false;

type BusinessState = { draft: string; count: number };

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function readStorage(state: unknown, version: number): BusinessState {
  if (!record(state) || Object.keys(state).length !== 2) throw new TypeError('invalid stored business state');
  let text: unknown;
  let counter: unknown;
  if (version === 1) {
    text = state.draft;
    counter = state.count;
  } else if (version === 2 && record(state.profile) && Object.keys(state.profile).length === 1) {
    text = state.profile.name;
    counter = state.counter;
  } else throw new TypeError('unsupported storage version or shape');
  if (typeof text !== 'string' || text.length > 16384 || typeof counter !== 'number' || !Number.isSafeInteger(counter) || counter < 0 || counter > 1000000000) throw new TypeError('invalid restored business state');
  return { draft: text, count: counter };
}

export function initialize(state: unknown, storageVersion = 1) {
  if (storageVersion !== 1 && storageVersion !== 2) throw new TypeError('unsupported storage version');
  if (state !== null) {
    const restored = readStorage(state, storageVersion);
    draft = restored.draft;
    count = restored.count;
  }
  return snapshot();
}

export function event(name: string, payload: { value?: string; composing?: number[] }) {
  if (name === 'edit' && payload.composing === undefined) draft = payload.value!;
  if (name === 'activate') { count += 1; dismissed = false; }
  if (name === 'dismiss') dismissed = true;
  return snapshot();
}

export function snapshot() { return { draft, count }; }

export function checkpoint(storageVersion: number) {
  if (storageVersion === 1) return snapshot();
  if (storageVersion === 2) return { profile: { name: draft }, counter: count };
  throw new TypeError('unsupported storage version');
}

export function migrateStorage(from: number, to: number, value: unknown) {
  if (from !== 1 || to !== 2) throw new TypeError('unsupported business migration');
  const previous = readStorage(value, from);
  return { profile: { name: previous.draft }, counter: previous.count };
}

export function mount(config: { session: string; root: string; edit: Handle; activate: Handle; dismiss: Handle }) {
  const editor = <Input key="editor" value={draft} placeholder="Native EDIT → QuickJS → TSX" events={{ edit: config.edit }} style={{ width: 360, height: 40, shrink: 0 }} />;
  const button = <Button key="increment" label={`Count ${count}`} events={{ activate: config.activate }} style={{ width: 180, height: 40, shrink: 0 }} />;
  return {
    protocol: 1, session: config.session,
    command: {
      type: 'mount', root: config.root,
      tree: <Box style={{ padding: [20, 20, 20, 20], gap: 12 }}>
        <Text text="Jadix TSX · Native controls · QuickJS" style={{ height: 32 }} />
        <Box style={{ axis: 'row', gap: 12, height: 40 }}>{count % 2 ? [button, editor] : [editor, button]}</Box>
        <Text text={`Business value: ${draft}`} style={{ height: 32 }} />
        <Text text="Press Count to move the keyed native input without recreating it." style={{ height: 32 }} />
        {{ ...PopoverContent({ anchor: 'increment', open: count % 2 === 1 && !dismissed,
          placement: 'bottom-start', gap: 8, events: { dismiss: config.dismiss },
          style: { width: 280, height: 76, padding: [12, 12, 12, 12] },
          children: [Text({ text: `Native Popover · Count ${count}`, style: { height: 32 } })] }), key: 'details' }}
      </Box>,
    },
  };
}
