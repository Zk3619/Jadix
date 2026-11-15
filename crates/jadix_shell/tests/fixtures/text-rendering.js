"use strict";
(() => {
  // src/generated.mjs
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
          },
          "maxLength": {
            "schema": {
              "type": "number"
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
          },
          "maxLength": {
            "schema": {
              "type": "number"
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

  // src/index.mjs
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
    if ((component === "Input" || component === "Textarea") && Object.hasOwn(props, "maxLength") && (!Number.isSafeInteger(props.maxLength) || props.maxLength < 0 || props.maxLength > 1024 * 1024)) {
      throw new RangeError("maxLength must be an integer from 0 to 1048576");
    }
    for (const [name, reference] of Object.entries(events)) {
      if (!Object.hasOwn(schema.events, name) || !handle(reference)) throw new TypeError(`invalid event ${name}`);
    }
    if (!Array.isArray(children) || !schema.children && children.length) throw new TypeError("unsupported children");
    return { component, props, style, events, children };
  }
  var Box = ({ children = [], ...props }) => createElement("Box", props, children);
  var Text = (props) => createElement("Text", props);
  function createApplication() {
    if (typeof globalThis.__jadixHost !== "function" || globalThis.__jadixDispatch) throw new Error("application host unavailable or already bound");
    const host = (request) => {
      const response = JSON.parse(globalThis.__jadixHost(JSON.stringify(request)));
      if (!response.ok) throw new Error(response.error);
      return response.value;
    };
    const identity = host({ type: "identity" });
    const callbacks = /* @__PURE__ */ new Map();
    let sequence = 0;
    globalThis.__jadixDispatch = (key, value) => {
      callbacks.get(key)?.(value);
    };
    return {
      ...identity,
      event(component, event, callback) {
        if (typeof callback !== "function") throw new TypeError("event callback required");
        const key = String(++sequence);
        const reference = host({ type: "register", component, event, key });
        callbacks.set(key, callback);
        return Object.freeze(Object.defineProperty(reference, "release", { value() {
          host({ type: "release", key });
          callbacks.delete(key);
        } }));
      },
      mount(tree) {
        return host({ type: "mount", tree });
      },
      patch(patches) {
        if (!Array.isArray(patches) || !patches.length || patches.length > 1024 || patches.some((p) => !p || !Array.isArray(p.path) || p.path.length > 32 || p.path.some((i) => !Number.isSafeInteger(i) || i < 0))) throw new TypeError("invalid leaf patches");
        return host({ type: "patch", patches });
      }
    };
  }

  // src/jsx-runtime.mjs
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

  // tests/text-rendering.tsx
  var spec = globalThis.textSpec ?? { text: "AAAA\nBBBB\nCCCC", style: { max_lines: 2, text_overflow: "ellipsis" } };
  createApplication().mount(/* @__PURE__ */ jsx(Box, { style: { width: 240, height: 120, background: [0, 0, 0, 255] }, children: [
    /* @__PURE__ */ jsx(Text, { text: spec.text, style: { width: 220, font_size: 20, font_family: "monospace", foreground: [255, 255, 255, 255], ...spec.style } }, "caption")
  ] }));
})();
