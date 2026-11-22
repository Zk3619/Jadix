"use strict";
var NativePanel = (() => {
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
  function bytes(value2, invalid = "unpaired surrogate") {
    let size = value2.length;
    for (let i = 0; i < value2.length; i++) {
      const code = value2.charCodeAt(i);
      if (code < 128) continue;
      if (code < 2048) {
        size++;
        continue;
      }
      if (code >= 55296 && code <= 57343) {
        const low = value2.charCodeAt(++i);
        if (code > 56319 || !(low >= 56320 && low <= 57343)) throw new TypeError(invalid);
      }
      size += 2;
    }
    return size;
  }

  // ../../../crates/jadix_shell/sdk/src/index.mjs
  var registry = new Map(schemas.map((schema) => [schema.name, schema]));
  function check(schema, value2, depth = 0, budget = { nodes: 0, bytes: 0 }) {
    if (++budget.nodes > 4096 || depth > 32) throw new RangeError("value limit");
    switch (schema.type) {
      case "null":
        if (value2 !== null) throw new TypeError("expected null");
        break;
      case "bool":
        if (typeof value2 !== "boolean") throw new TypeError("expected boolean");
        break;
      case "number":
        if (typeof value2 !== "number" || !Number.isFinite(value2)) throw new TypeError("expected finite number");
        break;
      case "string": {
        if (typeof value2 !== "string") throw new TypeError("string limit");
        const size = bytes(value2);
        if (size > schema.max_bytes) throw new TypeError("string limit");
        budget.bytes += size;
        break;
      }
      case "array":
        if (!Array.isArray(value2) || value2.length > schema.max_items || value2.length > 4096) throw new TypeError("array limit");
        value2.forEach((item) => check(schema.items, item, depth + 1, budget));
        break;
      case "object":
        if (value2 === null || typeof value2 !== "object" || Array.isArray(value2)) throw new TypeError("expected object");
        if (Object.keys(value2).length > 4096) throw new RangeError("object limit");
        for (const [name, field] of Object.entries(schema.fields)) {
          if (field.required && !Object.hasOwn(value2, name)) throw new TypeError(`missing property ${name}`);
        }
        for (const [name, item] of Object.entries(value2)) {
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
  function token(value2) {
    return typeof value2 === "string" && /^[1-9][0-9]{0,19}$/.test(value2) && BigInt(value2) <= 18446744073709551615n;
  }
  function handle(value2) {
    return value2 !== null && typeof value2 === "object" && Object.keys(value2).length === 2 && token(value2.session) && token(value2.sequence);
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
  function createApplication() {
    if (typeof globalThis.__jadixHost !== "function" || globalThis.__jadixDispatch) throw new Error("application host unavailable or already bound");
    const host = (request) => {
      const response = JSON.parse(globalThis.__jadixHost(JSON.stringify(request)));
      if (!response.ok) throw new Error(response.error);
      return response.value;
    };
    const identity = host({ type: "identity" });
    const callbacks = /* @__PURE__ */ new Map();
    const windows = /* @__PURE__ */ new Map();
    const localeListeners = /* @__PURE__ */ new Set();
    globalThis.__jadixLocaleChanged = (snapshot) => {
      for (const callback of [...localeListeners]) if (localeListeners.has(callback)) callback(snapshot);
    };
    let sequence = 0;
    globalThis.__jadixDispatch = (key, value2) => {
      callbacks.get(key)?.(value2);
    };
    let pendingDesktop = 0;
    const desktop = (request) => {
      if (pendingDesktop >= 64) return Promise.reject(new RangeError("desktop request limit"));
      const key = "desktop:" + String(++sequence);
      pendingDesktop++;
      return new Promise((resolve, reject) => {
        callbacks.set(key, (result) => {
          callbacks.delete(key);
          pendingDesktop--;
          if (Object.hasOwn(result, "error")) reject(new Error(result.error));
          else resolve(result.value);
        });
        try {
          host({ ...request, key });
        } catch (error) {
          callbacks.delete(key);
          pendingDesktop--;
          reject(error);
        }
      });
    };
    return {
      ...identity,
      persistence(save, restore) {
        if (typeof save !== "function" || typeof restore !== "function") throw new TypeError("persistence callbacks required");
        if (globalThis.__jadixCheckpoint) throw new Error("persistence already configured");
        globalThis.__jadixCheckpoint = () => {
          const json = JSON.stringify(save());
          if (typeof json !== "string" || bytes(json) > 65536) throw new RangeError("checkpoint exceeds 64 KiB");
          return json;
        };
        globalThis.__jadixRestore = restore;
      },
      virtualRange(options) {
        return host({ ...options, type: "virtual_range" });
      },
      emojiEntry(query, emoji) {
        return host({ type: "emoji_entry", query, emoji });
      },
      searchEmoji(query = {}) {
        return host({ type: "emoji_search", query });
      },
      layoutEmoji(query = {}, columns = 8, headers) {
        return host({ type: "emoji_layout", query, columns, headers });
      },
      locateEmoji(query, columns, emoji, headers) {
        return host({ type: "emoji_locate", query, columns, emoji, headers });
      },
      navigateEmoji(query, columns, current, key, rtl = false, page_rows = 6, headers) {
        return host({ type: "emoji_navigate", query, columns, current, key, rtl, page_rows, headers });
      },
      locale() {
        return host({ type: "i18n", request: { action: "snapshot" } });
      },
      onLocaleChange(callback) {
        if (typeof callback !== "function") throw new TypeError("locale callback required");
        if (localeListeners.size >= 256) throw new RangeError("locale listener limit");
        localeListeners.add(callback);
        return () => localeListeners.delete(callback);
      },
      setLocale(locale, fallback, timezone) {
        return host({ type: "i18n", request: { action: "configure", locale, fallback, timezone } });
      },
      resetLocale() {
        return host({ type: "i18n", request: { action: "reset" } });
      },
      availableLocales() {
        return host({ type: "i18n", request: { action: "snapshot" } }).availableLocales;
      },
      t(key, args = {}) {
        if (typeof key !== "string" || !args || typeof args !== "object" || Array.isArray(args)) throw new TypeError("invalid translation");
        return host({ type: "i18n", request: { action: "translate", key, args: Object.fromEntries(Object.entries(args).map(([key2, value2]) => [key2, String(value2)])) } });
      },
      formatNumber(value2, digits) {
        return host({ type: "i18n", request: { action: "number", value: value2, digits } });
      },
      formatPercent(value2) {
        return host({ type: "i18n", request: { action: "percent", value: value2 } });
      },
      formatCompact(value2) {
        return host({ type: "i18n", request: { action: "compact", value: value2 } });
      },
      formatCurrency(value2, code) {
        return host({ type: "i18n", request: { action: "currency", value: value2, code } });
      },
      formatDate(year, month, day, short = false) {
        return host({ type: "i18n", request: { action: "date", year, month, day, short } });
      },
      formatTime(hour, minute, second, hour12 = false) {
        return host({ type: "i18n", request: { action: "time", hour, minute, second, hour12 } });
      },
      formatList(items) {
        return host({ type: "i18n", request: { action: "list", items } });
      },
      share(options) {
        if (!options || typeof options !== "object" || Array.isArray(options) || Object.keys(options).some((key) => !["subject", "text", "url", "files"].includes(key)) || ["subject", "text", "url"].some((key) => options[key] !== void 0 && typeof options[key] !== "string") || options.files !== void 0 && (!Array.isArray(options.files) || options.files.length > 64 || options.files.some((file) => typeof file !== "string"))) return Promise.reject(new TypeError("invalid share options"));
        return desktop({ type: "share", options });
      },
      openUrl(url) {
        if (typeof url !== "string") return Promise.reject(new TypeError("URL must be a string"));
        return desktop({ type: "open_url", url });
      },
      files(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid file request"));
        return desktop({ type: "files", request });
      },
      clipboard(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid clipboard request"));
        return desktop({ type: "clipboard", request });
      },
      updater(request = { action: "snapshot" }) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid updater request"));
        return desktop({ type: "updater", request });
      },
      performance() {
        return desktop({ type: "performance" });
      },
      deepLinks(request) {
        return desktop({ type: "deep_links", request });
      },
      setAppAddress(address) {
        return desktop({ type: "deep_links", request: { action: "address", address } });
      },
      websocket(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid WebSocket request"));
        return desktop({ type: "web_socket", request });
      },
      fetch(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid HTTP request"));
        return desktop({ type: "fetch", request });
      },
      openAuthSession(request) {
        if (!request || typeof request !== "object" || Array.isArray(request) || typeof request.url !== "string" || typeof request.callback_scheme !== "string")
          return Promise.reject(new TypeError("invalid authentication session"));
        return desktop({ type: "auth_session", request: { action: "open", request } });
      },
      closeAuthSession() {
        return desktop({ type: "auth_session", request: { action: "close" } });
      },
      haptic(feedback = { type: "selection" }) {
        return desktop({ type: "haptic", feedback });
      },
      notifications(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid notification request"));
        return desktop({ type: "notifications", request });
      },
      globalShortcut(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid global shortcut request"));
        return desktop({ type: "global_shortcut", request });
      },
      speech(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid speech request"));
        return desktop({ type: "speech", request });
      },
      audio(request) {
        if (!request || typeof request !== "object" || Array.isArray(request)) return Promise.reject(new TypeError("invalid audio request"));
        return desktop({ type: "audio", request });
      },
      setTray(tray) {
        return desktop({ type: "set_tray", tray });
      },
      trayEvents() {
        return desktop({ type: "tray_events" });
      },
      evaluateWeb(path, script) {
        if (!Array.isArray(path) || path.length > 32 || path.some((i) => !Number.isSafeInteger(i) || i < 0) || typeof script !== "string" || bytes(script) > 1024 * 1024) return Promise.reject(new TypeError("invalid web evaluation"));
        return desktop({ type: "evaluate_web", path, script });
      },
      stopScroll(path) {
        if (!Array.isArray(path) || path.length > 32 || path.some((i) => !Number.isSafeInteger(i) || i < 0))
          return Promise.reject(new TypeError("invalid scroll path"));
        return desktop({ type: "stop_scroll", path });
      },
      zoomTo(path, scale, anchor = [0, 0]) {
        if (!Array.isArray(path) || path.length > 32 || path.some((i) => !Number.isSafeInteger(i) || i < 0) || !Number.isFinite(scale) || scale <= 0 || !Array.isArray(anchor) || anchor.length !== 2 || !anchor.every(Number.isFinite)) return Promise.reject(new TypeError("invalid viewport zoom"));
        return desktop({ type: "zoom_to", path, scale, anchor });
      },
      window(id, options, onChange) {
        if (typeof id !== "string" || !id || bytes(id) > 128 || options !== null && (typeof options !== "object" || Array.isArray(options)) || onChange !== void 0 && typeof onChange !== "function") throw new TypeError("invalid window declaration");
        let entry = windows.get(id);
        if (!entry) {
          if (windows.size >= 64) throw new RangeError("window declaration limit");
          entry = { key: "window:" + String(++sequence), state: null, options: null, onChange };
          windows.set(id, entry);
          callbacks.set(entry.key, (value2) => {
            if (Object.hasOwn(value2, "state")) entry.state = value2.state;
            entry.onChange?.(value2);
          });
        }
        entry.onChange = onChange;
        const update = (next) => {
          host({ type: "window", id, key: entry.key, options: next });
          entry.options = next;
        };
        update(options);
        return Object.freeze({
          get state() {
            return entry.state;
          },
          update,
          close() {
            update(null);
          },
          release() {
            update(null);
            callbacks.delete(entry.key);
            windows.delete(id);
          }
        });
      },
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

  // mobile.tsx
  var app = createApplication();
  var count = 0;
  var value = "";
  var edit = app.event("Input", "edit", (event) => {
    value = event.value;
    render();
  });
  var activate = app.event("Button", "activate", () => {
    count++;
    render();
  });
  app.persistence(() => ({ version: 1, count, value }), (state) => {
    if (!state || typeof state !== "object" || Array.isArray(state) || state.version !== 1 || typeof state.count !== "number" || !Number.isSafeInteger(state.count) || state.count < 0 || typeof state.value !== "string") throw new TypeError("invalid application checkpoint");
    count = state.count;
    value = state.value;
    render();
  });
  function render() {
    const editor = /* @__PURE__ */ jsx(Input, { value, placeholder: "\u4E2D\u6587 \u{1F600} \u0627\u0644\u0639\u0631\u0628\u064A\u0629", events: { edit }, style: { height: 48 } }, "editor");
    const button = /* @__PURE__ */ jsx(Button, { label: `Count ${count}`, events: { activate }, style: { height: 48 } }, "increment");
    app.mount(/* @__PURE__ */ jsxs(Box, { style: { padding: [20, 20, 20, 20], gap: 12 }, children: [
      /* @__PURE__ */ jsx(Text, { text: "Jadix Shell \xB7 native SDK", style: { height: 36 } }),
      count % 2 ? [button, editor] : [editor, button],
      /* @__PURE__ */ jsx(Text, { text: `Value: ${value}`, style: { height: 48 } })
    ] }));
  }
  render();
})();
