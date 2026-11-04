package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.res.Configuration;
import android.os.Bundle;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.view.accessibility.AccessibilityNodeInfo;
import android.widget.Button;
import android.widget.EditText;
import android.widget.ScrollView;
import android.widget.CheckBox;
import android.widget.Switch;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

public final class NativeChecks extends Instrumentation {
    private int expectedRows = 100;
    private boolean discardNextState;
    @Override public void callActivityOnCreate(Activity activity, Bundle saved) {
        if (discardNextState && saved != null && saved.containsKey("nativeStateId")) {
            discardNextState = false;
            new NativeStateStore(activity, saved.getString("nativeStateId")).delete();
        }
        super.callActivityOnCreate(activity, saved);
    }
    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        if (arguments != null) expectedRows = Integer.parseInt(arguments.getString("jadix.rows", "100"));
        start();
    }
    private void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
    private void requireSafeViewport(MainActivity activity) {
        NativeView view = activity.content;
        android.view.View decor = activity.getWindow().getDecorView();
        android.view.WindowInsets insets = decor.getRootWindowInsets();
        require(insets != null, "window safe insets available");
        int left, top, right, bottom;
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets safe = insets.getInsets(android.view.WindowInsets.Type.systemBars() | android.view.WindowInsets.Type.displayCutout() | android.view.WindowInsets.Type.ime());
            left = safe.left; top = safe.top; right = safe.right; bottom = safe.bottom;
        } else {
            left = insets.getSystemWindowInsetLeft(); top = insets.getSystemWindowInsetTop();
            right = insets.getSystemWindowInsetRight(); bottom = insets.getSystemWindowInsetBottom();
        }
        int[] windowPosition = new int[2], viewPosition = new int[2];
        decor.getLocationOnScreen(windowPosition); view.getLocationOnScreen(viewPosition);
        android.graphics.Rect expected = new android.graphics.Rect(windowPosition[0] + left, windowPosition[1] + top, windowPosition[0] + decor.getWidth() - right, windowPosition[1] + decor.getHeight() - bottom);
        android.graphics.Rect actual = new android.graphics.Rect(viewPosition[0] + view.getPaddingLeft(), viewPosition[1] + view.getPaddingTop(), viewPosition[0] + view.getWidth() - view.getPaddingRight(), viewPosition[1] + view.getHeight() - view.getPaddingBottom());
        require(actual.equals(expected), "native viewport matches window safe area: actual=" + actual + " expected=" + expected);
    }
    private JSONObject state(NativeView view) {
        try { return new JSONObject(view.state()); }
        catch (Exception error) { throw new AssertionError(error); }
    }
    private void repairOrder(NativeView view, EditText input) {
        android.view.ViewGroup parent = (android.view.ViewGroup) input.getParent();
        int position = parent.indexOfChild(input);
        require(position + 1 < parent.getChildCount(), "focused input has a following sibling");
        android.view.View sibling = parent.getChildAt(position + 1);
        parent.removeView(sibling);
        parent.addView(sibling, position);
        require(input.hasFocus(), "sibling-only perturbation keeps input focused");
        float density = view.getResources().getDisplayMetrics().density;
        float width = (view.getWidth() - view.getPaddingLeft() - view.getPaddingRight()) / density;
        float height = (view.getHeight() - view.getPaddingTop() - view.getPaddingBottom()) / density;
        NativeView.nativeResize(view.token(), width - 1, height, 10000);
        require(parent.indexOfChild(input) == position, "Rust publication restores native sibling order");
        require(input.hasFocus(), "retained input keeps focus after native reorder");
        NativeView.nativeResize(view.token(), width, height, 10001);
    }
    private void checkRetainedEditing(NativeView view, EditText input) {
        input.requestFocus();
        InputConnection connection = input.onCreateInputConnection(new EditorInfo());
        input.selectAll();
        String text = "A😀אבe\u0301Z" + ("Password / 密码".contentEquals(input.getHint()) ? "-secret" : "Name / 中文".contentEquals(input.getHint()) ? "" : "\n第二行");
        connection.commitText(text, 1);
        input.setSelection(5, 1);
        repairOrder(view, input);
        require(input.getSelectionStart() == 5 && input.getSelectionEnd() == 1, "retained input keeps reverse UTF-16 selection");
        require(text.contentEquals(input.getText()), "retained input keeps Unicode text");
        input.setSelection(input.length());
        connection.setComposingText("zhong", 1);
        int start = android.view.inputmethod.BaseInputConnection.getComposingSpanStart(input.getText());
        int end = android.view.inputmethod.BaseInputConnection.getComposingSpanEnd(input.getText());
        require(start >= 0 && end > start, "native composition exists before reorder");
        repairOrder(view, input);
        require(start == android.view.inputmethod.BaseInputConnection.getComposingSpanStart(input.getText())
            && end == android.view.inputmethod.BaseInputConnection.getComposingSpanEnd(input.getText()), "retained input keeps composing span");
        require(connection.commitText("中", 1), "retained connection commits after reorder");
        String expected = text + "中";
        require(expected.contentEquals(input.getText()), "post-reorder native text commits");
        if ("Name / 中文".contentEquals(input.getHint())) {
            require(expected.equals(state(view).optString("value")), "post-reorder native commit reaches Rust");
        } else if ("Password / 密码".contentEquals(input.getHint())) {
            require(!view.state().contains(expected), "retained password remains outside public state");
        } else {
            require(expected.equals(state(view).optJSONObject("extensions").optString("note")), "post-reorder multiline commit reaches Rust");
        }
    }
    private void checkLegacyDialogIme(MainActivity activity) throws Exception {
        android.app.UiAutomation automation = getUiAutomation();
        android.accessibilityservice.AccessibilityServiceInfo service = automation.getServiceInfo();
        int originalFlags = service.flags;
        service.flags |= android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS;
        automation.setServiceInfo(service);
        try {
            ui(() -> {
                android.app.Dialog modal = activity.content.activeDialog();
                EditText field = findInput(modal.getWindow().getDecorView(), "Dialog name / 中文");
                field.requestFocus();
                ((android.view.inputmethod.InputMethodManager) activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).showSoftInput(field, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
            });
            boolean visible = false;
            for (int attempt = 0; attempt < 40 && !visible; attempt++) {
                android.os.SystemClock.sleep(100);
                for (android.view.accessibility.AccessibilityWindowInfo window : automation.getWindows()) {
                    if (window.getType() == android.view.accessibility.AccessibilityWindowInfo.TYPE_INPUT_METHOD) visible = true;
                }
            }
            require(visible, "legacy native IME window becomes visible");
            AtomicReference<String> geometry = new AtomicReference<>("");
            String previous = "";
            int stable = 0;
            for (int attempt = 0; attempt < 40 && stable < 5; attempt++) {
                android.os.SystemClock.sleep(100);
                ui(() -> {
                    android.view.View decor = activity.content.activeDialog().getWindow().getDecorView();
                    android.graphics.Rect viewport = new android.graphics.Rect();
                    decor.getWindowVisibleDisplayFrame(viewport);
                    Button close = findButton(decor, "Close");
                    int[] position = new int[2]; close.getLocationOnScreen(position);
                    geometry.set(viewport.toString() + ":" + position[1] + ":" + close.getHeight());
                });
                String current = geometry.get();
                stable = current.equals(previous) ? stable + 1 : 0; previous = current;
            }
            require(stable >= 5, "legacy IME geometry settles");
            ui(() -> {
                android.view.View decor = activity.content.activeDialog().getWindow().getDecorView();
                android.graphics.Rect viewport = new android.graphics.Rect(); decor.getWindowVisibleDisplayFrame(viewport);
                Button close = findButton(decor, "Close");
                int[] position = new int[2]; close.getLocationOnScreen(position);
                require(close.isShown() && position[1] >= viewport.top && position[1] + close.getHeight() <= viewport.bottom, "legacy dialog close fully above IME: " + geometry.get());
                require(findInput(decor, "Dialog name / 中文").hasFocus() && decor.hasWindowFocus(), "legacy dialog input owns focus");
            });
            sendStringSync("k"); waitForIdleSync();
            ui(() -> {
                EditText field = findInput(activity.content.activeDialog().getWindow().getDecorView(), "Dialog name / 中文");
                require(field.getText().toString().endsWith("k") && state(activity.content).optString("value").endsWith("k"), "legacy system key reaches modal and Rust state");
                field.selectAll(); field.onCreateInputConnection(new EditorInfo()).commitText("旋转😀", 1);
            });
            android.graphics.Bitmap screen = automation.takeScreenshot();
            require(screen != null, "legacy dialog IME screenshot");
            try (java.io.FileOutputStream stream = new java.io.FileOutputStream(new java.io.File(getTargetContext().getFilesDir(), "dialog-ime.png"))) {
                require(screen.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, stream), "legacy dialog IME screenshot saved");
            }
            screen.recycle();
        } finally { service.flags = originalFlags; automation.setServiceInfo(service); }
    }
    private void ui(Runnable action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
    }
    private void checkTreeAccessibility(NativeView host) throws Exception {
        Button item = new Button(getTargetContext());
        JSONObject descriptor = new JSONObject()
            .put("role", "treeitem")
            .put("selected", true)
            .put("expanded", false);
        host.applyAccessibility(item, descriptor);
        AccessibilityNodeInfo info = item.createAccessibilityNodeInfo();
        require("Tree item".contentEquals(info.getExtras().getCharSequence("AccessibilityNodeInfo.roleDescription")),
            "Android exposes TreeItem role description");
        require(info.isSelected(), "Android exposes selected TreeItem state");
        if (android.os.Build.VERSION.SDK_INT >= 36) {
            require(expandedStateMatches(info, "EXPANDED_STATE_COLLAPSED"),
                "Android exposes native collapsed TreeItem state");
        } else if (android.os.Build.VERSION.SDK_INT >= 30) {
            require("Collapsed".contentEquals(info.getStateDescription()), "Android exposes collapsed TreeItem state");
        } else {
            require("Collapsed".contentEquals(info.getExtras().getCharSequence(
                "androidx.view.accessibility.AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY")),
                "Android exposes compatible collapsed TreeItem state");
        }
        info.recycle();
        descriptor.put("selected", false).put("expanded", true);
        host.applyAccessibility(item, descriptor);
        info = item.createAccessibilityNodeInfo();
        require(!info.isSelected(), "Android updates TreeItem selection");
        if (android.os.Build.VERSION.SDK_INT >= 36) {
            require(expandedStateMatches(info, "EXPANDED_STATE_FULL"),
                "Android updates native expanded TreeItem state");
        } else if (android.os.Build.VERSION.SDK_INT >= 30) {
            require("Expanded".contentEquals(info.getStateDescription()), "Android updates expanded TreeItem state");
        } else {
            require("Expanded".contentEquals(info.getExtras().getCharSequence(
                "androidx.view.accessibility.AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY")),
                "Android updates compatible expanded TreeItem state");
        }
        info.recycle();
        descriptor.put("role", JSONObject.NULL).put("selected", JSONObject.NULL).put("expanded", JSONObject.NULL);
        host.applyAccessibility(item, descriptor);
        info = item.createAccessibilityNodeInfo();
        require(info.getExtras().getCharSequence("AccessibilityNodeInfo.roleDescription") == null
            && !info.isSelected(), "Android clears retired semantic overrides");
        if (android.os.Build.VERSION.SDK_INT >= 36) {
            require(expandedStateMatches(info, "EXPANDED_STATE_UNDEFINED"),
                "Android clears native expanded state");
        } else if (android.os.Build.VERSION.SDK_INT >= 30) {
            require(info.getStateDescription() == null, "Android clears expanded state description");
        } else {
            require(info.getExtras().getCharSequence(
                "androidx.view.accessibility.AccessibilityNodeInfoCompat.STATE_DESCRIPTION_KEY") == null,
                "Android clears compatible expanded state description");
        }
        info.recycle();
    }
    private boolean expandedStateMatches(AccessibilityNodeInfo info, String state) throws Exception {
        int expected = AccessibilityNodeInfo.class.getField(state).getInt(null);
        int actual = (Integer) AccessibilityNodeInfo.class
            .getMethod("getExpandedState")
            .invoke(info);
        return actual == expected;
    }
    @Override public void onStart() {
        Bundle report = new Bundle();
        try {
            Intent intent = new Intent(getTargetContext(), MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            MainActivity activity = (MainActivity) startActivitySync(intent);
            waitForIdleSync();
            boolean extended = activity.content.findKind("textarea") != null;
            String previous = activity.content.token();
            ui(() -> {
                NativeView view = activity.content;
                require(view.getChildCount() == 1, "single mounted root");
                require(countRows(view) == expectedRows, "all expected native row widgets mounted");
                requireSafeViewport(activity);
                try { checkTreeAccessibility(view); }
                catch (Exception error) { throw new AssertionError(error); }
                if (view.findKind("textarea") != null) {
                    android.widget.ImageView image = findImage(view, "Native image");
                    require(image != null && image.getDrawable() instanceof android.graphics.drawable.BitmapDrawable, "embedded PNG decodes to native image");
                    require("Native image".contentEquals(image.getContentDescription()), "native image accessibility alternative");
                    android.widget.ImageView failedImage = findImage(view, "Unavailable image");
                    require(failedImage != null && failedImage.getDrawable() != null, "corrupt PNG keeps visible placeholder");
                    if (android.os.Build.VERSION.SDK_INT >= 30) require("Image unavailable".contentEquals(failedImage.getStateDescription()), "image failure retains accessibility status");
                    require(image.getWidth() == failedImage.getWidth() && image.getHeight() == failedImage.getHeight(), "image failure retains declared dimensions");
                    CheckBox checkbox = (CheckBox) view.findKind("checkbox");
                    checkbox.performClick();
                    require(checkbox.isChecked() && hasText(view, "Checked: true"), "checkbox updates Rust signal");
                    checkbox.performClick();
                    require(!checkbox.isChecked() && hasText(view, "Checked: false"), "checkbox toggles back");
                    Switch toggle = (Switch) view.findKind("switch");
                    require(toggle != null && "Airplane mode".contentEquals(toggle.getText()), "native Switch label");
                    toggle.performClick();
                    require(toggle.isChecked() && checkbox.isChecked() && state(view).optJSONObject("extensions").optBoolean("checked"), "Switch updates shared Rust boolean");
                    checkbox.performClick();
                    require(!toggle.isChecked() && !checkbox.isChecked() && !state(view).optJSONObject("extensions").optBoolean("checked"), "Checkbox publication updates retained Switch");
                    report.putBoolean("switch_checked", true);
                    EditText password = findInput(view, "Password / 密码");
                    require(password != null && (password.getInputType() & android.text.InputType.TYPE_MASK_VARIATION) == android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD,
                        "password input uses native password variation");
                    password.setText("secret-密😀");
                    require("secret-密😀".equals(password.getText().toString()) && !"secret-密😀".equals(state(view).optString("value")),
                        "password value stays outside public sample state");
                    EditText note = (EditText) view.findKind("textarea");
                    note.requestFocus();
                    InputConnection noteConnection = note.onCreateInputConnection(new EditorInfo());
                    noteConnection.commitText("第一行😀\n第二行", 1);
                    require(hasText(view, "Note: 第一行😀\n第二行"), "multiline Unicode reaches Rust signal");
                    noteConnection.setComposingText("中", 1);
                    require(!hasText(view, "第二行中"), "multiline composition stays draft");
                    noteConnection.commitText("中", 1);
                    require(hasText(view, "第二行中"), "multiline composition commits");
                }
                Button button = (Button) view.findKind("button");
                require(button != null, "real Android Button");
                button = findButton(view, "Continue");
                button.performClick(); button.performClick();
                require(state(view).optInt("count") == 2, "Rust callback state");
                EditText input = findInput(view, "Name / 中文");
                input.requestFocus();
                InputConnection connection = input.onCreateInputConnection(new EditorInfo());
                require(connection != null, "native InputConnection");
                connection.setComposingText("zhong", 1);
                require(state(view).optString("value").isEmpty(), "composition must not commit Rust signal");
                connection.setComposingText("中", 1);
                require(state(view).optString("value").isEmpty(), "Chinese composition draft");
                connection.commitText("中文😀", 1);
                require("中文😀".equals(state(view).optString("value")), "Chinese emoji commit");
                input.setSelection(2, 4);
                connection.setComposingText("汉😀", 1);
                connection.finishComposingText();
                require(state(view).optString("value").contains("汉"), "replacement and finish composition");
                checkRetainedEditing(view, input);
                if (extended) {
                    checkRetainedEditing(view, findInput(view, "Password / 密码"));
                    EditText note = (EditText) view.findKind("textarea");
                    checkRetainedEditing(view, note);
                    note.selectAll();
                    note.onCreateInputConnection(new EditorInfo()).commitText("第一行😀\n第二行中", 1);
                }
                report.putBoolean("retained_editing_checked", true);
                findButton(view, "Reset").performClick();
                require(input.getText().length() == 0, "controlled reset reaches native widget");
                connection.commitText("旋转😀", 1);
                ScrollView scroll = (ScrollView) view.findKind("scroll");
                require(scroll.getChildCount() == 1, "native scroll content");
                scroll.scrollTo(0, 300);
                require(scroll.getScrollY() > 0, "native scroll offset");
            });
            if (extended) {
                ui(() -> findButton(activity.content, "Open editor").performClick());
                waitForIdleSync();
                ui(() -> {
                    android.app.Dialog modal = activity.content.activeDialog();
                    require(modal != null && modal.isShowing(), "real native Dialog window opens");
                    EditText field = findInput(modal.getWindow().getDecorView(), "Dialog name / 中文");
                    require(field != null && field.getWidth() > 0, "dialog lays out retained native input");
                    require("旋转😀".equals(field.getText().toString()), "dialog reads shared Rust input state");
                    field.requestFocus(); field.selectAll();
                    InputConnection connection = field.onCreateInputConnection(new EditorInfo());
                    connection.commitText("弹层😀",1);
                    require("弹层😀".equals(state(activity.content).optString("value")), "dialog edits shared Rust signal");
                    findButton(modal.getWindow().getDecorView(), "Advanced").performClick();
                    android.app.Dialog nested = activity.content.activeDialog();
                    require(nested != null && nested != modal && hasText(nested.getWindow().getDecorView(), "Confirm changes"), "nested native Dialog window opens");
                    findButton(nested.getWindow().getDecorView(), "Return").performClick();
                    require(!nested.isShowing() && activity.content.activeDialog() == modal, "nested dialog_close restores parent context");
                    field.selectAll(); connection.commitText("旋转😀",1);
                    findButton(modal.getWindow().getDecorView(), "Done").performClick();
                    require(!modal.isShowing(), "dialog_close restores native window");
                    findButton(activity.content, "Open editor").performClick();
                });
                waitForIdleSync();
                if (android.os.Build.VERSION.SDK_INT >= 30) {
                    AtomicReference<Boolean> served = new AtomicReference<>(false);
                    for (int attempt=0;attempt<50 && !served.get();attempt++) {
                        ui(() -> {
                            android.app.Dialog modal = activity.content.activeDialog();
                            EditText field = findInput(modal.getWindow().getDecorView(), "Dialog name / 中文");
                            field.requestFocus();
                            android.view.inputmethod.InputMethodManager keyboard = (android.view.inputmethod.InputMethodManager) activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE);
                            served.set(field.hasFocus() && modal.getWindow().getDecorView().hasWindowFocus() && keyboard.isActive(field));
                        });
                        if (!served.get()) android.os.SystemClock.sleep(100);
                    }
                    require(served.get(), "dialog editor is served by IME before show request");
                    ui(() -> {
                        android.app.Dialog modal = activity.content.activeDialog();
                        EditText field = findInput(modal.getWindow().getDecorView(), "Dialog name / 中文");
                        field.requestFocus();
                        android.view.inputmethod.InputMethodManager keyboard = (android.view.inputmethod.InputMethodManager) activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE);
                        keyboard.showSoftInput(field, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
                    });
                    AtomicReference<Boolean> visible = new AtomicReference<>(false);
                    for (int attempt=0;attempt<30 && !visible.get();attempt++) {
                        android.os.SystemClock.sleep(100);
                        ui(() -> {
                            android.view.WindowInsets insets = activity.content.activeDialog().getWindow().getDecorView().getRootWindowInsets();
                            visible.set(insets != null && insets.isVisible(android.view.WindowInsets.Type.ime()));
                        });
                    }
                    require(visible.get(), "real native dialog IME becomes visible");
                    AtomicReference<String> geometry = new AtomicReference<>("");
                    String previousGeometry = "";
                    int stable = 0;
                    for (int attempt=0;attempt<40 && stable<5;attempt++) {
                        android.os.SystemClock.sleep(100);
                        ui(() -> {
                            android.app.Dialog modal = activity.content.activeDialog();
                            android.graphics.Rect viewport = new android.graphics.Rect();
                            modal.getWindow().getDecorView().getWindowVisibleDisplayFrame(viewport);
                            android.graphics.Rect close = new android.graphics.Rect();
                            findButton(modal.getWindow().getDecorView(), "Close").getGlobalVisibleRect(close);
                            android.view.WindowInsets insets = modal.getWindow().getDecorView().getRootWindowInsets();
                            int bottom = insets.getInsets(android.view.WindowInsets.Type.ime()).bottom;
                            geometry.set(insets.isVisible(android.view.WindowInsets.Type.ime()) ? bottom + ":" + viewport.bottom + ":" + close.top + ":" + close.bottom : "");
                        });
                        String current = geometry.get();
                        stable = !current.isEmpty() && current.equals(previousGeometry) ? stable+1 : 0;
                        previousGeometry = current;
                    }
                    require(stable>=5, "dialog keyboard and layout geometry settle before visibility assertion");
                    ui(() -> {
                        android.app.Dialog modal = activity.content.activeDialog();
                        EditText field = findInput(modal.getWindow().getDecorView(), "Dialog name / 中文");
                        require(field.hasFocus() && modal.getWindow().getDecorView().hasWindowFocus(), "dialog input owns native view and window focus");
                        android.graphics.Rect viewport = new android.graphics.Rect();
                        modal.getWindow().getDecorView().getWindowVisibleDisplayFrame(viewport);
                        android.graphics.Rect close = new android.graphics.Rect();
                        require(findButton(modal.getWindow().getDecorView(), "Close").getGlobalVisibleRect(close), "dialog close remains visible above IME");
                        require(close.bottom <= viewport.bottom, "dialog close does not overlap keyboard");
                    });
                    sendStringSync("k");
                    waitForIdleSync();
                    ui(() -> {
                        android.app.Dialog modal = activity.content.activeDialog();
                        EditText field = findInput(modal.getWindow().getDecorView(), "Dialog name / 中文");
                        require(field.getText().toString().endsWith("k") && state(activity.content).optString("value").endsWith("k"), "system key input reaches modal and shared Rust state");
                        field.selectAll(); field.onCreateInputConnection(new EditorInfo()).commitText("旋转😀",1);
                    });
                    android.graphics.Bitmap screen = getUiAutomation().takeScreenshot();
                    require(screen != null, "dialog IME screenshot");
                    try (java.io.FileOutputStream stream = new java.io.FileOutputStream(new java.io.File(getTargetContext().getFilesDir(), "dialog-ime.png"))) {
                        require(screen.compress(android.graphics.Bitmap.CompressFormat.PNG,100,stream), "dialog IME screenshot saved");
                    }
                    screen.recycle();
                } else checkLegacyDialogIme(activity);
                report.putBoolean("dialog_ime_checked", true);
                sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK);
                waitForIdleSync();
                AtomicReference<Boolean> stillOpen = new AtomicReference<>(false);
                ui(() -> stillOpen.set(activity.content.activeDialog() != null));
                if (stillOpen.get()) { sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK); waitForIdleSync(); }
                ui(() -> {
                    require(activity.content.activeDialog() == null, "native Back dismisses controlled Dialog");
                    findButton(activity.content, "Open editor").performClick();
                });
                waitForIdleSync();
                ui(() -> {
                    android.app.Dialog modal = activity.content.activeDialog();
                    require(modal != null, "dialog can reopen after Back");
                    require("旋转😀".equals(findInput(modal.getWindow().getDecorView(), "Dialog name / 中文").getText().toString()), "reopen retains dialog input state");
                });
            }
            ActivityMonitor monitor = addMonitor(MainActivity.class.getName(), null, false);
            ui(activity::recreate);
            Activity recreated = monitor.waitForActivityWithTimeout(10000);
            removeMonitor(monitor);
            require(recreated instanceof MainActivity, "Activity recreation");
            waitForIdleSync();
            MainActivity next = (MainActivity) recreated;
            ui(() -> {
                require(!previous.equals(next.content.token()), "rotation creates a fresh root generation");
                require(next.content.activeDialog() == null, "recreation releases old native modal windows");
                require(state(next.content).optInt("count") == 2, "counter survives recreation");
                require("旋转😀".equals(state(next.content).optString("value")), "committed input survives recreation");
                if (extended) require(hasText(next.content, "Note: 第一行😀\n第二行中"), "multiline committed state survives recreation");
                boolean rejected = false;
                try { NativeView.nativeState(previous); } catch (IllegalStateException expected) { rejected = true; }
                require(rejected, "destroyed root rejects stale native callbacks");
            });
            ActivityMonitor rotationMonitor = addMonitor(MainActivity.class.getName(), null, false);
            ui(() -> next.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE));
            Activity rotatedActivity = rotationMonitor.waitForActivityWithTimeout(10000);
            removeMonitor(rotationMonitor);
            require(rotatedActivity instanceof MainActivity, "orientation configuration recreates Activity");
            waitForIdleSync();
            MainActivity rotated = (MainActivity) rotatedActivity;
            ui(() -> {
                require(rotated.getResources().getConfiguration().orientation == Configuration.ORIENTATION_LANDSCAPE, "landscape orientation");
                require(state(rotated.content).optInt("count") == 2, "counter survives rotation");
                require("旋转😀".equals(state(rotated.content).optString("value")), "input survives rotation");
                if (extended) {
                    require(hasText(rotated.content, "Note: 第一行😀\n第二行中"), "multiline state survives landscape rotation");
                    require(findInput(rotated.content, "Password / 密码").length() == 0, "ephemeral password is not restored from Activity state");
                }
            });
            checkStateStore();
            ui(() -> {
                require(rotated.content.takeError() == null, "normal native events have no hidden errors");
                StringBuilder rejected = new StringBuilder();
                for (int index = 0; index < 1024 * 1024; index++) rejected.append('x');
                findInput(rotated.content, "Name / 中文").setText(rejected.toString());
            });
            waitForIdleSync();
            ui(() -> {
                require(rotated.content.takeError() != null, "oversized edit exposes a bounded native callback error");
                require(rotated.content.takeError() == null, "native error is drained once");
                require("旋转😀".equals(state(rotated.content).optString("value")), "oversized edit keeps business state");
                require("旋转😀".contentEquals(findInput(rotated.content, "Name / 中文").getText()), "oversized edit restores controlled native text");
            });
            StringBuilder large = new StringBuilder();
            for (int index = 0; index < 80000; index++) large.append("保😀");
            String largeValue = large.toString();
            AtomicReference<String> savedId = new AtomicReference<>();
            AtomicReference<Integer> bundleBytes = new AtomicReference<>();
            ui(() -> {
                EditText input = findInput(rotated.content, "Name / 中文");
                input.requestFocus();
                InputConnection connection = input.onCreateInputConnection(new EditorInfo());
                require(connection.setSelection(0, input.getText().length()), "select old value before large replacement");
                require(connection.commitText(largeValue, 1), "large committed native input");
            });
            waitForIdleSync();
            ui(() -> {
                EditText input = findInput(rotated.content, "Name / 中文");
                require(largeValue.equals(state(rotated.content).optString("value")), "large input reaches Rust before save");
                Bundle saved = new Bundle();
                rotated.onSaveInstanceState(saved);
                require(saved.containsKey("nativeStateId") && !saved.containsKey("nativeState") && !saved.containsKey("value"), "Bundle contains state identity instead of text");
                android.os.Parcel parcel = android.os.Parcel.obtain();
                try { parcel.writeBundle(saved); bundleBytes.set(parcel.dataSize()); }
                finally { parcel.recycle(); }
                require(bundleBytes.get() < 4096, "large form keeps lifecycle Bundle bounded");
                savedId.set(saved.getString("nativeStateId"));
                require(rotated.stateError == null, "large state writes successfully");
            });
            ActivityMonitor largeMonitor = addMonitor(MainActivity.class.getName(), null, false);
            ui(rotated::recreate);
            MainActivity restored = (MainActivity) largeMonitor.waitForActivityWithTimeout(10000);
            removeMonitor(largeMonitor);
            require(restored != null, "large state Activity recreation");
            waitForIdleSync();
            ui(() -> {
                require(savedId.get().equals(restored.stateStore.id), "recreation reuses the same per-Activity file");
                require(largeValue.equals(state(restored.content).optString("value")), "large Unicode state survives recreation");
                require(state(restored.content).optInt("count") == 2, "large state retains counter");
                require(restored.stateError == null, "large restore has no recovery error");
                require(restored.content.takeError() == null, "large native edit and restore have no callback error");
                discardNextState = true;
            });
            ActivityMonitor missingMonitor = addMonitor(MainActivity.class.getName(), null, false);
            ui(restored::recreate);
            MainActivity recovered = (MainActivity) missingMonitor.waitForActivityWithTimeout(10000);
            removeMonitor(missingMonitor);
            require(recovered != null, "missing state file recovery Activity");
            waitForIdleSync();
            ui(() -> {
                require(recovered.stateError != null, "missing saved state is reported");
                require(state(recovered.content).optInt("count") == 0 && state(recovered.content).optString("value").isEmpty(), "missing state starts a fresh healthy root");
                findButton(recovered.content, "Continue").performClick();
                require(state(recovered.content).optInt("count") == 1, "recovery root remains interactive");
                recovered.content.close();
                recovered.content.close();
                require(recovered.content.getChildCount() == 0, "idempotent native unmount");
                recovered.finish();
            });
            report.putString("stream", "JADIX_NATIVE_CHECKS_PASS: extensions=" + extended + "; dialog_ime_checked=" + report.getBoolean("dialog_ime_checked") + "; retained_editing_checked=" + report.getBoolean("retained_editing_checked") + "; retained_editors=" + (extended ? 3 : 1) + "; switch_checked=" + report.getBoolean("switch_checked") + "; controls, callbacks, Chinese composition, emoji, reset, scroll, Dialog/nested/IME/Back, recreation, landscape rotation, stale root, oversized edit rejection/recovery, large state/file/bundle/recovery, unmount; large_utf8_bytes=" + largeValue.getBytes(java.nio.charset.StandardCharsets.UTF_8).length + "; bundle_bytes=" + bundleBytes.get() + "\n");
            finish(Activity.RESULT_OK, report);
        } catch (Throwable error) {
            report.putString("stream", "JADIX_NATIVE_CHECKS_FAIL: " + android.util.Log.getStackTraceString(error));
            finish(Activity.RESULT_CANCELED, report);
        }
    }

    private void checkStateStore() throws Exception {
        NativeStateStore first = new NativeStateStore(getTargetContext(), null);
        NativeStateStore second = new NativeStateStore(getTargetContext(), null);
        try {
            first.write("first 中文😀");
            second.write("second");
            require(!first.id.equals(second.id) && "second".equals(second.read()), "independent state files");
            StringBuilder large = new StringBuilder();
            for (int index = 0; index <= NativeStateStore.MAX_BYTES; index++) large.append('x');
            boolean rejected = false;
            try { first.write(large.toString()); } catch (java.io.IOException expected) { rejected = true; }
            require(rejected && "first 中文😀".equals(first.read()), "over-budget save keeps previous atomic state");
            first.delete();
            require("second".equals(second.read()), "disposing one state file keeps another");
            rejected = false;
            try { first.read(); } catch (java.io.FileNotFoundException expected) { rejected = true; }
            require(rejected, "deleted state file is unavailable");
            rejected = false;
            try { new NativeStateStore(getTargetContext(), "../escape"); } catch (IllegalArgumentException expected) { rejected = true; }
            require(rejected, "invalid state identity cannot select a path");
            java.io.File corrupt = new java.io.File(getTargetContext().getFilesDir(), "native-state-" + first.id + ".json");
            try (java.io.FileOutputStream stream = new java.io.FileOutputStream(corrupt)) { stream.write(new byte[] {(byte) 0xc3, (byte) 0x28}); }
            rejected = false;
            try { first.read(); } catch (java.nio.charset.CharacterCodingException expected) { rejected = true; }
            require(rejected, "malformed UTF-8 state is rejected");
        } finally { first.delete(); second.delete(); }
        NativeStateChecks.run(getTargetContext());
    }
    private int countRows(android.view.View view) {
        int count = view instanceof android.widget.TextView && ((android.widget.TextView) view).getText().toString().matches("Row [0-9]{3} .*") ? 1 : 0;
        if (view instanceof android.view.ViewGroup) {
            android.view.ViewGroup group = (android.view.ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) count += countRows(group.getChildAt(index));
        }
        return count;
    }
    private Button findButton(android.view.View view, String caption) {
        if (view instanceof Button && caption.equals(((Button) view).getText().toString())) return (Button) view;
        if (view instanceof android.view.ViewGroup) {
            android.view.ViewGroup parent = (android.view.ViewGroup) view;
            for (int index = 0; index < parent.getChildCount(); index++) {
                Button found = findButton(parent.getChildAt(index), caption);
                if (found != null) return found;
            }
        }
        return null;
    }

    private android.widget.ImageView findImage(android.view.View view, String alternative) {
        if (view instanceof android.widget.ImageView && alternative.equals(String.valueOf(view.getContentDescription()))) return (android.widget.ImageView) view;
        if (view instanceof android.view.ViewGroup) {
            android.view.ViewGroup parent = (android.view.ViewGroup) view;
            for (int index = 0; index < parent.getChildCount(); index++) {
                android.widget.ImageView found = findImage(parent.getChildAt(index), alternative);
                if (found != null) return found;
            }
        }
        return null;
    }
    private EditText findInput(android.view.View view, String placeholder) {
        if (view instanceof EditText && placeholder.contentEquals(((EditText) view).getHint())) return (EditText) view;
        if (view instanceof android.view.ViewGroup) {
            android.view.ViewGroup parent = (android.view.ViewGroup) view;
            for (int index=0;index<parent.getChildCount();index++) {
                EditText found = findInput(parent.getChildAt(index),placeholder);
                if (found != null) return found;
            }
        }
        return null;
    }
    private boolean hasText(android.view.View view, String text) {
        if (view instanceof android.widget.TextView && !(view instanceof EditText) && ((android.widget.TextView) view).getText().toString().contains(text)) return true;
        if (view instanceof android.view.ViewGroup) {
            android.view.ViewGroup group = (android.view.ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) if (hasText(group.getChildAt(index), text)) return true;
        }
        return false;
    }
}
