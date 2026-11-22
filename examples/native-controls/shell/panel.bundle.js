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
        "fields": {
          "accessibleActiveDescendant": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleControls": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 4096
              },
              "max_items": 64
            },
            "required": false
          },
          "accessibleRole": {
            "schema": {
              "type": "string",
              "max_bytes": 64
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "keyboard": {
            "schema": {
              "type": "object",
              "fields": {
                "focusable": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "keyDown": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "keyUp": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "shortcuts": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "key": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "modifiers": {
                          "schema": {
                            "type": "object",
                            "fields": {
                              "shift": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "control": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "alt": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "meta": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "capsLock": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              }
                            }
                          },
                          "required": false
                        }
                      }
                    },
                    "max_items": 64
                  },
                  "required": false
                }
              }
            },
            "required": false
          },
          "focusRequest": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "interaction": {
            "schema": {
              "type": "object",
              "fields": {
                "press": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "pinch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "rotation": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "touch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hover": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "longPressMillis": {
                  "schema": {
                    "type": "number"
                  },
                  "required": false
                },
                "pan": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "panActivation": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "activeOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "activeOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "pointerEvents": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "hitSlop": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "number"
                    },
                    "max_items": 4
                  },
                  "required": false
                },
                "fileDrop": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
      },
      "events": {
        "key": {
          "type": "object",
          "fields": {
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 4
              },
              "required": true
            },
            "key": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "code": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "modifiers": {
              "schema": {
                "type": "object",
                "fields": {
                  "shift": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "control": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "alt": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "meta": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "capsLock": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  }
                }
              },
              "required": true
            },
            "repeat": {
              "schema": {
                "type": "bool"
              },
              "required": true
            }
          }
        },
        "shortcut": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            }
          }
        },
        "interaction": {
          "type": "object",
          "fields": {
            "kind": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "timestamp": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "modifiers": {
              "schema": {
                "type": "object",
                "fields": {
                  "shift": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "control": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "alt": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "meta": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "capsLock": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  }
                }
              },
              "required": false
            },
            "translation": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "velocity": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "rate": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "touches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            },
            "activeTouches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            }
          }
        },
        "fileDrop": {
          "type": "object",
          "fields": {
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 16
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "uris": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "string",
                  "max_bytes": 8192
                },
                "max_items": 64
              },
              "required": true
            }
          }
        }
      },
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
          },
          "accessibleActiveDescendant": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleControls": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 4096
              },
              "max_items": 64
            },
            "required": false
          },
          "accessibleRole": {
            "schema": {
              "type": "string",
              "max_bytes": 64
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "interaction": {
            "schema": {
              "type": "object",
              "fields": {
                "press": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "pinch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "rotation": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "touch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hover": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "longPressMillis": {
                  "schema": {
                    "type": "number"
                  },
                  "required": false
                },
                "pan": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "panActivation": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "activeOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "activeOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "pointerEvents": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "hitSlop": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "number"
                    },
                    "max_items": 4
                  },
                  "required": false
                },
                "fileDrop": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
      },
      "events": {
        "interaction": {
          "type": "object",
          "fields": {
            "kind": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "timestamp": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "modifiers": {
              "schema": {
                "type": "object",
                "fields": {
                  "shift": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "control": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "alt": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "meta": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "capsLock": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  }
                }
              },
              "required": false
            },
            "translation": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "velocity": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "rate": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "touches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            },
            "activeTouches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            }
          }
        },
        "fileDrop": {
          "type": "object",
          "fields": {
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 16
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "uris": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "string",
                  "max_bytes": 8192
                },
                "max_items": 64
              },
              "required": true
            }
          }
        }
      },
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
          },
          "accessibleActiveDescendant": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleControls": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 4096
              },
              "max_items": 64
            },
            "required": false
          },
          "accessibleRole": {
            "schema": {
              "type": "string",
              "max_bytes": 64
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "focusRequest": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "navigationKeys": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 16
              },
              "max_items": 12
            },
            "required": false
          },
          "keyboard": {
            "schema": {
              "type": "object",
              "fields": {
                "focusable": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "keyDown": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "keyUp": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "shortcuts": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "key": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "modifiers": {
                          "schema": {
                            "type": "object",
                            "fields": {
                              "shift": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "control": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "alt": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "meta": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              },
                              "capsLock": {
                                "schema": {
                                  "type": "bool"
                                },
                                "required": false
                              }
                            }
                          },
                          "required": false
                        }
                      }
                    },
                    "max_items": 64
                  },
                  "required": false
                }
              }
            },
            "required": false
          },
          "interaction": {
            "schema": {
              "type": "object",
              "fields": {
                "press": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "pinch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "rotation": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "touch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hover": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "longPressMillis": {
                  "schema": {
                    "type": "number"
                  },
                  "required": false
                },
                "pan": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "panActivation": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "activeOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "activeOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "pointerEvents": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "hitSlop": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "number"
                    },
                    "max_items": 4
                  },
                  "required": false
                },
                "fileDrop": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
      },
      "events": {
        "activate": {
          "type": "null"
        },
        "navigate": {
          "type": "object",
          "fields": {
            "key": {
              "schema": {
                "type": "string",
                "max_bytes": 16
              },
              "required": true
            },
            "shift": {
              "schema": {
                "type": "bool"
              },
              "required": true
            },
            "control": {
              "schema": {
                "type": "bool"
              },
              "required": true
            }
          }
        },
        "hover": {
          "type": "object",
          "fields": {
            "inside": {
              "schema": {
                "type": "bool"
              },
              "required": true
            }
          }
        },
        "pointerMove": {
          "type": "object",
          "fields": {
            "x": {
              "required": true,
              "schema": {
                "type": "number"
              }
            },
            "y": {
              "required": true,
              "schema": {
                "type": "number"
              }
            }
          }
        },
        "key": {
          "type": "object",
          "fields": {
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 4
              },
              "required": true
            },
            "key": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "code": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "modifiers": {
              "schema": {
                "type": "object",
                "fields": {
                  "shift": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "control": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "alt": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "meta": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "capsLock": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  }
                }
              },
              "required": true
            },
            "repeat": {
              "schema": {
                "type": "bool"
              },
              "required": true
            }
          }
        },
        "shortcut": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            }
          }
        },
        "interaction": {
          "type": "object",
          "fields": {
            "kind": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "timestamp": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "modifiers": {
              "schema": {
                "type": "object",
                "fields": {
                  "shift": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "control": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "alt": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "meta": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "capsLock": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  }
                }
              },
              "required": false
            },
            "translation": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "velocity": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "rate": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "touches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            },
            "activeTouches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            }
          }
        },
        "fileDrop": {
          "type": "object",
          "fields": {
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 16
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "uris": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "string",
                  "max_bytes": 8192
                },
                "max_items": 64
              },
              "required": true
            }
          }
        }
      },
      "children": true
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
          },
          "navigationKeys": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 16
              },
              "max_items": 12
            },
            "required": false
          },
          "accessibleActiveDescendant": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleControls": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 4096
              },
              "max_items": 64
            },
            "required": false
          },
          "accessibleRole": {
            "schema": {
              "type": "string",
              "max_bytes": 64
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "focusRequest": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
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
        },
        "navigate": {
          "type": "object",
          "fields": {
            "key": {
              "schema": {
                "type": "string",
                "max_bytes": 16
              },
              "required": true
            },
            "shift": {
              "schema": {
                "type": "bool"
              },
              "required": true
            },
            "control": {
              "schema": {
                "type": "bool"
              },
              "required": true
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
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
        "fields": {
          "accessibleActiveDescendant": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleControls": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 4096
              },
              "max_items": 64
            },
            "required": false
          },
          "accessibleRole": {
            "schema": {
              "type": "string",
              "max_bytes": 64
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
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
            },
            "width": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "height": {
              "schema": {
                "type": "number"
              },
              "required": false
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
        },
        "scrollZoom": {
          "type": "object",
          "fields": {
            "scale": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
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
            },
            "width": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "height": {
              "schema": {
                "type": "number"
              },
              "required": true
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            }
          }
        },
        "scrollActivity": {
          "type": "object",
          "fields": {
            "activity": {
              "required": true,
              "schema": {
                "type": "object",
                "fields": {
                  "phase": {
                    "required": true,
                    "schema": {
                      "type": "string",
                      "max_bytes": 32
                    }
                  },
                  "distance": {
                    "required": false,
                    "schema": {
                      "type": "number"
                    }
                  }
                }
              }
            }
          }
        }
      },
      "children": true
    },
    {
      "name": "Hosted",
      "kind": "hosted",
      "properties": {
        "type": "object",
        "fields": {
          "specification": {
            "schema": {
              "type": "string",
              "max_bytes": 1048576
            },
            "required": true
          },
          "accessibleActiveDescendant": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleControls": {
            "schema": {
              "type": "array",
              "items": {
                "type": "string",
                "max_bytes": 4096
              },
              "max_items": 64
            },
            "required": false
          },
          "accessibleRole": {
            "schema": {
              "type": "string",
              "max_bytes": 64
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
      },
      "events": {
        "hosted": {
          "type": "object",
          "fields": {
            "event": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "data": {
              "schema": {
                "type": "string",
                "max_bytes": 16384
              },
              "required": true
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
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
          "margin": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "close_on_escape": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "close_on_outside_click": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
      },
      "events": {
        "open": {
          "type": "object",
          "fields": {}
        },
        "close": {
          "type": "object",
          "fields": {}
        },
        "cancel": {
          "type": "object",
          "fields": {
            "backdrop": {
              "schema": {
                "type": "bool"
              },
              "required": true
            }
          }
        },
        "positionChange": {
          "type": "object",
          "fields": {
            "position": {
              "schema": {
                "type": "object",
                "fields": {
                  "frame": {
                    "schema": {
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
                        },
                        "width": {
                          "schema": {
                            "type": "number"
                          },
                          "required": true
                        },
                        "height": {
                          "schema": {
                            "type": "number"
                          },
                          "required": true
                        }
                      }
                    },
                    "required": true
                  },
                  "placement": {
                    "schema": {
                      "type": "string",
                      "max_bytes": 16
                    },
                    "required": true
                  },
                  "availableWidth": {
                    "schema": {
                      "type": "number"
                    },
                    "required": true
                  },
                  "availableHeight": {
                    "schema": {
                      "type": "number"
                    },
                    "required": true
                  }
                }
              },
              "required": false
            }
          }
        },
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
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
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
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
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
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
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
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "focusRequest": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
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
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
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
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "focusRequest": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
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
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
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
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "interaction": {
            "schema": {
              "type": "object",
              "fields": {
                "press": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "pinch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "rotation": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "touch": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hover": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "longPressMillis": {
                  "schema": {
                    "type": "number"
                  },
                  "required": false
                },
                "pan": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "panActivation": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "activeOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "activeOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetX": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      },
                      "failOffsetY": {
                        "schema": {
                          "type": "number"
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "pointerEvents": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "hitSlop": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "number"
                    },
                    "max_items": 4
                  },
                  "required": false
                }
              }
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
            },
            "required": false
          }
        }
      },
      "events": {
        "loadStart": {
          "type": "null"
        },
        "progress": {
          "type": "object",
          "fields": {
            "loaded": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "total": {
              "schema": {
                "type": "number"
              },
              "required": true
            }
          }
        },
        "load": {
          "type": "object",
          "fields": {
            "width": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "height": {
              "schema": {
                "type": "number"
              },
              "required": true
            }
          }
        },
        "error": {
          "type": "object",
          "fields": {
            "message": {
              "schema": {
                "type": "string",
                "max_bytes": 1024
              },
              "required": true
            }
          }
        },
        "loadEnd": {
          "type": "null"
        },
        "interaction": {
          "type": "object",
          "fields": {
            "kind": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "phase": {
              "schema": {
                "type": "string",
                "max_bytes": 32
              },
              "required": true
            },
            "position": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": true
            },
            "timestamp": {
              "schema": {
                "type": "number"
              },
              "required": true
            },
            "modifiers": {
              "schema": {
                "type": "object",
                "fields": {
                  "shift": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "control": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "alt": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "meta": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  },
                  "capsLock": {
                    "schema": {
                      "type": "bool"
                    },
                    "required": false
                  }
                }
              },
              "required": false
            },
            "translation": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "velocity": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "number"
                },
                "max_items": 2
              },
              "required": false
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "rate": {
              "schema": {
                "type": "number"
              },
              "required": false
            },
            "touches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            },
            "activeTouches": {
              "schema": {
                "type": "array",
                "items": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "position": {
                      "schema": {
                        "type": "array",
                        "items": {
                          "type": "number"
                        },
                        "max_items": 2
                      },
                      "required": true
                    },
                    "force": {
                      "schema": {
                        "type": "number"
                      },
                      "required": true
                    },
                    "kind": {
                      "schema": {
                        "type": "string",
                        "max_bytes": 32
                      },
                      "required": true
                    },
                    "tilt": {
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
                },
                "max_items": 10
              },
              "required": false
            }
          }
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            }
          }
        }
      },
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
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
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
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
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
          },
          "accessibleName": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleDescription": {
            "schema": {
              "type": "string",
              "max_bytes": 4096
            },
            "required": false
          },
          "accessibleSelected": {
            "schema": {
              "type": "bool"
            },
            "required": false
          },
          "focusRequest": {
            "schema": {
              "type": "number"
            },
            "required": false
          },
          "accessibility": {
            "schema": {
              "type": "object",
              "fields": {
                "accessible": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "hidden": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "disabled": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "busy": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "readOnly": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "role": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 32
                  },
                  "required": false
                },
                "checked": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 8
                  },
                  "required": false
                },
                "live": {
                  "schema": {
                    "type": "string",
                    "max_bytes": 16
                  },
                  "required": false
                },
                "value": {
                  "schema": {
                    "type": "object",
                    "fields": {
                      "min": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "max": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "now": {
                        "schema": {
                          "type": "number"
                        },
                        "required": true
                      },
                      "text": {
                        "schema": {
                          "type": "string",
                          "max_bytes": 4096
                        },
                        "required": false
                      }
                    }
                  },
                  "required": false
                },
                "actions": {
                  "schema": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "fields": {
                        "id": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 128
                          },
                          "required": true
                        },
                        "label": {
                          "schema": {
                            "type": "string",
                            "max_bytes": 512
                          },
                          "required": true
                        }
                      }
                    },
                    "max_items": 32
                  },
                  "required": false
                },
                "pressed": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                },
                "invalid": {
                  "schema": {
                    "type": "bool"
                  },
                  "required": false
                }
              }
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
        },
        "accessibilityAction": {
          "type": "object",
          "fields": {
            "id": {
              "schema": {
                "type": "string",
                "max_bytes": 128
              },
              "required": true
            },
            "value": {
              "schema": {
                "type": "number"
              },
              "required": false
            }
          }
        }
      },
      "children": false
    }
  ];

  // ../../../crates/jadix_shell/sdk/src/utf8.mjs
  function bytes(value, invalid = "unpaired surrogate") {
    let size = value.length;
    for (let i = 0; i < value.length; i++) {
      const code = value.charCodeAt(i);
      if (code < 128) continue;
      if (code < 2048) {
        size++;
        continue;
      }
      if (code >= 55296 && code <= 57343) {
        const low = value.charCodeAt(++i);
        if (code > 56319 || !(low >= 56320 && low <= 57343)) throw new TypeError(invalid);
      }
      size += 2;
    }
    return size;
  }

  // ../../../crates/jadix_shell/sdk/src/index.mjs
  var registry = new Map(schemas.map((schema) => [schema.name, schema]));
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
      case "string": {
        if (typeof value !== "string") throw new TypeError("string limit");
        const size = bytes(value);
        if (size > schema.max_bytes) throw new TypeError("string limit");
        budget.bytes += size;
        break;
      }
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
    if (component === "Hosted" && children.length && JSON.parse(props.specification).type !== "menu_button") throw new TypeError("unsupported hosted children");
    return { component, props, style, events, children };
  }
  var Box = ({ children = [], ...props }) => createElement("Box", props, children);
  var Text = (props) => createElement("Text", props);
  var Button = ({ children = [], ...props }) => createElement("Button", props, children);
  var Input = (props) => createElement("Input", props);
  var PopoverContent = ({ children = [], ...props }) => createElement("PopoverContent", props, children);

  // ../../../crates/jadix_shell/sdk/src/jsx-runtime.mjs
  var Fragment = /* @__PURE__ */ Symbol.for("jadix.fragment");
  function flatten(children, result = [], depth = 0, budget = { nodes: 0 }) {
    if (++budget.nodes > 4096 || depth > 32) throw new RangeError("children limit");
    if (Array.isArray(children)) {
      for (const child of children) flatten(child, result, depth + 1, budget);
    } else result.push(children);
    return result;
  }
  function jsx(component, props = {}, key) {
    if (key !== void 0) {
      if (typeof key !== "string" || !key || key.length > 256) throw new TypeError("invalid element key");
      if (bytes(key, "invalid element key") > 256) throw new TypeError("invalid element key");
    }
    const { children, ...attributes } = props;
    const normalized = children === void 0 ? [] : flatten(children);
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
