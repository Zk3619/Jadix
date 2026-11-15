export const schemas = [
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
      },
      "wheel": {
        "type": "object",
        "fields": {
          "deltaX": {
            "schema": {
              "type": "number"
            },
            "required": true
          },
          "deltaY": {
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
