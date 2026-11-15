package dev.jadix.nativeui;

import android.content.Context;
import android.app.Dialog;
import android.content.res.ColorStateList;
import android.graphics.Color;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Typeface;
import android.os.Looper;
import android.os.Build;
import android.text.Editable;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.util.Base64;
import android.view.View;
import android.view.ViewGroup;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.WindowInsets;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.KeyEvent;
import android.view.Window;
import android.view.WindowManager;
import android.graphics.drawable.ColorDrawable;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.InputConnectionWrapper;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.CompoundButton;
import android.widget.Switch;
import android.widget.Spinner;
import android.widget.ArrayAdapter;
import android.widget.AdapterView;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.ImageView;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.PopupWindow;
import java.math.BigInteger;
import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.HashSet;
import java.util.ArrayList;
import java.util.WeakHashMap;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

public final class NativeView extends FrameLayout implements AutoCloseable {
    private final NativeBatch batches = new NativeBatch();
    private final HashMap<String, View> views = new HashMap<>();
    private final HashMap<String, JSONObject> descriptors = new HashMap<>();
    private final HashMap<String, BigInteger> toggles = new HashMap<>();
    private final WeakHashMap<View, NativeAccessibility> accessibility = new WeakHashMap<>();
    private final HashMap<String, NativeDialog> dialogs = new HashMap<>();
    private final ArrayList<String> dialogOrder = new ArrayList<>();
    private final HashMap<String, NativePopover> popovers = new HashMap<>();
    private final ArrayList<String> popoverOrder = new ArrayList<>();
    private boolean popoverUpdatePending;
    private String root;
    private String lastError;
    private java.util.function.Consumer<String> stateListener;
    private boolean publishing;
    private long environment = 1;
    private boolean resizing;
    private int modalViewportBottom = Integer.MAX_VALUE;
    private final float density;

    public static native String nativeCreate(NativeView peer, float width, float height, long count, String text);
    public static native String nativeEvent(String root, String json);
    private static native String nativeDispatch(String root, String json);
    public static native String nativeResize(String root, float width, float height, long environment);
    public static native String nativeState(String root);
    public static native String nativeRestore(String root, String state);
    public static native void nativeDispose(String root);

    public NativeView(Context context) {
        super(context);
        density = context.getResources().getDisplayMetrics().density;
        setClipChildren(true);
        setSaveFromParentEnabled(false);
        setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        getViewTreeObserver().addOnScrollChangedListener(this::schedulePopovers);
        setOnApplyWindowInsetsListener((view, insets) -> {
            int left, top, right, bottom;
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets safe = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime());
                left = safe.left; top = safe.top; right = safe.right; bottom = safe.bottom;
            } else {
                left = insets.getSystemWindowInsetLeft(); top = insets.getSystemWindowInsetTop();
                right = insets.getSystemWindowInsetRight(); bottom = insets.getSystemWindowInsetBottom();
            }
            if (getPaddingLeft() != left || getPaddingTop() != top || getPaddingRight() != right || getPaddingBottom() != bottom) {
                setPadding(left, top, right, bottom);
                scheduleResize();
            }
            return insets;
        });
    }

    private void uiThread() {
        if (Looper.myLooper() != Looper.getMainLooper()) throw new IllegalStateException("UI thread required");
    }

    public void mount(long count, String text) {
        uiThread();
        if (root != null) throw new IllegalStateException("root already mounted");
        lastError = null;
        int width = getWidth() > 0 ? Math.max(0, getWidth() - getPaddingLeft() - getPaddingRight()) : getResources().getDisplayMetrics().widthPixels;
        int height = getHeight() > 0 ? Math.max(0, getHeight() - getPaddingTop() - getPaddingBottom()) : getResources().getDisplayMetrics().heightPixels;
        root = nativeCreate(this, width / density, height / density, count, text);
        schedulePopovers();
    }

    public String token() { return root; }
    public String state() { uiThread(); return nativeState(root); }
    public void restore(String state) { uiThread(); nativeRestore(root, state); }
    public String takeError() { uiThread(); String error = lastError; lastError = null; return error; }
    public void setStateListener(java.util.function.Consumer<String> listener) { uiThread(); stateListener = listener; }
    public View findKind(String kind) {
        for (String id : descriptors.keySet()) if (kind.equals(descriptors.get(id).optString("kind"))) return views.get(id);
        return null;
    }
    public Dialog activeDialog() {
        if (dialogOrder.isEmpty()) return null;
        return dialogs.get(dialogOrder.get(dialogOrder.size()-1)).window;
    }
    public PopupWindow activePopover() {
        return popoverOrder.isEmpty() ? null : popovers.get(popoverOrder.get(popoverOrder.size()-1)).window;
    }

    private int px(double value) { return (int) Math.round(value * density); }

    private View create(JSONObject node) throws JSONException {
        String kind = node.getString("kind");
        switch (kind) {
            case "box": return new FrameLayout(getContext());
            case "dialog": return new NativeDialog(getContext());
            case "popover": return new NativePopover(getContext());
            case "scroll":
                ScrollView scroll = new ScrollView(getContext());
                scroll.setFillViewport(false);
                scroll.setClipChildren(true);
                return scroll;
            case "text": return new TextView(getContext());
            case "button": return "radio".equals(node.optString("role"))
                ? new android.widget.RadioButton(getContext()) : new Button(getContext());
            case "input": return new NativeInput(getContext());
            case "textarea":
                NativeInput input = new NativeInput(getContext());
                input.setSingleLine(false);
                input.setImeOptions(EditorInfo.IME_ACTION_NONE);
                input.setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE);
                input.setGravity(android.view.Gravity.TOP | android.view.Gravity.START);
                return input;
            case "checkbox": return new CheckBox(getContext());
            case "switch": return new Switch(getContext());
            case "select": return new NativeSelect(getContext());
            case "image":
                ImageView image = new ImageView(getContext());
                image.setScaleType(ImageView.ScaleType.FIT_CENTER);
                image.setAdjustViewBounds(true);
                return image;
            default: throw new IllegalArgumentException("unsupported native kind: " + kind);
        }
    }

    private int color(JSONArray color) throws JSONException {
        return Color.argb(color.getInt(3), color.getInt(0), color.getInt(1), color.getInt(2));
    }

    private void textStyle(TextView text, JSONObject style) throws JSONException {
        text.setTextSize(TypedValue.COMPLEX_UNIT_SP, (float) style.getDouble("font_size"));
        text.setTextColor(color(style.getJSONArray("foreground")));
        String family = style.getString("font_family").split(",")[0].replace("\"", "").trim();
        text.setTypeface(Typeface.create("system-ui".equals(family) ? "sans-serif" : family,
                style.getInt("font_weight") >= 600 ? Typeface.BOLD : Typeface.NORMAL));
    }

    void applyAccessibility(View view, JSONObject node) throws JSONException {
        String role = node.isNull("role") ? null : node.getString("role");
        String supplementalDescription = node.isNull("description") ? null : node.getString("description");
        Object selectedValue = node.opt("selected");
        Object expandedValue = node.opt("expanded");
        Boolean selected = selectedValue instanceof Boolean ? (Boolean) selectedValue : null;
        Boolean expanded = expandedValue instanceof Boolean ? (Boolean) expandedValue : null;
        if (role == null && selected == null && expanded == null && supplementalDescription == null) {
            if (accessibility.remove(view) != null) {
                view.setAccessibilityDelegate(null);
                if (view.isAttachedToWindow()) view.sendAccessibilityEvent(android.view.accessibility.AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
            }
            return;
        }
        NativeAccessibility delegate = accessibility.get(view);
        if (delegate == null) {
            delegate = new NativeAccessibility();
            accessibility.put(view, delegate);
            view.setAccessibilityDelegate(delegate);
        }
        if (delegate.update(role, selected, expanded, supplementalDescription) && view.isAttachedToWindow()) {
            view.sendAccessibilityEvent(android.view.accessibility.AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
        }
    }

    private static final class NativeAccessibility extends View.AccessibilityDelegate {
        private String role;
        private Boolean selected;
        private Boolean expanded;
        private String supplementalDescription;

        boolean update(String role, Boolean selected, Boolean expanded, String supplementalDescription) {
            boolean changed = !java.util.Objects.equals(this.role, role)
                || !java.util.Objects.equals(this.selected, selected)
                || !java.util.Objects.equals(this.expanded, expanded)
                || !java.util.Objects.equals(this.supplementalDescription, supplementalDescription);
            this.role = role;
            this.selected = selected;
            this.expanded = expanded;
            this.supplementalDescription = supplementalDescription;
            return changed;
        }

        @Override public void onInitializeAccessibilityNodeInfo(View host, AccessibilityNodeInfo info) {
            super.onInitializeAccessibilityNodeInfo(host, info);
            if (Build.VERSION.SDK_INT >= 36) {
                info.setSupplementalDescription(supplementalDescription);
            } else {
                info.getExtras().putCharSequence(
                    "androidx.view.accessibility.AccessibilityNodeInfoCompat.SUPPLEMENTAL_DESCRIPTION_KEY",
                    supplementalDescription);
            }
            String description;
            switch (role == null ? "" : role) {
                case "radiogroup": description = "Radio group"; break;
                case "radio":
                    description = "Radio button";
                    info.setClassName(android.widget.RadioButton.class.getName());
                    info.setCheckable(true);
                    info.setChecked(Boolean.TRUE.equals(selected));
                    break;
                case "tablist": description = "Tab list"; break;
                case "tab": description = "Tab"; info.setClassName(View.class.getName()); break;
                case "tabpanel": description = "Tab panel"; break;
                case "tree": description = "Tree"; break;
                case "treeitem": description = "Tree item"; info.setClassName(View.class.getName()); break;
                default: description = null;
            }
            if (description != null) info.getExtras().putCharSequence("AccessibilityNodeInfo.roleDescription", description);
            if (selected != null && !"radio".equals(role)) info.setSelected(selected);
            if (expanded != null) {
                String state = expanded ? "Expanded" : "Collapsed";
                if (Build.VERSION.SDK_INT >= 30) {
                    info.setStateDescription(state);
                } else {
                    info.getExtras().putCharSequence(
                        "androidx.view.accessibility.AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY", state);
                }
                if (Build.VERSION.SDK_INT >= 36) {
                    try {
                        String constant = expanded ? "EXPANDED_STATE_FULL" : "EXPANDED_STATE_COLLAPSED";
                        int value = AccessibilityNodeInfo.class.getField(constant).getInt(null);
                        AccessibilityNodeInfo.class.getMethod("setExpandedState", int.class).invoke(info, value);
                    } catch (ReflectiveOperationException ignored) {
                    }
                }
            }
        }
    }

    private void style(View view, JSONObject node) throws JSONException {
        JSONObject style = node.getJSONObject("style");
        if (view instanceof TextView) textStyle((TextView) view, style);
        JSONArray background = style.getJSONArray("background");
        if (view.getBackground() == null) view.setBackgroundColor(Color.TRANSPARENT);
        view.setBackgroundTintList(background.getInt(3) == 0 ? null : ColorStateList.valueOf(color(background)));
        if (node.isNull("name")) view.setContentDescription(null);
        else view.setContentDescription(node.getString("name"));
        applyAccessibility(view, node);
        if (view instanceof NativeSelect) ((NativeSelect) view).configure(node);
        if (view instanceof ImageView) {
            String source = node.getString("source");
            if (!source.startsWith("data:image/png;base64,")) throw new IllegalArgumentException("unsupported native image source");
            byte[] bytes = Base64.decode(source.substring("data:image/png;base64,".length()), Base64.DEFAULT);
            if (bytes.length < 33) throw new IllegalArgumentException("invalid native PNG header");
            int width = ByteBuffer.wrap(bytes).getInt(16), height = ByteBuffer.wrap(bytes).getInt(20);
            if (width <= 0 || height <= 0 || width > 4096 || height > 4096 || (long) width * height > 4 * 1024 * 1024) {
                throw new IllegalArgumentException("invalid native image dimensions");
            }
            Bitmap bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.length);
            if (bitmap == null) {
                ((ImageView) view).setImageResource(android.R.drawable.ic_menu_report_image);
                view.setBackgroundTintList(null);
                view.setBackgroundColor(Color.rgb(238, 238, 238));
            } else {
                view.setBackgroundColor(color(background));
                view.setBackgroundTintList(null);
                bitmap.setDensity(android.util.DisplayMetrics.DENSITY_DEFAULT);
                ((ImageView) view).setImageBitmap(bitmap);
            }
            if (Build.VERSION.SDK_INT >= 30) view.setStateDescription(bitmap == null ? "Image unavailable" : null);
            view.setContentDescription(node.isNull("name") ? node.getString("alt") : node.getString("name"));
            view.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
        }
        if (view instanceof TextView) {
            String value = node.getString("text");
            TextView text = (TextView) view;
            if (text instanceof CompoundButton) {
                boolean radio = "radio".equals(node.optString("role"));
                ((CompoundButton) text).setChecked(radio ? node.optBoolean("selected") : node.getBoolean("checked"));
                text.setText(value);
                text.setEnabled(node.getBoolean("enabled"));
                if (!radio && view.getTag() != null) toggles.put((String) view.getTag(), new BigInteger(node.getString("editRevision")));
            } else if (text instanceof NativeInput) {
                NativeInput input = (NativeInput) text;
                boolean password = node.optBoolean("password", false);
                int expectedType = android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE
                    | (password ? android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD : android.text.InputType.TYPE_TEXT_VARIATION_NORMAL);
                if ((input.getInputType() & android.text.InputType.TYPE_MASK_VARIATION) != (expectedType & android.text.InputType.TYPE_MASK_VARIATION)) {
                    int start = Math.max(0, input.getSelectionStart()), end = Math.max(0, input.getSelectionEnd());
                    Typeface font = input.getTypeface();
                    input.setInputType(expectedType);
                    input.setTypeface(font);
                    input.setSelection(Math.min(start, input.length()), Math.min(end, input.length()));
                }
                BigInteger incoming = new BigInteger(node.getString("editRevision"));
                if (incoming.compareTo(input.revision) > 0) input.revision = incoming;
                if (!input.getText().toString().equals(value)) {
                    input.setText(value);
                    input.setSelection(value.length());
                }
                input.setHint(node.getString("placeholder"));
                input.setEnabled(node.getBoolean("enabled"));
            } else {
                if (!text.getText().toString().equals(value)) text.setText(value);
                if (text instanceof Button) text.setEnabled(node.getBoolean("enabled"));
            }
        }
    }

    public float[] measure(String json) throws JSONException {
        uiThread();
        JSONObject request = new JSONObject(json);
        View view = create(request.getJSONObject("node"));
        style(view, request.getJSONObject("node"));
        int widthMode = "exact".equals(request.getString("widthMode")) ? MeasureSpec.EXACTLY : MeasureSpec.AT_MOST;
        int heightMode = "exact".equals(request.getString("heightMode")) ? MeasureSpec.EXACTLY : MeasureSpec.AT_MOST;
        view.measure(MeasureSpec.makeMeasureSpec(px(request.getDouble("maxWidth")), widthMode),
                MeasureSpec.makeMeasureSpec(px(request.getDouble("maxHeight")), heightMode));
        return new float[] { view.getMeasuredWidth() / density, view.getMeasuredHeight() / density };
    }

    private void emit(String id, JSONObject payload) {
        if (publishing || root == null || !eventVisible(id)) return;
        try {
            JSONObject event = new JSONObject().put("root", root).put("node", id).put("payload", payload);
            try {
                String state = stateListener == null ? nativeEvent(root, event.toString()) : nativeDispatch(root, event.toString());
                if (stateListener != null && !state.isEmpty()) stateListener.accept(state);
            }
            catch (IllegalStateException error) {
                lastError = error.getMessage();
                View view = views.get(id);
                JSONObject descriptor = descriptors.get(id);
                if (view != null && descriptor != null) {
                    publishing = true;
                    try { style(view, descriptor); }
                    finally { publishing = false; }
                }
            }
        } catch (JSONException error) { throw new IllegalStateException(error); }
    }

    private boolean eventVisible(String id) {
        String active = dialogOrder.isEmpty() ? null : dialogOrder.get(dialogOrder.size()-1);
        boolean inModal = active == null;
        String current = id;
        while (current != null) {
            JSONObject node = descriptors.get(current);
            if (node == null) return false;
            if (("dialog".equals(node.optString("kind")) || "popover".equals(node.optString("kind"))) && !node.optBoolean("open")) return false;
            if (current.equals(active)) inModal = true;
            current = node.isNull("parent") ? null : node.optString("parent");
        }
        return inModal;
    }

    private JSONObject payload(String kind) {
        try { return new JSONObject().put("kind", kind); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }

    private void listen(String id, View view, String kind) {
        view.setTag(id);
        if ("button".equals(kind)) view.setOnClickListener(clicked -> emit(id, payload("activate")));
        if (view instanceof CompoundButton && !"button".equals(kind)) ((CompoundButton) view).setOnCheckedChangeListener((button, checked) -> {
            if (publishing || root == null) return;
            BigInteger revision = toggles.getOrDefault(id, BigInteger.ZERO).add(BigInteger.ONE);
            toggles.put(id, revision);
            try { emit(id, payload("toggle").put("checked", checked).put("revision", revision.toString())); }
            catch (JSONException error) { throw new IllegalStateException(error); }
        });
        if (view instanceof Button || view instanceof CompoundButton || view instanceof EditText || view instanceof NativeSelect) view.setOnFocusChangeListener((focused, hasFocus) -> {
            if (!hasFocus) emit(id, payload("blur"));
            else emit(id, payload(focused.isInTouchMode() ? "focus" : "focusVisible"));
        });
        if (view instanceof ScrollView) view.setOnScrollChangeListener((scrolled, x, y, oldX, oldY) -> {
            try { emit(id, payload("scroll").put("x", x / density).put("y", y / density)); }
            catch (JSONException error) { throw new IllegalStateException(error); }
        });
    }

    public void publish(String json) throws JSONException {
        uiThread();
        JSONObject snapshot = new JSONObject(json);
        publishSnapshot(snapshot, null, true, null);
    }

    public String publishBatch(String json) throws JSONException {
        uiThread();
        NativeBatch.Prepared prepared = batches.prepare(json);
        publishSnapshot(prepared.snapshot(), prepared.topology ? null : prepared.updates, prepared.topology, prepared.nodes);
        if (prepared.focusChanged) {
            publishing = true;
            try {
            View target = prepared.focus == null ? null : views.get(prepared.focus);
            if (target != null) target.requestFocus();
            else if (findFocus() != null) findFocus().clearFocus();
            } finally { publishing = false; }
        }
        batches.commit(prepared);
        return prepared.root + ":" + prepared.previous + ":" + prepared.revision;
    }

    private void publishSnapshot(JSONObject snapshot, JSONArray updates, boolean topology, HashMap<String, JSONObject> frameDescriptors) throws JSONException {
        if (snapshot.getInt("protocol") != 1) throw new IllegalArgumentException("native protocol mismatch");
        if (root != null && !root.equals(snapshot.getString("root"))) throw new IllegalArgumentException("wrong native root");
        JSONArray nodes = snapshot.getJSONArray("nodes");
        JSONArray rendering = updates == null ? nodes : updates;
        for (int index = 0; index < rendering.length(); index++) {
            JSONObject node = rendering.getJSONObject(index);
            View existing = views.get(node.getString("id"));
            if (existing == null) continue;
            if (!descriptors.get(node.getString("id")).getString("kind").equals(node.getString("kind"))) {
                throw new IllegalArgumentException("node kind changed");
            }
            if ("button".equals(node.getString("kind"))
                    && (existing instanceof android.widget.RadioButton) != "radio".equals(node.optString("role"))) {
                throw new IllegalArgumentException("button/radio role changes require a new node identity");
            }
        }
        publishing = true;
        try {
            HashSet<String> live = new HashSet<>();
            for (int index = 0; index < rendering.length(); index++) {
                JSONObject node = rendering.getJSONObject(index);
                String id = node.getString("id");
                live.add(id);
                String kind = node.getString("kind");
                View view = views.get(id);
                if (view == null) {
                    view = create(node);
                    views.put(id, view);
                    listen(id, view, kind);
                    if (view instanceof NativeDialog) dialogs.put(id, (NativeDialog) view);
                    if (view instanceof NativePopover) popovers.put(id, (NativePopover) view);
                } else if (!descriptors.get(id).getString("kind").equals(kind)) {
                    throw new IllegalArgumentException("node kind changed");
                }
                JSONObject previous = descriptors.get(id);
                if (previous == null || !previous.toString().equals(node.toString())) style(view, node);
                JSONArray frame = node.getJSONArray("frame");
                double parentX = 0, parentY = 0;
                if (!node.isNull("parent")) {
                    JSONArray parentFrame = (frameDescriptors == null ? descriptors : frameDescriptors).get(node.getString("parent")).getJSONArray("frame");
                    parentX = parentFrame.getDouble(0); parentY = parentFrame.getDouble(1);
                }
                int width = px(frame.getDouble(2)), height = px(frame.getDouble(3));
                int left = px(frame.getDouble(0) - parentX), top = px(frame.getDouble(1) - parentY);
                if (view instanceof NativeDialog) {
                    NativeDialog dialog = (NativeDialog) view;
                    dialog.width = width; dialog.height = height;
                    dialog.content.setBackgroundColor(color(node.getJSONObject("style").getJSONArray("background")));
                    dialog.content.setAccessibilityPaneTitle(node.optString("name", "Dialog"));
                    BigInteger revision = new BigInteger(node.getString("editRevision"));
                    if (revision.compareTo(dialog.revision) > 0) dialog.revision = revision;
                    width = 0; height = 0; left = 0; top = 0;
                }
                if (view instanceof NativePopover) {
                    NativePopover popover = (NativePopover) view;
                    popover.width = width; popover.height = height;
                    popover.content.setBackgroundColor(color(node.getJSONObject("style").getJSONArray("background")));
                    popover.content.setAccessibilityPaneTitle(node.optString("name", "Popover"));
                    BigInteger revision = new BigInteger(node.getString("editRevision"));
                    if (revision.compareTo(popover.revision) > 0) popover.revision = revision;
                    width = 0; height = 0; left = 0; top = 0;
                }
                FrameLayout.LayoutParams current = view.getLayoutParams() instanceof FrameLayout.LayoutParams ? (FrameLayout.LayoutParams) view.getLayoutParams() : null;
                if (current == null || current.width != width || current.height != height || current.leftMargin != left || current.topMargin != top) {
                    FrameLayout.LayoutParams layout = new FrameLayout.LayoutParams(width, height);
                    layout.leftMargin = left; layout.topMargin = top;
                    view.setLayoutParams(layout);
                }
                descriptors.put(id, node);
            }
            reconcileHierarchy(nodes);
            if (topology) {
            for (int index=dialogOrder.size()-1;index>=0;index--) {
                String id = dialogOrder.get(index);
                if (!live.contains(id)) dialogs.get(id).hide();
            }
            for (String id : new HashSet<>(views.keySet())) if (!live.contains(id)) {
                View view = views.remove(id);
                NativePopover popover = popovers.remove(id);
                if (popover != null) popover.hide(false);
                popoverOrder.remove(id);
                NativeDialog dialog = dialogs.remove(id);
                if (dialog != null) dialog.hide();
                if (view.getParent() != null) ((ViewGroup) view.getParent()).removeView(view);
                descriptors.remove(id);
                toggles.remove(id);
            }
            }
            if (!dialogs.isEmpty() || !popovers.isEmpty()) {
            ArrayList<String> opened = new ArrayList<>();
            for (int index = 0; index < nodes.length(); index++) {
                JSONObject node = nodes.getJSONObject(index);
                if (!"dialog".equals(node.getString("kind")) || !node.getBoolean("open")) continue;
                boolean visible = true;
                JSONObject ancestor = node;
                while (!ancestor.isNull("parent")) {
                    ancestor = descriptors.get(ancestor.getString("parent"));
                    if (("dialog".equals(ancestor.getString("kind")) || "popover".equals(ancestor.getString("kind"))) && !ancestor.getBoolean("open")) { visible = false; break; }
                }
                if (visible) opened.add(node.getString("id"));
            }
            int prefix = 0;
            while (prefix < opened.size() && prefix < dialogOrder.size() && opened.get(prefix).equals(dialogOrder.get(prefix))) prefix++;
            for (int index=dialogOrder.size()-1;index>=prefix;index--) {
                NativeDialog dialog = dialogs.get(dialogOrder.get(index));
                if (dialog != null) dialog.hide();
            }
            for (String id : dialogs.keySet()) if (!opened.contains(id)) dialogs.get(id).hide();
            dialogOrder.clear();
            for (int index = 0; index < nodes.length(); index++) {
                String id = nodes.getJSONObject(index).getString("id");
                if (opened.contains(id)) { dialogOrder.add(id); dialogs.get(id).show(); }
            }
            syncPopovers(nodes);
            schedulePopovers();
            }
        } finally { publishing = false; }
    }

    private void reconcileHierarchy(JSONArray nodes) throws JSONException {
        HashMap<ViewGroup, Integer> indices = new HashMap<>();
        for (int index = 0; index < nodes.length(); index++) {
            JSONObject node = nodes.getJSONObject(index);
            View view = views.get(node.getString("id"));
            ViewGroup parent = node.isNull("parent") ? this : (ViewGroup) views.get(node.getString("parent"));
            if (parent instanceof NativeDialog) parent = ((NativeDialog) parent).content;
            if (parent instanceof NativePopover) parent = ((NativePopover) parent).content;
            int childIndex = indices.getOrDefault(parent, 0);
            indices.put(parent, childIndex + 1);
            if (view.getParent() != parent) {
                if (view.getParent() != null) ((ViewGroup) view.getParent()).removeView(view);
                parent.addView(view, childIndex);
            } else if (parent.getChildAt(childIndex) != view) {
                ArrayList<View> ordered = new ArrayList<>();
                for (int position = 0; position < parent.getChildCount(); position++) {
                    View child = parent.getChildAt(position);
                    if (child != view) ordered.add(child);
                }
                ordered.add(childIndex, view);
                for (int position = childIndex; position < ordered.size(); position++) parent.bringChildToFront(ordered.get(position));
            }
        }
    }

    public void clear(String ignored) {
        uiThread();
        publishing = true;
        try {
            batches.clear();
            for (int index=popoverOrder.size()-1;index>=0;index--) {
                NativePopover popover = popovers.get(popoverOrder.get(index));
                if (popover != null) popover.hide(false);
            }
            popovers.clear(); popoverOrder.clear();
            for (int index=dialogOrder.size()-1;index>=0;index--) dialogs.get(dialogOrder.get(index)).hide();
            dialogs.clear(); dialogOrder.clear(); removeAllViews(); views.clear(); descriptors.clear(); toggles.clear();
        }
        finally { publishing = false; }
    }

    @Override protected void onSizeChanged(int width, int height, int oldWidth, int oldHeight) {
        super.onSizeChanged(width, height, oldWidth, oldHeight);
        if (width == oldWidth && height == oldHeight) return;
        scheduleResize();
    }

    @Override protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        super.onLayout(changed, left, top, right, bottom);
        if (changed) schedulePopovers();
    }

    private void scheduleResize() {
        environment++;
        if (!resizing) {
            resizing = true;
            postOnAnimation(() -> {
                resizing = false;
                int[] location = new int[2];
                getLocationOnScreen(location);
                int height = Math.max(0, getHeight() - getPaddingTop() - getPaddingBottom());
                height = Math.min(height, Math.max(0, modalViewportBottom - location[1] - getPaddingTop()));
                if (root != null) nativeResize(root, Math.max(0, getWidth() - getPaddingLeft() - getPaddingRight()) / density,
                        height / density, environment);
            });
        }
    }

    @Override public void close() {
        uiThread();
        stateListener = null;
        if (root != null) { String previous = root; root = null; nativeDispose(previous); }
        clear("{}");
    }

    private final class NativeSelect extends Spinner {
        private String optionsSignature;
        private String publishedValue = "";
        private BigInteger revision = BigInteger.ZERO;
        private OptionAdapter choices;

        NativeSelect(Context context) {
            super(context, Spinner.MODE_DROPDOWN);
            setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
                @Override public void onItemSelected(AdapterView<?> parent, View selected, int position, long itemId) {
                    String id = (String) getTag();
                    if (publishing || root == null || id == null || views.get(id) != NativeSelect.this
                            || choices == null || position != getSelectedItemPosition()) return;
                    String value = choices.values.get(position);
                    if (publishedValue.equals(value)) return;
                    revision = revision.add(BigInteger.ONE);
                    try { emit(id, payload("select").put("value", value).put("revision", revision.toString())); }
                    catch (JSONException error) { throw new IllegalStateException(error); }
                }
                @Override public void onNothingSelected(AdapterView<?> parent) {}
            });
        }

        void configure(JSONObject node) throws JSONException {
            String signature = node.getJSONArray("options").toString() + node.getString("placeholder");
            if (!signature.equals(optionsSignature)) {
                choices = new OptionAdapter(node);
                optionsSignature = signature;
                setAdapter(choices);
            }
            choices.typography = node.getJSONObject("style");
            publishedValue = node.getString("value");
            int selected = choices.values.indexOf(publishedValue);
            if (selected < 0) throw new IllegalArgumentException("unknown native selection");
            setEnabled(node.getBoolean("enabled"));
            setPrompt(node.getString("placeholder"));
            setContentDescription(node.isNull("name") ? node.getString("placeholder") : node.getString("name"));
            BigInteger incoming = new BigInteger(node.getString("editRevision"));
            if (incoming.compareTo(revision) > 0) revision = incoming;
            if (getSelectedItemPosition() != selected) setSelection(selected, false);
            if (getSelectedItemPosition() != selected) throw new IllegalArgumentException("native selection was not retained");
            for (int index = 0; index < getChildCount(); index++) {
                if (getChildAt(index) instanceof TextView) choices.styled(getChildAt(index), selected);
            }
        }

        private final class OptionAdapter extends ArrayAdapter<String> {
            final ArrayList<String> values = new ArrayList<>();
            final ArrayList<Boolean> enabled = new ArrayList<>();
            JSONObject typography;

            OptionAdapter(JSONObject node) throws JSONException {
                super(NativeSelect.this.getContext(), android.R.layout.simple_spinner_item);
                typography = node.getJSONObject("style");
                setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
                values.add(""); enabled.add(false); add(node.getString("placeholder"));
                JSONArray options = node.getJSONArray("options");
                for (int index = 0; index < options.length(); index++) {
                    JSONObject option = options.getJSONObject(index);
                    values.add(option.getString("value")); enabled.add(option.getBoolean("enabled"));
                    add(option.getString("label"));
                }
            }
            @Override public boolean areAllItemsEnabled() { return false; }
            @Override public boolean isEnabled(int position) { return enabled.get(position); }
            private View styled(View view, int position) {
                try {
                    TextView text = (TextView) view;
                    textStyle(text, typography);
                    int foreground = color(typography.getJSONArray("foreground"));
                    TypedValue disabledAlpha = new TypedValue();
                    float alpha = getContext().getTheme().resolveAttribute(android.R.attr.disabledAlpha, disabledAlpha, true)
                            ? disabledAlpha.getFloat() : 0.38f;
                    int disabledColor = Color.argb(Math.round(Color.alpha(foreground) * alpha),
                            Color.red(foreground), Color.green(foreground), Color.blue(foreground));
                    text.setTextColor(new ColorStateList(new int[][] { new int[] { android.R.attr.state_enabled }, new int[0] },
                            new int[] { foreground, disabledColor }));
                }
                catch (JSONException error) { throw new IllegalStateException(error); }
                view.setEnabled(NativeSelect.this.isEnabled() && enabled.get(position));
                return view;
            }
            @Override public View getView(int position, View recycled, ViewGroup parent) {
                return styled(super.getView(position, recycled, parent), position);
            }
            @Override public View getDropDownView(int position, View recycled, ViewGroup parent) {
                return styled(super.getDropDownView(position, recycled, parent), position);
            }
        }
    }

    private void schedulePopovers() {
        if (popoverUpdatePending || popovers.isEmpty()) return;
        popoverUpdatePending = true;
        String expectedRoot = root;
        postOnAnimation(() -> {
            popoverUpdatePending = false;
            if (root == null || !root.equals(expectedRoot)) return;
            for (String id : new ArrayList<>(popoverOrder)) {
                NativePopover popover = popovers.get(id);
                if (popover != null && popupVisible(id)) popover.show();
            }
        });
    }

    private boolean popupVisible(String id) {
        return eventVisible(id);
    }

    private void syncPopovers(JSONArray nodes) throws JSONException {
        ArrayList<String> opened = new ArrayList<>();
        for (int index=0;index<nodes.length();index++) {
            JSONObject node = nodes.getJSONObject(index);
            String id = node.getString("id");
            if ("popover".equals(node.getString("kind")) && node.getBoolean("open") && popupVisible(id)) opened.add(id);
        }
        for (int index=popoverOrder.size()-1;index>=0;index--) {
            String id = popoverOrder.get(index);
            if (!opened.contains(id)) {
                NativePopover popover = popovers.get(id);
                if (popover != null) popover.hide(true);
                popoverOrder.remove(index);
            }
        }
        for (String id : opened) if (!popoverOrder.contains(id)) popoverOrder.add(id);
        for (String id : new ArrayList<>(popoverOrder)) popovers.get(id).show();
    }

    private final class NativePopover extends FrameLayout {
        final FrameLayout content;
        final PopupWindow window;
        BigInteger revision = BigInteger.ZERO;
        int width, height;
        boolean suppressDismiss;
        boolean dismissPending;
        boolean restoreAfterDismiss;
        View anchor;

        NativePopover(Context context) {
            super(context);
            setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
            content = new FrameLayout(context) {
                @Override public boolean dispatchKeyEvent(KeyEvent event) {
                    if (event.getKeyCode() == KeyEvent.KEYCODE_BACK || event.getKeyCode() == KeyEvent.KEYCODE_ESCAPE) {
                        if (event.getAction() == KeyEvent.ACTION_UP && !event.isCanceled()) requestDismiss(false);
                        return true;
                    }
                    return super.dispatchKeyEvent(event);
                }
            };
            content.setClipChildren(true);
            content.setFocusableInTouchMode(true);
            window = new PopupWindow(content, 0, 0, true);
            window.setBackgroundDrawable(new ColorDrawable(Color.TRANSPARENT));
            window.setElevation(px(8));
            window.setAnimationStyle(0);
            window.setOutsideTouchable(true);
            window.setClippingEnabled(true);
            window.setInputMethodMode(PopupWindow.INPUT_METHOD_NEEDED);
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
            if (Build.VERSION.SDK_INT >= 29) {
                window.setIsLaidOutInScreen(true);
                window.setTouchModal(false);
            }
            window.setTouchInterceptor((view, event) -> {
                boolean outside = event.getAction() == MotionEvent.ACTION_OUTSIDE
                        || event.getAction() == MotionEvent.ACTION_DOWN && (event.getX() < 0 || event.getY() < 0
                        || event.getX() >= content.getWidth() || event.getY() >= content.getHeight());
                if (!outside) return false;
                JSONObject node = descriptors.get(getTag());
                if (node != null && node.optBoolean("closeOnOutsideClick")) requestDismiss(true);
                return true;
            });
            window.setOnDismissListener(() -> {
                if (!suppressDismiss) requestDismiss(false);
            });
            content.getViewTreeObserver().addOnScrollChangedListener(NativeView.this::schedulePopovers);
        }

        void requestDismiss(boolean backdrop) {
            String id = (String) getTag();
            if (publishing || root == null || views.get(id) != this || dismissPending || !popupVisible(id)) return;
            restoreAfterDismiss = !backdrop;
            dismissPending = true;
            String expectedRoot = root;
            post(() -> {
                dismissPending = false;
                if (!expectedRoot.equals(root) || views.get(id) != this || !popupVisible(id)) return;
                revision = revision.add(BigInteger.ONE);
                try {
                    emit(id, payload("dismiss").put("backdrop", backdrop).put("revision", revision.toString()));
                    JSONObject node = descriptors.get(id);
                    if (node != null && node.optBoolean("open") && !window.isShowing()) show();
                } catch (JSONException error) { throw new IllegalStateException(error); }
            });
        }

        int[] geometry() {
            JSONObject node = descriptors.get(getTag());
            if (node == null) return null;
            anchor = views.get(node.optString("anchor"));
            if (anchor == null || !anchor.isAttachedToWindow() || !NativeView.this.isAttachedToWindow()) return null;
            android.graphics.Rect viewport = new android.graphics.Rect();
            NativeView.this.getWindowVisibleDisplayFrame(viewport);
            int[] origin = new int[2];
            NativeView.this.getLocationOnScreen(origin);
            if (!viewport.intersect(origin[0] + NativeView.this.getPaddingLeft(), origin[1] + NativeView.this.getPaddingTop(),
                    origin[0] + NativeView.this.getWidth() - NativeView.this.getPaddingRight(), origin[1] + NativeView.this.getHeight() - NativeView.this.getPaddingBottom())) return null;
            int inset = px(8), gap = px(node.optDouble("gap"));
            int panelWidth = Math.min(width, Math.max(0, viewport.width() - inset*2));
            int panelHeight = Math.min(height, Math.max(0, viewport.height() - inset*2));
            int[] location = new int[2];
            anchor.getLocationOnScreen(location);
            String placement = node.optString("placement");
            String[] parts = placement.split("-");
            String side = parts[0], align = parts.length > 1 ? parts[1] : "center";
            boolean vertical = "top".equals(side) || "bottom".equals(side);
            boolean before = "top".equals(side) || "left".equals(side);
            int start = vertical ? location[1] : location[0];
            int extent = vertical ? anchor.getHeight() : anchor.getWidth();
            int lower = vertical ? viewport.top : viewport.left;
            int upper = vertical ? viewport.bottom : viewport.right;
            int panelExtent = vertical ? panelHeight : panelWidth;
            int roomBefore = start - lower - inset - gap;
            int roomAfter = upper - start - extent - inset - gap;
            if ((before ? roomBefore : roomAfter) < panelExtent && (before ? roomAfter > roomBefore : roomBefore > roomAfter)) before = !before;
            int primary = before ? start - gap - panelExtent : start + extent + gap;
            int crossStart = vertical ? location[0] : location[1];
            int crossExtent = vertical ? anchor.getWidth() : anchor.getHeight();
            int crossSize = vertical ? panelWidth : panelHeight;
            int cross = "start".equals(align) ? crossStart : "end".equals(align) ? crossStart + crossExtent - crossSize : crossStart + (crossExtent-crossSize)/2;
            int left = vertical ? cross : primary, top = vertical ? primary : cross;
            left = Math.max(viewport.left + inset, Math.min(left, viewport.right - inset - panelWidth));
            top = Math.max(viewport.top + inset, Math.min(top, viewport.bottom - inset - panelHeight));
            if (Build.VERSION.SDK_INT < 29) {
                int[] inWindow = new int[2];
                NativeView.this.getLocationInWindow(inWindow);
                left -= origin[0] - inWindow[0];
                top -= origin[1] - inWindow[1];
            }
            return new int[]{left, top, panelWidth, panelHeight};
        }

        void position() {
            int[] bounds = geometry();
            if (bounds == null || !window.isShowing()) return;
            window.update(bounds[0], bounds[1], bounds[2], bounds[3]);
        }

        void show() {
            int[] bounds = geometry();
            if (bounds == null || bounds[2] == 0 || bounds[3] == 0) return;
            if (window.isShowing()) { position(); return; }
            window.setWidth(bounds[2]); window.setHeight(bounds[3]);
            window.showAtLocation(NativeView.this, Gravity.TOP | Gravity.LEFT, bounds[0], bounds[1]);
            ArrayList<View> focusable = new ArrayList<>();
            content.addFocusables(focusable, View.FOCUS_FORWARD, View.FOCUSABLES_ALL);
            for (View view : focusable) if (view != content && view.isEnabled() && view.isShown() && view.requestFocus()) return;
            content.requestFocus();
        }

        void hide(boolean restore) {
            boolean hadFocus = content.hasFocus() || restoreAfterDismiss;
            restoreAfterDismiss = false;
            suppressDismiss = true;
            try { if (window.isShowing()) window.dismiss(); }
            finally { suppressDismiss = false; }
            if (restore && hadFocus && anchor != null && anchor.isAttachedToWindow() && anchor.isShown()) {
                anchor.setFocusableInTouchMode(true);
                anchor.requestFocus();
            }
        }
    }

    private final class NativeDialog extends FrameLayout {
        final FrameLayout content;
        final Dialog window;
        BigInteger revision = BigInteger.ZERO;
        int width, height;
        View previousFocus;

        NativeDialog(Context context) {
            super(context);
            setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
            content = new FrameLayout(context);
            content.setClipChildren(true);
            window = new Dialog(context) {
                @Override public void onBackPressed() { requestDismiss(false); }
                @Override public boolean onTouchEvent(MotionEvent event) {
                    View decor = getWindow().getDecorView();
                    if (event.getAction() == MotionEvent.ACTION_DOWN && (event.getX() < 0 || event.getY() < 0 || event.getX() >= decor.getWidth() || event.getY() >= decor.getHeight())) {
                        JSONObject node = descriptors.get(getTag());
                        if (node != null && node.optBoolean("closeOnOverlayClick")) requestDismiss(true);
                        return true;
                    }
                    return super.onTouchEvent(event);
                }
            };
            window.requestWindowFeature(Window.FEATURE_NO_TITLE);
            window.setContentView(content);
            window.setCanceledOnTouchOutside(false);
            window.setOnCancelListener(ignored -> requestDismiss(false));
            window.getWindow().setBackgroundDrawable(new ColorDrawable(Color.TRANSPARENT));
            window.getWindow().addFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND);
            window.getWindow().setDimAmount(0.32f);
            window.getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
            if (Build.VERSION.SDK_INT >= 30) content.setOnApplyWindowInsetsListener((view, insets) -> {
                if (window.isShowing() && activeDialog() == window) {
                    android.graphics.Rect visible = new android.graphics.Rect();
                    content.getWindowVisibleDisplayFrame(visible);
                    int bottom = insets.isVisible(WindowInsets.Type.ime()) ? visible.bottom : Integer.MAX_VALUE;
                    if (modalViewportBottom != bottom) { modalViewportBottom = bottom; scheduleResize(); }
                }
                return insets;
            });
        }

        void requestDismiss(boolean backdrop) {
            if (publishing || root == null || views.get(getTag()) != this) return;
            revision = revision.add(BigInteger.ONE);
            try { emit((String) getTag(), payload("dismiss").put("backdrop", backdrop).put("revision", revision.toString())); }
            catch (JSONException error) { throw new IllegalStateException(error); }
        }

        void show() {
            if (!window.isShowing()) {
                previousFocus = NativeView.this.findFocus();
                for (int index=dialogOrder.size()-1;index>=0;index--) {
                    NativeDialog previous = dialogs.get(dialogOrder.get(index));
                    if (previous != this && previous.window.isShowing()) { previousFocus = previous.window.getCurrentFocus(); break; }
                }
                window.show();
            }
            window.getWindow().setGravity(Gravity.CENTER);
            window.getWindow().setLayout(width, height);
        }

        void hide() {
            if (!window.isShowing()) return;
            window.dismiss();
            if (modalViewportBottom != Integer.MAX_VALUE) { modalViewportBottom = Integer.MAX_VALUE; scheduleResize(); }
            if (previousFocus != null && previousFocus.isAttachedToWindow()) previousFocus.requestFocus();
            previousFocus = null;
        }
    }

    private final class NativeInput extends EditText {
        BigInteger revision = BigInteger.ZERO;
        int imeDepth;
        boolean editPending;
        NativeInput(Context context) {
            super(context);
            setSingleLine(true);
            setImeOptions(EditorInfo.IME_ACTION_DONE);
            setFilters(new android.text.InputFilter[0]);
            setOnEditorActionListener((view, action, event) -> {
                JSONObject descriptor = descriptors.get(getTag());
                if (descriptor == null || !"input".equals(descriptor.optString("kind"))) return false;
                if (event != null && (event.getKeyCode() != KeyEvent.KEYCODE_ENTER || event.getAction() != KeyEvent.ACTION_DOWN
                        || event.getRepeatCount() != 0 || event.isCtrlPressed() || event.isAltPressed() || event.isMetaPressed())) return true;
                if (event == null && action != EditorInfo.IME_ACTION_DONE && action != EditorInfo.IME_ACTION_GO
                        && action != EditorInfo.IME_ACTION_SEARCH && action != EditorInfo.IME_ACTION_SEND) return false;
                Editable text = getText();
                if (publishing || !isEnabled() || imeDepth != 0 || root == null
                        || BaseInputConnection.getComposingSpanStart(text) >= 0 || BaseInputConnection.getComposingSpanEnd(text) >= 0) return true;
                String expectedRoot = root;
                // TextWatcher posts its final edit first, so Submit observes the committed value.
                post(() -> {
                    if (!expectedRoot.equals(root) || views.get(getTag()) != this || !isEnabled() || imeDepth != 0
                            || BaseInputConnection.getComposingSpanStart(getText()) >= 0
                            || BaseInputConnection.getComposingSpanEnd(getText()) >= 0 || revision.bitLength() > 64) return;
                    BigInteger next = revision.add(BigInteger.ONE);
                    if (next.bitLength() > 64) return;
                    revision = next;
                    try { emit((String) getTag(), payload("submit").put("revision", revision.toString())); }
                    catch (JSONException error) { throw new IllegalStateException(error); }
                });
                return true;
            });
            addTextChangedListener(new TextWatcher() {
                public void beforeTextChanged(CharSequence text, int start, int count, int after) {}
                public void onTextChanged(CharSequence text, int start, int before, int count) {}
                public void afterTextChanged(Editable text) { if (!publishing && imeDepth == 0) scheduleEdit(); }
            });
        }
        void scheduleEdit() {
            if (editPending || getTag() == null || root == null) return;
            editPending = true;
            String expectedRoot = root;
            post(() -> {
                editPending = false;
                if (expectedRoot.equals(root) && views.get(getTag()) == this) edited();
            });
        }
        @Override protected void onSelectionChanged(int start, int end) {
            super.onSelectionChanged(start, end);
            if (!publishing && imeDepth == 0) scheduleEdit();
        }
        void edited() {
            if (publishing || getTag() == null || root == null) return;
            revision = revision.add(BigInteger.ONE);
            Editable text = getText();
            int start = BaseInputConnection.getComposingSpanStart(text), end = BaseInputConnection.getComposingSpanEnd(text);
            int selectionStart = Math.max(0, getSelectionStart()), selectionEnd = Math.max(0, getSelectionEnd());
            try {
                emit((String) getTag(), payload("edit").put("value", text.toString()).put("revision", revision.toString())
                    .put("selection", new JSONArray().put(Math.min(selectionStart, selectionEnd)).put(Math.max(selectionStart, selectionEnd)))
                    .put("composing", start < 0 || end < 0 ? JSONObject.NULL : new JSONArray().put(Math.min(start, end)).put(Math.max(start, end))));
            } catch (JSONException error) { throw new IllegalStateException(error); }
        }
        @Override public InputConnection onCreateInputConnection(EditorInfo info) {
            InputConnection connection = super.onCreateInputConnection(info);
            if (connection == null) return null;
            return new InputConnectionWrapper(connection, false) {
                @Override public boolean setComposingText(CharSequence text, int cursor) {
                    imeDepth++; boolean result;
                    try { result = super.setComposingText(text, cursor); } finally { imeDepth--; }
                    edited(); return result;
                }
                @Override public boolean setComposingRegion(int start, int end) {
                    imeDepth++; boolean result;
                    try { result = super.setComposingRegion(start, end); } finally { imeDepth--; }
                    edited(); return result;
                }
                @Override public boolean commitText(CharSequence text, int cursor) {
                    imeDepth++; boolean result;
                    try { result = super.commitText(text, cursor); } finally { imeDepth--; }
                    edited(); return result;
                }
                @Override public boolean finishComposingText() {
                    imeDepth++; boolean result;
                    try { result = super.finishComposingText(); } finally { imeDepth--; }
                    edited(); return result;
                }
            };
        }
    }
}
