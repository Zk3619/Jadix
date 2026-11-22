package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.Button;
import android.widget.EditText;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

/** Exercises the shared SDK and QuickJS application through the native Android host. */
public final class NativeShellChecks extends Instrumentation {
    private String token;
    private MainActivity activity;
    private static final String VALUE = "中文😀 العربية";
    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        token = arguments == null ? null : arguments.getString("jadix.token");
        start();
    }
    private void require(boolean value, String message) {
        if (!value) throw new AssertionError(message);
    }
    private void ui(Runnable action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
        waitForIdleSync();
    }
    private View find(View view, boolean editor) {
        if (editor && view instanceof EditText && "中文 😀 العربية".contentEquals(((EditText)view).getHint())) return view;
        if (!editor && view instanceof Button && ((Button)view).getText().toString().startsWith("Count ")) return view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup)view;
            for (int i = 0; i < group.getChildCount(); i++) {
                View found = find(group.getChildAt(i), editor);
                if (found != null) return found;
            }
        }
        return null;
    }
    private void ready() {
        long deadline = SystemClock.uptimeMillis() + 15000;
        while (true) {
            boolean[] ready = {false};
            ui(() -> ready[0] = activity.content.hasWindowFocus() && find(activity.content, true) != null && find(activity.content, false) != null);
            if (ready[0]) return;
            require(SystemClock.uptimeMillis() < deadline, "Shell controls did not mount");
            SystemClock.sleep(20);
        }
    }
    private void state(int count) {
        try {
            JSONObject state = new JSONObject(activity.content.state());
            require(state.getInt("version") == 1 && state.getInt("count") == count && VALUE.equals(state.getString("value")), "Shared Shell checkpoint differs: " + state);
        } catch (org.json.JSONException error) { throw new AssertionError(error); }
    }
    @Override public void onStart() {
        Bundle result = new Bundle(); boolean passed = false;
        try {
            require(token != null && UUID.fromString(token).toString().equals(token), "Fresh UUID token required");
            activity = (MainActivity)startActivitySync(new Intent(getTargetContext(), MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            ready();
            EditText[] editor = {null};
            ui(() -> {
                editor[0] = (EditText)find(activity.content, true);
                editor[0].setShowSoftInputOnFocus(false);
                require(editor[0].requestFocus(), "Shell editor focus failed");
                InputConnection connection = editor[0].onCreateInputConnection(new EditorInfo());
                require(connection != null && connection.commitText(VALUE, 1), "Unicode input connection failed");
            });
            ui(() -> state(0));
            for (int count = 1; count <= 20; count++) {
                final int expected = count;
                ui(() -> require(find(activity.content, false).performClick(), "Shell activation failed"));
                ui(() -> {
                    state(expected);
                    require(find(activity.content, true) == editor[0], "Keyed reorder replaced the native editor");
                    require(VALUE.contentEquals(editor[0].getText()) && editor[0].getSelectionStart() == VALUE.length(), "Keyed reorder lost text/selection");
                    require(((Button)find(activity.content, false)).getText().toString().equals("Count " + expected), "SDK render did not publish counter");
                });
            }
            ActivityMonitor monitor = addMonitor(MainActivity.class.getName(), null, false);
            try {
                ui(() -> activity.recreate());
                Activity next = monitor.waitForActivityWithTimeout(10000);
                require(next instanceof MainActivity, "Activity recreation failed");
                activity = (MainActivity)next;
            } finally { removeMonitor(monitor); }
            ready();
            ui(() -> { state(20); require(VALUE.contentEquals(((EditText)find(activity.content, true)).getText()), "Recreated Shell editor lost checkpoint"); });
            result.putString("jadix_shell", new JSONObject().put("schema", 1).put("token", token).put("passed", true)
                .put("api", android.os.Build.VERSION.SDK_INT).put("activations", 20).put("unicode", VALUE)
                .put("keyed_identity_retained", true).put("activity_checkpoint_restored", true).toString());
            result.putString("stream", "JADIX_NATIVE_SHELL_PASS"); passed = true;
        } catch (Throwable error) { result.putString("stream", "JADIX_NATIVE_SHELL_FAIL: " + android.util.Log.getStackTraceString(error)); }
        finally {
            try { ui(() -> { if (activity != null) activity.finish(); }); }
            catch (Throwable error) { passed = false; result.remove("jadix_shell"); result.putString("stream", "JADIX_NATIVE_SHELL_FAIL: cleanup: " + error); }
        }
        finish(passed ? Activity.RESULT_OK : Activity.RESULT_CANCELED, result);
    }
}
