package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Fragment;
import android.content.Context;
import android.content.ContextWrapper;
import android.content.pm.PackageManager;
import java.util.ArrayDeque;
import java.util.function.Consumer;

/** A host-independent permission result bridge, scoped to one root and UI thread. */
public final class NativePermissions extends Fragment {
    private final ArrayDeque<Request> queue = new ArrayDeque<>();
    private Request active;
    private boolean closed;
    private static final int REQUEST = 0x4a44;
    private final java.util.HashMap<Integer, Consumer<android.content.Intent>> activityResults = new java.util.HashMap<>();
    private int nextActivityRequest = REQUEST + 1;
    private Runnable returned;
    private boolean away;
    void onReturn(Runnable callback) { returned = callback; away = false; }
    int launch(android.content.Intent intent, Consumer<android.content.Intent> completion) {
        if (closed || getActivity() == null) throw new IllegalStateException("Native activity unavailable");
        if (nextActivityRequest > 65535 || activityResults.size() >= 64) throw new IllegalStateException("Native activity request budget exceeded");
        int request = nextActivityRequest++;
        activityResults.put(request, completion);
        try { startActivityForResult(intent, request); return request; }
        catch (RuntimeException error) { activityResults.remove(request); throw error; }
    }
    void cancelActivity(int request) { activityResults.remove(request); }
    @Override public void onActivityResult(int request, int result, android.content.Intent data) {
        super.onActivityResult(request, result, data);
        Consumer<android.content.Intent> completion = activityResults.remove(request);
        if (completion != null) completion.accept(result == Activity.RESULT_OK ? data : null);
    }
    @Override public void onPause() { super.onPause(); if (returned != null) away = true; }
    @Override public void onResume() {
        super.onResume();
        if (away && returned != null) { Runnable callback = returned; away = false; new android.os.Handler(android.os.Looper.getMainLooper()).postDelayed(() -> { if (!closed && returned == callback) { returned = null; callback.run(); } }, 300); }
    }
    private static final class Request {
        final String permission; final java.util.function.BiConsumer<Boolean, String> completion;
        Request(String permission, java.util.function.BiConsumer<Boolean, String> completion) { this.permission = permission; this.completion = completion; }
    }
    public NativePermissions() {}
    static Activity activity(Context context) {
        for (int i = 0; context != null && i < 16; i++) {
            if (context instanceof Activity) return (Activity) context;
            if (!(context instanceof ContextWrapper)) break;
            Context next = ((ContextWrapper) context).getBaseContext(); if (next == context) break; context = next;
        }
        return null;
    }
    static NativePermissions attach(NativeView root) {
        Activity activity = activity(root.getContext());
        if (activity == null || activity.isFinishing() || activity.isDestroyed() || activity.getFragmentManager().isStateSaved()) return null;
        NativePermissions fragment = new NativePermissions();
        activity.getFragmentManager().beginTransaction().add(fragment, "jadix-permissions-" + System.identityHashCode(root)).commit();
        activity.getFragmentManager().executePendingTransactions(); return fragment;
    }
    void request(String permission, java.util.function.BiConsumer<Boolean, String> completion) {
        if (closed || getActivity() == null) { completion.accept(false, "unavailable"); return; }
        final boolean granted;
        try { granted = getActivity().checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED; }
        catch (SecurityException error) { completion.accept(false, "denied"); return; }
        catch (RuntimeException error) { completion.accept(false, "failed"); return; }
        if (granted) { completion.accept(true, null); return; }
        if (queue.size() >= 64) { completion.accept(false, "conflict"); return; }
        queue.add(new Request(permission, completion)); next();
    }
    private void next() {
        if (closed || active != null || queue.isEmpty()) return;
        active = queue.remove();
        try { requestPermissions(new String[] {active.permission}, REQUEST); }
        catch (SecurityException error) { finish(false, "denied"); }
        catch (RuntimeException error) { finish(false, "failed"); }
    }
    private void finish(boolean granted, String error) {
        Request previous = active; active = null;
        if (previous != null) previous.completion.accept(granted, error);
        next();
    }
    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request == REQUEST) finish(results.length == 1 && results[0] == PackageManager.PERMISSION_GRANTED, null);
    }
    void close() {
        closed = true; queue.clear(); active = null; activityResults.clear(); returned = null;
        if (isAdded()) getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
    }
    @Override public void onDestroy() { closed = true; returned = null; activityResults.clear(); finish(false, "unavailable"); while (!queue.isEmpty()) queue.remove().completion.accept(false, "unavailable"); super.onDestroy(); }
}
