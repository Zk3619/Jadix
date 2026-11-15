'use strict';

Component({
  options: {virtualHost: true},
  properties: {node: {type: Object, value: null}},
  methods: {
    emit(payload) {
      const node = this.data.node;
      if (node) this.triggerEvent('jadixevent', {root: node.root, node: node.id, payload}, {bubbles: true, composed: true});
    },
    onTap() { this.emit({kind: 'activate'}); },
    onRadio() {
      const node = this.data.node;
      if (node && node.kind === 'button' && node.role === 'radio' && node.enabled && !node.selected)
        this.emit({kind: 'activate'});
    },
    onFocus() { this.emit({kind: 'focus'}); },
    onBlur() { this.emit({kind: 'blur'}); },
    onConfirm(event) {
      const node = this.data.node;
      if (!node || !node.enabled || !['input', 'textarea'].includes(node.kind)) return;
      // Confirmation can carry a final committed value without a preceding
      // input event. Deliver it first, preserving the shared revision ordering.
      if (typeof event.detail.value === 'string' && event.detail.value !== node.text) this.onInput(event);
      this.emit({kind: 'submit'});
    },
    onDismiss() {
      const node = this.data.node;
      if (node.closeOnOverlayClick || node.closeOnOutsideClick) this.emit({kind: 'dismiss', backdrop: true});
    },
    onPanelTap() {},
    onInput(event) {
      const value = event.detail.value;
      let cursor = Number.isInteger(event.detail.cursor) ? event.detail.cursor : value.length;
      cursor = Math.max(0, Math.min(cursor, value.length));
      if (cursor > 0 && cursor < value.length && value.charCodeAt(cursor) >= 0xdc00 && value.charCodeAt(cursor) <= 0xdfff) cursor--;
      this.emit({kind: 'edit', value, selection: [cursor, cursor], composing: null});
    },
    onToggle(event) {
      this.emit({kind: 'toggle', checked: Array.isArray(event.detail.value) ? event.detail.value.length > 0 : !!event.detail.value});
    },
    onSelect(event) {
      const value = this.data.node.optionValues[Number(event.detail.value)];
      if (this.data.node.enabled && value !== undefined) this.emit({kind: 'select', value});
    },
    onScroll(event) {
      this.emit({kind: 'scroll', x: event.detail.scrollLeft, y: event.detail.scrollTop});
    },
  },
});
