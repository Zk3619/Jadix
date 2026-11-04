"use strict";
var NativePanel = (() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

  // panel.tsx
  var panel_exports = {};
  __export(panel_exports, {
    checkpoint: () => checkpoint,
    event: () => event,
    initialize: () => initialize,
    migrateStorage: () => migrateStorage,
    mount: () => mount,
    snapshot: () => snapshot
  });

  // ../../../crates/jadix_shell/sdk/src/generated.mjs
  var schemas = [
    {
      "name": "Box",
      "kind": "container",
      "properties": {
        "type": "object",
        "fields": {}
      },
      "events": {},
      "children": true
    },
    {
      "name": "Text",
      "kind": "text",
      "properties": {
        "type": "object",
        "fields": {
          "text": {
            "schema": {
              "type": "string",
              "max_bytes": 65536
            },
            "required": true
          }
        }
      },
      "events": {},
      "children": false
    },
    {
      "name": "Button",
      "kind": "button",
      "properties": {
        "type": "object",
        "fields": {
          "label": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": true
          },
          "disabled": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "activate": {
          "type": "null"
        }
      },
      "children": false
    },
    {
      "name": "Input",
      "kind": "input",
      "properties": {
        "type": "object",
        "fields": {
          "value": {
            "schema": {
              "type": "string",
              "max_bytes": 65536
            },
            "required": true
          },
          "placeholder": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "disabled": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "edit": {
          "type": "object",
          "fields": {
            "value": {
              "schema": {
                "type": "string",
                "max_bytes": 65536
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            },
            "selection": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "composing": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            }
          }
        }
      },
      "children": false
    },
    {
      "name": "Scroll",
      "kind": "scroll",
      "properties": {
        "type": "object",
        "fields": {}
      },
      "events": {
        "scroll": {
          "type": "object",
          "fields": {
            "x": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "y": {
              "schema": {
                "type": "number"
              },
              "required": true
            }
          }
        }
      },
      "children": true
    },
    {
      "name": "PopoverContent",
      "kind": "popover",
      "properties": {
        "type": "object",
        "fields": {
          "anchor": {
            "schema": {
              "type": "string",
              "max_bytes": 256
            },
            "required": true
          },
          "open": {
            "schema": {
              "type": "bool"
            },
            "required": true
          },
          "placement": {
            "schema": {
              "type": "string",
              "max_bytes": 16
            },
            "required": false
          },
          "gap": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "close_on_outside_click": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "dismiss": {
          "type": "object",
          "fields": {
            "backdrop": {
              "schema": {
                "type": "bool"
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            }
          }
        }
      },
      "children": true
    },
    {
      "name": "Textarea",
      "kind": "textarea",
      "properties": {
        "type": "object",
        "fields": {
          "value": {
            "schema": {
              "type": "string",
              "max_bytes": 65536
            },
            "required": true
          },
          "placeholder": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "disabled": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "edit": {
          "type": "object",
          "fields": {
            "value": {
              "schema": {
                "type": "string",
                "max_bytes": 65536
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            },
            "selection": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "composing": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            }
          }
        }
      },
      "children": false
    },
    {
      "name": "Checkbox",
      "kind": "checkbox",
      "properties": {
        "type": "object",
        "fields": {
          "label": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": true
          },
          "checked": {
            "schema": {
              "type": "bool"
            },
            "required": true
          },
          "disabled": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "toggle": {
          "type": "object",
          "fields": {
            "checked": {
              "schema": {
                "type": "bool"
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            }
          }
        }
      },
      "children": false
    },
    {
      "name": "Switch",
      "kind": "switch",
      "properties": {
        "type": "object",
        "fields": {
          "label": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": true
          },
          "checked": {
            "schema": {
              "type": "bool"
            },
            "required": true
          },
          "disabled": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "toggle": {
          "type": "object",
          "fields": {
            "checked": {
              "schema": {
                "type": "bool"
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            }
          }
        }
      },
      "children": false
    },
    {
      "name": "Image",
      "kind": "image",
      "properties": {
        "type": "object",
        "fields": {
          "src": {
            "schema": {
              "type": "string",
              "max_bytes": 1048576
            },
            "required": true
          },
          "alt": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          }
        }
      },
      "events": {},
      "children": false
    },
    {
      "name": "DialogContent",
      "kind": "dialog",
      "properties": {
        "type": "object",
        "fields": {
          "open": {
            "schema": {
              "type": "bool"
            },
            "required": true
          },
          "close_on_overlay_click": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "dismiss": {
          "type": "object",
          "fields": {
            "backdrop": {
              "schema": {
                "type": "bool"
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            }
          }
        }
      },
      "children": true
    },
    {
      "name": "Select",
      "kind": "select",
      "properties": {
        "type": "object",
        "fields": {
          "options": {
            "schema": {
              "type": "array",
              "max_items": 256,
              "items": {
                "type": "object",
                "fields": {
                  "value": {
                    "schema": {
                      "type": "string",
                      "max_bytes": 4096
                    },
                    "required": true
                  },
                  "label": {
                    "schema": {
                      "type": "string",
                      "max_bytes": 4096
                    },
                    "required": true
                  },
                  "enabled": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": true
                  }
                }
              }
            },
            "required": true
          },
          "value": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": true
          },
          "placeholder": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "disabled": {
            "schema": {
              "type": "bool"
            },
            "required": false
          }
        }
      },
      "events": {
        "select": {
          "type": "object",
          "fields": {
            "value": {
              "schema": {
                "type": "string",
                "max_bytes": 4096
              },
              "required": true
            },
            "revision": {
              "schema": {
                "type": "string",
                "max_bytes": 20
              },
              "required": true
            }
          }
        }
      },
      "children": false
    }
  ];

  // ../../../crates/jadix_shell/sdk/src/index.mjs
  var registry = new Map(schemas.map((schema) => [schema.name, schema]));
  function bytes(value) {
    let size = 0;
    for (const character of value) {
      const code = character.codePointAt(0);
      if (code >= 55296 && code <= 57343) throw new TypeError("unpaired surrogate");
      size += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
    }
    return size;
  }
  function check(schema, value, depth = 0, budget = { nodes: 0, bytes: 0 }) {
    if (++budget.nodes > 4096 || depth > 32) throw new RangeError("value limit");
    switch (schema.type) {
      case "null":
        if (value !== null) throw new TypeError("expected null");
        break;
      case "bool":
        if (typeof value !== "boolean") throw new TypeError("expected boolean");
        break;
      case "number":
        if (typeof value !== "number" || !Number.isFinite(value)) throw new TypeError("expected finite number");
        break;
      case "string":
        if (typeof value !== "string" || bytes(value) > schema.max_bytes) throw new TypeError("string limit");
        budget.bytes += bytes(value);
        break;
      case "array":
        if (!Array.isArray(value) || value.length > schema.max_items || value.length > 4096) throw new TypeError("array limit");
        value.forEach((item) => check(schema.items, item, depth + 1, budget));
        break;
      case "object":
        if (value === null || typeof value !== "object" || Array.isArray(value)) throw new TypeError("expected object");
        if (Object.keys(value).length > 4096) throw new RangeError("object limit");
        for (const [name, field] of Object.entries(schema.fields)) {
          if (field.required && !Object.hasOwn(value, name)) throw new TypeError(`missing property ${name}`);
        }
        for (const [name, item] of Object.entries(value)) {
          if (!Object.hasOwn(schema.fields, name)) throw new TypeError(`unknown property ${name}`);
          budget.bytes += bytes(name);
          check(schema.fields[name].schema, item, depth + 1, budget);
        }
        break;
      default:
        throw new TypeError("unknown schema");
    }
    if (budget.bytes > 1024 * 1024) throw new RangeError("message limit");
  }
  function token(value) {
    return typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value) && BigInt(value) <= 18446744073709551615n;
  }
  function handle(value) {
    return value !== null && typeof value === "object" && Object.keys(value).length === 2 && token(value.session) && token(value.sequence);
  }
  function createElement(component, properties = {}, children = []) {
    const schema = registry.get(component);
    if (!schema) throw new TypeError(`unknown component ${component}`);
    const { events = {}, style = {}, ...props } = properties;
    check(schema.properties, props);
    for (const [name, reference] of Object.entries(events)) {
      if (!Object.hasOwn(schema.events, name) || !handle(reference)) throw new TypeError(`invalid event ${name}`);
    }
    if (!Array.isArray(children) || !schema.children && children.length) throw new TypeError("unsupported children");
    return { component, props, style, events, children };
  }
  var Box = ({ children = [], ...props }) => createElement("Box", props, children);
  var Text = (props) => createElement("Text", props);
  var Button = (props) => createElement("Button", props);
  var Input = (props) => createElement("Input", props);
  var PopoverContent = ({ children = [], ...props }) => createElement("PopoverContent", props, children);

  // ../../../crates/jadix_shell/sdk/src/jsx-runtime.mjs
  var Fragment = /* @__PURE__ */ Symbol.for("jadix.fragment");
  function jsx(component, props = {}, key) {
    if (key !== void 0) {
      if (typeof key !== "string" || !key || key.length > 256) throw new TypeError("invalid element key");
      let size = 0;
      for (const character of key) {
        const code = character.codePointAt(0);
        if (code >= 55296 && code <= 57343) throw new TypeError("invalid element key");
        size += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
      }
      if (size > 256) throw new TypeError("invalid element key");
    }
    const { children, ...attributes } = props;
    const normalized = children === void 0 ? [] : Array.isArray(children) ? children : [children];
    if (component === Fragment) return { ...Box({ children: normalized }), ...key === void 0 ? {} : { key } };
    if (typeof component !== "function") throw new TypeError("unregistered JSX component");
    const element = component(children === void 0 ? attributes : { ...attributes, children: normalized });
    return key === void 0 ? element : { ...element, key };
  }
  var jsxs = jsx;

  // panel.tsx
  var draft = "\u4E2D\u6587\u{1F600}";
  var count = 0;
  var dismissed = false;
  function record(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }
  function readStorage(state, version) {
    if (!record(state) || Object.keys(state).length !== 2) throw new TypeError("invalid stored business state");
    let text;
    let counter;
    if (version === 1) {
      text = state.draft;
      counter = state.count;
    } else if (version === 2 && record(state.profile) && Object.keys(state.profile).length === 1) {
      text = state.profile.name;
      counter = state.counter;
    } else throw new TypeError("unsupported storage version or shape");
    if (typeof text !== "string" || text.length > 16384 || typeof counter !== "number" || !Number.isSafeInteger(counter) || counter < 0 || counter > 1e9) throw new TypeError("invalid restored business state");
    return { draft: text, count: counter };
  }
  function initialize(state, storageVersion = 1) {
    if (storageVersion !== 1 && storageVersion !== 2) throw new TypeError("unsupported storage version");
    if (state !== null) {
      const restored = readStorage(state, storageVersion);
      draft = restored.draft;
      count = restored.count;
    }
    return snapshot();
  }
  function event(name, payload) {
    if (name === "edit" && payload.composing === void 0) draft = payload.value;
    if (name === "activate") {
      count += 1;
      dismissed = false;
    }
    if (name === "dismiss") dismissed = true;
    return snapshot();
  }
  function snapshot() {
    return { draft, count };
  }
  function checkpoint(storageVersion) {
    if (storageVersion === 1) return snapshot();
    if (storageVersion === 2) return { profile: { name: draft }, counter: count };
    throw new TypeError("unsupported storage version");
  }
  function migrateStorage(from, to, value) {
    if (from !== 1 || to !== 2) throw new TypeError("unsupported business migration");
    const previous = readStorage(value, from);
    return { profile: { name: previous.draft }, counter: previous.count };
  }
  function mount(config) {
    const editor = /* @__PURE__ */ jsx(Input, { value: draft, placeholder: "Native EDIT \u2192 QuickJS \u2192 TSX", events: { edit: config.edit }, style: { width: 360, height: 40, shrink: 0 } }, "editor");
    const button = /* @__PURE__ */ jsx(Button, { label: `Count ${count}`, events: { activate: config.activate }, style: { width: 180, height: 40, shrink: 0 } }, "increment");
    return {
      protocol: 1,
      session: config.session,
      command: {
        type: "mount",
        root: config.root,
        tree: /* @__PURE__ */ jsxs(Box, { style: { padding: [20, 20, 20, 20], gap: 12 }, children: [
          /* @__PURE__ */ jsx(Text, { text: "Jadix TSX \xB7 Native controls \xB7 QuickJS", style: { height: 32 } }),
          /* @__PURE__ */ jsx(Box, { style: { axis: "row", gap: 12, height: 40 }, children: count % 2 ? [button, editor] : [editor, button] }),
          /* @__PURE__ */ jsx(Text, { text: `Business value: ${draft}`, style: { height: 32 } }),
          /* @__PURE__ */ jsx(Text, { text: "Press Count to move the keyed native input without recreating it.", style: { height: 32 } }),
          { ...PopoverContent({
            anchor: "increment",
            open: count % 2 === 1 && !dismissed,
            placement: "bottom-start",
            gap: 8,
            events: { dismiss: config.dismiss },
            style: { width: 280, height: 76, padding: [12, 12, 12, 12] },
            children: [Text({ text: `Native Popover \xB7 Count ${count}`, style: { height: 32 } })]
          }), key: "details" }
        ] })
      }
    };
  }
  return __toCommonJS(panel_exports);
})();
