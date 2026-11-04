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
    private int keyEvents;
    private String phaseName;

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
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                View found = named(group.getChildAt(index), label);
                if (found != null) return found;
            }
        }
        return null;
    }

    private int rows(View view) {
        int count = view instanceof TextView && ((TextView) view).getText().toString().startsWith("Row ") ? 1 : 0;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) count += rows(group.getChildAt(index));
        }
        return count;
    }

    private void tab(boolean reverse) {
        long now = SystemClock.uptimeMillis();
        int modifiers = reverse ? KeyEvent.META_SHIFT_ON | KeyEvent.META_SHIFT_LEFT_ON : 0;
        sendKeySync(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_TAB, 0, modifiers));
        sendKeySync(new KeyEvent(now, SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, KeyEvent.KEYCODE_TAB, 0, modifiers));
        keyEvents += 2;
        waitForIdleSync();
    }

    private JSONObject focused(MainActivity activity, View expected, String label) throws Exception {
        long deadline = SystemClock.uptimeMillis() + 5000;
        JSONObject[] result = new JSONObject[1];
        String[] diagnostic = new String[1];
        long stableSince = 0;
        while (SystemClock.uptimeMillis() < deadline) {
            result[0] = null;
            ui(() -> {
                View actual = activity.getWindow().getDecorView().findFocus();
                Rect fullDiagnostic = new Rect(), visibleDiagnostic = new Rect();
                expected.getGlobalVisibleRect(visibleDiagnostic);
                int[] diagnosticLocation = new int[2];
                expected.getLocationOnScreen(diagnosticLocation);
                fullDiagnostic.set(diagnosticLocation[0], diagnosticLocation[1], diagnosticLocation[0] + expected.getWidth(), diagnosticLocation[1] + expected.getHeight());
                diagnostic[0] = "phase=" + phaseName + "; same=" + (actual == expected) + "; window=" + expected.hasWindowFocus() + "; attached=" + expected.isAttachedToWindow() + "; actual=" + (actual == null ? "null" : actual.getClass().getSimpleName() + ":" + (actual instanceof EditText ? ((EditText) actual).getHint() : actual instanceof Button ? ((Button) actual).getText() : actual.getTag())) + "; full=" + fullDiagnostic + "; visible=" + visibleDiagnostic;
                if (activity.getWindow().getDecorView().findFocus() != expected || !expected.hasWindowFocus()) return;
                Rect full = new Rect(), visible = new Rect();
                full.set(0, 0, expected.getWidth(), expected.getHeight());
                int[] location = new int[2];
                expected.getLocationOnScreen(location);
                full.offset(location[0], location[1]);
                if (!expected.getGlobalVisibleRect(visible) || !full.equals(visible)) return;
                try {
                    result[0] = new JSONObject().put("control", label).put("fully_visible", true)
                        .put("content_scroll", new JSONArray(new int[] {expected.getScrollX(), expected.getScrollY()}))
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
        throw new AssertionError("Keyboard focus/order or complete visibility failed at " + label + "; " + diagnostic[0]);
    }

    private void phase(MainActivity activity, String name) throws Exception {
        phaseName = name;
        String[] labels = {"Accept", "Airplane mode", "Password / 密码", "Multiline / 多行", "Open editor", "Continue", "Name / 中文", "Reset"};
        ArrayList<View> controls = new ArrayList<>();
        String[] before = new String[1];
        ui(() -> {
            require(rows(activity.content) == expectedRows, "Unexpected retained row count");
            before[0] = activity.content.state();
            for (String label : labels) {
                View control = named(activity.content, label);
                require(control != null && control.isEnabled(), "Missing enabled native control: " + label);
                if (control instanceof EditText) ((EditText) control).setShowSoftInputOnFocus(false);
                controls.add(control);
            }
            ((android.view.inputmethod.InputMethodManager) activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.content.getWindowToken(), 0);
        });
        setInTouchMode(false);
        sendKeyDownUpSync(KeyEvent.KEYCODE_TAB);
        waitForIdleSync();
        ui(() -> require(controls.get(0).requestFocus(), "Initial keyboard focus request failed; focusable=" + controls.get(0).isFocusable() + "; touch=" + controls.get(0).isInTouchMode() + "; visible=" + controls.get(0).isShown()));
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
            require(activity.content.takeError() == null, "Native navigation callback error");
            for (int index = 0; index < labels.length; index++) require(named(activity.content, labels[index]) == controls.get(index), "Navigation replaced a native control");
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
