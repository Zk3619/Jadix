#ifdef _MSC_VER
#define __attribute__(...)
#endif
#include "../platforms/ohos/native.cpp"
#include <iostream>
#include <stdexcept>

struct ArkUI_AccessibilityState { bool disabled = false; bool selected = false; int32_t checked = ARKUI_ACCESSIBILITY_UNCHECKED; };
struct ArkUI_AccessibilityElementInfo { int32_t id = 0; bool accessibility_focused = false; };
struct ArkUI_AccessibilityEventInfo { ArkUI_AccessibilityEventType type = ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_INVALID; int32_t focus_id = -1; };
struct ArkUI_AccessibilityElementInfoList {};
static std::vector<ArkUI_AccessibilityEventType> accessibility_events;
struct ArkUI_UIInputEvent {
    int32_t input_type = ARKUI_UIINPUTEVENT_TYPE_KEY;
    ArkUI_KeyEventType type = ARKUI_KEY_EVENT_DOWN;
    int32_t code = 0;
    std::vector<int32_t> pressed;
    mutable bool consumed = false;
    mutable bool stopped = false;
};
struct ArkUI_NodeEvent { ArkUI_NodeHandle node; ArkUI_NodeEventType type; void* data; ArkUI_UIInputEvent* input = nullptr; ArkUI_NodeComponentEvent* component = nullptr; };
struct ArkUI_DialogDismissEvent { void* data; int32_t reason; bool blocked = false; };
struct Attribute { std::vector<ArkUI_NumberValue> numbers; std::string text; bool selected = false; bool disabled = false; int32_t checked = ARKUI_ACCESSIBILITY_UNCHECKED; };
struct TestNode {
    ArkUI_NodeType kind = ARKUI_NODE_STACK;
    std::unordered_map<int32_t, Attribute> attributes;
    std::unordered_map<int32_t, void*> events;
    std::vector<ArkUI_NodeHandle> children;
    void (*receiver)(ArkUI_NodeEvent*) = nullptr;
};
struct TestDialog {
    ArkUI_NodeHandle content = nullptr;
    void* data = nullptr;
    void (*dismiss)(ArkUI_DialogDismissEvent*) = nullptr;
    bool shown = false;
    bool modal = false;
};
struct Delivery { int32_t kind; std::string value; bool popup_closed; int32_t detail = 0; };
static ArkUI_NativeNodeAPI_1 test_node_api{};
static ArkUI_NativeDialogAPI_1 test_dialog_api{};
static std::unordered_map<ArkUI_NodeHandle, std::unique_ptr<TestNode>> nodes;
static std::unordered_map<ArkUI_NativeDialogHandle, std::unique_ptr<TestDialog>> controllers;
static std::vector<Delivery> deliveries;
static ArkUI_NodeHandle focused = nullptr;
static bool fail_close = false;
static bool fail_row_attribute = false;
[[noreturn]] static void unsupported_sdk_call();

static void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}

extern "C" void* OH_ArkUI_QueryModuleInterfaceByName(ArkUI_NativeAPIVariantKind kind, const char*) {
    return kind == ARKUI_NATIVE_NODE ? static_cast<void*>(&test_node_api) : static_cast<void*>(&test_dialog_api);
}
extern "C" ArkUI_NodeEventType OH_ArkUI_NodeEvent_GetEventType(ArkUI_NodeEvent* event) { return event->type; }
extern "C" ArkUI_NodeHandle OH_ArkUI_NodeEvent_GetNodeHandle(ArkUI_NodeEvent* event) { return event->node; }
extern "C" void* OH_ArkUI_NodeEvent_GetUserData(ArkUI_NodeEvent* event) { return event->data; }
extern "C" ArkUI_UIInputEvent* OH_ArkUI_NodeEvent_GetInputEvent(ArkUI_NodeEvent* event) { return event->input; }
extern "C" int32_t OH_ArkUI_UIInputEvent_GetType(const ArkUI_UIInputEvent* event) { return event->input_type; }
extern "C" ArkUI_KeyEventType OH_ArkUI_KeyEvent_GetType(const ArkUI_UIInputEvent* event) { return event->type; }
extern "C" int32_t OH_ArkUI_KeyEvent_GetKeyCode(const ArkUI_UIInputEvent* event) { return event->code; }
extern "C" void OH_ArkUI_KeyEvent_SetConsumed(const ArkUI_UIInputEvent* event, bool consumed) { event->consumed = consumed; }
extern "C" void OH_ArkUI_KeyEvent_StopPropagation(const ArkUI_UIInputEvent* event, bool stopped) { event->stopped = stopped; }
extern "C" int32_t OH_ArkUI_UIInputEvent_GetPressedKeys(const ArkUI_UIInputEvent* event, int32_t* codes, int32_t* length) {
    if (static_cast<size_t>(*length) < event->pressed.size()) return 1;
    *length = static_cast<int32_t>(event->pressed.size());
    std::copy(event->pressed.begin(), event->pressed.end(), codes);
    return 0;
}
extern "C" void OH_ArkUI_DialogDismissEvent_SetShouldBlockDismiss(ArkUI_DialogDismissEvent* event, bool blocked) { event->blocked = blocked; }
extern "C" void* OH_ArkUI_DialogDismissEvent_GetUserData(ArkUI_DialogDismissEvent* event) { return event->data; }
extern "C" int32_t OH_ArkUI_DialogDismissEvent_GetDismissReason(ArkUI_DialogDismissEvent* event) { return event->reason; }
extern "C" ArkUI_AccessibilityState* OH_ArkUI_AccessibilityState_Create() { return new ArkUI_AccessibilityState(); }
extern "C" void OH_ArkUI_AccessibilityState_Dispose(ArkUI_AccessibilityState* state) { delete state; }
extern "C" void OH_ArkUI_AccessibilityState_SetDisabled(ArkUI_AccessibilityState* state, int32_t value) { state->disabled = value != 0; }
extern "C" void OH_ArkUI_AccessibilityState_SetSelected(ArkUI_AccessibilityState* state, int32_t value) { state->selected = value != 0; }
extern "C" void OH_ArkUI_AccessibilityState_SetCheckedState(ArkUI_AccessibilityState* state, int32_t value) { state->checked = value; }
static alignas(void*) unsigned char test_accessibility_provider_storage[sizeof(void*)]{};
static ArkUI_AccessibilityProvider* test_accessibility_provider =
    reinterpret_cast<ArkUI_AccessibilityProvider*>(test_accessibility_provider_storage);
extern "C" int32_t OH_ArkUI_GetNodeHandleFromNapiValue(napi_env, napi_value value, ArkUI_NodeHandle* node) { *node = reinterpret_cast<ArkUI_NodeHandle>(value); return 0; }
extern "C" ArkUI_AccessibilityProvider* OH_ArkUI_AccessibilityProvider_Create(ArkUI_NodeHandle) { return test_accessibility_provider; }
extern "C" void OH_ArkUI_AccessibilityProvider_Dispose(ArkUI_AccessibilityProvider*) {}
extern "C" int32_t OH_ArkUI_AccessibilityProviderRegisterCallbackWithInstance(const char*, ArkUI_AccessibilityProvider*, ArkUI_AccessibilityProviderCallbacksWithInstance*) { return 0; }
extern "C" ArkUI_AccessibilityElementInfo* OH_ArkUI_AddAndGetAccessibilityElementInfo(ArkUI_AccessibilityElementInfoList*) { unsupported_sdk_call(); }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetElementId(ArkUI_AccessibilityElementInfo* info, int32_t id) { info->id = id; return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetParentId(ArkUI_AccessibilityElementInfo*, int32_t) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetComponentType(ArkUI_AccessibilityElementInfo*, const char*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetContents(ArkUI_AccessibilityElementInfo*, const char*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetAccessibilityText(ArkUI_AccessibilityElementInfo*, const char*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetAccessibilityDescription(ArkUI_AccessibilityElementInfo*, const char*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetChildNodeIds(ArkUI_AccessibilityElementInfo*, int32_t, int64_t*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetOperationActions(ArkUI_AccessibilityElementInfo*, int32_t, ArkUI_AccessibleAction*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetScreenRect(ArkUI_AccessibilityElementInfo*, ArkUI_AccessibleRect*) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetCheckable(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetChecked(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetFocusable(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetFocused(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetVisible(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetAccessibilityFocused(ArkUI_AccessibilityElementInfo* info, bool value) { info->accessibility_focused = value; return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetSelected(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetClickable(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetEnabled(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetIsPassword(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetScrollable(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityElementInfoSetEditable(ArkUI_AccessibilityElementInfo*, bool) { return 0; }
extern "C" ArkUI_AccessibilityEventInfo* OH_ArkUI_CreateAccessibilityEventInfo() { return new ArkUI_AccessibilityEventInfo(); }
extern "C" void OH_ArkUI_DestoryAccessibilityEventInfo(ArkUI_AccessibilityEventInfo* event) { delete event; }
extern "C" int32_t OH_ArkUI_AccessibilityEventSetEventType(ArkUI_AccessibilityEventInfo* event, ArkUI_AccessibilityEventType type) { event->type = type; return 0; }
extern "C" int32_t OH_ArkUI_AccessibilityEventSetRequestFocusId(ArkUI_AccessibilityEventInfo* event, int32_t id) { event->focus_id = id; return 0; }
extern "C" void OH_ArkUI_SendAccessibilityAsyncEvent(ArkUI_AccessibilityProvider*, ArkUI_AccessibilityEventInfo* event, void (*callback)(int32_t)) {
    accessibility_events.push_back(event->type);
    if (callback) callback(0);
}
[[noreturn]] static void unsupported_sdk_call() { throw std::runtime_error("unexpected SDK call outside Select contract"); }
extern "C" ArkUI_NodeComponentEvent* OH_ArkUI_NodeEvent_GetNodeComponentEvent(ArkUI_NodeEvent* event) { return event->component; }
extern "C" ArkUI_TextChangeEvent* OH_ArkUI_NodeEvent_GetTextChangeEvent(ArkUI_NodeEvent*) { unsupported_sdk_call(); }
extern "C" int32_t OH_ArkUI_NodeContent_AddNode(ArkUI_NodeContentHandle, ArkUI_NodeHandle) { unsupported_sdk_call(); }
extern "C" int32_t OH_ArkUI_NodeContent_RemoveNode(ArkUI_NodeContentHandle, ArkUI_NodeHandle) { unsupported_sdk_call(); }
extern "C" int32_t OH_ArkUI_GetNodeContentFromNapiValue(napi_env, napi_value, ArkUI_NodeContentHandle*) { unsupported_sdk_call(); }
extern "C" OH_Drawing_FontCollection* OH_Drawing_CreateFontCollection() { unsupported_sdk_call(); }
extern "C" void OH_Drawing_DestroyFontCollection(OH_Drawing_FontCollection*) { unsupported_sdk_call(); }
extern "C" OH_Drawing_TypographyStyle* OH_Drawing_CreateTypographyStyle() { unsupported_sdk_call(); }
extern "C" void OH_Drawing_DestroyTypographyStyle(OH_Drawing_TypographyStyle*) { unsupported_sdk_call(); }
extern "C" OH_Drawing_TextStyle* OH_Drawing_CreateTextStyle() { unsupported_sdk_call(); }
extern "C" void OH_Drawing_DestroyTextStyle(OH_Drawing_TextStyle*) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_SetTextStyleFontSize(OH_Drawing_TextStyle*, double) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_SetTextStyleFontWeight(OH_Drawing_TextStyle*, int) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_SetTextStyleFontFamilies(OH_Drawing_TextStyle*, int, const char*[]) { unsupported_sdk_call(); }
extern "C" OH_Drawing_TypographyCreate* OH_Drawing_CreateTypographyHandler(OH_Drawing_TypographyStyle*, OH_Drawing_FontCollection*) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_DestroyTypographyHandler(OH_Drawing_TypographyCreate*) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_TypographyHandlerPushTextStyle(OH_Drawing_TypographyCreate*, OH_Drawing_TextStyle*) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_TypographyHandlerAddText(OH_Drawing_TypographyCreate*, const char*) { unsupported_sdk_call(); }
extern "C" OH_Drawing_Typography* OH_Drawing_CreateTypography(OH_Drawing_TypographyCreate*) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_DestroyTypography(OH_Drawing_Typography*) { unsupported_sdk_call(); }
extern "C" void OH_Drawing_TypographyLayout(OH_Drawing_Typography*, double) { unsupported_sdk_call(); }
extern "C" double OH_Drawing_TypographyGetHeight(OH_Drawing_Typography*) { unsupported_sdk_call(); }
extern "C" double OH_Drawing_TypographyGetLongestLine(OH_Drawing_Typography*) { unsupported_sdk_call(); }
extern "C" double OH_Drawing_TypographyGetAlphabeticBaseline(OH_Drawing_Typography*) { unsupported_sdk_call(); }
extern "C" void jadix_ohos_receive(void*, int32_t kind, const char* text, const char*, int32_t detail, int32_t, int32_t, float, float) {
    auto closed = std::none_of(controllers.begin(), controllers.end(), [](const auto& entry) { return entry.second->shown; });
    deliveries.push_back({kind, text ? text : "", closed, detail});
}

static void initialize() {
    test_node_api.createNode = [](ArkUI_NodeType kind) -> ArkUI_NodeHandle {
        auto node = std::make_unique<TestNode>();
        node->kind = kind;
        auto handle = reinterpret_cast<ArkUI_NodeHandle>(node.get());
        nodes.emplace(handle, std::move(node));
        return handle;
    };
    test_node_api.disposeNode = [](ArkUI_NodeHandle node) {
        require(nodes.at(node)->children.empty(), "disposed node retains children");
        require(nodes.at(node)->events.empty(), "disposed node retains registered callbacks");
        nodes.erase(node);
    };
    test_node_api.setLengthMetricUnit = [](ArkUI_NodeHandle, ArkUI_LengthMetricUnit) -> int32_t { return 0; };
    test_node_api.setAttribute = [](ArkUI_NodeHandle node, ArkUI_NodeAttributeType kind, const ArkUI_AttributeItem* item) -> int32_t {
        if (fail_row_attribute && kind == NODE_BUTTON_LABEL) return 7;
        auto& value = nodes.at(node)->attributes[kind];
        value.numbers.clear();
        if (item->value) value.numbers.assign(item->value, item->value + item->size);
        value.text = item->string ? item->string : "";
        if (kind == NODE_ACCESSIBILITY_STATE) {
            auto* state = static_cast<ArkUI_AccessibilityState*>(item->object);
            value.disabled = state->disabled;
            value.selected = state->selected;
            value.checked = state->checked;
        }
        if (kind == NODE_FOCUS_STATUS && item->value[0].i32) focused = node;
        return 0;
    };
    test_node_api.getAttribute = [](ArkUI_NodeHandle, ArkUI_NodeAttributeType) -> const ArkUI_AttributeItem* { return nullptr; };
    test_node_api.resetAttribute = [](ArkUI_NodeHandle node, ArkUI_NodeAttributeType kind) -> int32_t {
        nodes.at(node)->attributes.erase(kind);
        return 0;
    };
    test_node_api.addChild = [](ArkUI_NodeHandle parent, ArkUI_NodeHandle child) -> int32_t {
        nodes.at(parent)->children.push_back(child);
        return 0;
    };
    test_node_api.removeChild = [](ArkUI_NodeHandle parent, ArkUI_NodeHandle child) -> int32_t {
        auto& children = nodes.at(parent)->children;
        children.erase(std::remove(children.begin(), children.end(), child), children.end());
        return 0;
    };
    test_node_api.addNodeEventReceiver = [](ArkUI_NodeHandle node, void (*receiver)(ArkUI_NodeEvent*)) -> int32_t {
        nodes.at(node)->receiver = receiver;
        return 0;
    };
    test_node_api.registerNodeEvent = [](ArkUI_NodeHandle node, ArkUI_NodeEventType type, int32_t, void* data) -> int32_t {
        nodes.at(node)->events[type] = data;
        return 0;
    };
    test_node_api.unregisterNodeEvent = [](ArkUI_NodeHandle node, ArkUI_NodeEventType type) { nodes.at(node)->events.erase(type); };
    test_dialog_api.create = []() -> ArkUI_NativeDialogHandle {
        auto controller = std::make_unique<TestDialog>();
        auto handle = reinterpret_cast<ArkUI_NativeDialogHandle>(controller.get());
        controllers.emplace(handle, std::move(controller));
        return handle;
    };
    test_dialog_api.dispose = [](ArkUI_NativeDialogHandle handle) {
        require(!controllers.at(handle)->shown && !controllers.at(handle)->content, "disposed live dialog");
        controllers.erase(handle);
    };
    test_dialog_api.setContent = [](ArkUI_NativeDialogHandle handle, ArkUI_NodeHandle content) -> int32_t {
        controllers.at(handle)->content = content;
        return 0;
    };
    test_dialog_api.removeContent = [](ArkUI_NativeDialogHandle handle) -> int32_t { controllers.at(handle)->content = nullptr; return 0; };
    test_dialog_api.setModalMode = [](ArkUI_NativeDialogHandle handle, bool modal) -> int32_t { controllers.at(handle)->modal = modal; return 0; };
    test_dialog_api.setAutoCancel = [](ArkUI_NativeDialogHandle, bool) -> int32_t { return 0; };
    test_dialog_api.enableCustomStyle = [](ArkUI_NativeDialogHandle, bool) -> int32_t { return 0; };
    test_dialog_api.enableCustomAnimation = [](ArkUI_NativeDialogHandle, bool) -> int32_t { return 0; };
    test_dialog_api.registerOnWillDismissWithUserData = [](ArkUI_NativeDialogHandle handle, void* data, void (*dismiss)(ArkUI_DialogDismissEvent*)) -> int32_t {
        auto& controller = *controllers.at(handle);
        controller.data = data;
        controller.dismiss = dismiss;
        return 0;
    };
    test_dialog_api.show = [](ArkUI_NativeDialogHandle handle, bool) -> int32_t { controllers.at(handle)->shown = true; return 0; };
    test_dialog_api.close = [](ArkUI_NativeDialogHandle handle) -> int32_t {
        if (fail_close) return 7;
        controllers.at(handle)->shown = false;
        return 0;
    };
}

static SelectParts& make_select() {
    static int target_identity = 0;
    auto trigger = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(11, &target_identity));
    require(trigger != nullptr, "create production Select trigger");
    const char* values[] = {"apple", "disabled", "東京"};
    const char* labels[] = {"Apple", "Disabled", "東京 🍎"};
    const bool enabled[] = {true, false, true};
    require(jadix_ohos_select(trigger, values, labels, enabled, 3, 2, true) == 0, "configure options");
    return *selects.at(trigger);
}

static void click(SelectParts& parts, size_t index) {
    auto node = parts.rows.at(index).node;
    auto& fixture = *nodes.at(node);
    ArkUI_NodeEvent event{node, NODE_ON_CLICK, fixture.events.at(NODE_ON_CLICK)};
    fixture.receiver(&event);
}

static void dismiss(SelectParts& parts, int32_t reason) {
    auto& fixture = *controllers.at(parts.controller);
    ArkUI_DialogDismissEvent event{fixture.data, reason};
    fixture.dismiss(&event);
    require(event.blocked, "automatic dismissal was not blocked");
}

static void clear_select(ArkUI_NodeHandle trigger) {
    fail_close = false;
    fail_row_attribute = false;
    jadix_ohos_dispose(trigger);
    require(nodes.empty() && controllers.empty(), "native handles leaked");
    deliveries.clear();
}

static bool press(ArkUI_NodeHandle node, int32_t code, ArkUI_KeyEventType type = ARKUI_KEY_EVENT_DOWN, int32_t modifier = 0) {
    auto& fixture = *nodes.at(node);
    ArkUI_UIInputEvent input;
    input.code = code;
    input.type = type;
    input.pressed.push_back(code);
    if (modifier) input.pressed.push_back(modifier);
    ArkUI_NodeEvent event{node, NODE_ON_KEY_EVENT, fixture.events.at(NODE_ON_KEY_EVENT), &input};
    fixture.receiver(&event);
    require(input.consumed == input.stopped, "key consumption and propagation disagree");
    return input.consumed;
}

static void keyboard_navigation() {
    auto& parts = make_select();
    auto trigger = parts.trigger;
    require(!press(trigger, ARKUI_KEYCODE_ENTER) && !parts.shown, "native button activation was intercepted");
    require(!press(trigger, ARKUI_KEYCODE_DPAD_DOWN, ARKUI_KEY_EVENT_DOWN, ARKUI_KEYCODE_CTRL_LEFT), "modified shortcut was intercepted");
    require(press(trigger, ARKUI_KEYCODE_DPAD_DOWN, ARKUI_KEY_EVENT_UP) && !parts.shown, "key release opened popup");
    require(press(trigger, ARKUI_KEYCODE_DPAD_DOWN) && parts.shown && focused == parts.rows[2].node, "arrow did not open selected row");
    require(press(parts.rows[2].node, ARKUI_KEYCODE_DPAD_UP) && focused == parts.rows[0].node, "disabled option was not skipped");
    require(press(parts.rows[0].node, ARKUI_KEYCODE_DPAD_UP) && focused == parts.rows[2].node, "previous did not wrap");
    require(press(parts.rows[2].node, ARKUI_KEYCODE_MOVE_HOME) && focused == parts.rows[0].node, "Home did not focus first enabled option");
    require(press(parts.rows[0].node, ARKUI_KEYCODE_MOVE_END) && focused == parts.rows[2].node, "End did not focus last enabled option");
    require(!press(parts.rows[1].node, ARKUI_KEYCODE_DPAD_DOWN) && focused == parts.rows[2].node, "disabled row handled key");
    require(parts.selected == 2 && deliveries.empty(), "navigation changed controlled value");
    require(press(parts.rows[2].node, ARKUI_KEYCODE_ESCAPE) && !parts.shown && focused == trigger && deliveries.empty(), "Escape did not dismiss without selection");
    require(!press(trigger, ARKUI_KEYCODE_ESCAPE), "closed popup consumed Escape");
    const char* values[] = {"0", "1", "2", "3", "4", "5", "6", "7"};
    const bool enabled[] = {true, true, true, true, true, true, true, true};
    require(jadix_ohos_select(trigger, values, values, enabled, 8, -1, true) == 0, "configure scrollable options");
    require(press(trigger, ARKUI_KEYCODE_MOVE_END) && focused == parts.rows[7].node, "End did not open at last option");
    require(nodes.at(parts.scroll)->attributes.at(NODE_SCROLL_OFFSET).numbers[1].f32 == 88, "focused option was not scrolled into view");
    require(press(parts.rows[7].node, ARKUI_KEYCODE_DPAD_DOWN) && focused == parts.rows[0].node, "next did not wrap");
    require(nodes.at(parts.scroll)->attributes.at(NODE_SCROLL_OFFSET).numbers[1].f32 == 0, "wrapped focus remained offscreen");
    clear_select(trigger);
}

static void theme_and_retention() {
    auto& parts = make_select();
    auto trigger = parts.trigger;
    require(open_select(parts) == 0 && parts.shown, "open native option dialog");
    auto row = parts.rows[2].node;
    require(focused == row && controllers.at(parts.controller)->modal, "selected row focus or modal mode");
    require(nodes.at(row)->attributes.at(NODE_ACCESSIBILITY_STATE).selected, "selected accessibility state");
    require(nodes.at(parts.rows[1].node)->attributes.at(NODE_ACCESSIBILITY_STATE).disabled, "disabled accessibility state");
    require(nodes.at(row)->attributes.at(NODE_BUTTON_LABEL).text == "東京 🍎", "Unicode option label");
    require(jadix_ohos_update(trigger, 11, "東京 🍎", "", true, false, false, 36, 700, "Test Font", 0xffeeeeee, 0xff112233, ARKUI_DIRECTION_RTL, "Fruit", "Help 中文", 0, -1, 1) == 0, "publish popup theme");
    require(nodes.at(trigger)->attributes.at(NODE_ACCESSIBILITY_DESCRIPTION).text == "Help 中文, Expanded", "description must retain expansion state");
    require(nodes.at(trigger)->attributes.at(NODE_ACCESSIBILITY_TEXT).text == "Fruit", "description must not replace name");
    require(nodes.at(trigger)->attributes.at(NODE_DIRECTION).numbers[0].i32 == ARKUI_DIRECTION_RTL, "RTL direction did not reach native node");
    require(parts.shown && parts.rows[2].node == row && focused == row, "theme publication replaced popup or focus");
    auto& attributes = nodes.at(row)->attributes;
    require(attributes.at(NODE_FONT_SIZE).numbers[0].f32 == 36, "font size did not reach popup");
    require(attributes.at(NODE_FONT_WEIGHT).numbers[0].i32 == 6, "font weight did not reach popup");
    require(attributes.at(NODE_FONT_FAMILY).text == "Test Font", "font family did not reach popup");
    require(attributes.at(NODE_HEIGHT).numbers[0].f32 == 70, "large-font option height");
    require(attributes.at(NODE_FONT_COLOR).numbers[0].u32 == 0xffeeeeee, "foreground did not reach popup");
    require(nodes.at(parts.scroll)->attributes.at(NODE_BACKGROUND_COLOR).numbers[0].u32 == 0xff112233, "popup background");
    require(deliveries.empty(), "publication echoed an event");
    require(jadix_ohos_update(trigger, 11, "東京 🍎", "", true, false, false, 14, 400, "", 0xff111111, 0xffffffff, ARKUI_DIRECTION_LTR, "Fruit", "", 0, -1, -1) == 0, "reset popup theme");
    require(!nodes.at(trigger)->attributes.count(NODE_ACCESSIBILITY_DESCRIPTION), "cleared description retained stale text");
    require(nodes.at(trigger)->attributes.at(NODE_DIRECTION).numbers[0].i32 == ARKUI_DIRECTION_LTR, "LTR direction did not replace RTL");
    require(jadix_ohos_update(trigger, 11, "東京 🍎", "", true, false, false, 14, 400, "", 0xff111111, 0xffffffff, ARKUI_DIRECTION_AUTO, "Fruit", "", 0, -1, -1) == 0, "reset native direction");
    require(nodes.at(trigger)->attributes.at(NODE_DIRECTION).numbers[0].i32 == ARKUI_DIRECTION_AUTO, "automatic direction did not reset explicit direction");
    require(!attributes.count(NODE_FONT_FAMILY), "system font did not reset");
    clear_select(trigger);
}

static void selection_and_identity() {
    auto& parts = make_select();
    auto trigger = parts.trigger;
    require(open_select(parts) == 0, "open selection");
    auto row = parts.rows[2].node;
    const char* values[] = {"apple", "disabled", "東京"};
    const char* labels[] = {"Apple", "Disabled", "東京 🍎"};
    const bool enabled[] = {true, false, true};
    require(jadix_ohos_select(trigger, values, labels, enabled, 3, 2, true) == 0 && parts.shown && parts.rows[2].node == row, "unchanged publication replaced popup");
    click(parts, 1);
    require(deliveries.empty() && parts.shown, "disabled option selected");
    ArkUI_NodeEvent foreign{trigger, NODE_ON_CLICK, &parts};
    choose_select(&foreign);
    require(deliveries.empty() && parts.shown, "foreign option selected");
    click(parts, 2);
    require(deliveries.size() == 1 && deliveries[0].kind == 6 && deliveries[0].value == "東京" && deliveries[0].popup_closed, "selected event preceded close or lost value");
    require(!parts.shown && focused == trigger && parts.selected == 2, "selection focus or controlled state");
    click(parts, 2);
    require(deliveries.size() == 1, "closed popup emitted duplicate selection");
    require(open_select(parts) == 0, "reopen selection");
    require(jadix_ohos_select(trigger, values, labels, enabled, 3, 0, true) == 0 && !parts.shown, "controlled option change kept stale popup");
    require(parts.rows[2].node == nullptr && nodes.size() == 3, "old option handles retained");
    require(jadix_ohos_select(trigger, values, labels, enabled, 3, 4, true) != 0 && parts.selected == 0, "invalid selected index mutated state");
    clear_select(trigger);
}

static void dismissal_and_disposal() {
    auto& parts = make_select();
    auto trigger = parts.trigger;
    require(open_select(parts) == 0, "open dismissal scenario");
    dismiss(parts, 99);
    require(parts.shown && deliveries.empty(), "unknown dismissal closed popup");
    dismiss(parts, DIALOG_DISMISS_TOUCH_OUTSIDE);
    require(!parts.shown && focused == trigger && deliveries.empty(), "dismissal changed controlled value");
    require(open_select(parts) == 0, "reopen before owner close");
    require(jadix_ohos_select_close(trigger) == 0 && !parts.shown && deliveries.empty(), "owner close emitted selection");
    require(open_select(parts) == 0, "reopen before disposal");
    clear_select(trigger);
}

static void failed_close_is_terminal() {
    auto& parts = make_select();
    auto trigger = parts.trigger;
    require(open_select(parts) == 0, "open failure scenario");
    fail_close = true;
    click(parts, 0);
    require(parts.failed && parts.shown && deliveries.size() == 1 && deliveries[0].kind == -1, "failed close did not fail adapter");
    click(parts, 2);
    require(deliveries.size() == 1 && open_select(parts) != 0, "failed popup accepted more input");
    clear_select(trigger);
}

static void partial_creation_is_disposed() {
    auto& parts = make_select();
    auto trigger = parts.trigger;
    fail_row_attribute = true;
    select_failure(parts, open_select(parts));
    require(parts.failed && !parts.shown && deliveries.size() == 1 && deliveries[0].kind == -1, "partial creation was not terminal");
    clear_select(trigger);
}

static void radio_control_contract() {
    deliveries.clear();
    int target = 42;
    auto wrapper = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(12, &target));
    require(wrapper != nullptr, "create native Radio");
    auto control = toggles.at(wrapper).control;
    auto label = toggles.at(wrapper).label;
    require(nodes.at(control)->kind == ARKUI_NODE_RADIO, "Radio must use actual ArkUI radio node");
    auto update = [&](bool checked, int role) {
        return jadix_ohos_update(wrapper, 12, "??", "", true, false, checked, 22, 600, "", 0xff112233, 0xffffffff, ARKUI_DIRECTION_RTL, "Choice ??", checked ? "Radio help" : "", role, checked, -1);
    };
    require(update(true, 5) == 0, "update checked Radio");
    require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_ROLE).numbers[0].u32 == ARKUI_NODE_RADIO,
        "Radio role must be applied to the native accessibility control");
    require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_STATE).checked == ARKUI_ACCESSIBILITY_CHECKED, "Radio checked accessibility state");
    require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_DESCRIPTION).text == "Radio help", "Radio description must reach actual accessibility control");
    require(nodes.at(control)->attributes.at(NODE_RADIO_CHECKED).numbers[0].i32 == 1, "native checked state");
    require(nodes.at(control)->attributes.at(NODE_TAB_STOP).numbers[0].i32 == 1, "tab entry");
    require(nodes.at(wrapper)->attributes.at(NODE_DIRECTION).numbers[0].i32 == ARKUI_DIRECTION_RTL, "radio container direction");
    require(nodes.at(control)->attributes.at(NODE_DIRECTION).numbers[0].i32 == ARKUI_DIRECTION_RTL, "radio control direction");
    require(nodes.at(label)->attributes.at(NODE_DIRECTION).numbers[0].i32 == ARKUI_DIRECTION_RTL, "radio label direction");
    require(nodes.at(label)->attributes.at(NODE_TEXT_CONTENT).text == "??", "Unicode label");
    require(update(false, 4) == 0 && toggles.at(wrapper).control == control, "retain Radio on update");
    require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_ROLE).numbers[0].u32 == ARKUI_NODE_RADIO,
        "non-tab-stop Radio retains its native accessibility role");
    require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_STATE).checked == ARKUI_ACCESSIBILITY_UNCHECKED, "Radio unchecked accessibility state");
    require(!nodes.at(control)->attributes.count(NODE_ACCESSIBILITY_DESCRIPTION), "Radio description must clear");
    require(nodes.at(control)->attributes.at(NODE_TAB_STOP).numbers[0].i32 == 0, "remove tab entry");
    ArkUI_NodeComponentEvent component{};
    ArkUI_NodeEvent changed{control, NODE_RADIO_EVENT_ON_CHANGE, &target, nullptr, &component};
    nodes.at(control)->receiver(&changed);
    require(deliveries.empty(), "deselection must not activate");
    component.data[0].i32 = 1;
    nodes.at(control)->receiver(&changed);
    require(deliveries.size() == 1 && deliveries[0].kind == 0, "selection activates exactly once");
    ArkUI_UIInputEvent input{};
    input.code = ARKUI_KEYCODE_DPAD_RIGHT;
    ArkUI_NodeEvent key{control, NODE_ON_KEY_EVENT, &target, &input};
    nodes.at(control)->receiver(&key);
    require(input.consumed && input.stopped && deliveries.back().kind == 7 && deliveries.back().detail == 3, "Radio navigation callback");
    auto count = deliveries.size();
    input.type = ARKUI_KEY_EVENT_UP;
    nodes.at(control)->receiver(&key);
    input.type = ARKUI_KEY_EVENT_DOWN;
    input.pressed = {ARKUI_KEYCODE_CTRL_LEFT};
    nodes.at(control)->receiver(&key);
    require(deliveries.size() == count, "keyup and modified keys must not activate");
    require(jadix_ohos_radio_group(wrapper, "42") == 0, "assign native Radio group");
    require(nodes.at(control)->attributes.at(NODE_RADIO_GROUP).text == "42", "Radio group identity");
    require(jadix_ohos_radio_group(wrapper, "") == 0, "clear native Radio group");
    require(nodes.at(control)->attributes.count(NODE_RADIO_GROUP) == 0, "removed Radio group identity");
    jadix_ohos_dispose(wrapper);
    require(nodes.empty() && toggles.empty(), "Radio cleanup releases children and callbacks");
}

static void unsupported_accessibility_roles_are_cleared() {
    int target = 42;
    auto node = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(0, &target));
    require(node != nullptr, "create generic accessibility container");
    auto update = [&](int role) {
        return jadix_ohos_update(node, 0, "", "", true, false, false, 14, 400, "", 0xff112233,
            0xffffffff, ARKUI_DIRECTION_LTR, "Tree", "", role, -1, -1);
    };
    for (int role : {1, 2, 3}) {
        require(update(role) == 0, "update unsupported native accessibility role");
        require(!nodes.at(node)->attributes.count(NODE_ACCESSIBILITY_ROLE),
            "unsupported Tree, TreeItem, or RadioGroup role must not map to a different ArkUI role");
    }
    jadix_ohos_dispose(node);
}

static void toggle_accessibility_state_contract() {
    int target = 42;
    for (int kind : {6, 7}) {
        auto wrapper = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(kind, &target));
        require(wrapper != nullptr, "create native checkbox or switch");
        auto control = toggles.at(wrapper).control;
        auto update = [&](bool checked) {
            return jadix_ohos_update(wrapper, kind, "Choice", "", true, false, checked, 14, 400, "", 0xff112233, 0xffffffff,
                ARKUI_DIRECTION_LTR, "Choice", "", 0, -1, -1);
        };
        require(update(true) == 0, "update checked toggle control");
        require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_STATE).checked == ARKUI_ACCESSIBILITY_CHECKED,
            "toggle checked accessibility state");
        require(update(false) == 0, "update unchecked toggle control");
        require(nodes.at(control)->attributes.at(NODE_ACCESSIBILITY_STATE).checked == ARKUI_ACCESSIBILITY_UNCHECKED,
            "toggle unchecked accessibility state");
        jadix_ohos_dispose(wrapper);
    }
    require(nodes.empty() && toggles.empty(), "toggle accessibility contract releases native controls");
}

static void focus_lifecycle_contract() {
    int target = 42;
    for (int kind : {2, 3, 4, 6, 7, 11, 12}) {
        deliveries.clear();
        auto node = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(kind, &target));
        require(node != nullptr, "create focusable native control");
        if (kind == 11) require(jadix_ohos_select(node, nullptr, nullptr, nullptr, 0, -1, true) == 0, "enable focusable Select");
        auto found = toggles.find(node);
        auto control = found == toggles.end() ? node : found->second.control;
        auto& fixture = *nodes.at(control);
        for (auto type : {NODE_ON_FOCUS, NODE_ON_BLUR}) {
            require(fixture.events.count(type) == 1, "focus lifecycle callback not registered");
            ArkUI_NodeEvent event{control, type, fixture.events.at(type)};
            fixture.receiver(&event);
        }
        require(deliveries.size() == 2 && deliveries[0].kind == 1 && deliveries[1].kind == 8,
            "native focus and blur must retain their order and identity");
        jadix_ohos_dispose(node);
        require(nodes.empty() && toggles.empty() && selects.empty(), "focus cleanup retains native callbacks");
    }
}

static void submit_contract() {
    int target = 42;
    deliveries.clear();
    auto input = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(3, &target));
    auto textarea = static_cast<ArkUI_NodeHandle>(jadix_ohos_create(4, &target));
    require(input && textarea, "create native editors");
    auto& fixture = *nodes.at(input);
    require(fixture.events.count(NODE_TEXT_INPUT_ON_SUBMIT) == 1, "input submit callback registered");
    require(nodes.at(textarea)->events.count(NODE_TEXT_INPUT_ON_SUBMIT) == 0, "textarea keeps native newline");
    ArkUI_NodeEvent event{input, NODE_TEXT_INPUT_ON_SUBMIT, fixture.events.at(NODE_TEXT_INPUT_ON_SUBMIT)};
    fixture.receiver(&event);
    require(deliveries.size() == 1 && deliveries[0].kind == 9, "native submit reaches Rust bridge");
    jadix_ohos_dispose(input);
    jadix_ohos_dispose(textarea);
    require(nodes.empty(), "submit callbacks disposed with native editors");
}

static void accessibility_provider_action_lifecycle() {
    int target = 42;
    auto native = jadix_ohos_create(2, &target);
    require(native != nullptr, "create native accessibility action target");
    auto* provider = jadix_ohos_accessibility_create(reinterpret_cast<void*>(1), reinterpret_cast<void*>(1));
    require(provider != nullptr, "create XComponent accessibility provider");
    const JadixAccessibilityElement element{
        71, 0, nullptr, 0, "button", "Save", "Save changes", 0, 0, 40, 20,
        JADIX_A11Y_ENABLED | JADIX_A11Y_CLICKABLE | JADIX_A11Y_FOCUSABLE, &target
    };
    const JadixAccessibilityElement next_element{
        72, 0, nullptr, 0, "button", "Cancel", "Cancel changes", 0, 30, 40, 50,
        JADIX_A11Y_ENABLED | JADIX_A11Y_FOCUSABLE, &target
    };
    const JadixAccessibilityElement elements[]{element, next_element};
    require(jadix_ohos_accessibility_replace(provider, elements, 2) == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL,
        "publish virtual accessibility element");
    auto* state = static_cast<JadixAccessibilityProvider*>(provider);
    require(!accessibility_events.empty() && accessibility_events.back() == ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_PAGE_CONTENT_UPDATE,
        "publish page content update after tree replacement");
    deliveries.clear();
    require(jadix_a11y_action(state->instance_id.c_str(), 71, ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_CLICK, nullptr, 1)
        == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL, "dispatch virtual click action");
    require(deliveries.size() == 1 && deliveries[0].kind == 0, "virtual click reaches Jadix click event");
    require(jadix_a11y_action(state->instance_id.c_str(), 71,
        ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_GAIN_ACCESSIBILITY_FOCUS, nullptr, 2)
        == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL, "accept accessibility focus action");
    ArkUI_AccessibilityElementInfo focused_info{};
    require(jadix_a11y_focused(state->instance_id.c_str(), 0, ARKUI_ACCESSIBILITY_NATIVE_FOCUS_TYPE_ACCESSIBILITY, 3,
        &focused_info) == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL && focused_info.accessibility_focused,
        "return accessibility-focused element with focused state");
    ArkUI_AccessibilityElementInfo subtree_focused_info{};
    require(jadix_a11y_focused(state->instance_id.c_str(), 71, ARKUI_ACCESSIBILITY_NATIVE_FOCUS_TYPE_ACCESSIBILITY, 3,
        &subtree_focused_info) == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL && subtree_focused_info.id == 71,
        "return focused node when the subtree root is the focused element");
    ArkUI_AccessibilityElementInfo next_info{};
    require(jadix_a11y_next_focus(state->instance_id.c_str(), 71, ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_FORWARD, 6,
        &next_info) == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL && next_info.id == 72,
        "resolve next focusable node in stable visual order");
    require(accessibility_events.back() == ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_ACCESSIBILITY_FOCUSED,
        "report accessibility focus change");
    require(jadix_a11y_action(state->instance_id.c_str(), 71,
        ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_CLEAR_ACCESSIBILITY_FOCUS, nullptr, 4)
        == ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL, "accept accessibility focus clear action");
    require(jadix_a11y_focused(state->instance_id.c_str(), 0, ARKUI_ACCESSIBILITY_NATIVE_FOCUS_TYPE_ACCESSIBILITY, 5,
        &focused_info) == ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED, "clear focused element");
    require(accessibility_events.back() == ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_ACCESSIBILITY_FOCUS_CLEARED,
        "report cleared accessibility focus");
    require(jadix_a11y_action(state->instance_id.c_str(), 73, ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_CLICK, nullptr, 1)
        == ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED, "reject stale virtual element id");
    jadix_ohos_accessibility_dispose(provider);
    jadix_ohos_dispose(native);
    require(nodes.empty(), "accessibility provider action target disposed");
}

int main() {
    initialize();
    try {
        for (bool multiline : {false, true}) {
            auto editor = jadix_ohos_create(multiline ? 4 : 3, nullptr);
            auto node = static_cast<ArkUI_NodeHandle>(editor);
            auto attribute = multiline ? NODE_TEXT_AREA_MAX_LENGTH : NODE_TEXT_INPUT_MAX_LENGTH;
            require(jadix_ohos_max_length(editor, multiline, 4) == 0, "set native max length");
            require(nodes.at(node)->attributes.at(attribute).numbers[0].i32 == 4, "max length forwarded");
            require(jadix_ohos_max_length(editor, multiline, 0) == 0, "zero max length");
            require(jadix_ohos_max_length(editor, multiline, -1) == 0, "reset native max length");
            require(nodes.at(node)->attributes.count(attribute) == 0, "removed native limit");
            require(jadix_ohos_max_length(editor, multiline, -2) != 0, "reject invalid limit");
            jadix_ohos_dispose(editor);
        }
        theme_and_retention();
        keyboard_navigation();
        selection_and_identity();
        dismissal_and_disposal();
        failed_close_is_terminal();
        partial_creation_is_disposed();
        radio_control_contract();
        unsupported_accessibility_roles_are_cleared();
        toggle_accessibility_state_contract();
        focus_lifecycle_contract();
        submit_contract();
        accessibility_provider_action_lifecycle();
        std::cout << "6 production C++ Select callback scenarios passed; ArkUI runtime acceptance=false\n";
        std::cout << "1 production C++ Radio callback scenario passed; ArkUI runtime acceptance=false\n";
        std::cout << "3 production C++ unsupported accessibility role scenarios passed; ArkUI runtime acceptance=false\n";
        std::cout << "2 production C++ toggle accessibility state scenarios passed; ArkUI runtime acceptance=false\n";
        std::cout << "7 production C++ focus lifecycle controls passed; ArkUI runtime acceptance=false\n";
        std::cout << "1 production C++ Submit callback scenario passed; ArkUI runtime acceptance=false\n";
        std::cout << "1 production C++ accessibility provider focus, content notification and action lifecycle passed; ArkUI runtime acceptance=false\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        fail_close = false;
        fail_row_attribute = false;
        selects.clear();
        return 1;
    }
}
