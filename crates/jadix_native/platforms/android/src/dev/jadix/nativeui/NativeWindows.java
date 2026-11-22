package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Dialog;
import android.content.pm.ActivityInfo;
import android.graphics.Rect;
import android.view.KeyEvent;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.WeakHashMap;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Independent native roots implementing the shared window declaration protocol. */
final class NativeWindows implements AutoCloseable {
    private final NativeView owner;
    private final LinkedHashMap<String, Page> pages = new LinkedHashMap<>();
    private boolean scheduled;
    private boolean closing;
    private final Runnable pump = this::pump;
    private static final WeakHashMap<Activity, Orientation> orientations = new WeakHashMap<>();
    private static final class Orientation {
        final int original;
        final ArrayList<Page> foreground = new ArrayList<>();
        Orientation(Activity activity) { original = activity.getRequestedOrientation(); }
    }
    NativeWindows(NativeView owner) { this.owner = owner; }
    void start() { closing = false; }
    void schedule() { if (!closing && !scheduled) { scheduled = true; owner.post(pump); } }
    private void pump() {
        scheduled = false;
        String session = owner.webResourceSession();
        if (closing || session == null) return;
        try {
            JSONArray work = new JSONArray(NativeRuntime.windowWork(session));
            for (int i = 0; i < work.length(); i++) {
                JSONObject request = work.getJSONObject(i);
                JSONObject reply = new JSONObject().put("request", request.getString("request"));
                try { command(request.getJSONObject("command")); }
                catch (RuntimeException | JSONException error) { reply.put("error", error.getMessage() == null ? "Android page request failed" : error.getMessage()); }
                NativeRuntime.windowEvent(session, reply.toString());
            }
            owner.environmentChanged();
        } catch (JSONException error) { throw new IllegalStateException("Invalid window transport", error); }
    }
    private void command(JSONObject command) throws JSONException {
        String id = command.getString("id"), operation = command.getString("operation");
        if (id.isEmpty() || id.length() > 1024) throw new IllegalArgumentException("Invalid page ID");
        if (operation.equals("open") || operation.equals("open_declared")) {
            if (pages.containsKey(id)) throw new IllegalArgumentException("Page already exists");
            if (pages.size() >= 32) throw new IllegalStateException("Page limit exceeded");
            Activity activity = NativePermissions.activity(owner.getContext());
            if (activity == null || activity.isFinishing() || activity.isDestroyed()) throw new IllegalStateException("Page requires a live Activity");
            JSONObject content = command.optJSONObject("content");
            if (content == null) throw new IllegalArgumentException("Page requires an application content factory");
            String parent = command.optString("parent", "");
            if (!parent.isEmpty() && !pages.containsKey(parent)) throw new IllegalArgumentException("Unknown parent page");
            JSONObject attributes = command.optJSONObject("attributes");
            if (attributes == null) attributes = new JSONObject();
            for (java.util.Iterator<String> keys = attributes.keys(); keys.hasNext();) { String key = keys.next(); if (attributes.isNull(key)) keys.remove(); }
            validate(attributes);
            String initial = command.optString("initial_mode", "normal");
            if (!initial.equals("normal") && !initial.equals("full_screen") && !initial.equals("maximized")) throw new IllegalArgumentException("Unsupported Android page mode");
            if (attributes.isNull("mode")) attributes.put("mode", initial);
            if (attributes.isNull("close_on_blur") && !command.isNull("close_on_blur")) attributes.put("close_on_blur", command.getBoolean("close_on_blur"));
            Page page = new Page(id, parent, activity, command, attributes);
            pages.put(id, page);
            try { page.create(content); if (attributes.isNull("visible") ? command.optBoolean("initially_visible", true) : attributes.getBoolean("visible")) page.show(); }
            catch (RuntimeException | JSONException error) { pages.remove(id); page.dispose(false); throw error; }
            return;
        }
        Page page = pages.get(id);
        if (page == null) throw new IllegalArgumentException("Unknown page");
        switch (operation) {
            case "update": case "update_declared":
                JSONObject changes = command.getJSONObject("attributes"); validate(changes);
                for (java.util.Iterator<String> keys = changes.keys(); keys.hasNext();) { String key = keys.next(); if (!changes.isNull(key)) page.attributes.put(key, changes.get(key)); }
                if (!changes.isNull("visible")) { if (changes.getBoolean("visible")) page.show(); else page.hide(); }
                page.apply(); break;
            case "close": case "close_declared": destroy(page, true); break;
            case "set_visibility": page.attributes.put("visible", command.getBoolean("visible")); if (command.getBoolean("visible")) page.show(); else page.hide(); break;
            case "focus": case "restore": page.attributes.put("visible", true); page.show(); break;
            case "set_mode":
                String mode = command.getString("mode");
                if (!mode.equals("normal") && !mode.equals("full_screen") && !mode.equals("maximized")) throw new IllegalArgumentException("Unsupported Android page mode");
                page.attributes.put("mode", mode); page.apply(); break;
            default: throw new IllegalArgumentException("Unknown Android page request");
        }
    }
    private static void validate(JSONObject attributes) throws JSONException {
        String[] desktop = {"position", "min_content_size", "max_content_size", "workspaces", "level", "participates_in_cycle"};
        for (String key : desktop) if (!attributes.isNull(key)) throw new IllegalArgumentException(key + " is unavailable for Android pages");
        String mode = attributes.isNull("mode") ? "normal" : attributes.getString("mode");
        if (!mode.equals("normal") && !mode.equals("full_screen") && !mode.equals("maximized")) throw new IllegalArgumentException("Unsupported Android page mode");
        String direction = attributes.isNull("screen_orientations") ? "landscape_and_portrait" : attributes.getString("screen_orientations");
        if (!direction.equals("landscape_and_portrait") && !direction.equals("portrait") && !direction.equals("landscape")) throw new IllegalArgumentException("Unknown screen orientation policy");
    }
    private void destroy(Page page, boolean notify) {
        for (Page child : new ArrayList<>(pages.values())) if (child.parent.equals(page.id)) destroy(child, notify);
        if (pages.remove(page.id) != null) page.dispose(notify);
    }
    private void event(JSONObject event) {
        String session = owner.webResourceSession();
        if (!closing && session != null) NativeRuntime.windowEvent(session, event.toString());
    }
    /** Called from Rust disposal as well: release child Rust owners after that borrow ends. */
    void clear() {
        owner.removeCallbacks(pump); scheduled = false;
        ArrayList<Page> children = new ArrayList<>(pages.values()); pages.clear();
        for (Page page : children) { page.detach(); owner.post(() -> page.dispose(false)); }
    }
    @Override public void close() {
        closing = true; owner.removeCallbacks(pump); scheduled = false;
        for (Page page : new ArrayList<>(pages.values())) page.dispose(false);
        pages.clear();
    }
    private final class Page {
        final String id, parent, kind;
        final Activity activity;
        final JSONObject attributes;
        final int width, height;
        Dialog dialog;
        NativeView root;
        boolean disposed;
        boolean fullPage;
        Page(String id, String parent, Activity activity, JSONObject command, JSONObject attributes) {
            this.id = id; this.parent = parent; this.activity = activity; this.attributes = attributes;
            kind = command.optString("kind", "normal");
            try { attributes.put("modal", command.optBoolean("modal", false)); } catch (JSONException error) { throw new IllegalArgumentException(error); }
            fullPage = kind.equals("normal");
            width = command.optInt("width", 640); height = command.optInt("height", 480);
            if (width <= 0 || height <= 0) throw new IllegalArgumentException("Page dimensions must be positive");
            if (!attributes.has("title")) try { attributes.put("title", command.optString("title", "")); } catch (JSONException error) { throw new IllegalArgumentException(error); }
        }
        void create(JSONObject content) throws JSONException {
            int theme = attributes.optString("appearance", "inherit").equals("dark") ? android.R.style.Theme_Material_NoActionBar : android.R.style.Theme_Material_Light_NoActionBar;
            if (attributes.optString("appearance", "inherit").equals("inherit")) theme = android.R.style.Theme_DeviceDefault_NoActionBar;
            dialog = new Dialog(activity, theme) {
                @Override public boolean dispatchKeyEvent(KeyEvent event) {
                    if (super.dispatchKeyEvent(event)) return true;
                    int key = event.getKeyCode();
                    if (key != KeyEvent.KEYCODE_BACK && key != KeyEvent.KEYCODE_ESCAPE) return false;
                    boolean closes = attributes.optBoolean(key == KeyEvent.KEYCODE_BACK ? "close_on_press" : "close_on_escape", true);
                    if (!closes) return false;
                    if (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0 && !event.isCanceled()) userClose();
                    return true;
                }
            };
            dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
            dialog.setCancelable(false);
            dialog.setOnDismissListener(d -> { if (!disposed) destroy(this, true); });
            dialog.setOnCancelListener(d -> userClose());
            dialog.setOnShowListener(d -> { orientation(true, true); observe(); });
            root = new NativeView(dialog.getContext());
            dialog.setContentView(root);
            float density = root.getResources().getDisplayMetrics().density;
            float viewportWidth = Math.min(width, root.getResources().getDisplayMetrics().widthPixels / density);
            float viewportHeight = Math.min(height, root.getResources().getDisplayMetrics().heightPixels / density);
            root.mountWindow(owner.webResourceSession(), content, viewportWidth, viewportHeight, owner);
            root.addOnLayoutChangeListener((v,l,t,r,b,ol,ot,or,ob) -> observe());
            root.getViewTreeObserver().addOnWindowFocusChangeListener(focused -> {
                if (!disposed && dialog.isShowing()) {
                    if (focused) orientation(true, true);
                    else if (attributes.optBoolean("close_on_blur", false)) { userClose(); return; }
                    observe();
                }
            });
        }
        void show() {
            if (disposed) throw new IllegalStateException("Page was disposed");
            dialog.show(); apply(); orientation(true, true); dialog.getWindow().getDecorView().requestFocus(); observe();
        }
        void hide() { if (dialog != null) dialog.hide(); orientation(false, false); observe(); }
        void apply() {
            if (dialog == null) return;
            dialog.setTitle(attributes.optString("title", ""));
            Window window = dialog.getWindow();
            float density = root.getResources().getDisplayMetrics().density;
            JSONObject size = attributes.optJSONObject("content_size");
            String mode = attributes.optString("mode", "normal");
            boolean fill = fullPage || mode.equals("full_screen") || mode.equals("maximized");
            window.setLayout(fill ? WindowManager.LayoutParams.MATCH_PARENT : (int)Math.min((size == null ? width : size.optInt("width", width)) * (double)density, root.getResources().getDisplayMetrics().widthPixels),
                fill ? WindowManager.LayoutParams.MATCH_PARENT : (int)Math.min((size == null ? height : size.optInt("height", height)) * (double)density, root.getResources().getDisplayMetrics().heightPixels));
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
            if (mode.equals("full_screen")) window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN); else window.clearFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN);
            dialog.setCanceledOnTouchOutside(false);
            window.addFlags(WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH);
            window.getDecorView().setOnTouchListener((v, event) -> {
                if (event.getActionMasked() == android.view.MotionEvent.ACTION_OUTSIDE && attributes.optBoolean("close_on_blur", false)) { userClose(); return true; }
                return false;
            });
            if (!attributes.optBoolean("modal", false)) window.addFlags(WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL);
            else window.clearFlags(WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL);
            window.setBackgroundDrawableResource(android.R.color.transparent);
            root.setNativeAppearance(attributes.optString("appearance", "inherit"));
            if (!attributes.isNull("visible") && !attributes.optBoolean("visible")) hide();
            else if (dialog.isShowing()) orientation(true, false);
            observe();
        }
        void orientation(boolean visible, boolean activate) {
            Orientation state = orientations.get(activity);
            if (visible && state == null) { state = new Orientation(activity); orientations.put(activity, state); }
            if (state == null) return;
            if (!visible || activate) state.foreground.remove(this);
            if (visible && !state.foreground.contains(this)) state.foreground.add(this);
            int desired = state.original;
            if (!state.foreground.isEmpty()) {
                Page top = state.foreground.get(state.foreground.size()-1);
                String policy = top.attributes.optString("screen_orientations", "landscape_and_portrait");
                desired = policy.equals("portrait") ? ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT : policy.equals("landscape") ? ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE : ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR;
            }
            if (!activity.isDestroyed() && activity.getRequestedOrientation() != desired) activity.setRequestedOrientation(desired);
            if (state.foreground.isEmpty()) orientations.remove(activity);
        }
        void userClose() {
            if (disposed) return;
            if (!attributes.optBoolean("dismiss_on_close", true)) hide(); else destroy(this, true);
        }
        void observe() {
            if (disposed || dialog == null || root == null) return;
            try {
                float density = root.getResources().getDisplayMetrics().density;
                int[] origin = new int[2]; root.getLocationOnScreen(origin);
                Rect display = new Rect(); root.getWindowVisibleDisplayFrame(display);
                JSONObject frame = NativeServices.object("x", origin[0]/density, "y", origin[1]/density, "width", root.getWidth()/density, "height", root.getHeight()/density);
                JSONObject screen = NativeServices.object("x", 0, "y", 0, "width", root.getResources().getDisplayMetrics().widthPixels/density, "height", root.getResources().getDisplayMetrics().heightPixels/density);
                JSONObject state = NativeServices.object("id", id, "frame", frame, "normal_frame", JSONObject.NULL, "content_width", root.getWidth()/density, "content_height", root.getHeight()/density,
                    "backing_scale", density, "display", NativeServices.object("id", root.getDisplay() == null ? JSONObject.NULL : root.getDisplay().getDisplayId(), "frame", screen, "work_area", NativeServices.object("x", display.left/density,"y",display.top/density,"width",display.width()/density,"height",display.height()/density)),
                    "mode", attributes.optString("mode", "normal"), "key", dialog.isShowing() && root.hasWindowFocus(), "visible", dialog.isShowing(), "occluded", !dialog.isShowing());
                event(NativeServices.object("id", id, "state", state));
            } catch (RuntimeException error) { /* A detached Activity has no observable window. */ }
        }
        void detach() { orientation(false, false); if (dialog != null) { dialog.setOnDismissListener(null); dialog.setOnCancelListener(null); dialog.setOnShowListener(null); dialog.dismiss(); } }
        void dispose(boolean notify) {
            if (disposed) return;
            disposed = true; detach();
            if (root != null) root.close(); root = null; dialog = null;
            if (notify) event(NativeServices.object("destroyed", id));
        }
    }
}
