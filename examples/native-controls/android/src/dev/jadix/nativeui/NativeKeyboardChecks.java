package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Rect;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.TextView;
import java.util.ArrayList;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

public final class NativeKeyboardChecks extends Instrumentation {
    private String token;
    private int expectedRows;
    private final JSONArray phases = new JSONArray();
    private final java.util.HashMap<String,View> retainedEditors = new java.util.HashMap<>();
    private int keyEvents;
    private String phaseName;
    private final ArrayList<String> focusTrace = new ArrayList<>();

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        token = arguments == null ? null : arguments.getString("jadix.token");
        expectedRows = arguments == null ? 0 : Integer.parseInt(arguments.getString("jadix.rows", "0"));
        start();
    }

    private void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private void ui(Runnable action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> {
            try { action.run(); } catch (Throwable error) { failure.set(error); }
        });
        if (failure.get() != null) throw new AssertionError(failure.get());
        waitForIdleSync();
    }

    private View named(View view, String label) {
        if (view instanceof EditText && label.contentEquals(((EditText) view).getHint())) return view;
        if (view instanceof Button && label.contentEquals(((Button) view).getText())) return view;
        if (view instanceof NativeContentButton && label.contentEquals(view.getContentDescription())) return view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                View found = named(group.getChildAt(index), label);
                if (found != null) return found;
            }
        }
        return null;
    }


    private void tab(boolean reverse) {
        long now = SystemClock.uptimeMillis();
        int modifiers = reverse ? KeyEvent.META_SHIFT_ON | KeyEvent.META_SHIFT_LEFT_ON : 0;
        sendKeySync(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_TAB, 0, modifiers));
        sendKeySync(new KeyEvent(now, SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, KeyEvent.KEYCODE_TAB, 0, modifiers));
        keyEvents += 2;
        waitForIdleSync();
    }

    private JSONObject focused(MainActivity activity, String id, String label) throws Exception {
        long deadline = SystemClock.uptimeMillis() + 5000;
        JSONObject[] result = new JSONObject[1];
        String[] diagnostic = new String[1];
        long stableSince = 0;
        while (SystemClock.uptimeMillis() < deadline) {
            result[0] = null;
            ui(() -> {
                View actual = activity.getWindow().getDecorView().findFocus();
                View current = activity.content.nodeView(id);
                if (current == null) { diagnostic[0] = "Logical focus target is not mounted: " + id; return; }
                if (retainedEditors.containsKey(id)) require(current == retainedEditors.get(id), "Navigation replaced a retained editor");
                Rect fullDiagnostic = new Rect(), visibleDiagnostic = new Rect();
                current.getGlobalVisibleRect(visibleDiagnostic);
                int[] diagnosticLocation = new int[2];
                current.getLocationOnScreen(diagnosticLocation);
                fullDiagnostic.set(diagnosticLocation[0], diagnosticLocation[1], diagnosticLocation[0] + current.getWidth(), diagnosticLocation[1] + current.getHeight());
                diagnostic[0] = "phase=" + phaseName + "; same=" + (actual == current) + "; window=" + current.hasWindowFocus() + "; attached=" + current.isAttachedToWindow() + "; actual=" + (actual == null ? "null" : actual.getClass().getSimpleName() + ":" + (actual instanceof EditText ? ((EditText) actual).getHint() : actual instanceof Button ? ((Button) actual).getText() : actual.getTag())) + "; full=" + fullDiagnostic + "; visible=" + visibleDiagnostic;
                if (activity.getWindow().getDecorView().findFocus() != current || !current.hasWindowFocus()) return;
                Rect full = new Rect(), visible = new Rect();
                full.set(0, 0, current.getWidth(), current.getHeight());
                int[] location = new int[2];
                current.getLocationOnScreen(location);
                full.offset(location[0], location[1]);
                if (!current.getGlobalVisibleRect(visible) || !full.equals(visible)) return;
                try {
                    result[0] = new JSONObject().put("control", label).put("fully_visible", true)
                        .put("content_scroll", new JSONArray(new int[] {current.getScrollX(), current.getScrollY()}))
                        .put("bounds", new JSONArray(new int[] {full.left, full.top, full.right, full.bottom}));
                } catch (Exception error) { throw new AssertionError(error); }
            });
            if (result[0] == null) stableSince = 0;
            else {
                if (stableSince == 0) stableSince = SystemClock.uptimeMillis();
                if (SystemClock.uptimeMillis() - stableSince >= 300) return result[0];
            }
            SystemClock.sleep(25);
        }
        throw new AssertionError("Keyboard focus/order or complete visibility failed at " + label + "; " + diagnostic[0] + "; transitions=" + focusTrace);
    }

    private void phase(MainActivity activity, String name) throws Exception {
        phaseName = name;
        String[] labels = {"Accept", "Airplane mode", "Password / 密码", "Multiline / 多行", "Open editor", "Continue", "Name / 中文", "Reset"};
        ArrayList<String> controls = new ArrayList<>();
        String[] before = new String[1];
        ui(() -> {
            NativeChecks.checkRows(activity.content, expectedRows);
            before[0] = activity.content.state();
            try {
                java.lang.reflect.Field nodesField = NativeView.class.getDeclaredField("orderedNodes");
                nodesField.setAccessible(true);
                JSONArray nodes = (JSONArray) nodesField.get(activity.content);
                for (String label : labels) {
                    JSONObject descriptor = null;
                    for (int index = 0; index < nodes.length(); index++) {
                        JSONObject node = nodes.getJSONObject(index);
                        if (label.equals(node.optString("name")) || label.equals(node.optString("placeholder")) || label.equals(node.optString("text"))) {
                            descriptor = node; break;
                        }
                    }
                    require(descriptor != null && descriptor.optBoolean("enabled"), "Missing enabled logical control: " + label);
                    String id = descriptor.getString("id");
                    View control = activity.content.nodeView(id);
                    if (control instanceof EditText) {
                        ((EditText) control).setShowSoftInputOnFocus(false);
                        retainedEditors.put(id, control);
                    }
                    controls.add(id);
                }
            } catch (Exception error) { throw new AssertionError(error); }
            ((android.view.inputmethod.InputMethodManager) activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.content.getWindowToken(), 0);
        });
        setInTouchMode(false);
        sendKeyDownUpSync(KeyEvent.KEYCODE_TAB);
        waitForIdleSync();
        ui(() -> require(activity.content.nodeView(controls.get(0)).requestFocus(), "Initial keyboard focus request failed; focusable=" + activity.content.nodeView(controls.get(0)).isFocusable() + "; touch=" + activity.content.nodeView(controls.get(0)).isInTouchMode() + "; visible=" + activity.content.nodeView(controls.get(0)).isShown()));
        JSONArray forward = new JSONArray(), backward = new JSONArray();
        forward.put(focused(activity, controls.get(0), labels[0]));
        for (int index = 1; index < controls.size(); index++) {
            tab(false);
            forward.put(focused(activity, controls.get(index), labels[index]));
        }
        backward.put(focused(activity, controls.get(controls.size() - 1), labels[labels.length - 1]));
        for (int index = controls.size() - 2; index >= 0; index--) {
            tab(true);
            backward.put(focused(activity, controls.get(index), labels[index]));
        }
        ui(() -> {
            require(before[0].equals(activity.content.state()), "Tab navigation changed Rust business state");
            String callbackError = activity.content.takeError();
            require(callbackError == null, "Native navigation callback error: " + callbackError);
            for (String id : controls) {
                require(activity.content.descriptor(id) != null, "Navigation removed a logical control");
                if (retainedEditors.containsKey(id)) require(activity.content.nodeView(id) == retainedEditors.get(id), "Navigation replaced a retained editor");
            }
        });
        phases.put(new JSONObject().put("name", name).put("forward", forward).put("backward", backward));
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        MainActivity activity = null;
        try {
            require(token != null && UUID.fromString(token).toString().equals(token), "Fresh UUID token required");
            require(expectedRows == 100 || expectedRows == 1000, "Expected row count required");
            activity = (MainActivity) startActivitySync(new Intent(getTargetContext(), MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            final MainActivity current = activity;
            waitForIdleSync();
            AtomicBoolean ready = new AtomicBoolean();
            long deadline = SystemClock.uptimeMillis() + 10000;
            while (!ready.get() && SystemClock.uptimeMillis() < deadline) {
                ui(() -> ready.set(current.content.hasWindowFocus() && current.content.getHeight() > 0));
                if (!ready.get()) SystemClock.sleep(25);
            }
            require(ready.get(), "Native window not ready");
            ui(() -> {
                current.content.getViewTreeObserver().addOnGlobalFocusChangeListener((oldFocus, newFocus) -> {
                    if (focusTrace.size() >= 16) focusTrace.remove(0);
                    String transition = phaseName + ":" + (oldFocus == null ? null : oldFocus.getTag()) + " -> " + (newFocus == null ? null : newFocus.getTag());
                    if (newFocus == null) transition += " " + android.util.Log.getStackTraceString(new Throwable("focus cleared"));
                    focusTrace.add(transition);
                });
                current.getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_ALT_FOCUSABLE_IM);
                current.getWindow().setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE | android.view.WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN);
                try {
                    JSONObject state = new JSONObject(current.content.state());
                    state.put("value", "键盘 العربية 😀");
                    current.content.restore(state.toString());
                } catch (Exception error) { throw new AssertionError(error); }
            });
            phase(current, "initial");
            float density = current.getResources().getDisplayMetrics().density;
            ui(() -> current.content.setLayoutParams(new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Math.round(360 * density))));
            phase(current, "compact-viewport");
            ui(() -> {
                for (int index = 0; index < 25; index++) {
                    current.content.close();
                    current.content.mount(0, "键盘 العربية 😀");
                }
            });
            phase(current, "after-25-remounts");
            android.graphics.Bitmap screenshot = getUiAutomation().takeScreenshot();
            require(screenshot != null, "Native keyboard screenshot unavailable");
            try (java.io.FileOutputStream stream = new java.io.FileOutputStream(new java.io.File(getTargetContext().getFilesDir(), "keyboard-navigation.png"))) {
                require(screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, stream), "Native screenshot encoding failed");
            } finally { screenshot.recycle(); }
            result.putString("jadix_keyboard", new JSONObject().put("schema", 1).put("token", token).put("passed", true)
                .put("retained_rows", expectedRows).put("controls", 8).put("key_events", keyEvents).put("remounts", 25)
                .put("initial_focus_key_events", 6)
                .put("event_authority", "Instrumentation-injected Tab/Shift+Tab; programmatic initial focus, viewport and remounts; soft keyboard suppressed")
                .put("phases", phases).toString());
            result.putString("stream", "JADIX_NATIVE_KEYBOARD_PASS");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            try {
                android.graphics.Bitmap screenshot = getUiAutomation().takeScreenshot();
                if (screenshot != null) {
                    try (java.io.FileOutputStream stream = new java.io.FileOutputStream(new java.io.File(getTargetContext().getFilesDir(), "keyboard-failure.png"))) {
                        screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, stream);
                    } finally { screenshot.recycle(); }
                }
            } catch (Throwable captureError) { error.addSuppressed(captureError); }
            result.putString("stream", "JADIX_NATIVE_KEYBOARD_FAIL: " + android.util.Log.getStackTraceString(error));
            finish(Activity.RESULT_CANCELED, result);
        } finally {
            if (activity != null) {
                final MainActivity current = activity;
                runOnMainSync(current::finish);
            }
        }
    }
}
