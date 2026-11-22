import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createVirtualList, Text} from '../src/index.mjs';

test('virtual menus use shared scrolling policy and retain measured viewport geometry', () => {
  let onScroll, geometry, releases = 0;
  const app = {
    virtualRange(value) {
      geometry = value;
      return {start: 0, end: 1, visible_start: 0, visible_end: 1, offset: value.offset, before: 0, after: 900};
    },
    event(component, name, callback) {
      assert.equal(component, 'Scroll'); assert.equal(name, 'scroll'); onScroll = callback;
      return Object.defineProperty({session: '1', sequence: '1'}, 'release', {value() { releases++; }});
    },
  };
  const list = createVirtualList(app, {id: 'menu', count: 100, itemExtent: 20,
    height: null, menuScroll: true, renderItem: index => Text({text: String(index)}), onChange() {}});
  const view = list.render();
  assert.deepEqual(view.style.scrollbars, [false, false]);
  assert.equal(view.style.scroll_bounces, false);
  assert.equal(view.style.grow, 1);
  onScroll({x: 0, y: 40, viewport: [240, 180]});
  assert.equal(geometry.viewport, 180); assert.equal(geometry.offset, 40);
  assert.equal(list.render({scroll_bounces: true}).style.scroll_bounces, true);
  list.release(); list.release(); assert.equal(releases, 1);
});
