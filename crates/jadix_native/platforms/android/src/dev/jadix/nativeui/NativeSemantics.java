package dev.jadix.nativeui;

import android.os.Build;
import android.os.Bundle;
import android.view.View;
import android.view.accessibility.AccessibilityNodeInfo;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Maps the shared accessibility contract; widget defaults remain authoritative. */
final class NativeSemantics {
    private static final int CUSTOM_ACTION = 0x3f4b0000;
    static void apply(View view, JSONObject semantics) {
        boolean hidden = semantics.optBoolean("hidden");
        boolean explicit = !semantics.isNull("accessible");
        boolean accessible = semantics.optBoolean("accessible");
        int importance = hidden ? View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            : explicit ? accessible ? View.IMPORTANT_FOR_ACCESSIBILITY_YES : View.IMPORTANT_FOR_ACCESSIBILITY_NO
            : view.getContentDescription() != null ? View.IMPORTANT_FOR_ACCESSIBILITY_YES : View.IMPORTANT_FOR_ACCESSIBILITY_AUTO;
        if (view.getImportantForAccessibility() != importance) view.setImportantForAccessibility(importance);
        if (Build.VERSION.SDK_INT >= 28) view.setScreenReaderFocusable(explicit && accessible && !hidden);
        String live = semantics.optString("live");
        int region = "polite".equals(live) ? View.ACCESSIBILITY_LIVE_REGION_POLITE
            : "assertive".equals(live) ? View.ACCESSIBILITY_LIVE_REGION_ASSERTIVE : View.ACCESSIBILITY_LIVE_REGION_NONE;
        if (view.getAccessibilityLiveRegion() != region) view.setAccessibilityLiveRegion(region);
    }

    static String state(JSONObject semantics, String expanded) {
        String value = "mixed".equals(semantics.optString("checked")) ? "Mixed" : "";
        if (!semantics.isNull("pressed")) value += (value.isEmpty() ? "" : ", ") + (semantics.optBoolean("pressed") ? "Pressed" : "Not pressed");
        if (semantics.optBoolean("invalid")) value += (value.isEmpty() ? "" : ", ") + "Invalid";
        if (semantics.optBoolean("busy")) value += (value.isEmpty() ? "" : ", ") + "Busy";
        if (semantics.optBoolean("readOnly")) value += (value.isEmpty() ? "" : ", ") + "Read only";
        if (expanded != null) value += (value.isEmpty() ? "" : ", ") + expanded;
        JSONObject range = semantics.optJSONObject("value");
        if (range != null && !range.isNull("text")) value += (value.isEmpty() ? "" : ", ") + range.optString("text");
        return value.isEmpty() ? null : value;
    }

    static void initialize(View host, AccessibilityNodeInfo info, String role, JSONObject semantics) {
        String widget = null;
        switch (role == null ? "" : role) {
            case "button": case "key": widget = "android.widget.Button"; break;
            case "imagebutton": widget = "android.widget.ImageButton"; break;
            case "radiogroup": widget = "android.widget.RadioGroup"; break;
            case "toolbar": widget = "android.widget.Toolbar"; break;
            case "link": widget = "android.widget.TextView"; info.getExtras().putCharSequence("AccessibilityNodeInfo.roleDescription", "Link"); break;
            case "textbox": widget = "android.widget.EditText"; break;
            case "img": widget = "android.widget.ImageView"; break;
            case "text": case "summary": case "timer": widget = "android.widget.TextView"; break;
            case "adjustable": case "scrollbar": case "spinbutton": widget = "android.widget.SeekBar"; break;
            case "progressbar": widget = "android.widget.ProgressBar"; break;
            case "checkbox": case "menuitemcheckbox": widget = "android.widget.CheckBox"; info.setCheckable(true); break;
            case "radio": case "menuitemradio": widget = "android.widget.RadioButton"; info.setCheckable(true); break;
            case "switch": widget = "android.widget.Switch"; info.setCheckable(true); break;
            case "combobox": widget = "android.widget.Spinner"; break;
            case "list": case "listbox": case "tree": case "menu": widget = "android.widget.ListView"; break;
            case "grid": case "table": widget = "android.widget.GridView"; break;
            case "dialog": case "alertdialog": widget = "android.app.Dialog"; break;
            case "heading": if (Build.VERSION.SDK_INT >= 28) info.setHeading(true); break;
            default: break;
        }
        if (widget != null) info.setClassName(widget);
        if (role != null) info.getExtras().putString("dev.jadix.role", role);
        if (semantics.optBoolean("hidden")) info.setVisibleToUser(false);
        if (semantics.optBoolean("disabled")) info.setEnabled(false);
        String checked = semantics.optString("checked");
        if (!semantics.isNull("checked")) {
            info.setCheckable(true); info.setChecked("true".equals(checked));
            info.getExtras().putString("dev.jadix.checked", checked);
        }
        info.setContentInvalid(semantics.optBoolean("invalid"));
        if (!semantics.isNull("pressed")) { info.setCheckable(true); info.setChecked(semantics.optBoolean("pressed")); }
        if (semantics.optBoolean("readOnly") || !info.isEnabled()) {
            info.setEditable(false);
            for (AccessibilityNodeInfo.AccessibilityAction action : new java.util.ArrayList<>(info.getActionList())) {
                int id=action.getId();
                if(id==AccessibilityNodeInfo.ACTION_CLICK||id==AccessibilityNodeInfo.ACTION_LONG_CLICK
                    ||id==AccessibilityNodeInfo.ACTION_SET_TEXT||id==AccessibilityNodeInfo.ACTION_PASTE
                    ||id==AccessibilityNodeInfo.ACTION_CUT||id==AccessibilityNodeInfo.ACTION_EXPAND
                    ||id==AccessibilityNodeInfo.ACTION_COLLAPSE||id==AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
                    ||id==AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
                    ||id==AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.getId())info.removeAction(action);
            }
        }
        info.getExtras().putBoolean("dev.jadix.busy", semantics.optBoolean("busy"));
        info.getExtras().putBoolean("dev.jadix.readOnly", semantics.optBoolean("readOnly"));
        JSONObject range = semantics.optJSONObject("value");
        if (range != null) {
            boolean scaled = normalized(range);
            info.setRangeInfo(AccessibilityNodeInfo.RangeInfo.obtain(AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_FLOAT,
                scaled ? 0 : (float)range.optDouble("min"), scaled ? 1 : (float)range.optDouble("max"),
                scaled ? (float)fraction(range) : (float)range.optDouble("now")));
            info.getExtras().putDouble("dev.jadix.range.min",range.optDouble("min"));
            info.getExtras().putDouble("dev.jadix.range.max",range.optDouble("max"));
            info.getExtras().putDouble("dev.jadix.range.now",range.optDouble("now"));
        }
        String description = state(semantics, null);
        if (description != null) {
            if (Build.VERSION.SDK_INT >= 30) info.setStateDescription(description);
            else info.getExtras().putCharSequence("androidx.view.accessibility.AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY", description);
        }
        JSONArray actions = semantics.optJSONArray("actions");
        if (actions == null || !info.isEnabled() || semantics.optBoolean("readOnly")) return;
        for (int i=0; i<actions.length(); i++) {
            JSONObject item = actions.optJSONObject(i); if (item == null) continue;
            info.addAction(new AccessibilityNodeInfo.AccessibilityAction(actionId(item.optString("id"), i), item.optString("label")));
        }
    }

    private static int actionId(String id, int index) {
        switch(id) {
            case "activate": return AccessibilityNodeInfo.ACTION_CLICK;
            case "longpress": return AccessibilityNodeInfo.ACTION_LONG_CLICK;
            case "expand": return AccessibilityNodeInfo.ACTION_EXPAND;
            case "collapse": return AccessibilityNodeInfo.ACTION_COLLAPSE;
            case "increment": return AccessibilityNodeInfo.ACTION_SCROLL_FORWARD;
            case "decrement": return AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD;
            case "setValue": return AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.getId();
            default: return CUSTOM_ACTION + index;
        }
    }
    static JSONObject action(View host, JSONObject semantics, int action, Bundle arguments) {
        if (semantics == null || !host.isEnabled() || semantics.optBoolean("hidden")
                || semantics.optBoolean("disabled") || semantics.optBoolean("readOnly")
                || Boolean.FALSE.equals(semantics.opt("accessible"))) return null;
        JSONArray actions = semantics.optJSONArray("actions");
        if (actions == null) return null;
        for (int i=0; i<actions.length(); i++) {
            JSONObject item = actions.optJSONObject(i); if (item == null) continue;
            String id = item.optString("id"); if (actionId(id,i) != action) continue;
            try {
                JSONObject payload = new JSONObject().put("kind", "accessibilityAction").put("id",id);
                if ("setValue".equals(id)) {
                    if (arguments == null || !arguments.containsKey(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE)) return null;
                    double value = arguments.getFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE);
                    JSONObject range = semantics.optJSONObject("value");
                    if (range == null || !Double.isFinite(value)) return null;
                    if (normalized(range)) {
                        if (value < 0 || value > 1) return null;
                        value = (1-value)*range.optDouble("min") + value*range.optDouble("max");
                    }
                    if (!Double.isFinite(value) || value < range.optDouble("min") || value > range.optDouble("max")) return null;
                    payload.put("value",value);
                }
                return payload;
            } catch (JSONException error) { throw new IllegalStateException(error); }
        }
        return null;
    }
    private static boolean normalized(JSONObject range) {
        float min=(float)range.optDouble("min"), max=(float)range.optDouble("max");
        return !Float.isFinite(min) || !Float.isFinite(max) || min >= max;
    }
    private static double fraction(JSONObject range) {
        double min=range.optDouble("min"), max=range.optDouble("max"), now=range.optDouble("now");
        double extent=max-min;
        double fraction=Double.isFinite(extent) ? (now-min)/extent : (now/2-min/2)/(max/2-min/2);
        return Math.max(0,Math.min(1,fraction));
    }
    static boolean mutates(int action) {
        return action == AccessibilityNodeInfo.ACTION_CLICK || action == AccessibilityNodeInfo.ACTION_LONG_CLICK
            || action == AccessibilityNodeInfo.ACTION_SET_TEXT || action == AccessibilityNodeInfo.ACTION_CUT
            || action == AccessibilityNodeInfo.ACTION_PASTE || action == AccessibilityNodeInfo.ACTION_EXPAND
            || action == AccessibilityNodeInfo.ACTION_COLLAPSE || action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
            || action == AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD
            || action == AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.getId() || action >= CUSTOM_ACTION;
    }
    private NativeSemantics() {}
}
