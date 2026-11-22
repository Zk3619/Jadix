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
import android.os.TransactionTooLargeException;
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

public final class NativeView extends NativeFrame implements AutoCloseable {
    NativeResources resources = new NativeResources(this);
    private boolean resourcesClosed;
    private NativeServices services;
    private Runnable serviceListener;
    private NativeServices services() {
        if (services == null) services = new NativeServices(this);
        return services;
    }
    public void serviceCommand(String input) throws JSONException { uiThread(); services().command(new JSONObject(input)); }
    void deliverServiceEvents(String input) {
        uiThread();
        if (webSession == null) return;
        NativeRuntime.serviceEvents(webSession, input);
        if (serviceListener != null) serviceListener.run();
        scheduleRootUpdate();
    }
    public void setServiceListener(Runnable listener) { uiThread(); serviceListener = listener; }
    private final java.util.ArrayDeque<JSONObject> deepLinks = new java.util.ArrayDeque<>();
    public boolean handleIntent(android.content.Intent intent) { return handleIntent(intent, false); }
    public boolean handleIntent(android.content.Intent intent, boolean coldStart) {
        uiThread();
        if (intent == null || webSession == null) return false;
        if (services().system.intent(intent)) return true;
        android.net.Uri data = intent.getData();
        if (!android.content.Intent.ACTION_VIEW.equals(intent.getAction()) || data == null || data.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 32768) return false;
        if (deepLinks.size() == 64) deepLinks.remove();
        deepLinks.add(NativeServices.object("url", data.toString(), "source", coldStart ? "cold_start" : "hot_start"));
        if (serviceListener != null) serviceListener.run();
        scheduleRootUpdate();
        return true;
    }
    public String deepLinkExchange(String ignored) { uiThread(); JSONArray links = new JSONArray(); while (!deepLinks.isEmpty()) links.put(deepLinks.remove()); return links.toString(); }
    public String openUrl(String input) throws JSONException {
        uiThread();
        return Boolean.toString(services().system.open(new JSONObject(input).getString("url")));
    }
    private volatile java.util.function.Predicate<android.net.Uri> resourcePolicy = uri -> "https".equals(uri.getScheme()) && uri.getHost() != null && uri.getUserInfo() == null;
    private JSONArray orderedNodes = new JSONArray();
    private final HashMap<String, Integer> nodePositions = new HashMap<>();
    private final java.util.HashSet<String> contentParents = new java.util.HashSet<>();
    private final NativeInteraction interactions = new NativeInteraction(this);
    private final NativeKeyboard keyboard = new NativeKeyboard(this);
    private final HashMap<String, NativeHosted.Factory> hostedFactories = new HashMap<>();
    private final NativeBatch batches = new NativeBatch();
    private final HashMap<String, View> views = new HashMap<>();
    // Stateless viewport leaves only. Editors, scrollers and hosted content keep
    // their own identities and never enter this pool.
    private final HashMap<String, java.util.ArrayDeque<View>> recycled = new HashMap<>();
    private int recycledCount;
    private String recycleKind(JSONObject node) {
        if (node == null) return null;
        String kind = node.optString("kind");
        return "box".equals(kind) || "text".equals(kind) || "image".equals(kind) ? kind : null;
    }
    private View acquire(JSONObject node) throws JSONException {
        java.util.ArrayDeque<View> pool = recycled.get(recycleKind(node));
        if (pool == null || pool.isEmpty()) return create(node);
        recycledCount--;
        return pool.removeFirst();
    }
    private void retire(View view, JSONObject node) {
        disposeView(view);
        if (view.getParent() instanceof ViewGroup) ((ViewGroup)view.getParent()).removeView(view);
        if (view instanceof ViewGroup) ((ViewGroup)view).removeAllViews();
        view.clearFocus(); view.setPressed(false); view.setSelected(false);
        view.setOnClickListener(null); view.setOnLongClickListener(null);
        view.setOnFocusChangeListener(null); view.setAccessibilityDelegate(null);
        accessibility.remove(view); view.setContentDescription(null); view.setTag(null);
        if (Build.VERSION.SDK_INT >= 30) view.setStateDescription(null);
        if (view instanceof TextView) ((TextView)view).setText("");
        String kind = recycleKind(node);
        if (kind == null || recycledCount >= 128) return;
        java.util.ArrayDeque<View> pool = recycled.computeIfAbsent(kind, key -> new java.util.ArrayDeque<>());
        if (pool.size() < 32) { pool.addLast(view); recycledCount++; }
    }
    private volatile String webSession;
    private Runnable quitPoll;
    private Runnable removeBackHandler;

    /** Opt an application-owned root into system Back, including gesture Back. */
    public void installBackHandler(android.app.Activity activity) {
        uiThread();
        if (removeBackHandler != null) removeBackHandler.run();
        removeBackHandler = NativeBack.install(activity, () -> requestClose(activity::finish));
    }

    /** Request an application exit; veto runs before cleanup and Activity teardown. */
    public boolean requestClose(Runnable accepted) {
        uiThread();
        if (quitPoll != null) return true;
        if (webSession == null || root == null) return false;
        final String session = webSession;
        int result = NativeRuntime.quit(session, true);
        if (result < 0) return false;
        if (result == 1) { accepted.run(); return true; }
        quitPoll = new Runnable() {
            @Override public void run() {
                if (!session.equals(webSession) || root == null) { quitPoll = null; return; }
                int state = NativeRuntime.quit(session, false);
                if (state == 0) { postDelayed(this, 8); return; }
                quitPoll = null;
                if (state == 1) accepted.run();
            }
        };
        post(quitPoll);
        return true;
    }
    String webResourceSession() { return webSession; }
    private boolean servicePumpScheduled;
    private final Runnable servicePump = () -> {
        servicePumpScheduled = false;
        if (webSession != null) NativeRuntime.pumpServices(this, webSession);
    };
    public void scheduleServicePump(String session) {
        uiThread();
        if (session.equals(webSession) && !servicePumpScheduled) {
            servicePumpScheduled = true; post(servicePump);
        }
    }
    private boolean webPumpScheduled;
    private final Runnable webPump = () -> {
        webPumpScheduled = false;
        if (webSession != null && NativeRuntime.pumpWeb(this, webSession)) scheduleWebPump("");
    };
    private final NativePerformance performance = new NativePerformance(this);
    public void bindWebSession(String session) { uiThread(); webSession = session; performance.bind(session); }
    public void setPerformanceVisible(boolean visible) { uiThread(); performance.setVisible(visible); }
    public String performanceSnapshot() { uiThread(); return performance.snapshot(); }
    @Override protected void onAttachedToWindow() { super.onAttachedToWindow(); performance.start(); }
    @Override protected void onDetachedFromWindow() { performance.stop(); super.onDetachedFromWindow(); }
    @Override protected void onWindowVisibilityChanged(int visibility) {
        super.onWindowVisibilityChanged(visibility);
        if (performance != null) { if (visibility == VISIBLE) performance.start(); else performance.stop(); }
    }
    @Override protected void dispatchDraw(android.graphics.Canvas canvas) {
        long started = System.nanoTime();
        super.dispatchDraw(canvas);
        performance.draw(canvas, System.nanoTime() - started);
    }
    public void configureImages(String limits) throws JSONException { uiThread(); resources.configureImages(new JSONObject(limits)); }
    public void scheduleWebPump(String ignored) {
        uiThread();
        if (webSession != null && !webPumpScheduled) { webPumpScheduled = true; postDelayed(webPump, 25); }
    }
    private final java.util.ArrayDeque<JSONObject> webResults = new java.util.ArrayDeque<>();
    private final HashMap<String, JSONObject> descriptors = new HashMap<>();
    private final HashMap<String, BigInteger> toggles = new HashMap<>();
    private final WeakHashMap<View, NativeAccessibility> accessibility = new WeakHashMap<>();
    private final HashMap<String, NativeDialog> dialogs = new HashMap<>();
    private final ArrayList<String> dialogOrder = new ArrayList<>();
    private final HashMap<String, NativePopover> popovers = new HashMap<>();
    private final ArrayList<String> popoverOrder = new ArrayList<>();
    private boolean popoverUpdatePending;
    private final NativeWindows windows = new NativeWindows(this);
    private String root;
    private String lastError;
    private java.util.function.Consumer<String> stateListener;
    private boolean publishing;
    private long environment = 1;
    private boolean resizing;
    private int modalViewportBottom = Integer.MAX_VALUE;
    private float density;
    private final NativeEnvironment preferences;

    public static native String nativeCreate(NativeView peer, float width, float height, long count, String text);
    public static native String nativeEvent(String root, String json);
    private static native String nativeDispatch(String root, String json);
    public static native String nativeResize(String root, float width, float height, long environment);
    public static native String nativeState(String root);
    public static native String nativeRestore(String root, String state);
    public static native void nativeDispose(String root);

    private static void setPaneTitle(View view, String title) {
        if (Build.VERSION.SDK_INT >= 28) view.setAccessibilityPaneTitle(title);
    }

    public NativeView(Context context) {
        super(context);
        density = context.getResources().getDisplayMetrics().density;
        preferences = new NativeEnvironment(this);
        setClipChildren(true);
        setSaveFromParentEnabled(false);
        setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        if (Build.VERSION.SDK_INT >= 26) setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_YES);
        getViewTreeObserver().addOnScrollChangedListener(() -> {
            applyWindowInsets(getRootWindowInsets());
            schedulePopovers();
        });
        setOnApplyWindowInsetsListener((view, insets) -> {
            applyWindowInsets(insets);
            return insets;
        });
    }

    private final int[] additionalSafeArea = new int[4];
    /** Root-local host overlay insets in physical pixels, in left/top/right/bottom order. */
    public void setAdditionalSafeAreaInsets(int left, int top, int right, int bottom) {
        uiThread();
        additionalSafeArea[0] = Math.max(0, left); additionalSafeArea[1] = Math.max(0, top);
        additionalSafeArea[2] = Math.max(0, right); additionalSafeArea[3] = Math.max(0, bottom);
        applyWindowInsets(getRootWindowInsets());
    }

    private void applyWindowInsets(WindowInsets insets) {
        if (getWidth() == 0 || getHeight() == 0) return;
        int left = 0, top = 0, right = 0, bottom = 0;
        if (insets != null) {
            if (Build.VERSION.SDK_INT >= 30) {
                int insetTypes = WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout();
                if (insets.isVisible(WindowInsets.Type.ime())) insetTypes |= WindowInsets.Type.ime();
                android.graphics.Insets safe = insets.getInsets(insetTypes);
                left = safe.left; top = safe.top; right = safe.right; bottom = safe.bottom;
            } else {
                left = insets.getSystemWindowInsetLeft(); top = insets.getSystemWindowInsetTop();
                right = insets.getSystemWindowInsetRight(); bottom = insets.getSystemWindowInsetBottom();
            }
        }
        // Window insets describe the decor edges. Embedded roots and hosts using
        // adjustResize need only the portion that actually overlaps this view.
        View window = getRootView();
        int[] position = new int[2];
        getLocationInWindow(position);
        left = Math.min(getWidth(), Math.max(0, left - position[0]));
        top = Math.min(getHeight(), Math.max(0, top - position[1]));
        right = Math.min(getWidth() - left, Math.max(0, right - (window.getWidth() - position[0] - getWidth())));
        bottom = Math.min(getHeight() - top, Math.max(0, bottom - (window.getHeight() - position[1] - getHeight())));
        // Clamp before adding to avoid overflow from untrusted host dimensions.
        left += Math.min(getWidth() - left, additionalSafeArea[0]);
        top += Math.min(getHeight() - top, additionalSafeArea[1]);
        right += Math.min(Math.max(0, getWidth() - left - right), additionalSafeArea[2]);
        bottom += Math.min(Math.max(0, getHeight() - top - bottom), additionalSafeArea[3]);
        right = Math.min(right, getWidth() - left);
        bottom = Math.min(bottom, getHeight() - top);
        if (getPaddingLeft() != left || getPaddingTop() != top || getPaddingRight() != right || getPaddingBottom() != bottom) {
            setPadding(left, top, right, bottom);
            scheduleResize();
        }
    }

    private void uiThread() {
        if (Looper.myLooper() != Looper.getMainLooper()) throw new IllegalStateException("UI thread required");
    }

    public void mount(long count, String text) {
        uiThread();
        if (root != null) throw new IllegalStateException("root already mounted");
        windows.start();
        preferences.start();
        if (resourcesClosed) { resources = resources.reopen(); resourcesClosed = false; }
        lastError = null;
        int width = getWidth() > 0 ? Math.max(0, getWidth() - getPaddingLeft() - getPaddingRight()) : getResources().getDisplayMetrics().widthPixels;
        int height = getHeight() > 0 ? Math.max(0, getHeight() - getPaddingTop() - getPaddingBottom()) : getResources().getDisplayMetrics().heightPixels;
        try { root = nativeCreate(this, width / density, height / density, count, text); }
        catch (RuntimeException | Error error) { preferences.close(); throw error; }
        schedulePopovers();
    }

    public String token() { return root; }
    public void scheduleWindowPump(String session) { uiThread(); if (session.equals(webSession)) windows.schedule(); }
    void mountWindow(String session, JSONObject content, float width, float height, NativeView parent) {
        uiThread();
        if (root != null) throw new IllegalStateException("root already mounted");
        resourcePolicy = parent.resourcePolicy;
        hostedFactories.putAll(parent.hostedFactories);
        windows.start();
        preferences.start();
        try {
            root = NativeRuntime.createWindowRoot(session, this, width, height, content.toString());
            if (root == null) throw new IllegalStateException("window factory returned no root");
        } catch (RuntimeException | Error error) { close(); throw error; }
    }
    private String nativeAppearance = "inherit";
    String nativeAppearance() { return nativeAppearance; }
    void setNativeAppearance(String appearance) {
        if (!nativeAppearance.equals(appearance)) { nativeAppearance = appearance; environmentChanged(); }
    }
    public String environmentSnapshot(String ignored) { return preferences.snapshot(); }
    /** Override this root's locale; null follows the system again. */
    public void setLocale(java.util.Locale locale) { preferences.setLocale(locale); }
    /** Override shared text scaling; 0 follows the system again. Layout lengths stay unchanged. */
    public void setFontScale(float scale) { preferences.setFontScale(scale); }
    void environmentChanged() {
        uiThread();
        density = getResources().getDisplayMetrics().density;
        scheduleResize();
    }
    @Override protected void onConfigurationChanged(android.content.res.Configuration configuration) {
        super.onConfigurationChanged(configuration);
        if (preferences != null) environmentChanged();
    }
    public String state() { uiThread(); return nativeState(root); }
    public void restore(String state) { uiThread(); nativeRestore(root, state); }
    public String takeError() { uiThread(); String error = lastError; lastError = null; return error; }
    void serviceFailure(String error) { uiThread(); lastError = error; close(); }
    public void setStateListener(java.util.function.Consumer<String> listener) { uiThread(); stateListener = listener; }
    static boolean isAutofillTransactionTooLarge(Throwable error) {
        for (int depth = 0; error != null && depth < 16; depth++, error = error.getCause()) {
            if (error instanceof TransactionTooLargeException) return true;
        }
        return false;
    }
    private void handleAutofillTransactionTooLarge(android.view.ViewStructure structure) {
        setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        structure.setChildCount(0);
        lastError = "Android Autofill structure exceeded the Binder transaction limit";
    }
    @Override public void dispatchProvideAutofillStructure(android.view.ViewStructure structure, int flags) {
        try {
            super.dispatchProvideAutofillStructure(structure, flags);
        } catch (RuntimeException error) {
            if (!isAutofillTransactionTooLarge(error)) throw error;
            handleAutofillTransactionTooLarge(structure);
        }
    }
    @Override public void onProvideAutofillStructure(android.view.ViewStructure structure, int flags) {
        try {
            super.onProvideAutofillStructure(structure, flags);
        } catch (RuntimeException error) {
            if (!isAutofillTransactionTooLarge(error)) throw error;
            handleAutofillTransactionTooLarge(structure);
        }
    }
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

    float density() { return density; }
    void requireUiThread() { uiThread(); }
    boolean owns(String id, View view) { return root != null && views.get(id) == view; }
    JSONObject descriptor(String id) { return descriptors.get(id); }
    View nodeView(String id) { return views.get(id); }
    boolean resourceAllowed(android.net.Uri uri) { return resourcePolicy.test(uri); }
    NativeHosted.Factory hostedFactory(String kind) { return hostedFactories.get(kind); }
    public void registerHostedComponent(String kind, NativeHosted.Factory factory) {
        uiThread();
        if (kind == null || !kind.matches("[A-Za-z0-9._-]{1,128}")) throw new IllegalArgumentException("Invalid hosted component kind");
        if (hostedFactories.putIfAbsent(kind, java.util.Objects.requireNonNull(factory)) != null) throw new IllegalStateException("Hosted component already registered");
    }
    public void setResourcePolicy(java.util.function.Predicate<android.net.Uri> policy) { uiThread(); resourcePolicy = java.util.Objects.requireNonNull(policy); }
    public void registerAssetRoot(String name, java.io.File directory) throws java.io.IOException { uiThread(); resources.registerAssetRoot(name, directory); }
    public void registerFont(String family, byte[] bytes) throws Exception { uiThread(); resources.registerFont(family, bytes); }
    void resourcesChanged() {
        uiThread(); boolean previous = publishing; publishing = true;
        try { for (String id : views.keySet()) style(views.get(id), descriptors.get(id)); }
        catch (JSONException error) { throw new IllegalStateException(error); }
        finally { publishing = previous; }
        scheduleResize();
    }
    private static void disposeView(View view) {
        NativeAppearance.dispose(view);
        view.animate().cancel();
        if (view instanceof NativeImage) ((NativeImage)view).dispose();
        if (view instanceof NativeScroll) ((NativeScroll)view).dispose();
        if (view instanceof NativeHosted) ((NativeHosted)view).close();
    }
    /** A borrowed handle validates its owner on every operation, including after root disposal. */
    public final class ScrollHandle {
        private final String id; private final NativeScroll scroll;
        private ScrollHandle(String id, NativeScroll scroll) { this.id=id; this.scroll=scroll; }
        private void check() { uiThread(); if (!owns(id, scroll)) throw new IllegalStateException("Stale scroll handle"); }
        public JSONObject state() { check(); return scroll.state(); }
        public void scrollTo(double x,double y,boolean smooth) { check(); scroll.scroll(x,y,smooth); }
        public void stop() { check(); scroll.stop(); }
        public AutoCloseable observe(java.util.function.Consumer<JSONObject> listener) { check(); return scroll.observe(listener); }
    }
    public ScrollHandle scrollHandle(String id) { uiThread(); View view=views.get(id); if (!(view instanceof NativeScroll)) throw new IllegalArgumentException("Not a scroll node"); return new ScrollHandle(id,(NativeScroll)view); }
    public void scrollCommand(String json) throws JSONException {
        uiThread(); JSONObject command = new JSONObject(json); ScrollHandle handle = scrollHandle(command.getString("id"));
        switch (command.getString("operation")) {
            case "scroll": { JSONArray value = command.getJSONArray("offset"); handle.scrollTo(value.getDouble(0), value.getDouble(1), false); break; }
            case "stop": handle.stop(); break;
            case "zoom": { JSONArray value = command.getJSONArray("anchor"); handle.scroll.setZoom(command.getDouble("scale"), (float)value.getDouble(0)*density, (float)value.getDouble(1)*density); break; }
            default: throw new IllegalArgumentException("Unknown scroll operation");
        }
    }
    public String webExchange(String json) throws JSONException {
        uiThread(); JSONArray work = new JSONArray(json);
        if (work.length() > 128) throw new IllegalArgumentException("Web work budget exceeded");
        for (int i=0; i<work.length(); i++) {
            JSONObject item = work.getJSONObject(i); String request = item.getString("request"); View view = views.get(item.getString("node"));
            NativeWeb web = view instanceof NativeHosted ? ((NativeHosted)view).web() : null;
            java.util.function.BiConsumer<Boolean,String> complete = (ok,value) -> {
                try { if (webResults.size() < 128) webResults.add(new JSONObject().put("request",request).put("ok",ok).put("value",value)); }
                catch (JSONException error) { throw new IllegalStateException(error); }
            };
            if (web == null) complete.accept(false,"Web view unavailable");
            else web.evaluateRaw(item.getString("script"), value -> complete.accept(true,value), error -> complete.accept(false,error));
        }
        JSONObject ready = new JSONObject(); JSONArray results = new JSONArray();
        if (work.length() == 0) {
            for (java.util.Map.Entry<String,View> item : views.entrySet()) {
                NativeWeb web = item.getValue() instanceof NativeHosted ? ((NativeHosted)item.getValue()).web() : null;
                if (web != null) ready.put(item.getKey(),web.ready());
            }
            int bytes = 0;
            while (!webResults.isEmpty() && bytes < 8*1024*1024) {
                JSONObject result = webResults.remove(); bytes += result.toString().length(); results.put(result);
            }
        }
        return new JSONObject().put("ready",ready).put("results",results).toString();
    }
    private String requestedFocus;
    private String replacedFocus;
    private JSONArray materializationPlan() throws JSONException {
        android.view.accessibility.AccessibilityManager manager=(android.view.accessibility.AccessibilityManager)getContext().getSystemService(Context.ACCESSIBILITY_SERVICE);
        boolean exploration=manager!=null&&manager.isTouchExplorationEnabled();
        JSONArray candidates=new JSONArray();HashMap<String,Integer> indices=new HashMap<>();
        java.util.HashSet<String> relationTargets = new java.util.HashSet<>();
        for (int i = 0; i < orderedNodes.length(); i++) {
            JSONObject relations = orderedNodes.getJSONObject(i).optJSONObject("accessibilityRelations");
            if (relations == null) continue;
            JSONObject active = relations.optJSONObject("activeDescendant");
            if (active != null) relationTargets.add(active.optString("id"));
            JSONArray controls = relations.optJSONArray("controls");
            if (controls != null) for (int n = 0; n < controls.length(); n++) {
                JSONObject target = controls.optJSONObject(n);
                if (target != null) relationTargets.add(target.optString("id"));
            }
        }
        for(int i=0;i<orderedNodes.length();i++) {
            JSONObject node=orderedNodes.getJSONObject(i);String id=node.getString("id"),kind=node.getString("kind");View view=views.get(id);
            JSONObject appearance=node.getJSONObject("style").optJSONObject("view");
            boolean preserveSubtree="hosted".equals(kind)
                ||(appearance!=null&&(!appearance.isNull("animation")||!appearance.isNull("transform")));
            boolean pinned=exploration||relationTargets.contains(id)||id.equals(requestedFocus)||(view!=null&&view.hasFocus())
                ||java.util.Arrays.asList("input","textarea","hosted","dialog","popover","scroll").contains(kind)
                ||(appearance!=null&&(!appearance.isNull("animation")||!appearance.isNull("transform")));
            JSONObject viewport=null;
            if(view instanceof NativeScroll)viewport=((NativeScroll)view).viewport();
            else if("scroll".equals(kind)) {
                JSONArray frame=node.getJSONArray("frame"),offset=node.getJSONObject("style").optJSONArray("scroll_to");
                viewport=new JSONObject().put("offset",offset==null?new JSONArray().put(0).put(0):offset)
                    .put("size",new JSONArray().put(frame.getDouble(2)).put(frame.getDouble(3)))
                    .put("velocity",new JSONArray().put(0).put(0)).put("elapsed",0);
            }
            candidates.put(new JSONObject().put("parent",node.isNull("parent")?JSONObject.NULL:indices.get(node.getString("parent")))
                .put("frame",node.getJSONArray("frame")).put("pinned",pinned).put("preserve_subtree",preserveSubtree)
                .put("viewport",viewport==null?JSONObject.NULL:viewport));
            indices.put(id,i);
        }
        try{return (JSONArray)NativeRuntime.call(NativeRuntime.object("viewport").put("nodes",candidates));}
        catch(IllegalArgumentException error){JSONArray all=new JSONArray();for(int i=0;i<orderedNodes.length();i++)all.put(true);return all;}
    }
    void materialize() {
        if(publishing||root==null||orderedNodes.length()==0)return;
        publishing=true;
        try{renderNodes(orderedNodes,descriptors,materializationPlan());reconcileHierarchy(orderedNodes);updateScrollExtents();}
        catch(JSONException error){throw new IllegalStateException(error);}
        finally{publishing=false;}
    }
    private void updateScrollExtents(){for(View view:views.values())if(view instanceof NativeScroll)((NativeScroll)view).extent();}
    double[] documentExtent(String parent) {
        double[] size={0,0};JSONObject descriptor=descriptors.get(parent);if(descriptor==null)return size;
        JSONArray origin=descriptor.optJSONArray("frame");
        for(JSONObject node:descriptors.values())if(parent.equals(node.optString("parent"))){JSONArray frame=node.optJSONArray("frame");
            size[0]=Math.max(size[0],frame.optDouble(0)-origin.optDouble(0)+frame.optDouble(2));
            size[1]=Math.max(size[1],frame.optDouble(1)-origin.optDouble(1)+frame.optDouble(3));}
        return size;
    }
    @Override public boolean dispatchTouchEvent(MotionEvent event) { if (performance.touch(event)) return true; MotionEvent filtered=keyboard.filter(event,interactions.longPressed());
        try { return interactions.touch(filtered) || super.dispatchTouchEvent(filtered); }
        finally { if(filtered!=event)filtered.recycle(); } }
    void cancelNativeTouch() { long now=android.os.SystemClock.uptimeMillis(); MotionEvent cancel=MotionEvent.obtain(now,now,MotionEvent.ACTION_CANCEL,0,0,0); super.dispatchTouchEvent(cancel);cancel.recycle(); }
    @Override public boolean dispatchKeyEvent(KeyEvent event) { return interactions.key(event) || super.dispatchKeyEvent(event); }
    @Override public void requestDisallowInterceptTouchEvent(boolean value) {
        if (value && interactions != null) interactions.childClaimed();
        super.requestDisallowInterceptTouchEvent(value);
    }
    @Override public boolean dispatchHoverEvent(MotionEvent event) { interactions.hover(event); return super.dispatchHoverEvent(event); }
    @Override public boolean dispatchGenericMotionEvent(MotionEvent event) { return interactions.generic(event) || super.dispatchGenericMotionEvent(event); }
    private int px(double value) { return (int) Math.round(value * density); }

    private View create(JSONObject node) throws JSONException {
        String kind = node.getString("kind");
        switch (kind) {
            case "box": return new NativeFrame(getContext());
            case "dialog": return new NativeDialog(getContext());
            case "popover": return new NativePopover(getContext());
            case "scroll": return new NativeScroll(this);
            case "text": return new TextView(getContext());
            case "button": if (contentParents.contains(node.optString("id"))) return new NativeContentButton(getContext());
                return "radio".equals(node.optString("role"))
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
            case "image": return new NativeImage(this);
            case "hosted": return new NativeHosted(this);
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
        text.setTypeface(resources.font(style.getString("font_family"),
                style.getInt("font_weight") >= 600 ? Typeface.BOLD : Typeface.NORMAL));
    }

    void applyAccessibility(View view, JSONObject node) throws JSONException {
        String role = node.isNull("role") ? null : node.getString("role");
        String supplementalDescription = node.isNull("description") ? null : node.getString("description");
        Object selectedValue = node.opt("selected");
        Object expandedValue = node.opt("expanded");
        Boolean selected = selectedValue instanceof Boolean ? (Boolean) selectedValue : null;
        Boolean expanded = expandedValue instanceof Boolean ? (Boolean) expandedValue : null;
        JSONObject relations = node.optJSONObject("accessibilityRelations");
        NativeAccessibility delegate = accessibility.get(view);
        if (delegate == null) {
            delegate = new NativeAccessibility();
            accessibility.put(view, delegate);
            view.setAccessibilityDelegate(delegate);
        }
        boolean initialized = delegate.semantics != null;
        boolean previouslyHidden = initialized && delegate.semantics.optBoolean("hidden");
        boolean changed = delegate.update(role, selected, expanded, supplementalDescription, node);
        NativeSemantics.apply(view, delegate.semantics);
        android.view.accessibility.AccessibilityManager manager = (android.view.accessibility.AccessibilityManager)
            view.getContext().getSystemService(android.content.Context.ACCESSIBILITY_SERVICE);
        if (initialized && changed && manager != null && manager.isEnabled() && view.isAttachedToWindow()
                && view.isShown() && !previouslyHidden && accessibilityVisible(view)) {
            android.view.accessibility.AccessibilityEvent event = android.view.accessibility.AccessibilityEvent.obtain(
                android.view.accessibility.AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
            boolean focused = view.isAccessibilityFocused();
            for (android.view.ViewParent parent=view.getParent(); !focused && parent instanceof View; parent=parent.getParent()) focused=((View)parent).isAccessibilityFocused();
            event.setContentChangeTypes(focused && Build.VERSION.SDK_INT >= 30
                ? android.view.accessibility.AccessibilityEvent.CONTENT_CHANGE_TYPE_STATE_DESCRIPTION
                : android.view.accessibility.AccessibilityEvent.CONTENT_CHANGE_TYPE_UNDEFINED);
            view.sendAccessibilityEventUnchecked(event);
        }
    }

    private boolean accessibilityVisible(View view) {
        for (View item=view; item!=null; item=item.getParent() instanceof View ? (View)item.getParent() : null) {
            NativeAccessibility delegate=accessibility.get(item);
            if (delegate != null && delegate.semantics != null && delegate.semantics.optBoolean("hidden")) return false;
            if (item == this) break;
        }
        return true;
    }

    private final class NativeAccessibility extends View.AccessibilityDelegate {
        private static final int SHOW_CONTROLLED_CONTENT = 0x3f4a0001;
        private String role;
        private Boolean selected;
        private Boolean expanded;
        private String supplementalDescription;
        private JSONObject grid, cell, columnHeader, relations;
        private boolean routing;
        private JSONObject semantics;

        boolean update(String role, Boolean selected, Boolean expanded, String supplementalDescription, JSONObject node) {
            JSONObject nextGrid = node.optJSONObject("grid"), nextCell = node.optJSONObject("cell"), nextHeader = node.optJSONObject("columnHeader"), nextRelations = node.optJSONObject("accessibilityRelations");
            JSONObject nextSemantics = node.optJSONObject("accessibility");
            if (nextSemantics == null) nextSemantics = new JSONObject();
            boolean changed = !java.util.Objects.equals(String.valueOf(semantics), String.valueOf(nextSemantics))
                || !java.util.Objects.equals(this.role, role)
                || !java.util.Objects.equals(this.selected, selected)
                || !java.util.Objects.equals(this.expanded, expanded)
                || !java.util.Objects.equals(this.supplementalDescription, supplementalDescription)
                || !java.util.Objects.equals(String.valueOf(grid), String.valueOf(nextGrid))
                || !java.util.Objects.equals(String.valueOf(cell), String.valueOf(nextCell))
                || !java.util.Objects.equals(String.valueOf(columnHeader), String.valueOf(nextHeader))
                || !java.util.Objects.equals(String.valueOf(relations), String.valueOf(nextRelations));
            semantics = nextSemantics;
            this.role = role;
            this.selected = selected;
            this.expanded = expanded;
            this.supplementalDescription = supplementalDescription;
            grid = nextGrid; cell = nextCell; columnHeader = nextHeader; relations = nextRelations;
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
                case "tree": description = "Tree"; info.setClassName(android.widget.ListView.class.getName()); break;
                case "listbox": description = "List box"; info.setClassName(android.widget.ListView.class.getName()); break;
                case "option": description = "Option"; info.setClassName(View.class.getName()); break;
                case "heading": description = "Heading"; if (Build.VERSION.SDK_INT >= 28) info.setHeading(true); break;
                case "treeitem": description = "Tree item"; info.setClassName(View.class.getName()); break;
                case "grid": description = "Grid"; info.setClassName(android.widget.GridView.class.getName()); break;
                case "row": description = "Row"; break;
                case "gridcell": description = "Cell"; break;
                case "columnheader": description = "Column header"; break;
                default: description = null;
            }
            NativeSemantics.initialize(host, info, role, semantics);
            if (description != null) info.getExtras().putCharSequence("AccessibilityNodeInfo.roleDescription", description);
            if (relations != null) {
                JSONObject active = relations.optJSONObject("activeDescendant");
                View descendant = relationTarget(active);
                if (descendant != null && descendant != host) {
                    info.getExtras().putString("dev.jadix.activeDescendant", active.optString("id"));
                }
                JSONArray controls = relations.optJSONArray("controls");
                java.util.ArrayList<String> targets = new java.util.ArrayList<>();
                if (controls != null) for (int i = 0; i < controls.length(); i++) {
                    JSONObject target = controls.optJSONObject(i);
                    View controlled = relationTarget(target);
                    if (controlled != null && controlled != host) targets.add(target.optString("id"));
                }
                info.getExtras().putStringArrayList("dev.jadix.controls", targets);
                if (!targets.isEmpty()) info.addAction(new AccessibilityNodeInfo.AccessibilityAction(SHOW_CONTROLLED_CONTENT, "Show controlled content"));
            }
            if (selected != null && !"radio".equals(role)) info.setSelected(selected);
            if (grid != null) info.setCollectionInfo(AccessibilityNodeInfo.CollectionInfo.obtain(
                grid.optInt("rows"), grid.optInt("columns"), false));
            if (cell != null) info.setCollectionItemInfo(AccessibilityNodeInfo.CollectionItemInfo.obtain(
                cell.optInt("row"), Math.max(1, cell.optInt("rowSpan", 1)), cell.optInt("column"),
                Math.max(1, cell.optInt("columnSpan", 1)), false, Boolean.TRUE.equals(selected)));
            if (columnHeader != null) {
                info.setCollectionItemInfo(AccessibilityNodeInfo.CollectionItemInfo.obtain(0, 1,
                    columnHeader.optInt("column"), Math.max(1, columnHeader.optInt("columnSpan", 1)), true));
                if (Build.VERSION.SDK_INT >= 28) info.setHeading(true);
            }
            if (expanded != null) {
                String state = NativeSemantics.state(semantics, expanded ? "Expanded" : "Collapsed");
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
        private View relationTarget(JSONObject target) {
            View view = target == null ? null : views.get(target.optString("id"));
            return view != null && view.isAttachedToWindow() && view.isShown() ? view : null;
        }
        @Override public void onInitializeAccessibilityEvent(View host, android.view.accessibility.AccessibilityEvent event) {
            super.onInitializeAccessibilityEvent(host,event);
            AccessibilityNodeInfo info=host.createAccessibilityNodeInfo();
            try {
                event.setClassName(info.getClassName()); event.setEnabled(info.isEnabled()); event.setChecked(info.isChecked());
                AccessibilityNodeInfo.RangeInfo range=info.getRangeInfo();
                if (range!=null) {
                    event.setItemCount(100);
                    event.setCurrentItemIndex(Math.round(100*(range.getCurrent()-range.getMin())/(range.getMax()-range.getMin())));
                }
            } finally { info.recycle(); }
        }
        @Override public boolean performAccessibilityAction(View host, int action, android.os.Bundle arguments) {
            if (!accessibilityVisible(host)) return false;
            if (NativeSemantics.mutates(action) && (!host.isEnabled() || semantics.optBoolean("disabled")
                    || semantics.optBoolean("readOnly") || Boolean.FALSE.equals(semantics.opt("accessible")))) return false;
            if (routing) return super.performAccessibilityAction(host, action, arguments);
            JSONObject payload = NativeSemantics.action(host, semantics, action, arguments);
            if (payload != null && host.getTag() instanceof String) {
                String id = (String) host.getTag();
                if (views.get(id) != host || !eventVisible(id) || !host.isShown()) return false;
                emit(id, payload); return true;
            }
            View target = null;
            if (relations != null && action == AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS) {
                target = relationTarget(relations.optJSONObject("activeDescendant"));
            } else if (relations != null && action == SHOW_CONTROLLED_CONTENT) {
                JSONArray controls = relations.optJSONArray("controls");
                if (controls != null) for (int i = 0; i < controls.length() && target == null; i++) target = relationTarget(controls.optJSONObject(i));
            }
            if (target != null && target != host) {
                routing = true;
                try {
                    target.performAccessibilityAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SHOW_ON_SCREEN.getId(), null);
                    return target.performAccessibilityAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS, arguments);
                } finally { routing = false; }
            }
            return super.performAccessibilityAction(host, action, arguments);
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
        if (view instanceof NativeImage) ((NativeImage)view).update(node);
        if (view instanceof NativeScroll) ((NativeScroll)view).configure(style);
        if (view instanceof NativeHosted) ((NativeHosted)view).configure(node);
        NativeAppearance.view(view, style, density);
        JSONObject keyboard=node.optJSONObject("keyboard");
        if (view instanceof NativeContentButton) view.setEnabled(node.getBoolean("enabled"));
        boolean control = view instanceof Button || view instanceof NativeContentButton || view instanceof CompoundButton || view instanceof EditText || view instanceof NativeSelect;
        view.setFocusable(node.opt("tabStop") instanceof Boolean ? node.optBoolean("tabStop")
            : control || keyboard != null && keyboard.optBoolean("focusable"));
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
                    int start=Math.max(0,input.getSelectionStart()), end=Math.max(0,input.getSelectionEnd());
                    JSONArray selection = (JSONArray) NativeRuntime.call(NativeRuntime.object("textSelection")
                        .put("before", input.getText().toString()).put("after", value)
                        .put("selection", new JSONArray().put(start).put(end)));
                    input.setText(value);
                    input.setSelection(selection.getInt(0), selection.getInt(1));
                }
                input.setHint(node.getString("placeholder"));
                input.setEnabled(node.getBoolean("enabled"));
            } else {
                if (!text.getText().toString().equals(value)) text.setText(value);
                if (text instanceof Button) text.setEnabled(node.getBoolean("enabled"));
            }
            NativeAppearance.text(text, node, density);
        }
    }

    public float[] measure(String json) throws JSONException {
        uiThread();
        JSONObject request = new JSONObject(json);
        View view = create(request.getJSONObject("node"));
        try {
            style(view, request.getJSONObject("node"));
            int widthMode = "exact".equals(request.getString("widthMode")) ? MeasureSpec.EXACTLY : MeasureSpec.AT_MOST;
            int heightMode = "exact".equals(request.getString("heightMode")) ? MeasureSpec.EXACTLY : MeasureSpec.AT_MOST;
            view.measure(MeasureSpec.makeMeasureSpec(px(request.getDouble("maxWidth")), widthMode),
                    MeasureSpec.makeMeasureSpec(px(request.getDouble("maxHeight")), heightMode));
            return new float[] { view.getMeasuredWidth() / density, view.getMeasuredHeight() / density };
        } finally { disposeView(view); }
    }

    void emit(String id, JSONObject payload) {
        if (publishing || root == null || !eventVisible(id)) return;
        try {
            JSONObject event = new JSONObject().put("root", root).put("node", id).put("payload", payload);
            try {
                String state = stateListener == null ? nativeEvent(root, event.toString()) : nativeDispatch(root, event.toString());
                if (stateListener != null && !state.isEmpty()) stateListener.accept(state);
            }
            catch (IllegalStateException error) {
                lastError = error.getMessage();
                android.util.Log.e("JadixNativeEvent", "Native event rejected for " + id, error);
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

    static JSONObject payload(String kind) {
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
        view.setOnFocusChangeListener((focused, hasFocus) -> {
            if (!hasFocus) emit(id, payload("blur"));
            else emit(id, payload(focused.isInTouchMode() ? "focus" : "focusVisible"));
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
        if (prepared.labelsOnly) {
            publishing = true;
            try {
                for (int index = 0; index < prepared.updates.length(); index++) {
                    JSONObject node = prepared.updates.getJSONObject(index);
                    String id = node.getString("id");
                    View view = views.get(id);
                    if (view != null) style(view, node);
                    descriptors.put(id, node);
                    orderedNodes.put(nodePositions.get(id), node);
                }
            } finally { publishing = false; }
        } else publishSnapshot(prepared.snapshot(), prepared.topology ? null : prepared.updates, prepared.topology, prepared.nodes);
        if (prepared.focusChanged) {
            requestedFocus=prepared.focus;materialize();
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
            if (!descriptors.containsKey(node.getString("id"))) continue;
            if (!descriptors.get(node.getString("id")).getString("kind").equals(node.getString("kind"))) {
                throw new IllegalArgumentException("node kind changed");
            }
            if (existing != null && "button".equals(node.getString("kind"))
                    && "radio".equals(descriptors.get(node.getString("id")).optString("role")) != "radio".equals(node.optString("role"))) {
                throw new IllegalArgumentException("button/radio role changes require a new node identity");
            }
        }
        publishing = true;
        try {
            HashSet<String> live = new HashSet<>();
            HashMap<String,JSONObject> previousDescriptors=new HashMap<>(descriptors);
            for(int i=0;i<nodes.length();i++){JSONObject node=nodes.getJSONObject(i);String id=node.getString("id");live.add(id);descriptors.put(id,node);}
            for(String id:new HashSet<>(descriptors.keySet()))if(!live.contains(id))descriptors.remove(id);
            orderedNodes=nodes;
            nodePositions.clear();
            for (int index = 0; index < nodes.length(); index++) nodePositions.put(nodes.getJSONObject(index).getString("id"), index);
            renderNodes(nodes,previousDescriptors,materializationPlan());
            reconcileHierarchy(nodes);
            if (topology) {
            for (int index=dialogOrder.size()-1;index>=0;index--) {
                String id = dialogOrder.get(index);
                if (!live.contains(id)) dialogs.get(id).hide();
            }
            for (String id : new HashSet<>(views.keySet())) if (!live.contains(id)) {
                interactions.removing(id);
                View view = views.remove(id);
                retire(view, previousDescriptors.get(id));
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
            updateScrollExtents();
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
        materialize();
    }

    private void renderNodes(JSONArray rendering, HashMap<String,JSONObject> previousDescriptors, JSONArray plan) throws JSONException {
            contentParents.clear();
            for (int index = 0; index < rendering.length(); index++) {
                JSONObject node = rendering.getJSONObject(index);
                if (!node.isNull("parent")) contentParents.add(node.getString("parent"));
            }
            // Retire the departing viewport before allocating its replacement.
            for (int index = 0; index < rendering.length(); index++) if (!plan.getBoolean(index)) {
                String id = rendering.getJSONObject(index).getString("id");
                View old = views.remove(id);
                if (old != null) { interactions.removing(id); retire(old, previousDescriptors.get(id)); }
            }
            for (int index = 0; index < rendering.length(); index++) {
                JSONObject node = rendering.getJSONObject(index);
                String id = node.getString("id");
                if (!plan.getBoolean(index)) continue;
                String kind = node.getString("kind");
                View view = views.get(id);
                boolean restoreFocus = false;
                if (view != null && "button".equals(kind)
                        && (view instanceof NativeContentButton) != contentParents.contains(id)) {
                    restoreFocus = view.hasFocus();
                    interactions.removing(id);
                    retire(view, previousDescriptors.get(id));
                    views.remove(id);
                    view = null;
                }
                boolean created=view==null;
                if (created) {
                    view = acquire(node);
                    views.put(id, view);
                    listen(id, view, kind);
                    if (view instanceof NativeDialog) dialogs.put(id, (NativeDialog) view);
                    if (view instanceof NativePopover) popovers.put(id, (NativePopover) view);
                } else if (!previousDescriptors.get(id).getString("kind").equals(kind)) {
                    throw new IllegalArgumentException("node kind changed");
                }
                JSONObject previous = previousDescriptors.get(id);
                if (created || previous == null || !previous.toString().equals(node.toString())) style(view, node);
                JSONArray frame = node.getJSONArray("frame");
                double parentX = 0, parentY = 0;
                if (!node.isNull("parent")) {
                    JSONArray parentFrame = descriptors.get(node.getString("parent")).getJSONArray("frame");
                    parentX = parentFrame.getDouble(0); parentY = parentFrame.getDouble(1);
                }
                int width = px(frame.getDouble(2)), height = px(frame.getDouble(3));
                int left = px(frame.getDouble(0) - parentX), top = px(frame.getDouble(1) - parentY);
                if (view instanceof NativeDialog) {
                    NativeDialog dialog = (NativeDialog) view;
                    dialog.width = width; dialog.height = height;
                    dialog.content.setBackgroundColor(color(node.getJSONObject("style").getJSONArray("background")));
                    setPaneTitle(dialog.content, node.optString("name", "Dialog"));
                    BigInteger revision = new BigInteger(node.getString("editRevision"));
                    if (revision.compareTo(dialog.revision) > 0) dialog.revision = revision;
                    width = 0; height = 0; left = 0; top = 0;
                }
                if (view instanceof NativePopover) {
                    NativePopover popover = (NativePopover) view;
                    popover.width = width; popover.height = height;
                    popover.content.setBackgroundColor(color(node.getJSONObject("style").getJSONArray("background")));
                    setPaneTitle(popover.content, node.optString("name", "Popover"));
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
                if (restoreFocus) { requestedFocus = id; replacedFocus = id; }
            }
    }

    private void reconcileHierarchy(JSONArray nodes) throws JSONException {
        HashMap<ViewGroup, Integer> indices = new HashMap<>();
        for (int index = 0; index < nodes.length(); index++) {
            JSONObject node = nodes.getJSONObject(index);
            View view = views.get(node.getString("id"));
            if(view==null)continue;
            ViewGroup parent = node.isNull("parent") ? this : (ViewGroup) views.get(node.getString("parent"));
            if (parent instanceof NativeScroll) parent=((NativeScroll)parent).document;
            if (parent instanceof NativeHosted) parent=((NativeHosted)parent).content;
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
        if (replacedFocus != null) {
            View target = views.get(replacedFocus);
            replacedFocus = null;
            if (target != null) target.requestFocus();
        }
        performance.updateAccessibility();
    }

    private final Runnable reactiveWake = () -> { if (root != null) scheduleRootUpdate(); };

    public void scheduleReactiveWake(String delay) {
        uiThread();
        removeCallbacks(reactiveWake);
        if (!"null".equals(delay)) postDelayed(reactiveWake, Math.max(1, Long.parseLong(delay)));
    }

    public void clear(String ignored) {
        removeCallbacks(reactiveWake);
        uiThread();
        performance.bind(null);
        windows.clear();
        publishing = true;
        try {
            interactions.close(); keyboard.clear();
            if (services != null) { services.close(); services = null; }
            deepLinks.clear();
            for (View view : views.values()) disposeView(view);
            recycled.clear(); recycledCount = 0;
            orderedNodes=new JSONArray();nodePositions.clear();contentParents.clear();requestedFocus=null;replacedFocus=null;
            removeCallbacks(webPump); webPumpScheduled = false;
            removeCallbacks(servicePump); servicePumpScheduled = false; webSession = null; webResults.clear();
            if (quitPoll != null) { removeCallbacks(quitPoll); quitPoll = null; }
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
        applyWindowInsets(getRootWindowInsets());
        super.onLayout(changed, left, top, right, bottom);
        performance.updateAccessibility();
        if (changed) schedulePopovers();
    }

    private void scheduleResize() {
        environment++;
        scheduleRootUpdate();
    }

    private void scheduleRootUpdate() {
        if (!resizing) {
            resizing = true;
            postOnAnimation(() -> {
                resizing = false;
                int[] location = new int[2];
                getLocationOnScreen(location);
                int height = Math.max(0, getHeight() - getPaddingTop() - getPaddingBottom());
                height = Math.min(height, Math.max(0, modalViewportBottom - location[1] - getPaddingTop()));
                if (root != null) {
                    String state = nativeResize(root, Math.max(0, getWidth() - getPaddingLeft() - getPaddingRight()) / density,
                            height / density, environment);
                    if (stateListener != null && state != null) stateListener.accept(state);
                }
            });
        }
    }

    @Override public void close() {
        uiThread();
        if (removeBackHandler != null) { removeBackHandler.run(); removeBackHandler = null; }
        windows.close();
        preferences.close();
        stateListener = null;
        serviceListener = null;
        if (root != null) { String previous = root; root = null; nativeDispose(previous); }
        clear("{}");
        resources.close();
        resourcesClosed = true;
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

    private final class NativePopover extends NativeFrame {
        final NativeEventFrame content;
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
            content = new NativeEventFrame(NativeView.this) {
                @Override public boolean dispatchKeyEvent(KeyEvent event) {
                    if (super.dispatchKeyEvent(event)) return true;
                    int key = event.getKeyCode();
                    if (key != KeyEvent.KEYCODE_BACK && key != KeyEvent.KEYCODE_ESCAPE) return false;
                    JSONObject node = descriptors.get(NativePopover.this.getTag());
                    JSONObject style = node == null ? null : node.optJSONObject("style");
                    if (key == KeyEvent.KEYCODE_ESCAPE && style != null && !style.optBoolean("popover_close_on_escape", true)) return false;
                    if (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0 && !event.isCanceled()) requestDismiss(false);
                    return true;
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
            JSONObject style = node.optJSONObject("style");
            int inset = px(style == null ? 8 : style.optInt("popover_margin", 8)), gap = px(node.optDouble("gap"));
            int panelWidth = Math.min(width, Math.max(0, viewport.width() - inset*2));
            int panelHeight = Math.min(height, Math.max(0, viewport.height() - inset*2));
            android.graphics.Rect visibleAnchor = new android.graphics.Rect();
            if (!anchor.getGlobalVisibleRect(visibleAnchor) || !android.graphics.Rect.intersects(viewport, visibleAnchor)) return null;
            int[] location = new int[2];
            anchor.getLocationOnScreen(location);
            String placement = node.optString("placement");
            String[] parts = placement.split("-");
            String side = parts[0], align = parts.length > 1 ? parts[1] : "center";
            boolean vertical = "top".equals(side) || "bottom".equals(side);
            if (vertical && anchor.getLayoutDirection() == View.LAYOUT_DIRECTION_RTL) {
                if (align.equals("start")) align = "end"; else if (align.equals("end")) align = "start";
            }
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
            if (!window.isShowing()) return;
            if (bounds == null) { requestDismiss(false); return; }
            window.update(bounds[0], bounds[1], bounds[2], bounds[3]);
        }

        void show() {
            int[] bounds = geometry();
            if (bounds == null || bounds[2] == 0 || bounds[3] == 0) return;
            if (window.isShowing()) { position(); return; }
            JSONObject node = descriptors.get(getTag());
            JSONObject style = node == null ? null : node.optJSONObject("style");
            boolean autofocus = style == null || style.optBoolean("popover_focus_on_open", true);
            window.setFocusable(autofocus);
            window.setWidth(bounds[2]); window.setHeight(bounds[3]);
            window.showAtLocation(NativeView.this, Gravity.TOP | Gravity.LEFT, bounds[0], bounds[1]);
            if (!autofocus) return;
            ArrayList<View> focusable = new ArrayList<>();
            content.addFocusables(focusable, View.FOCUS_FORWARD, View.FOCUSABLES_ALL);
            for (View view : focusable) if (view != content && view.isEnabled() && view.isShown() && view.requestFocus()) return;
            content.requestFocus();
        }

        void hide(boolean restore) {
            content.cancelInput();
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

    private final class NativeDialog extends NativeFrame {
        final NativeEventFrame content;
        final Dialog window;
        BigInteger revision = BigInteger.ZERO;
        int width, height;
        View previousFocus;
        Runnable removeBackHandler;

        NativeDialog(Context context) {
            super(context);
            setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
            content = new NativeEventFrame(NativeView.this);
            content.setClipChildren(true);
            window = new Dialog(context) {
                @Override public void onBackPressed() { requestDismiss(false); }
                @Override public boolean dispatchKeyEvent(KeyEvent event) {
                    if (super.dispatchKeyEvent(event)) return true;
                    if (event.getKeyCode() != KeyEvent.KEYCODE_ESCAPE) return false;
                    if (event.getAction() == KeyEvent.ACTION_DOWN && event.getRepeatCount() == 0 && !event.isCanceled()) requestDismiss(false);
                    return true;
                }
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
                removeBackHandler = NativeBack.install(window, () -> requestDismiss(false));
            }
            window.getWindow().setGravity(Gravity.CENTER);
            window.getWindow().setLayout(width, height);
        }

        void hide() {
            content.cancelInput();
            if (removeBackHandler != null) { removeBackHandler.run(); removeBackHandler = null; }
            if (!window.isShowing()) return;
            window.dismiss();
            if (modalViewportBottom != Integer.MAX_VALUE) { modalViewportBottom = Integer.MAX_VALUE; scheduleResize(); }
            if (previousFocus != null && previousFocus.isAttachedToWindow()) previousFocus.requestFocus();
            previousFocus = null;
        }
    }

    private final class NativeInput extends EditText {
        private static final int MAX_TEXT_LENGTH = 1024 * 1024;
        private static final int MAX_AUTOFILL_TEXT_LENGTH = 16 * 1024;
        BigInteger revision = BigInteger.ZERO;
        int imeDepth;
        boolean editPending;
        boolean suppressRejectedEdit;
        NativeInput(Context context) {
            super(context);
            NativeSelection.install(this);
            setSingleLine(true);
            setImeOptions(EditorInfo.IME_ACTION_DONE);
            setFilters(new android.text.InputFilter[] {(source, start, end, dest, dstart, dend) -> {
                if (dest.length() - (dend - dstart) + (end - start) <= MAX_TEXT_LENGTH) return null;
                rejectOversizedText();
                return dest.subSequence(dstart,dend);
            }});
            setOnEditorActionListener((view, action, event) -> {
                JSONObject descriptor = descriptors.get(getTag());
                if (descriptor == null || !"input".equals(descriptor.optString("kind"))) return false;
                if (event != null && (event.getKeyCode() != KeyEvent.KEYCODE_ENTER || event.getAction() != KeyEvent.ACTION_DOWN
                        || event.getRepeatCount() != 0 || event.isCtrlPressed() || event.isAltPressed() || event.isMetaPressed())) return true;
                if (event == null && action != EditorInfo.IME_ACTION_DONE && action != EditorInfo.IME_ACTION_GO
                        && action != EditorInfo.IME_ACTION_SEARCH && action != EditorInfo.IME_ACTION_SEND) return false;
                Editable text = getText();
                if (publishing || !isEnabled() || imeDepth != 0 || root == null) return true;
                BaseInputConnection.removeComposingSpans(text);
                scheduleEdit();
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
                public void afterTextChanged(Editable text) {
                if (!publishing && imeDepth == 0 && !suppressRejectedEdit) scheduleEdit();
                }
            });
        }
        @Override public void setText(CharSequence text, TextView.BufferType type) {
            if (text != null && text.length() > MAX_TEXT_LENGTH) {
                rejectOversizedText();
                return;
            }
            super.setText(text, type);
        }
        @Override public void onProvideAutofillStructure(android.view.ViewStructure structure, int flags) {
            super.onProvideAutofillStructure(structure, flags);
            CharSequence text = structure.getText();
            if (text != null && text.length() > MAX_AUTOFILL_TEXT_LENGTH) structure.setText(null);
        }
        @Override public void autofill(android.view.autofill.AutofillValue value) {
            CharSequence text = value != null && value.isText() ? value.getTextValue() : null;
            if (text != null && text.length() > MAX_AUTOFILL_TEXT_LENGTH) {
                rejectOversizedText();
                return;
            }
            super.autofill(value);
        }
        private void rejectOversizedText() {
            setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO);
            suppressRejectedEdit = true;
            post(() -> suppressRejectedEdit = false);
            lastError = "Input exceeds the Android autofill-safe text limit";
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
            if (!publishing && imeDepth == 0 && !suppressRejectedEdit) scheduleEdit();
        }
        void edited() {
            if (publishing || getTag() == null || root == null) return;
            BigInteger next=revision.add(BigInteger.ONE);
            if (next.bitLength() > 64) return;
            revision = next;
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
