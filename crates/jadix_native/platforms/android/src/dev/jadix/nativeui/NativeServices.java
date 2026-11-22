package dev.jadix.nativeui;

import android.os.Handler;
import android.os.Looper;
import java.util.ArrayDeque;
import java.util.concurrent.CountDownLatch;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Root-owned adapters for the shared service protocol. Never calls JNI inside a command. */
final class NativeServices implements AutoCloseable {
    final NativeView root;
    final Handler main = new Handler(Looper.getMainLooper());
    final NativeFetch fetch;
    final NativeSpeech speech;
    final NativeAudio audio;
    final NativeSystem system;
    final NativeShare share;
    private final ArrayDeque<JSONObject> events = new ArrayDeque<>();
    private final ArrayDeque<CountDownLatch> acknowledgements = new ArrayDeque<>();
    private int eventBytes;
    private boolean scheduled;
    private boolean failed;
    private NativePermissions permissions;
    volatile boolean closed;
    NativeServices(NativeView root) {
        this.root = root; fetch = new NativeFetch(this); speech = new NativeSpeech(this);
        audio = new NativeAudio(this); system = new NativeSystem(this); share = new NativeShare(this);
    }
    NativePermissions activityBridge() { if (permissions == null) permissions = NativePermissions.attach(root); return permissions; }
    void permission(String permission, java.util.function.Consumer<Boolean> completion) {
        permission(permission, (granted, error) -> completion.accept(granted));
    }
    void permission(String permission, java.util.function.BiConsumer<Boolean, String> completion) {
        if (closed) return;
        final boolean granted;
        final NativePermissions bridge;
        try {
            granted = root.getContext().checkSelfPermission(permission) == android.content.pm.PackageManager.PERMISSION_GRANTED;
            bridge = granted ? null : activityBridge();
        } catch (SecurityException error) { completion.accept(false, "denied"); return; }
        catch (RuntimeException error) { completion.accept(false, "failed"); return; }
        if (granted) completion.accept(true, null);
        else if (bridge == null) completion.accept(false, "unavailable");
        else bridge.request(permission, completion);
    }
    static JSONObject object(Object... entries) {
        JSONObject value = new JSONObject();
        try { for (int i = 0; i < entries.length; i += 2) value.put((String) entries[i], entries[i + 1] == null ? JSONObject.NULL : entries[i + 1]); }
        catch (JSONException error) { throw new IllegalArgumentException(error); }
        return value;
    }
    void command(JSONObject value) throws JSONException {
        if (closed) throw new IllegalStateException("Native services disposed");
        JSONObject command = value.getJSONObject("command");
        switch (value.getString("service")) {
            case "fetch": fetch.command(command); break;
            case "speech": speech.command(command); break;
            case "audio": audio.command(command); break;
            case "desktop": system.command(command); break;
            case "share": share.command(command); break;
            default: throw new IllegalArgumentException("Unknown native service");
        }
    }
    void event(String service, int id, JSONObject event) { enqueue(object("service", service, "id", Integer.toUnsignedLong(id), "event", event), null); }
    void activation(JSONObject event) { enqueue(object("service", "activation", "event", event), null); }
    void shareResult(String id, JSONObject outcome, String error) { enqueue(object("service", "share", "id", id, "outcome", outcome, "error", error), null); }
    // HTTP producers wait for each chunk to enter Rust, keeping pending data bounded.
    void stream(int id, JSONObject event) throws InterruptedException {
        CountDownLatch acknowledged = new CountDownLatch(1);
        enqueue(object("service", "fetch", "id", Integer.toUnsignedLong(id), "event", event), acknowledged);
        acknowledged.await();
    }
    private void enqueue(JSONObject value, CountDownLatch acknowledged) {
        synchronized (events) {
            if (closed || failed) { if (acknowledged != null) acknowledged.countDown(); return; }
            int bytes = value.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length;
            if (acknowledged == null && replaceable(value)) {
                java.util.Iterator<JSONObject> pending = events.descendingIterator();
                while (pending.hasNext()) {
                    JSONObject previous = pending.next();
                    if (!previous.optString("service").equals(value.optString("service")) || !previous.optString("id").equals(value.optString("id"))) continue;
                    // A terminal or state transition for this request is an ordering barrier.
                    if (replaceable(previous)) {
                        eventBytes -= previous.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8).length;
                        pending.remove();
                    }
                    break;
                }
            }
            while (acknowledged != null && bytes <= 1024 * 1024 && (events.size() >= 128 || eventBytes + bytes > 1024 * 1024) && Looper.myLooper() != Looper.getMainLooper()) {
                try { events.wait(); }
                catch (InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    if (acknowledged != null) acknowledged.countDown();
                    return;
                }
                if (closed || failed) { if (acknowledged != null) acknowledged.countDown(); return; }
            }
            if (bytes > 1024 * 1024 || events.size() >= 128 || eventBytes + bytes > 1024 * 1024) {
                // Never block the UI or call JNI while Rust is issuing a command. Closing
                // the root on the next turn cancels every outstanding shared service job.
                failed = true;
                if (acknowledged != null) acknowledged.countDown();
                events.notifyAll();
                main.post(() -> root.serviceFailure("Native callback budget exceeded"));
                return;
            }
            events.add(value); eventBytes += bytes;
            if (acknowledged != null) acknowledgements.add(acknowledged);
            if (!scheduled) { scheduled = true; main.post(this::flush); }
        }
    }
    private static boolean replaceable(JSONObject value) {
        JSONObject event = value.optJSONObject("event");
        if (event == null) return false;
        String service = value.optString("service"), type = event.optString("type");
        return service.equals("audio") && type.equals("progress")
            || service.equals("speech") && type.equals("result") && !event.optBoolean("is_final");
    }
    private void flush() {
        JSONArray batch = new JSONArray(); ArrayDeque<CountDownLatch> acknowledged;
        synchronized (events) {
            scheduled = false;
            while (!events.isEmpty()) batch.put(events.remove());
            eventBytes = 0; acknowledged = new ArrayDeque<>(acknowledgements); acknowledgements.clear();
            events.notifyAll();
        }
        try { if (!closed && batch.length() != 0) root.deliverServiceEvents(batch.toString()); }
        finally { for (CountDownLatch latch : acknowledged) latch.countDown(); }
    }
    public void close() {
        if (closed) return;
        closed = true;
        fetch.close(); speech.close(); audio.close(); system.close(); share.close(); main.removeCallbacksAndMessages(null);
        if (permissions != null) { permissions.close(); permissions = null; }
        synchronized (events) {
            events.clear(); eventBytes = 0;
            for (CountDownLatch latch : acknowledgements) latch.countDown(); acknowledgements.clear();
            events.notifyAll();
        }
    }
}
