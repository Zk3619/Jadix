#include <arkui/native_interface.h>
#include <arkui/native_dialog.h>
#include <arkui/native_node.h>
#include <arkui/native_node_napi.h>
#include <arkui/native_key_event.h>
#include <arkui/native_interface_accessibility.h>
#include <ace/xcomponent/native_interface_xcomponent.h>
#include <native_drawing/drawing_font_collection.h>
#include <native_drawing/drawing_text_typography.h>
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <memory>
#include <mutex>
#include <atomic>
#include <string>
#include <unordered_map>
#include <vector>

extern "C" void jadix_ohos_receive(void*, int32_t, const char*, const char*, int32_t, int32_t, int32_t, float, float);

struct JadixAccessibilityElement {
    int64_t id;
    int64_t parent_id;
    const int64_t* child_ids;
    size_t child_count;
    const char* component_type;
    const char* text;
    const char* description;
    int32_t left, top, right, bottom;
    uint32_t flags;
    void* target;
};

enum JadixAccessibilityFlags : uint32_t {
    JADIX_A11Y_ENABLED = 1u << 0, JADIX_A11Y_SELECTED = 1u << 1,
    JADIX_A11Y_EXPANDED = 1u << 2, JADIX_A11Y_CHECKABLE = 1u << 3,
    JADIX_A11Y_CHECKED = 1u << 4, JADIX_A11Y_FOCUSABLE = 1u << 5,
    JADIX_A11Y_CLICKABLE = 1u << 6, JADIX_A11Y_SCROLLABLE = 1u << 7,
    JADIX_A11Y_EDITABLE = 1u << 8, JADIX_A11Y_PASSWORD = 1u << 9,
};

struct JadixAccessibilityNode {
    int64_t id = 0, parent_id = 0;
    std::vector<int64_t> children;
    std::string component, text, description;
    ArkUI_AccessibleRect rect{};
    uint32_t flags = 0;
    void* target = nullptr;
};

struct JadixAccessibilityProvider {
    ArkUI_AccessibilityProvider* provider = nullptr;
    std::string instance_id;
    std::mutex mutex;
    std::unordered_map<int64_t, JadixAccessibilityNode> nodes;
    int64_t accessibility_focus = 0;
};

static std::mutex jadix_a11y_registry_mutex;
static std::unordered_map<std::string, std::shared_ptr<JadixAccessibilityProvider>> jadix_a11y_registry;
static std::atomic<uint64_t> jadix_a11y_next_instance{1};

static std::shared_ptr<JadixAccessibilityProvider> jadix_a11y_find(const char* instance) {
    if (!instance) return {};
    std::lock_guard<std::mutex> lock(jadix_a11y_registry_mutex);
    auto found = jadix_a11y_registry.find(instance);
    return found == jadix_a11y_registry.end() ? nullptr : found->second;
}

static int32_t jadix_a11y_fill(ArkUI_AccessibilityElementInfo* info, JadixAccessibilityNode& node,
    int64_t accessibility_focus = 0) {
    if (!info) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    auto set = [](int32_t result) { return result; };
    if (set(OH_ArkUI_AccessibilityElementInfoSetElementId(info, static_cast<int32_t>(node.id))) != 0 ||
        set(OH_ArkUI_AccessibilityElementInfoSetParentId(info, static_cast<int32_t>(node.parent_id))) != 0 ||
        set(OH_ArkUI_AccessibilityElementInfoSetComponentType(info, node.component.c_str())) != 0 ||
        set(OH_ArkUI_AccessibilityElementInfoSetContents(info, node.text.c_str())) != 0 ||
        set(OH_ArkUI_AccessibilityElementInfoSetAccessibilityText(info, node.text.c_str())) != 0 ||
        set(OH_ArkUI_AccessibilityElementInfoSetAccessibilityDescription(info, node.description.c_str())) != 0) return -1;
    auto children = node.children;
    if (OH_ArkUI_AccessibilityElementInfoSetChildNodeIds(info, static_cast<int32_t>(children.size()), children.data()) != 0) return -1;
    ArkUI_AccessibleAction actions[2]{};
    int32_t action_count = 0;
    if (node.flags & JADIX_A11Y_CLICKABLE) {
        actions[action_count++] = {ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_CLICK, "Activate"};
    }
    if (node.flags & JADIX_A11Y_FOCUSABLE) {
        actions[action_count++] = {ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_GAIN_ACCESSIBILITY_FOCUS, "Focus"};
    }
    if (OH_ArkUI_AccessibilityElementInfoSetOperationActions(info, action_count, actions) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetScreenRect(info, &node.rect) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetCheckable(info, (node.flags & JADIX_A11Y_CHECKABLE) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetChecked(info, (node.flags & JADIX_A11Y_CHECKED) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetFocusable(info, (node.flags & JADIX_A11Y_FOCUSABLE) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetFocused(info, false) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetVisible(info, true) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetAccessibilityFocused(info, node.id == accessibility_focus) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetSelected(info, (node.flags & JADIX_A11Y_SELECTED) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetClickable(info, (node.flags & JADIX_A11Y_CLICKABLE) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetEnabled(info, (node.flags & JADIX_A11Y_ENABLED) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetIsPassword(info, (node.flags & JADIX_A11Y_PASSWORD) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetScrollable(info, (node.flags & JADIX_A11Y_SCROLLABLE) != 0) != 0 ||
        OH_ArkUI_AccessibilityElementInfoSetEditable(info, (node.flags & JADIX_A11Y_EDITABLE) != 0) != 0) return -1;
    return 0;
}

static int32_t jadix_a11y_find_by_id(const char* instance, int64_t id, ArkUI_AccessibilitySearchMode mode,
    int32_t, ArkUI_AccessibilityElementInfoList* list) {
    auto state = jadix_a11y_find(instance);
    if (!state || !list) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    std::lock_guard<std::mutex> lock(state->mutex);
    auto root = state->nodes.find(id);
    if (root == state->nodes.end()) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    std::vector<int64_t> selected{id};
    if ((mode & ARKUI_ACCESSIBILITY_NATIVE_SEARCH_MODE_PREFETCH_RECURSIVE_CHILDREN) != 0) {
        std::unordered_map<int64_t, bool> visited{{id, true}};
        for (size_t index = 0; index < selected.size(); ++index) {
            auto item = state->nodes.find(selected[index]);
            if (item == state->nodes.end()) continue;
            for (auto child : item->second.children) {
                if (visited.emplace(child, true).second) selected.push_back(child);
            }
        }
    } else if ((mode & ARKUI_ACCESSIBILITY_NATIVE_SEARCH_MODE_PREFETCH_CHILDREN) != 0) {
        selected.insert(selected.end(), root->second.children.begin(), root->second.children.end());
    }
    for (auto selected_id : selected) {
        auto item = state->nodes.find(selected_id);
        if (item == state->nodes.end()) continue;
        auto* info = OH_ArkUI_AddAndGetAccessibilityElementInfo(list);
        if (jadix_a11y_fill(info, item->second, state->accessibility_focus) != 0) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    }
    return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
}

static int32_t jadix_a11y_find_by_text(const char* instance, int64_t id, const char* query, int32_t,
    ArkUI_AccessibilityElementInfoList* list) {
    auto state = jadix_a11y_find(instance);
    if (!state || !query || !list) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    std::lock_guard<std::mutex> lock(state->mutex);
    auto root = state->nodes.find(id);
    if (root == state->nodes.end()) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    std::vector<int64_t> pending(root->second.children.begin(), root->second.children.end());
    while (!pending.empty()) {
        auto child = pending.back(); pending.pop_back();
        auto item = state->nodes.find(child);
        if (item == state->nodes.end()) continue;
        pending.insert(pending.end(), item->second.children.begin(), item->second.children.end());
        if (item->second.text.find(query) == std::string::npos && item->second.description.find(query) == std::string::npos) continue;
        auto* info = OH_ArkUI_AddAndGetAccessibilityElementInfo(list);
        if (jadix_a11y_fill(info, item->second, state->accessibility_focus) != 0) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    }
    return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
}

static int32_t jadix_a11y_focused(const char* instance, int64_t id, ArkUI_AccessibilityFocusType focus_type, int32_t,
    ArkUI_AccessibilityElementInfo* info) {
    auto state = jadix_a11y_find(instance);
    if (!state || !info) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    if (focus_type != ARKUI_ACCESSIBILITY_NATIVE_FOCUS_TYPE_ACCESSIBILITY)
        return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    std::lock_guard<std::mutex> lock(state->mutex);
    if (!state->accessibility_focus) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    auto focused = state->nodes.find(state->accessibility_focus);
    if (focused == state->nodes.end()) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    if (id != 0) {
        auto ancestor_id = focused->first;
        while (ancestor_id != id) {
            auto ancestor = state->nodes.find(ancestor_id);
            if (ancestor == state->nodes.end() || ancestor->second.parent_id == ancestor_id)
                return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
            ancestor_id = ancestor->second.parent_id;
        }
    }
    return jadix_a11y_fill(info, focused->second, state->accessibility_focus) == 0
        ? ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL : ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
}
static int32_t jadix_a11y_next_focus(const char* instance, int64_t id, ArkUI_AccessibilityFocusMoveDirection direction, int32_t,
    ArkUI_AccessibilityElementInfo* info) {
    auto state = jadix_a11y_find(instance);
    if (!state || !info) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    std::lock_guard<std::mutex> lock(state->mutex);
    auto origin = state->nodes.find(id);
    if (origin == state->nodes.end()) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    std::vector<JadixAccessibilityNode*> candidates;
    candidates.reserve(state->nodes.size());
    for (auto& entry : state->nodes) {
        auto& node = entry.second;
        if ((node.flags & (JADIX_A11Y_ENABLED | JADIX_A11Y_FOCUSABLE)) ==
            (JADIX_A11Y_ENABLED | JADIX_A11Y_FOCUSABLE)) candidates.push_back(&node);
    }
    if (direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_FORWARD || direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_BACKWARD) {
        std::sort(candidates.begin(), candidates.end(), [](const auto* left, const auto* right) {
            if (left->rect.leftTopY != right->rect.leftTopY) return left->rect.leftTopY < right->rect.leftTopY;
            if (left->rect.leftTopX != right->rect.leftTopX) return left->rect.leftTopX < right->rect.leftTopX;
            return left->id < right->id;
        });
        auto current = std::find_if(candidates.begin(), candidates.end(), [id](const auto* node) { return node->id == id; });
        if (current != candidates.end()) {
            if (direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_FORWARD && ++current != candidates.end())
                return jadix_a11y_fill(info, **current, state->accessibility_focus) == 0
                    ? ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL : ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
            if (direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_BACKWARD && current != candidates.begin())
                return jadix_a11y_fill(info, **--current, state->accessibility_focus) == 0
                    ? ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL : ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        }
        return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    }
    const double origin_x = (origin->second.rect.leftTopX + origin->second.rect.rightBottomX) / 2.0;
    const double origin_y = (origin->second.rect.leftTopY + origin->second.rect.rightBottomY) / 2.0;
    JadixAccessibilityNode* best = nullptr;
    double best_primary = 0, best_secondary = 0;
    for (auto* candidate : candidates) {
        const double x = (candidate->rect.leftTopX + candidate->rect.rightBottomX) / 2.0;
        const double y = (candidate->rect.leftTopY + candidate->rect.rightBottomY) / 2.0;
        const double dx = x - origin_x, dy = y - origin_y;
        const bool horizontal = direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_LEFT ||
            direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_RIGHT;
        const bool in_direction = direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_LEFT ? dx < 0 :
            direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_RIGHT ? dx > 0 :
            direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_UP ? dy < 0 :
            direction == ARKUI_ACCESSIBILITY_NATIVE_DIRECTION_DOWN ? dy > 0 : false;
        if (!in_direction) continue;
        const double primary = std::abs(horizontal ? dx : dy);
        const double secondary = std::abs(horizontal ? dy : dx);
        if (!best || primary < best_primary || (primary == best_primary && secondary < best_secondary) ||
            (primary == best_primary && secondary == best_secondary && candidate->id < best->id)) {
            best = candidate;
            best_primary = primary;
            best_secondary = secondary;
        }
    }
    return best && jadix_a11y_fill(info, *best, state->accessibility_focus) == 0
        ? ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL : ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
}
static int32_t jadix_a11y_clear_focus(const char* instance) {
    auto state = jadix_a11y_find(instance);
    if (!state) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    int64_t previous = 0;
    {
        std::lock_guard<std::mutex> lock(state->mutex);
        previous = state->accessibility_focus;
        state->accessibility_focus = 0;
    }
    if (!previous) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
    auto* event = OH_ArkUI_CreateAccessibilityEventInfo();
    if (!event) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    auto result = OH_ArkUI_AccessibilityEventSetEventType(event,
        ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_ACCESSIBILITY_FOCUS_CLEARED);
    if (result == 0) result = OH_ArkUI_AccessibilityEventSetRequestFocusId(event, static_cast<int32_t>(previous));
    if (result != 0) {
        OH_ArkUI_DestoryAccessibilityEventInfo(event);
        return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    }
    OH_ArkUI_SendAccessibilityAsyncEvent(state->provider, event, [](int32_t) {});
    OH_ArkUI_DestoryAccessibilityEventInfo(event);
    return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
}
static int32_t jadix_a11y_cursor(const char*, int64_t, int32_t, int32_t*) { return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED; }

static int32_t jadix_a11y_action(const char* instance, int64_t id, ArkUI_Accessibility_ActionType action,
    ArkUI_AccessibilityActionArguments*, int32_t) {
    auto state = jadix_a11y_find(instance);
    if (!state) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    void* target = nullptr;
    bool focus_changed = false;
    {
        std::lock_guard<std::mutex> lock(state->mutex);
        auto node = state->nodes.find(id);
        if (node == state->nodes.end() || !(node->second.flags & JADIX_A11Y_ENABLED)) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        if (action == ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_GAIN_ACCESSIBILITY_FOCUS) {
            if (!(node->second.flags & JADIX_A11Y_FOCUSABLE)) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
            state->accessibility_focus = id;
            focus_changed = true;
        } else if (action == ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_CLEAR_ACCESSIBILITY_FOCUS) {
            if (state->accessibility_focus != id) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
            state->accessibility_focus = 0;
            focus_changed = true;
        } else {
            target = node->second.target;
            if (action != ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_CLICK || !(node->second.flags & JADIX_A11Y_CLICKABLE) || !target)
                return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        }
    }
    if (focus_changed) {
        auto* event = OH_ArkUI_CreateAccessibilityEventInfo();
        if (!event) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        const auto type = action == ARKUI_ACCESSIBILITY_NATIVE_ACTION_TYPE_GAIN_ACCESSIBILITY_FOCUS
            ? ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_ACCESSIBILITY_FOCUSED
            : ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_ACCESSIBILITY_FOCUS_CLEARED;
        auto result = OH_ArkUI_AccessibilityEventSetEventType(event, type);
        if (result == 0) result = OH_ArkUI_AccessibilityEventSetRequestFocusId(event, static_cast<int32_t>(id));
        if (result != 0) {
            OH_ArkUI_DestoryAccessibilityEventInfo(event);
            return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        }
        OH_ArkUI_SendAccessibilityAsyncEvent(state->provider, event, [](int32_t) {});
        OH_ArkUI_DestoryAccessibilityEventInfo(event);
        return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
    }
    jadix_ohos_receive(target, 0, nullptr, nullptr, 0, 0, 0, 0, 0);
    return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
}

static ArkUI_AccessibilityProviderCallbacksWithInstance jadix_a11y_callbacks = {
    jadix_a11y_find_by_id, jadix_a11y_find_by_text, jadix_a11y_focused, jadix_a11y_next_focus,
    jadix_a11y_action, jadix_a11y_clear_focus, jadix_a11y_cursor
};

extern "C" void* jadix_ohos_accessibility_create(void* environment, void* xcomponent) {
    if (!environment || !xcomponent) return nullptr;
    ArkUI_NodeHandle node = nullptr;
    if (OH_ArkUI_GetNodeHandleFromNapiValue(static_cast<napi_env>(environment),
        static_cast<napi_value>(xcomponent), &node) != 0 || !node) return nullptr;
    auto provider = OH_ArkUI_AccessibilityProvider_Create(node);
    if (!provider) return nullptr;
    auto state = std::make_shared<JadixAccessibilityProvider>();
    state->provider = provider;
    state->instance_id = "jadix-" + std::to_string(jadix_a11y_next_instance.fetch_add(1));
    if (OH_ArkUI_AccessibilityProviderRegisterCallbackWithInstance(state->instance_id.c_str(), provider, &jadix_a11y_callbacks) != 0) {
        OH_ArkUI_AccessibilityProvider_Dispose(provider);
        return nullptr;
    }
    {
        std::lock_guard<std::mutex> lock(jadix_a11y_registry_mutex);
        jadix_a11y_registry.emplace(state->instance_id, state);
    }
    return state.get();
}

extern "C" int32_t jadix_ohos_accessibility_replace(void* handle, const JadixAccessibilityElement* elements, size_t count) {
    auto* raw = static_cast<JadixAccessibilityProvider*>(handle);
    if (!raw || (count && !elements) || count > 100000) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    auto state = jadix_a11y_find(raw->instance_id.c_str());
    if (!state || state.get() != raw) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
    std::unordered_map<int64_t, JadixAccessibilityNode> replacement;
    replacement.reserve(count + 1);
    JadixAccessibilityNode root;
    root.id = 0; root.parent_id = 0; root.component = "group"; root.flags = JADIX_A11Y_ENABLED;
    for (size_t index = 0; index < count; ++index) {
        const auto& source = elements[index];
        if (source.id <= 0 || source.id > INT32_MAX || source.parent_id < 0 || source.parent_id > INT32_MAX ||
            source.child_count > 100000 || (source.child_count && !source.child_ids) ||
            !source.component_type || !source.text || !source.description) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
        JadixAccessibilityNode node;
        node.id = source.id; node.parent_id = source.parent_id;
        node.children.assign(source.child_ids, source.child_ids + source.child_count);
        node.component = source.component_type; node.text = source.text; node.description = source.description;
        node.rect = {source.left, source.top, source.right, source.bottom}; node.flags = source.flags; node.target = source.target;
        if (!replacement.emplace(node.id, std::move(node)).second) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
        if (source.parent_id == 0) root.children.push_back(source.id);
    }
    replacement.emplace(0, std::move(root));
    for (const auto& entry : replacement) {
        if (entry.first == 0) continue;
        if (replacement.find(entry.second.parent_id) == replacement.end()) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
        for (auto child : entry.second.children) {
            auto found = replacement.find(child);
            if (found == replacement.end() || found->second.parent_id != entry.first) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_BAD_PARAMETER;
        }
    }
    int64_t cleared_focus = 0;
    {
        std::lock_guard<std::mutex> lock(state->mutex);
        state->nodes.swap(replacement);
        if (state->accessibility_focus && state->nodes.find(state->accessibility_focus) == state->nodes.end()) {
            cleared_focus = state->accessibility_focus;
            state->accessibility_focus = 0;
        }
    }
    if (cleared_focus) {
        auto* focus_event = OH_ArkUI_CreateAccessibilityEventInfo();
        if (!focus_event) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        auto focus_result = OH_ArkUI_AccessibilityEventSetEventType(focus_event,
            ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_ACCESSIBILITY_FOCUS_CLEARED);
        if (focus_result == 0) focus_result = OH_ArkUI_AccessibilityEventSetRequestFocusId(focus_event,
            static_cast<int32_t>(cleared_focus));
        if (focus_result != 0) {
            OH_ArkUI_DestoryAccessibilityEventInfo(focus_event);
            return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
        }
        OH_ArkUI_SendAccessibilityAsyncEvent(state->provider, focus_event, [](int32_t) {});
        OH_ArkUI_DestoryAccessibilityEventInfo(focus_event);
    }
    auto* event = OH_ArkUI_CreateAccessibilityEventInfo();
    if (!event) return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    const auto event_result = OH_ArkUI_AccessibilityEventSetEventType(event,
        ARKUI_ACCESSIBILITY_NATIVE_EVENT_TYPE_PAGE_CONTENT_UPDATE);
    if (event_result != 0) {
        OH_ArkUI_DestoryAccessibilityEventInfo(event);
        return ARKUI_ACCESSIBILITY_NATIVE_RESULT_FAILED;
    }
    OH_ArkUI_SendAccessibilityAsyncEvent(state->provider, event, [](int32_t) {});
    OH_ArkUI_DestoryAccessibilityEventInfo(event);
    return ARKUI_ACCESSIBILITY_NATIVE_RESULT_SUCCESSFUL;
}

extern "C" void jadix_ohos_accessibility_dispose(void* handle) {
    auto* raw = static_cast<JadixAccessibilityProvider*>(handle);
    if (!raw) return;
    std::shared_ptr<JadixAccessibilityProvider> state;
    {
        std::lock_guard<std::mutex> lock(jadix_a11y_registry_mutex);
        auto found = jadix_a11y_registry.find(raw->instance_id);
        if (found == jadix_a11y_registry.end() || found->second.get() != raw) return;
        state = std::move(found->second);
        jadix_a11y_registry.erase(found);
    }
    std::lock_guard<std::mutex> lock(state->mutex);
    state->nodes.clear();
    OH_ArkUI_AccessibilityProvider_Dispose(state->provider);
    state->provider = nullptr;
}

static ArkUI_NativeNodeAPI_1* node_api() {
    static ArkUI_NativeNodeAPI_1* api = nullptr;
    if (!api) {
        OH_ArkUI_GetModuleInterface(ARKUI_NATIVE_NODE, ArkUI_NativeNodeAPI_1, api);
    }
    return api;
}

static ArkUI_NativeDialogAPI_1* dialog_api() {
    static ArkUI_NativeDialogAPI_1* api = nullptr;
    if (!api) OH_ArkUI_GetModuleInterface(ARKUI_NATIVE_DIALOG, ArkUI_NativeDialogAPI_1, api);
    return api;
}

static int32_t number(ArkUI_NodeHandle node, ArkUI_NodeAttributeType attribute, float value) {
    ArkUI_NumberValue numeric[] = {{.f32 = value}};
    ArkUI_AttributeItem item = {numeric, 1, nullptr, nullptr};
    return node_api()->setAttribute(node, attribute, &item);
}

static int32_t integer(ArkUI_NodeHandle node, ArkUI_NodeAttributeType attribute, int32_t value) {
    ArkUI_NumberValue numeric[] = {{.i32 = value}};
    ArkUI_AttributeItem item = {numeric, 1, nullptr, nullptr};
    return node_api()->setAttribute(node, attribute, &item);
}

static int32_t string(ArkUI_NodeHandle node, ArkUI_NodeAttributeType attribute, const char* value) {
    ArkUI_AttributeItem item = {nullptr, 0, value, nullptr};
    return node_api()->setAttribute(node, attribute, &item);
}

struct ToggleParts { ArkUI_NodeHandle control; ArkUI_NodeHandle label; };
static std::unordered_map<ArkUI_NodeHandle, ToggleParts> toggles;

struct DialogParts {
    ArkUI_NativeDialogHandle controller;
    void* target;
    bool shown;
    bool backdrop;
};
static std::unordered_map<ArkUI_NodeHandle, DialogParts> dialogs;

struct SelectRow {
    std::string value;
    std::string label;
    bool enabled;
    ArkUI_NodeHandle node = nullptr;
};

struct SelectParts {
    ArkUI_NodeHandle trigger;
    void* target;
    std::vector<SelectRow> rows;
    int32_t selected = -1;
    bool enabled = false;
    bool shown = false;
    bool failed = false;
    float font_size = 14;
    uint32_t font_weight = 400;
    uint32_t foreground = 0xff111111;
    uint32_t background = 0xffffffff;
    std::string font_family;
    ArkUI_NativeDialogHandle controller = nullptr;
    ArkUI_NodeHandle scroll = nullptr;
    ArkUI_NodeHandle column = nullptr;

    void clear_rows() {
        for (auto& row : rows) {
            if (row.node) {
                node_api()->unregisterNodeEvent(row.node, NODE_ON_CLICK);
                node_api()->unregisterNodeEvent(row.node, NODE_ON_KEY_EVENT);
                node_api()->removeChild(column, row.node);
                node_api()->disposeNode(row.node);
            }
        }
        rows.clear();
    }

    ~SelectParts() {
        target = nullptr;
        if (controller) {
            if (shown) dialog_api()->close(controller);
            dialog_api()->removeContent(controller);
            dialog_api()->dispose(controller);
        }
        clear_rows();
        if (scroll && column) node_api()->removeChild(scroll, column);
        if (column) node_api()->disposeNode(column);
        if (scroll) node_api()->disposeNode(scroll);
    }
};
static std::unordered_map<ArkUI_NodeHandle, std::unique_ptr<SelectParts>> selects;

static int32_t style_select(SelectParts& parts) {
    if (!parts.scroll) return 0;
    auto height = std::max(44.0f, std::ceil(parts.font_size * 1.5f + 16));
    auto total_height = static_cast<float>(parts.rows.size()) * height;
    auto result = integer(parts.scroll, NODE_BACKGROUND_COLOR, static_cast<int32_t>(parts.background));
    result = result ? result : number(parts.scroll, NODE_HEIGHT, std::min(264.0f, total_height));
    result = result ? result : number(parts.column, NODE_HEIGHT, total_height);
    for (auto& row : parts.rows) {
        if (!row.node) continue;
        result = result ? result : number(row.node, NODE_HEIGHT, height);
        result = result ? result : number(row.node, NODE_FONT_SIZE, parts.font_size);
        result = result ? result : integer(row.node, NODE_FONT_WEIGHT, static_cast<int32_t>(std::clamp(parts.font_weight, 100u, 900u) / 100 - 1));
        result = result ? result : integer(row.node, NODE_FONT_COLOR, static_cast<int32_t>(parts.foreground));
        result = result ? result : integer(row.node, NODE_BACKGROUND_COLOR, static_cast<int32_t>(parts.background));
        if (parts.font_family.empty()) result = result ? result : node_api()->resetAttribute(row.node, NODE_FONT_FAMILY);
        else result = result ? result : string(row.node, NODE_FONT_FAMILY, parts.font_family.c_str());
    }
    return result;
}

static int32_t close_select(SelectParts& parts, bool restore_focus) {
    if (!parts.shown) return 0;
    parts.shown = false;
    auto result = dialog_api()->close(parts.controller);
    if (result) parts.shown = true;
    if (!result && restore_focus) result = integer(parts.trigger, NODE_FOCUS_STATUS, true);
    return result;
}

static void select_failure(SelectParts& parts, int32_t result) {
    if (result && parts.target) {
        parts.failed = true;
        jadix_ohos_receive(parts.target, -1, nullptr, nullptr, 0, 0, 0, 0, 0);
    }
}

extern "C" int32_t jadix_ohos_select_close(void* handle) {
    auto found = selects.find(static_cast<ArkUI_NodeHandle>(handle));
    return found == selects.end() ? -1 : close_select(*found->second, false);
}

static void dismiss_select(ArkUI_DialogDismissEvent* event) {
    OH_ArkUI_DialogDismissEvent_SetShouldBlockDismiss(event, true);
    auto* parts = static_cast<SelectParts*>(OH_ArkUI_DialogDismissEvent_GetUserData(event));
    if (!parts || !parts->target || !parts->shown) return;
    auto reason = OH_ArkUI_DialogDismissEvent_GetDismissReason(event);
    if (reason >= DIALOG_DISMISS_BACK_PRESS && reason <= DIALOG_DISMISS_SLIDE_DOWN) {
        select_failure(*parts, close_select(*parts, true));
    }
}

static void choose_select(ArkUI_NodeEvent* event) {
    auto* parts = static_cast<SelectParts*>(OH_ArkUI_NodeEvent_GetUserData(event));
    if (!parts || !parts->target || !parts->shown || !parts->enabled || parts->failed) return;
    auto node = OH_ArkUI_NodeEvent_GetNodeHandle(event);
    auto row = std::find_if(parts->rows.begin(), parts->rows.end(), [node](const auto& row) { return row.node == node; });
    if (row == parts->rows.end() || !row->enabled) return;
    auto value = row->value;
    auto result = close_select(*parts, true);
    select_failure(*parts, result);
    if (!result) jadix_ohos_receive(parts->target, 6, value.c_str(), nullptr, 0, 0, 0, 0, 0);
}

static void receive_select_row(ArkUI_NodeEvent* event);

static int32_t focus_select(SelectParts& parts, size_t index) {
    auto height = std::max(44.0f, std::ceil(parts.font_size * 1.5f + 16));
    auto maximum = std::max(0.0f, static_cast<float>(parts.rows.size()) * height - 264.0f);
    ArkUI_NumberValue offset[] = {{.f32 = 0}, {.f32 = std::min(maximum, static_cast<float>(index) * height)}, {.i32 = 0}};
    ArkUI_AttributeItem item = {offset, 3, nullptr, nullptr};
    auto result = node_api()->setAttribute(parts.scroll, NODE_SCROLL_OFFSET, &item);
    return result ? result : integer(parts.rows[index].node, NODE_FOCUS_STATUS, true);
}

static int32_t open_select(SelectParts& parts) {
    if (parts.failed) return -1;
    if (parts.shown || !parts.enabled || std::none_of(parts.rows.begin(), parts.rows.end(), [](const auto& row) { return row.enabled; })) return 0;
    int32_t result = 0;
    if (!parts.controller) {
        parts.controller = dialog_api()->create();
        parts.scroll = node_api()->createNode(ARKUI_NODE_SCROLL);
        parts.column = node_api()->createNode(ARKUI_NODE_COLUMN);
        if (!parts.controller || !parts.scroll || !parts.column) return -1;
        result = node_api()->setLengthMetricUnit(parts.scroll, ARKUI_LENGTH_METRIC_UNIT_VP);
        result = result ? result : node_api()->setLengthMetricUnit(parts.column, ARKUI_LENGTH_METRIC_UNIT_VP);
        result = result ? result : number(parts.scroll, NODE_WIDTH, 280);
        result = result ? result : number(parts.column, NODE_WIDTH, 280);
        result = result ? result : node_api()->addChild(parts.scroll, parts.column);
        result = result ? result : dialog_api()->setContent(parts.controller, parts.scroll);
        result = result ? result : dialog_api()->setModalMode(parts.controller, true);
        result = result ? result : dialog_api()->setAutoCancel(parts.controller, true);
        result = result ? result : dialog_api()->enableCustomStyle(parts.controller, true);
        result = result ? result : dialog_api()->enableCustomAnimation(parts.controller, false);
        result = result ? result : dialog_api()->registerOnWillDismissWithUserData(parts.controller, &parts, dismiss_select);
    }
    size_t focus = parts.rows.size();
    for (size_t index = 0; !result && index < parts.rows.size(); ++index) {
        auto& row = parts.rows[index];
        if (!row.node) {
            row.node = node_api()->createNode(ARKUI_NODE_BUTTON);
            if (!row.node) return -1;
            result = node_api()->setLengthMetricUnit(row.node, ARKUI_LENGTH_METRIC_UNIT_VP);
            result = result ? result : number(row.node, NODE_WIDTH, 280);
            result = result ? result : string(row.node, NODE_BUTTON_LABEL, row.label.c_str());
            result = result ? result : string(row.node, NODE_ACCESSIBILITY_TEXT, row.label.c_str());
            result = result ? result : integer(row.node, NODE_ENABLED, row.enabled);
            auto* state = OH_ArkUI_AccessibilityState_Create();
            if (!state) return -1;
            OH_ArkUI_AccessibilityState_SetDisabled(state, !row.enabled);
            OH_ArkUI_AccessibilityState_SetSelected(state, static_cast<int32_t>(index) == parts.selected);
            ArkUI_AttributeItem item = {nullptr, 0, nullptr, state};
            result = result ? result : node_api()->setAttribute(row.node, NODE_ACCESSIBILITY_STATE, &item);
            OH_ArkUI_AccessibilityState_Dispose(state);
            result = result ? result : node_api()->addNodeEventReceiver(row.node, receive_select_row);
            result = result ? result : node_api()->registerNodeEvent(row.node, NODE_ON_CLICK, 0, &parts);
            result = result ? result : node_api()->registerNodeEvent(row.node, NODE_ON_KEY_EVENT, 0, &parts);
            result = result ? result : node_api()->addChild(parts.column, row.node);
        }
        if (row.enabled && (focus == parts.rows.size() || static_cast<int32_t>(index) == parts.selected)) focus = index;
    }
    result = result ? result : style_select(parts);
    result = result ? result : dialog_api()->show(parts.controller, false);
    if (!result) {
        parts.shown = true;
        result = focus_select(parts, focus);
    }
    return result;
}

static void key_select(SelectParts& parts, ArkUI_NodeEvent* event) {
    if (!parts.target || !parts.enabled || parts.failed) return;
    auto node = OH_ArkUI_NodeEvent_GetNodeHandle(event);
    auto row = std::find_if(parts.rows.begin(), parts.rows.end(), [node](const auto& value) { return value.node == node && value.enabled; });
    if (node != parts.trigger && (row == parts.rows.end() || !parts.shown)) return;
    auto* input = OH_ArkUI_NodeEvent_GetInputEvent(event);
    if (!input || OH_ArkUI_UIInputEvent_GetType(input) != ARKUI_UIINPUTEVENT_TYPE_KEY) return;
    int32_t pressed[16];
    int32_t count = 16;
    if (OH_ArkUI_UIInputEvent_GetPressedKeys(input, pressed, &count) || count < 0 || count > 16) return;
    for (int32_t index = 0; index < count; ++index) {
        auto key = pressed[index];
        if (key == ARKUI_KEYCODE_SHIFT_LEFT || key == ARKUI_KEYCODE_SHIFT_RIGHT || key == ARKUI_KEYCODE_CTRL_LEFT || key == ARKUI_KEYCODE_CTRL_RIGHT ||
            key == ARKUI_KEYCODE_ALT_LEFT || key == ARKUI_KEYCODE_ALT_RIGHT || key == ARKUI_KEYCODE_META_LEFT || key == ARKUI_KEYCODE_META_RIGHT) return;
    }
    auto code = OH_ArkUI_KeyEvent_GetKeyCode(input);
    bool escape = code == ARKUI_KEYCODE_ESCAPE;
    bool first = code == ARKUI_KEYCODE_MOVE_HOME;
    bool last = code == ARKUI_KEYCODE_MOVE_END;
    bool previous = code == ARKUI_KEYCODE_DPAD_UP;
    if ((!escape && !first && !last && !previous && code != ARKUI_KEYCODE_DPAD_DOWN) || (escape && !parts.shown)) return;
    auto type = OH_ArkUI_KeyEvent_GetType(input);
    if (type != ARKUI_KEY_EVENT_DOWN && type != ARKUI_KEY_EVENT_UP) return;
    OH_ArkUI_KeyEvent_SetConsumed(input, true);
    OH_ArkUI_KeyEvent_StopPropagation(input, true);
    if (type == ARKUI_KEY_EVENT_UP) return;
    if (escape) { select_failure(parts, close_select(parts, true)); return; }
    bool was_shown = parts.shown;
    auto result = open_select(parts);
    if (!result && parts.shown && (was_shown || first || last)) {
        auto length = static_cast<int32_t>(parts.rows.size());
        auto index = row == parts.rows.end() ? parts.selected : static_cast<int32_t>(row - parts.rows.begin());
        if (first) index = -1;
        else if (last || index < 0) index = previous || last ? 0 : -1;
        auto direction = previous || last ? -1 : 1;
        for (int32_t attempt = 0; attempt < length; ++attempt) {
            index = (index + direction + length) % length;
            if (parts.rows[index].enabled) { result = focus_select(parts, static_cast<size_t>(index)); break; }
        }
    }
    select_failure(parts, result);
}

static void receive_select_row(ArkUI_NodeEvent* event) {
    auto* parts = static_cast<SelectParts*>(OH_ArkUI_NodeEvent_GetUserData(event));
    if (!parts) return;
    auto type = OH_ArkUI_NodeEvent_GetEventType(event);
    if (type == NODE_ON_CLICK) choose_select(event);
    else if (type == NODE_ON_KEY_EVENT) key_select(*parts, event);
}

static void receive_select(ArkUI_NodeEvent* event) {
    auto found = selects.find(OH_ArkUI_NodeEvent_GetNodeHandle(event));
    if (found == selects.end()) return;
    auto& parts = *found->second;
    auto type = OH_ArkUI_NodeEvent_GetEventType(event);
    if (type == NODE_ON_CLICK) select_failure(parts, open_select(parts));
    else if (type == NODE_ON_KEY_EVENT) key_select(parts, event);
    else if (type == NODE_ON_FOCUS && parts.target && parts.enabled) jadix_ohos_receive(parts.target, 1, nullptr, nullptr, 0, 0, 0, 0, 0);
    else if (type == NODE_ON_BLUR && parts.target) jadix_ohos_receive(parts.target, 8, nullptr, nullptr, 0, 0, 0, 0, 0);
}

extern "C" int32_t jadix_ohos_select(void* handle, const char* const* values, const char* const* labels, const bool* enabled, size_t count, int32_t selected, bool interactive) {
    auto found = selects.find(static_cast<ArkUI_NodeHandle>(handle));
    if (found == selects.end() || count > 256 || selected < -1 || selected >= static_cast<int32_t>(count) || (count && (!values || !labels || !enabled))) return -1;
    auto& parts = *found->second;
    bool changed = parts.rows.size() != count || parts.selected != selected || parts.enabled != interactive;
    for (size_t index = 0; index < count; ++index) {
        if (!values[index] || !labels[index]) return -1;
        if (!changed) changed = parts.rows[index].value != values[index] || parts.rows[index].label != labels[index] || parts.rows[index].enabled != enabled[index];
    }
    if (!changed) return 0;
    auto result = close_select(parts, false);
    if (result) return result;
    parts.clear_rows();
    parts.rows.reserve(count);
    for (size_t index = 0; index < count; ++index) parts.rows.push_back({values[index], labels[index], enabled[index]});
    parts.selected = selected;
    parts.enabled = interactive;
    return 0;
}

static void dismiss_dialog(ArkUI_DialogDismissEvent* event) {
    OH_ArkUI_DialogDismissEvent_SetShouldBlockDismiss(event, true);
    auto* parts = static_cast<DialogParts*>(OH_ArkUI_DialogDismissEvent_GetUserData(event));
    if (!parts || !parts->shown || !parts->target) return;
    auto reason = OH_ArkUI_DialogDismissEvent_GetDismissReason(event);
    if (reason < DIALOG_DISMISS_BACK_PRESS || reason > DIALOG_DISMISS_SLIDE_DOWN) return;
    bool backdrop = reason == DIALOG_DISMISS_TOUCH_OUTSIDE;
    if (backdrop && !parts->backdrop) return;
    jadix_ohos_receive(parts->target, 5, nullptr, nullptr, backdrop, 0, 0, 0, 0);
}

extern "C" bool jadix_ohos_dialog_supported() {
    return dialog_api() != nullptr;
}

extern "C" int32_t jadix_ohos_dialog(void* handle, bool open, bool backdrop) {
    auto found = dialogs.find(static_cast<ArkUI_NodeHandle>(handle));
    if (found == dialogs.end()) return -1;
    auto& parts = found->second;
    parts.backdrop = backdrop;
    if (parts.shown == open) return 0;
    if (!open) {
        parts.shown = false;
        auto result = dialog_api()->close(parts.controller);
        if (result) parts.shown = true;
        return result;
    }
    auto result = dialog_api()->show(parts.controller, false);
    if (!result) parts.shown = true;
    return result;
}

static void receive(ArkUI_NodeEvent* event) {
    auto* target = OH_ArkUI_NodeEvent_GetUserData(event);
    auto type = OH_ArkUI_NodeEvent_GetEventType(event);
    auto node = OH_ArkUI_NodeEvent_GetNodeHandle(event);
    if (type == NODE_ON_CLICK) {
        jadix_ohos_receive(target, 0, nullptr, nullptr, 0, 0, 0, 0, 0);
    } else if (type == NODE_ON_KEY_EVENT) {
        auto* input = OH_ArkUI_NodeEvent_GetInputEvent(event);
        if (!input || OH_ArkUI_UIInputEvent_GetType(input) != ARKUI_UIINPUTEVENT_TYPE_KEY) return;
        int32_t pressed[16];
        int32_t count = 16;
        if (OH_ArkUI_UIInputEvent_GetPressedKeys(input, pressed, &count) || count < 0 || count > 16) return;
        for (int32_t index = 0; index < count; ++index) {
            auto key = pressed[index];
            if (key == ARKUI_KEYCODE_SHIFT_LEFT || key == ARKUI_KEYCODE_SHIFT_RIGHT || key == ARKUI_KEYCODE_CTRL_LEFT || key == ARKUI_KEYCODE_CTRL_RIGHT ||
                key == ARKUI_KEYCODE_ALT_LEFT || key == ARKUI_KEYCODE_ALT_RIGHT || key == ARKUI_KEYCODE_META_LEFT || key == ARKUI_KEYCODE_META_RIGHT) return;
        }
        int32_t key;
        switch (OH_ArkUI_KeyEvent_GetKeyCode(input)) {
            case ARKUI_KEYCODE_DPAD_UP: key = 0; break;
            case ARKUI_KEYCODE_DPAD_DOWN: key = 1; break;
            case ARKUI_KEYCODE_DPAD_LEFT: key = 2; break;
            case ARKUI_KEYCODE_DPAD_RIGHT: key = 3; break;
            case ARKUI_KEYCODE_MOVE_HOME: key = 4; break;
            case ARKUI_KEYCODE_MOVE_END: key = 5; break;
            default: return;
        }
        auto action = OH_ArkUI_KeyEvent_GetType(input);
        if (action != ARKUI_KEY_EVENT_DOWN && action != ARKUI_KEY_EVENT_UP) return;
        OH_ArkUI_KeyEvent_SetConsumed(input, true);
        OH_ArkUI_KeyEvent_StopPropagation(input, true);
        if (action == ARKUI_KEY_EVENT_DOWN) jadix_ohos_receive(target, 7, nullptr, nullptr, key, 0, 0, 0, 0);
    } else if (type == NODE_TEXT_INPUT_ON_SUBMIT) {
        jadix_ohos_receive(target, 9, nullptr, nullptr, 0, 0, 0, 0, 0);
    } else if (type == NODE_RADIO_EVENT_ON_CHANGE) {
        auto* value = OH_ArkUI_NodeEvent_GetNodeComponentEvent(event);
        // Rust owns mutual exclusion. Native deselection is not another activation.
        if (value && value->data[0].i32 == 1) jadix_ohos_receive(target, 0, nullptr, nullptr, 0, 0, 0, 0, 0);
    } else if (type == NODE_ON_FOCUS) {
        jadix_ohos_receive(target, 1, nullptr, nullptr, 0, 0, 0, 0, 0);
    } else if (type == NODE_ON_BLUR) {
        jadix_ohos_receive(target, 8, nullptr, nullptr, 0, 0, 0, 0, 0);
    } else if (type == NODE_CHECKBOX_EVENT_ON_CHANGE || type == NODE_TOGGLE_ON_CHANGE) {
        auto* value = OH_ArkUI_NodeEvent_GetNodeComponentEvent(event);
        if (value) jadix_ohos_receive(target, 2, nullptr, nullptr, value->data[0].i32, 0, 0, 0, 0);
    } else if (type == NODE_SCROLL_EVENT_ON_SCROLL) {
        auto* value = OH_ArkUI_NodeEvent_GetNodeComponentEvent(event);
        if (value) jadix_ohos_receive(target, 4, nullptr, nullptr, 0, 0, 0, value->data[0].f32, value->data[1].f32);
    } else if (type == NODE_TEXT_INPUT_ON_CHANGE_WITH_PREVIEW_TEXT || type == NODE_TEXT_AREA_ON_CHANGE_WITH_PREVIEW_TEXT) {
        auto* value = OH_ArkUI_NodeEvent_GetTextChangeEvent(event);
        auto attribute = type == NODE_TEXT_INPUT_ON_CHANGE_WITH_PREVIEW_TEXT ? NODE_TEXT_INPUT_TEXT_SELECTION : NODE_TEXT_AREA_TEXT_SELECTION;
        auto* selection = node_api()->getAttribute(node, attribute);
        if (value && selection && selection->size >= 2) {
            jadix_ohos_receive(target, 3, value->pStr, value->pExtendStr, value->number,
                selection->value[0].i32, selection->value[1].i32, 0, 0);
        } else {
            jadix_ohos_receive(target, -1, nullptr, nullptr, 0, 0, 0, 0, 0);
        }
    }
}

extern "C" int32_t jadix_ohos_content(void* environment, void* value, void** result) {
    return OH_ArkUI_GetNodeContentFromNapiValue(static_cast<napi_env>(environment), static_cast<napi_value>(value), reinterpret_cast<ArkUI_NodeContentHandle*>(result));
}

extern "C" void* jadix_ohos_create(int32_t kind, void* target) {
    if (!node_api()) return nullptr;
    ArkUI_NodeType type;
    switch (kind) {
        case 0: type = ARKUI_NODE_STACK; break;
        case 9: case 10: type = ARKUI_NODE_STACK; break;
        case 1: type = ARKUI_NODE_TEXT; break;
        case 2: case 11: type = ARKUI_NODE_BUTTON; break;
        case 3: type = ARKUI_NODE_TEXT_INPUT; break;
        case 4: type = ARKUI_NODE_TEXT_AREA; break;
        case 5: type = ARKUI_NODE_SCROLL; break;
        case 6: case 7: case 12: type = ARKUI_NODE_ROW; break;
        case 8: type = ARKUI_NODE_IMAGE; break;
        default: return nullptr;
    }
    auto node = node_api()->createNode(type);
    if (!node) return nullptr;
    int32_t result = node_api()->setLengthMetricUnit(node, ARKUI_LENGTH_METRIC_UNIT_VP);
    if (kind == 11) {
        if (!dialog_api()) { node_api()->disposeNode(node); return nullptr; }
        auto parts = std::make_unique<SelectParts>();
        parts->trigger = node;
        parts->target = target;
        selects.emplace(node, std::move(parts));
        result = result ? result : node_api()->addNodeEventReceiver(node, receive_select);
        result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_CLICK, 0, target);
        result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_FOCUS, 0, target);
        result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_BLUR, 0, target);
        result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_KEY_EVENT, 0, target);
        if (result) { selects.erase(node); node_api()->disposeNode(node); return nullptr; }
    }
    if (kind == 9 || kind == 10) {
        auto* api = dialog_api();
        auto controller = api ? api->create() : nullptr;
        if (!controller) { node_api()->disposeNode(node); return nullptr; }
        auto found = dialogs.emplace(node, DialogParts{controller, target, false, false}).first;
        result = result ? result : api->setContent(controller, node);
        result = result ? result : api->setModalMode(controller, kind == 9);
        result = result ? result : api->setAutoCancel(controller, true);
        result = result ? result : api->enableCustomStyle(controller, true);
        result = result ? result : api->enableCustomAnimation(controller, false);
        result = result ? result : api->setContentAlignment(controller, ARKUI_ALIGNMENT_TOP_START, 0, 0);
        result = result ? result : api->registerOnWillDismissWithUserData(controller, &found->second, dismiss_dialog);
        if (result) {
            api->removeContent(controller);
            api->dispose(controller);
            dialogs.erase(found);
            node_api()->disposeNode(node);
            return nullptr;
        }
    }
    auto control = node;
    ArkUI_NodeHandle label = nullptr;
    if (kind == 6 || kind == 7 || kind == 12) {
        control = node_api()->createNode(kind == 12 ? ARKUI_NODE_RADIO : kind == 6 ? ARKUI_NODE_CHECKBOX : ARKUI_NODE_TOGGLE);
        label = node_api()->createNode(ARKUI_NODE_TEXT);
        if (!control || !label) {
            if (control) node_api()->disposeNode(control);
            if (label) node_api()->disposeNode(label);
            node_api()->disposeNode(node);
            return nullptr;
        }
        result = result ? result : node_api()->setLengthMetricUnit(control, ARKUI_LENGTH_METRIC_UNIT_VP);
        result = result ? result : node_api()->setLengthMetricUnit(label, ARKUI_LENGTH_METRIC_UNIT_VP);
        result = result ? result : integer(node, NODE_ROW_ALIGN_ITEMS, ARKUI_VERTICAL_ALIGNMENT_CENTER);
        result = result ? result : number(control, NODE_WIDTH, kind == 7 ? 44.0f : 24.0f);
        result = result ? result : number(control, NODE_HEIGHT, 28);
        result = result ? result : number(label, NODE_LAYOUT_WEIGHT, 1);
        result = result ? result : number(label, NODE_MARGIN, 8);
        result = result ? result : node_api()->addChild(node, control);
        result = result ? result : node_api()->addChild(node, label);
    }
    if (target && result == 0 && kind != 9 && kind != 10 && kind != 11) {
        result = node_api()->addNodeEventReceiver(control, receive);
        if (kind == 2) result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_CLICK, 0, target);
        if (kind >= 2 && kind <= 4) result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_FOCUS, 0, target);
        if (kind >= 2 && kind <= 4) result = result ? result : node_api()->registerNodeEvent(node, NODE_ON_BLUR, 0, target);
        if (kind == 3) result = result ? result : node_api()->registerNodeEvent(node, NODE_TEXT_INPUT_ON_CHANGE_WITH_PREVIEW_TEXT, 0, target);
        if (kind == 3) result = result ? result : node_api()->registerNodeEvent(node, NODE_TEXT_INPUT_ON_SUBMIT, 0, target);
        if (kind == 4) result = result ? result : node_api()->registerNodeEvent(node, NODE_TEXT_AREA_ON_CHANGE_WITH_PREVIEW_TEXT, 0, target);
        if (kind == 5) result = result ? result : node_api()->registerNodeEvent(node, NODE_SCROLL_EVENT_ON_SCROLL, 0, target);
        if (kind == 6 || kind == 7 || kind == 12) {
            result = result ? result : node_api()->registerNodeEvent(control, NODE_ON_FOCUS, 0, target);
            result = result ? result : node_api()->registerNodeEvent(control, NODE_ON_BLUR, 0, target);
            result = result ? result : node_api()->registerNodeEvent(control, kind == 12 ? NODE_RADIO_EVENT_ON_CHANGE : kind == 6 ? NODE_CHECKBOX_EVENT_ON_CHANGE : NODE_TOGGLE_ON_CHANGE, 0, target);
        }
    }
    if (kind == 12 && target && !result) result = result ? result : node_api()->registerNodeEvent(control, NODE_ON_KEY_EVENT, 0, target);
    if (kind == 5 && !result) result = integer(node, NODE_SCROLL_SCROLL_DIRECTION, ARKUI_SCROLL_DIRECTION_FREE);
    if (result) {
        if (kind == 6 || kind == 7 || kind == 12) {
            for (auto event : {NODE_ON_FOCUS, NODE_ON_BLUR, NODE_CHECKBOX_EVENT_ON_CHANGE, NODE_TOGGLE_ON_CHANGE, NODE_RADIO_EVENT_ON_CHANGE, NODE_ON_KEY_EVENT})
                node_api()->unregisterNodeEvent(control, event);
            node_api()->removeChild(node, control);
            node_api()->removeChild(node, label);
            node_api()->disposeNode(control);
            node_api()->disposeNode(label);
        }
        node_api()->disposeNode(node); return nullptr;
    }
    if (kind == 6 || kind == 7 || kind == 12) toggles.emplace(node, ToggleParts{control, label});
    return node;
}

extern "C" void jadix_ohos_dispose(void* handle) {
    if (handle && node_api()) {
        auto node = static_cast<ArkUI_NodeHandle>(handle);
        if (selects.find(node) != selects.end()) {
            node_api()->unregisterNodeEvent(node, NODE_ON_CLICK);
            node_api()->unregisterNodeEvent(node, NODE_ON_FOCUS);
            node_api()->unregisterNodeEvent(node, NODE_ON_BLUR);
            node_api()->unregisterNodeEvent(node, NODE_ON_KEY_EVENT);
            selects.erase(node);
        }
        auto dialog = dialogs.find(node);
        if (dialog != dialogs.end()) {
            auto& parts = dialog->second;
            parts.target = nullptr;
            if (parts.shown) {
                parts.shown = false;
                dialog_api()->close(parts.controller);
            }
            dialog_api()->removeContent(parts.controller);
            dialog_api()->dispose(parts.controller);
            dialogs.erase(dialog);
        }
        auto parts = toggles.find(node);
        if (parts != toggles.end()) {
            for (auto type : {NODE_ON_FOCUS, NODE_ON_BLUR, NODE_CHECKBOX_EVENT_ON_CHANGE, NODE_TOGGLE_ON_CHANGE, NODE_RADIO_EVENT_ON_CHANGE, NODE_ON_KEY_EVENT})
                node_api()->unregisterNodeEvent(parts->second.control, type);
            node_api()->removeChild(node, parts->second.control);
            node_api()->removeChild(node, parts->second.label);
            node_api()->disposeNode(parts->second.control);
            node_api()->disposeNode(parts->second.label);
            toggles.erase(parts);
        }
        for (auto type : {NODE_ON_CLICK, NODE_ON_FOCUS, NODE_ON_BLUR, NODE_ON_KEY_EVENT,
                NODE_TEXT_INPUT_ON_CHANGE_WITH_PREVIEW_TEXT, NODE_TEXT_INPUT_ON_SUBMIT, NODE_TEXT_AREA_ON_CHANGE_WITH_PREVIEW_TEXT,
                NODE_SCROLL_EVENT_ON_SCROLL})
            node_api()->unregisterNodeEvent(node, type);
        node_api()->disposeNode(node);
    }
}

extern "C" int32_t jadix_ohos_mount(void* content, void* node, bool attach) {
    return attach ? OH_ArkUI_NodeContent_AddNode(static_cast<ArkUI_NodeContentHandle>(content), static_cast<ArkUI_NodeHandle>(node))
        : OH_ArkUI_NodeContent_RemoveNode(static_cast<ArkUI_NodeContentHandle>(content), static_cast<ArkUI_NodeHandle>(node));
}

extern "C" int32_t jadix_ohos_child(void* parent, void* child, int32_t index) {
    if (dialogs.find(static_cast<ArkUI_NodeHandle>(child)) != dialogs.end()) return 0;
    return index < 0 ? node_api()->removeChild(static_cast<ArkUI_NodeHandle>(parent), static_cast<ArkUI_NodeHandle>(child))
        : node_api()->insertChildAt(static_cast<ArkUI_NodeHandle>(parent), static_cast<ArkUI_NodeHandle>(child), index);
}

extern "C" int32_t jadix_ohos_geometry(void* handle, float left, float top, float width, float height) {
    auto node = static_cast<ArkUI_NodeHandle>(handle);
    ArkUI_NumberValue position[] = {{.f32 = left}, {.f32 = top}};
    ArkUI_AttributeItem item = {position, 2, nullptr, nullptr};
    int32_t result = node_api()->setAttribute(node, NODE_POSITION, &item);
    result = result ? result : number(node, NODE_WIDTH, width);
    return result ? result : number(node, NODE_HEIGHT, height);
}

extern "C" int32_t jadix_ohos_update(void* handle, int32_t kind, const char* value, const char* placeholder, bool enabled, bool password, bool checked, float size, uint32_t weight, const char* family, uint32_t foreground, uint32_t background, int32_t direction, const char* accessible, const char* description, int32_t accessible_role, int32_t accessible_selected, int32_t accessible_expanded) {
    auto node = static_cast<ArkUI_NodeHandle>(handle);
    int32_t result = integer(node, NODE_ENABLED, enabled);
    result = result ? result : integer(node, NODE_BACKGROUND_COLOR, static_cast<int32_t>(background));
    result = result ? result : integer(node, NODE_DIRECTION, direction);
    result = result ? result : node_api()->resetAttribute(node, NODE_ACCESSIBILITY_ROLE);
    if (accessible_selected >= 0) {
        auto* state = OH_ArkUI_AccessibilityState_Create();
        if (!state) return -1;
        OH_ArkUI_AccessibilityState_SetDisabled(state, !enabled);
        OH_ArkUI_AccessibilityState_SetSelected(state, accessible_selected != 0);
        ArkUI_AttributeItem item = {nullptr, 0, nullptr, state};
        result = result ? result : node_api()->setAttribute(node, NODE_ACCESSIBILITY_STATE, &item);
        OH_ArkUI_AccessibilityState_Dispose(state);
    } else {
        result = result ? result : node_api()->resetAttribute(node, NODE_ACCESSIBILITY_STATE);
    }
    std::string description_text = description ? description : "";
    if (accessible_expanded >= 0) {
        if (!description_text.empty()) description_text += ", ";
        description_text += accessible_expanded ? "Expanded" : "Collapsed";
    }
    if (!description_text.empty()) {
        result = result ? result : string(node, NODE_ACCESSIBILITY_DESCRIPTION, description_text.c_str());
    } else {
        result = result ? result : node_api()->resetAttribute(node, NODE_ACCESSIBILITY_DESCRIPTION);
    }
    if (kind == 8 && (!accessible || !accessible[0])) accessible = placeholder;
    if (accessible && accessible[0]) result = result ? result : string(node, NODE_ACCESSIBILITY_TEXT, accessible);
    else result = result ? result : node_api()->resetAttribute(node, NODE_ACCESSIBILITY_TEXT);
    if (kind == 6 || kind == 7 || kind == 12) {
        auto parts = toggles.find(node);
        if (parts == toggles.end()) return -1;
        auto control = parts->second.control;
        result = result ? result : integer(control, NODE_ENABLED, enabled);
        result = result ? result : integer(control, NODE_DIRECTION, direction);
        if (kind == 12 && (accessible_role == 4 || accessible_role == 5)) {
            ArkUI_NumberValue role[] = {{.u32 = static_cast<uint32_t>(ARKUI_NODE_RADIO)}};
            ArkUI_AttributeItem item = {role, 1, nullptr, nullptr};
            result = result ? result : node_api()->setAttribute(control, NODE_ACCESSIBILITY_ROLE, &item);
        } else {
            result = result ? result : node_api()->resetAttribute(control, NODE_ACCESSIBILITY_ROLE);
        }
        auto* control_state = OH_ArkUI_AccessibilityState_Create();
        if (!control_state) return -1;
        OH_ArkUI_AccessibilityState_SetDisabled(control_state, !enabled);
        OH_ArkUI_AccessibilityState_SetSelected(control_state, accessible_selected > 0);
        OH_ArkUI_AccessibilityState_SetCheckedState(
            control_state, checked ? ARKUI_ACCESSIBILITY_CHECKED : ARKUI_ACCESSIBILITY_UNCHECKED);
        ArkUI_AttributeItem control_state_item = {nullptr, 0, nullptr, control_state};
        result = result ? result : node_api()->setAttribute(control, NODE_ACCESSIBILITY_STATE, &control_state_item);
        OH_ArkUI_AccessibilityState_Dispose(control_state);
        result = result ? result : integer(parts->second.label, NODE_DIRECTION, direction);
        result = result ? result : string(control, NODE_ACCESSIBILITY_TEXT, accessible && accessible[0] ? accessible : value);
        if (!description_text.empty()) result = result ? result : string(control, NODE_ACCESSIBILITY_DESCRIPTION, description_text.c_str());
        else result = result ? result : node_api()->resetAttribute(control, NODE_ACCESSIBILITY_DESCRIPTION);
        if (kind == 12) {
            result = result ? result : integer(node, NODE_ACCESSIBILITY_MODE, ARKUI_ACCESSIBILITY_MODE_DISABLED);
            result = result ? result : integer(control, NODE_TAB_STOP, enabled && accessible_role == 5);
            result = result ? result : integer(parts->second.label, NODE_ACCESSIBILITY_MODE, ARKUI_ACCESSIBILITY_MODE_DISABLED);
        }
        auto attribute = kind == 12 ? NODE_RADIO_CHECKED : kind == 6 ? NODE_CHECKBOX_SELECT : NODE_TOGGLE_VALUE;
        auto* current = node_api()->getAttribute(control, attribute);
        if (!current || current->size < 1 || current->value[0].i32 != static_cast<int32_t>(checked)) {
            result = result ? result : integer(control, attribute, checked);
        }
        node = parts->second.label;
    }
    if (kind == 8) {
        auto* current = node_api()->getAttribute(node, NODE_IMAGE_SRC);
        if (!current || !current->string || std::strcmp(current->string, value)) {
            result = result ? result : string(node, NODE_IMAGE_SRC, value);
        }
        current = node_api()->getAttribute(node, NODE_IMAGE_ALT);
        if (!current || !current->string || std::strcmp(current->string, placeholder)) {
            result = result ? result : string(node, NODE_IMAGE_ALT, placeholder);
        }
        result = result ? result : integer(node, NODE_IMAGE_OBJECT_FIT, ARKUI_OBJECT_FIT_CONTAIN);
    }
    if ((kind >= 1 && kind <= 4) || kind == 6 || kind == 7 || kind == 12 || kind == 11) {
        auto attribute = (kind == 1 || kind == 6 || kind == 7 || kind == 12) ? NODE_TEXT_CONTENT : (kind == 2 || kind == 11) ? NODE_BUTTON_LABEL : kind == 3 ? NODE_TEXT_INPUT_TEXT : NODE_TEXT_AREA_TEXT;
        auto* current = node_api()->getAttribute(node, attribute);
        if (!current || !current->string || std::strcmp(current->string, value)) result = result ? result : string(node, attribute, value);
        result = result ? result : number(node, NODE_FONT_SIZE, size);
        result = result ? result : integer(node, NODE_FONT_WEIGHT, static_cast<int32_t>(std::clamp(weight, 100u, 900u) / 100 - 1));
        result = result ? result : integer(node, NODE_FONT_COLOR, static_cast<int32_t>(foreground));
        if (family && family[0]) result = result ? result : string(node, NODE_FONT_FAMILY, family);
        else result = result ? result : node_api()->resetAttribute(node, NODE_FONT_FAMILY);
    }
    if (kind == 3 || kind == 4) result = result ? result : string(node, kind == 3 ? NODE_TEXT_INPUT_PLACEHOLDER : NODE_TEXT_AREA_PLACEHOLDER, placeholder);
    if (kind == 3) result = result ? result : integer(node, NODE_TEXT_INPUT_TYPE, password ? ARKUI_TEXTINPUT_TYPE_PASSWORD : ARKUI_TEXTINPUT_TYPE_NORMAL);
    if (kind == 11) {
        auto found = selects.find(node);
        if (found == selects.end()) return -1;
        auto& parts = *found->second;
        parts.font_size = size;
        parts.font_weight = weight;
        parts.foreground = foreground;
        parts.background = background;
        parts.font_family = family ? family : "";
        result = result ? result : style_select(parts);
    }
    return result;
}

extern "C" int32_t jadix_ohos_focus(void* handle, bool focused) {
    auto node = static_cast<ArkUI_NodeHandle>(handle);
    auto parts = toggles.find(node);
    return integer(parts == toggles.end() ? node : parts->second.control, NODE_FOCUS_STATUS, focused);
}

extern "C" int32_t jadix_ohos_radio_group(void* handle, const char* group) {
    if (!handle || !group) return -1;
    auto found = toggles.find(static_cast<ArkUI_NodeHandle>(handle));
    if (found == toggles.end()) return -1;
    auto control = found->second.control;
    return group[0] ? string(control, NODE_RADIO_GROUP, group)
                    : node_api()->resetAttribute(control, NODE_RADIO_GROUP);
}

extern "C" int32_t jadix_ohos_max_length(void* handle, bool multiline, int32_t limit) {
    if (!handle || limit < -1) return -1;
    auto node = static_cast<ArkUI_NodeHandle>(handle);
    auto attribute = multiline ? NODE_TEXT_AREA_MAX_LENGTH : NODE_TEXT_INPUT_MAX_LENGTH;
    return limit < 0 ? node_api()->resetAttribute(node, attribute) : integer(node, attribute, limit);
}

extern "C" int32_t jadix_ohos_scroll(void* handle, float horizontal, float vertical) {
    ArkUI_NumberValue values[] = {{.f32 = horizontal}, {.f32 = vertical}, {.i32 = 0}};
    ArkUI_AttributeItem item = {values, 3, nullptr, nullptr};
    return node_api()->setAttribute(static_cast<ArkUI_NodeHandle>(handle), NODE_SCROLL_OFFSET, &item);
}

extern "C" int32_t jadix_ohos_measure(const char* text, const char* family, double size, uint32_t weight, double maximum, double* result) {
    auto* collection = OH_Drawing_CreateFontCollection();
    auto* style = OH_Drawing_CreateTypographyStyle();
    auto* font = OH_Drawing_CreateTextStyle();
    if (!collection || !style || !font) {
        if (font) OH_Drawing_DestroyTextStyle(font);
        if (style) OH_Drawing_DestroyTypographyStyle(style);
        if (collection) OH_Drawing_DestroyFontCollection(collection);
        return -1;
    }
    OH_Drawing_SetTextStyleFontSize(font, size);
    OH_Drawing_SetTextStyleFontWeight(font, static_cast<int>(std::clamp(weight, 100u, 900u) / 100 - 1));
    if (family && family[0]) OH_Drawing_SetTextStyleFontFamilies(font, 1, &family);
    auto* builder = OH_Drawing_CreateTypographyHandler(style, collection);
    OH_Drawing_Typography* typography = nullptr;
    if (builder) {
        OH_Drawing_TypographyHandlerPushTextStyle(builder, font);
        OH_Drawing_TypographyHandlerAddText(builder, text[0] ? text : " ");
        typography = OH_Drawing_CreateTypography(builder);
    }
    if (typography) {
        OH_Drawing_TypographyLayout(typography, std::max(1.0, maximum));
        result[0] = OH_Drawing_TypographyGetLongestLine(typography);
        result[1] = OH_Drawing_TypographyGetHeight(typography);
        result[2] = OH_Drawing_TypographyGetAlphabeticBaseline(typography);
        OH_Drawing_DestroyTypography(typography);
    }
    if (builder) OH_Drawing_DestroyTypographyHandler(builder);
    OH_Drawing_DestroyTextStyle(font);
    OH_Drawing_DestroyTypographyStyle(style);
    OH_Drawing_DestroyFontCollection(collection);
    return typography ? 0 : -1;
}
