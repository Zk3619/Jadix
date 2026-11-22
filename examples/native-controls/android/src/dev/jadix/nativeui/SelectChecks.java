package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Bitmap;
import android.os.Bundle;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.accessibility.AccessibilityNodeInfo;
import android.widget.Button;
import android.widget.Spinner;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

public final class SelectChecks extends Instrumentation {
    private final ArrayList<String> checks = new ArrayList<>();
    private MainActivity activity;
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    private interface Action { void run() throws Exception; }
    private void ui(Action action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
    }
    private void require(boolean condition, String message) { if (!condition) throw new AssertionError(message); }
    private JSONObject state() throws Exception { return new JSONObject(activity.content.state()); }
    private Spinner select(View view, String name) {
        if (view instanceof Spinner && name.contentEquals(view.getContentDescription())) return (Spinner) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                Spinner found = select(group.getChildAt(index), name);
                if (found != null) return found;
            }
        }
        return null;
    }
    private Spinner choose() { return select(activity.content, "Choose fruit"); }
    private View button(View view, String label) {
        if (view instanceof Button && label.contentEquals(((Button) view).getText())) return (Button) view;
        if (view instanceof NativeContentButton && label.contentEquals(view.getContentDescription())) return view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                View found = button(group.getChildAt(index), label);
                if (found != null) return found;
            }
        }
        return null;
    }
    private AccessibilityNodeInfo popupItem(String label) throws Exception {
        for (int attempt = 0; attempt < 40; attempt++) {
            AccessibilityNodeInfo root = getUiAutomation().getRootInActiveWindow();
            if (root != null) for (AccessibilityNodeInfo node : root.findAccessibilityNodeInfosByText(label)) {
                if (label.contentEquals(node.getText())) return node;
            }
            Thread.sleep(50);
        }
        throw new AssertionError("native popup option missing: " + label);
    }
    private void selected(String value, int position) {
        for (int attempt = 0; attempt < 60; attempt++) {
            waitForIdleSync();
            AtomicReference<Boolean> ready = new AtomicReference<>(false);
            ui(() -> ready.set(value.equals(state().getString("value")) && choose().getSelectedItemPosition() == position));
            if (ready.get()) break;
            android.os.SystemClock.sleep(50);
        }
        ui(() -> {
            require(value.equals(state().getString("value")), "native selection reached Rust: expected=" + value + " actual=" + state());
            require(choose().getSelectedItemPosition() == position, "published native selection retained");
        });
    }
    private boolean hasWindow(int id) {
        for (android.view.accessibility.AccessibilityWindowInfo window : getUiAutomation().getWindows()) {
            if (window.getId() == id) return true;
        }
        return false;
    }
    private void screenshot(String name) throws Exception {
        Bitmap bitmap = getUiAutomation().takeScreenshot();
        require(bitmap != null, "native screenshot available");
        try (FileOutputStream file = new FileOutputStream(new File(activity.getFilesDir(), name))) {
            require(bitmap.compress(Bitmap.CompressFormat.PNG, 100, file), "native screenshot encoded");
        } finally { bitmap.recycle(); }
    }
    private void clickOption(String label) throws Exception {
        AccessibilityNodeInfo option = popupItem(label);
        require(option.isEnabled(), "native popup option is enabled");
        android.graphics.Rect bounds = new android.graphics.Rect();
        option.getBoundsInScreen(bounds);
        require(!bounds.isEmpty(), "native popup option has visible bounds");
        long time = android.os.SystemClock.uptimeMillis();
        android.view.MotionEvent down = android.view.MotionEvent.obtain(time, time, android.view.MotionEvent.ACTION_DOWN,
                bounds.exactCenterX(), bounds.exactCenterY(), 0);
        android.view.MotionEvent up = android.view.MotionEvent.obtain(time, time + 20, android.view.MotionEvent.ACTION_UP,
                bounds.exactCenterX(), bounds.exactCenterY(), 0);
        try { sendPointerSync(down); sendPointerSync(up); }
        finally { down.recycle(); up.recycle(); }
    }
    @Override public void onStart() {
        Bundle result = new Bundle();
        try {
            android.accessibilityservice.AccessibilityServiceInfo service = getUiAutomation().getServiceInfo();
            service.flags |= android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS;
            getUiAutomation().setServiceInfo(service);
            activity = (MainActivity) startActivitySync(new Intent(getTargetContext(), MainActivity.class)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK));
            waitForIdleSync();
            AtomicReference<Spinner> original = new AtomicReference<>();
            AtomicReference<Object> adapter = new AtomicReference<>();
            AtomicReference<String> oldRoot = new AtomicReference<>();
            ui(() -> {
                Spinner choose = choose(), disabled = select(activity.content, "Disabled fruit");
                require(choose != null && disabled != null, "real android.widget.Spinner instances");
                require(choose.getAdapter().getCount() == 4, "placeholder and three native options");
                android.widget.ListAdapter options = (android.widget.ListAdapter) choose.getAdapter();
                require(!options.areAllItemsEnabled() && !options.isEnabled(0)
                        && !options.isEnabled(3), "native adapter disables prompt and option");
                android.widget.TextView enabledRow = (android.widget.TextView) choose.getAdapter().getDropDownView(1, null, choose);
                android.widget.TextView disabledRow = (android.widget.TextView) choose.getAdapter().getDropDownView(3, null, choose);
                require(disabledRow.getText().toString().equals("Disabled option") && !disabledRow.isEnabled(), "disabled option has native disabled state");
                require(android.graphics.Color.alpha(disabledRow.getCurrentTextColor()) < android.graphics.Color.alpha(enabledRow.getCurrentTextColor()),
                        "disabled option uses native theme disabled text alpha");
                require(!disabled.isEnabled() && disabled.getSelectedItemPosition() == 1, "disabled native control");
                require(state().getString("value").isEmpty() && activity.content.takeError() == null, "initial publication emits no selection");
                original.set(choose); adapter.set(choose.getAdapter()); oldRoot.set(activity.content.token());
            });
            checks.add("native Spinner, four choices, disabled option/control and event-free initial publication");
            ui(() -> require(choose().performClick(), "native dropdown opens"));
            waitForIdleSync();
            require(!popupItem("Disabled option").isEnabled(), "native popup exposes disabled option");
            screenshot("select-popup.png");
            clickOption("Apple / 苹果");
            selected("apple", 1);
            checks.add("real native popup Unicode option activation reaches Rust; disabled option is not enabled");
            ui(() -> require(choose().requestFocusFromTouch(), "native Spinner takes keyboard focus"));
            sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_CENTER);
            waitForIdleSync();
            popupItem("Banana / 香蕉");
            sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_DOWN);
            sendKeyDownUpSync(KeyEvent.KEYCODE_ENTER);
            selected("banana", 2);
            ui(() -> require(choose().requestFocusFromTouch(), "retained Spinner takes keyboard focus"));
            sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_CENTER);
            waitForIdleSync();
            popupItem("Banana / 香蕉");
            sendKeyDownUpSync(KeyEvent.KEYCODE_DPAD_DOWN);
            sendKeyDownUpSync(KeyEvent.KEYCODE_ENTER);
            selected("banana", 2);
            checks.add("injected native keyboard selects next option and skips disabled final option");
            ui(() -> {
                button(activity.content, "Continue").performClick();
                float density = activity.getResources().getDisplayMetrics().density;
                NativeView.nativeResize(activity.content.token(), activity.content.getWidth() / density,
                        activity.content.getHeight() / density, 1000);
                require(choose() == original.get() && choose().getAdapter() == adapter.get(), "unrelated publication retains Spinner and adapter identity");
                require(state().getInt("count") == 1, "unrelated callback reaches Rust");
            });
            selected("banana", 2);
            ui(() -> require(activity.content.takeError() == null, "publication does not synthesize selection events"));
            checks.add("unrelated counter and environment publication retain native control, adapter and selection");
            ui(() -> choose().setSelection(3));
            selected("banana", 2);
            ui(() -> {
                require(activity.content.takeError() != null, "disabled-option rejection returned through callback");
                require(activity.content.takeError() == null, "rejection is drained once");
            });
            checks.add("programmatic disabled option event is rejected and native selection restored");
            ui(() -> select(activity.content, "Disabled fruit").setSelection(2));
            waitForIdleSync();
            ui(() -> {
                require(select(activity.content, "Disabled fruit").getSelectedItemPosition() == 1, "disabled control restored");
                require(activity.content.takeError() != null, "disabled control event rejected");
                require("banana".equals(state().getString("value")), "disabled control cannot mutate business state");
            });
            checks.add("programmatic disabled control event is rejected and published value restored");
            ui(() -> button(activity.content, "Reset selection").performClick());
            selected("", 0);
            ui(() -> require(activity.content.takeError() == null, "controlled prompt reset emits no invalid empty event"));
            ui(() -> choose().performClick());
            waitForIdleSync();
            clickOption("Apple / 苹果");
            selected("apple", 1);
            checks.add("external reset restores prompt without synthetic events; next popup selection works");
            ui(() -> {
                String before = activity.content.state();
                JSONObject invalid = new JSONObject(before).put("count", 99).put("value", "missing");
                boolean rejected = false;
                try { activity.content.restore(invalid.toString()); } catch (IllegalStateException expected) { rejected = true; }
                require(rejected && before.equals(activity.content.state()), "unknown saved selection rejected atomically");
            });
            checks.add("invalid saved selection cannot change count, selection or live root");
            ActivityMonitor monitor = addMonitor(MainActivity.class.getName(), null, false);
            ui(() -> activity.recreate());
            Activity replacement = monitor.waitForActivityWithTimeout(10000);
            removeMonitor(monitor);
            require(replacement instanceof MainActivity, "Activity recreated");
            activity = (MainActivity) replacement;
            selected("apple", 1);
            ui(() -> {
                require(!oldRoot.get().equals(activity.content.token()), "recreation creates a new root generation");
                require(state().getInt("count") == 1, "recreation retains business count");
                boolean rejected = false;
                try { NativeView.nativeState(oldRoot.get()); } catch (IllegalStateException expected) { rejected = true; }
                require(rejected, "retired root rejects native calls");
                original.get().setSelection(2);
            });
            selected("apple", 1);
            screenshot("select-native.png");
            checks.add("Activity recreation retains selection/count, replaces root and rejects stale native callbacks");
            ui(() -> choose().performClick());
            waitForIdleSync();
            int popupWindow = popupItem("Banana / 香蕉").getWindowId();
            for (int attempt = 0; attempt < 60 && !hasWindow(popupWindow); attempt++) android.os.SystemClock.sleep(50);
            require(hasWindow(popupWindow), "open native popup is enumerated before disposal");
            ui(() -> { activity.content.close(); activity.content.mount(0, ""); });
            selected("", 0);
            for (int attempt = 0; attempt < 60 && hasWindow(popupWindow); attempt++) android.os.SystemClock.sleep(50);
            require(!hasWindow(popupWindow), "unmount removes native popup window");
            ui(() -> require(choose().performClick(), "replacement native dropdown opens"));
            waitForIdleSync();
            clickOption("Apple / 苹果");
            selected("apple", 1);
            ui(() -> require(activity.content.takeError() == null, "replacement root remains usable after popup disposal"));
            checks.add("unmount disposes an open native dropdown and replacement root starts cleanly");
            JSONObject report = new JSONObject().put("passed", true).put("checks", new JSONArray(checks));
            try (FileOutputStream file = new FileOutputStream(new File(activity.getFilesDir(), "select-checks.json"))) {
                file.write(report.toString(2).getBytes(StandardCharsets.UTF_8));
            }
            ui(() -> activity.finish());
            result.putString("stream", "JADIX_SELECT_CHECKS_PASS: scenarios=" + checks.size()
                    + "; native_select_checked=true; popup_checked=true; recreate_checked=true; disposal_checked=true;\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            result.putString("stream", "JADIX_SELECT_CHECKS_FAIL: " + android.util.Log.getStackTraceString(error));
            finish(Activity.RESULT_CANCELED, result);
        }
    }
}
