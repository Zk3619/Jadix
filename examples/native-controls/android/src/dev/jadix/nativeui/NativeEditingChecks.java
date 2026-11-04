package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.EditText;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

public final class NativeEditingChecks extends Instrumentation {
    private String token;
    private int expectedRows;
    private int keyEvents;
    private ClipboardManager clipboard;
    private ClipData originalClip;
    private boolean clipboardCaptured;
    private final JSONArray editors = new JSONArray();

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
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
        waitForIdleSync();
    }

    private EditText input(View view, String placeholder) {
        if (view instanceof EditText && placeholder.contentEquals(((EditText) view).getHint())) return (EditText) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                EditText found = input(group.getChildAt(index), placeholder);
                if (found != null) return found;
            }
        }
        return null;
    }

    private int rows(View view) {
        int count = view instanceof android.widget.TextView && ((android.widget.TextView) view).getText().toString().startsWith("Row ") ? 1 : 0;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) count += rows(group.getChildAt(index));
        }
        return count;
    }

    private boolean sameClip(ClipData left, ClipData right) {
        if (left == null || right == null) return left == right;
        android.content.ClipDescription before = left.getDescription(), after = right.getDescription();
        if (!android.text.TextUtils.equals(before.getLabel(), after.getLabel()) || before.getMimeTypeCount() != after.getMimeTypeCount() || left.getItemCount() != right.getItemCount()) return false;
        for (int index = 0; index < before.getMimeTypeCount(); index++) if (!before.getMimeType(index).equals(after.getMimeType(index))) return false;
        for (int index = 0; index < left.getItemCount(); index++) {
            ClipData.Item original = left.getItemAt(index), restored = right.getItemAt(index);
            if (!android.text.TextUtils.equals(original.getText(), restored.getText()) || !java.util.Objects.equals(original.getHtmlText(), restored.getHtmlText()) || !java.util.Objects.equals(original.getUri(), restored.getUri())) return false;
            String originalIntent = original.getIntent() == null ? null : original.getIntent().toUri(0);
            String restoredIntent = restored.getIntent() == null ? null : restored.getIntent().toUri(0);
            if (!java.util.Objects.equals(originalIntent, restoredIntent)) return false;
        }
        return true;
    }

    private void shortcut(int key, boolean shift) {
        long now = SystemClock.uptimeMillis();
        int modifiers = KeyEvent.META_CTRL_ON | KeyEvent.META_CTRL_LEFT_ON;
        if (shift) modifiers |= KeyEvent.META_SHIFT_ON | KeyEvent.META_SHIFT_LEFT_ON;
        sendKeySync(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, key, 0, modifiers));
        sendKeySync(new KeyEvent(now, SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, key, 0, modifiers));
        keyEvents += 2;
        waitForIdleSync();
    }

    private void clip(String text) { clipboard.setPrimaryClip(ClipData.newPlainText("Jadix edit probe", text)); }

    private String clipboardText() {
        ClipData data = clipboard.getPrimaryClip();
        return data == null || data.getItemCount() != 1 || data.getItemAt(0).getText() == null ? null : data.getItemAt(0).getText().toString();
    }

    private void value(MainActivity activity, EditText editor, String placeholder, String expected) {
        ui(() -> {
            require(input(activity.content, placeholder) == editor && editor.isAttachedToWindow(), "Editor identity replaced: " + placeholder);
            require(editor.hasFocus() && editor.hasWindowFocus(), "Native focus missing: " + placeholder);
            require(expected.contentEquals(editor.getText()), "Native text mismatch: " + placeholder + "; expected=" + expected + "; actual=" + editor.getText());
            try {
                JSONObject state = new JSONObject(activity.content.state());
                String business = placeholder.equals("Name / 中文") ? state.getString("value") : state.getJSONObject("extensions").getString("note");
                require(expected.equals(business), "Rust business text mismatch: " + placeholder);
            } catch (Exception error) { throw new AssertionError(error); }
            require(activity.content.takeError() == null, "Native callback failed");
        });
    }

    private void editor(MainActivity activity, String placeholder, String initial) throws Exception {
        EditText[] reference = new EditText[1];
        ui(() -> {
            reference[0] = input(activity.content, placeholder);
            require(reference[0] != null, "Missing editor: " + placeholder);
            reference[0].setShowSoftInputOnFocus(false);
            require(reference[0].requestFocus(), "Cannot focus editor");
            InputConnection connection = reference[0].onCreateInputConnection(new EditorInfo());
            require(connection != null && connection.commitText(initial, 1), "Native Unicode commit failed");
        });
        EditText field = reference[0];
        value(activity, field, placeholder, initial);
        ui(() -> field.setSelection(0, field.length()));
        shortcut(KeyEvent.KEYCODE_C, false);
        ui(() -> require(initial.equals(clipboardText()), "Native copy did not preserve Unicode"));
        shortcut(KeyEvent.KEYCODE_X, false);
        value(activity, field, placeholder, "");
        shortcut(KeyEvent.KEYCODE_V, false);
        value(activity, field, placeholder, initial);
        shortcut(KeyEvent.KEYCODE_Z, false);
        value(activity, field, placeholder, "");
        shortcut(KeyEvent.KEYCODE_Z, true);
        value(activity, field, placeholder, initial);
        String suffix = " 追加🧪";
        ui(() -> { clip(suffix); field.setSelection(field.length()); });
        shortcut(KeyEvent.KEYCODE_V, false);
        value(activity, field, placeholder, initial + suffix);
        ui(() -> {
            ViewGroup parent = (ViewGroup) field.getParent();
            int position = parent.indexOfChild(field);
            require(position + 1 < parent.getChildCount(), "Editor has no following sibling");
            View sibling = parent.getChildAt(position + 1);
            parent.removeView(sibling);
            parent.addView(sibling, position);
            float density = activity.getResources().getDisplayMetrics().density;
            float width = (activity.content.getWidth() - activity.content.getPaddingLeft() - activity.content.getPaddingRight()) / density;
            float height = (activity.content.getHeight() - activity.content.getPaddingTop() - activity.content.getPaddingBottom()) / density;
            NativeView.nativeResize(activity.content.token(), width - 1, height, 90000);
            require(parent.indexOfChild(field) == position, "Publication did not restore sibling order");
            NativeView.nativeResize(activity.content.token(), width, height, 90001);
        });
        value(activity, field, placeholder, initial + suffix);
        shortcut(KeyEvent.KEYCODE_Z, false);
        value(activity, field, placeholder, initial);
        shortcut(KeyEvent.KEYCODE_Z, true);
        value(activity, field, placeholder, initial + suffix);
        String controlled = "受控重置😀";
        ui(() -> {
            try {
                JSONObject state = new JSONObject(activity.content.state());
                if (placeholder.equals("Name / 中文")) state.put("value", controlled);
                else state.getJSONObject("extensions").put("note", controlled);
                activity.content.restore(state.toString());
            } catch (Exception error) { throw new AssertionError(error); }
        });
        shortcut(KeyEvent.KEYCODE_Z, false);
        value(activity, field, placeholder, controlled);
        editors.put(new JSONObject().put("control", placeholder).put("checks", new JSONArray(new String[] {
            "unicode-commit", "copy", "cut", "paste", "undo", "redo", "history-after-republish-reorder", "controlled-reset-clears-history"
        })).put("native_identity_retained", true).put("business_roundtrip", true));
    }

    private void password(MainActivity activity) {
        String secret = "私密😀 العربية";
        String[] publicValue = new String[1];
        EditText[] password = new EditText[1];
        ui(() -> {
            password[0] = input(activity.content, "Password / 密码");
            require(password[0] != null, "Missing native password");
            password[0].setShowSoftInputOnFocus(false);
            require(password[0].requestFocus(), "Password focus failed");
            try { publicValue[0] = new JSONObject(activity.content.state()).getString("value"); }
            catch (Exception error) { throw new AssertionError(error); }
            clip(secret);
        });
        shortcut(KeyEvent.KEYCODE_V, false);
        ui(() -> { require(secret.contentEquals(password[0].getText()), "Password Unicode paste failed"); password[0].setSelection(0, password[0].length()); clip("copy-guard"); });
        shortcut(KeyEvent.KEYCODE_C, false);
        ui(() -> require("copy-guard".equals(clipboardText()), "Password copied into clipboard"));
        shortcut(KeyEvent.KEYCODE_X, false);
        ui(() -> {
            require(secret.contentEquals(password[0].getText()) && "copy-guard".equals(clipboardText()), "Password cut changed secret or clipboard");
            try { require(publicValue[0].equals(new JSONObject(activity.content.state()).getString("value")), "Private password replaced public business field"); }
            catch (Exception error) { throw new AssertionError(error); }
            require(activity.content.takeError() == null, "Password callback error");
        });
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        MainActivity activity = null;
        try {
            require(token != null && UUID.fromString(token).toString().equals(token), "Fresh token required");
            require(expectedRows == 100 || expectedRows == 1000, "Row count required");
            activity = (MainActivity) startActivitySync(new Intent(getTargetContext(), MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            MainActivity current = activity;
            waitForIdleSync();
            long deadline = SystemClock.uptimeMillis() + 10000;
            boolean[] ready = new boolean[1];
            while (!ready[0] && SystemClock.uptimeMillis() < deadline) {
                ui(() -> ready[0] = current.content.hasWindowFocus() && current.content.getHeight() > 0);
                if (!ready[0]) SystemClock.sleep(25);
            }
            require(ready[0], "Window not ready");
            ui(() -> {
                require(rows(current.content) == expectedRows, "Retained native row count mismatch");
                current.getWindow().addFlags(WindowManager.LayoutParams.FLAG_ALT_FOCUSABLE_IM);
                clipboard = (ClipboardManager) current.getSystemService(Context.CLIPBOARD_SERVICE);
                originalClip = clipboard.getPrimaryClip();
                clipboardCaptured = true;
            });
            editor(current, "Name / 中文", "中文😀 العربية");
            editor(current, "Multiline / 多行", "首行😀 العربية\n第二行中文");
            password(current);
            ui(() -> {
                if (originalClip == null) clipboard.clearPrimaryClip(); else clipboard.setPrimaryClip(originalClip);
                ClipData restored = clipboard.getPrimaryClip();
                require(sameClip(originalClip, restored), "Clipboard content restore failed");
                clipboardCaptured = false;
            });
            android.graphics.Bitmap screenshot = getUiAutomation().takeScreenshot();
            require(screenshot != null, "Editing screenshot unavailable");
            try (java.io.FileOutputStream stream = new java.io.FileOutputStream(new java.io.File(getTargetContext().getFilesDir(), "editing.png"))) {
                require(screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, stream), "Editing screenshot encoding failed");
            } finally { screenshot.recycle(); }
            result.putString("jadix_editing", new JSONObject().put("schema", 1).put("token", token).put("passed", true)
                .put("retained_rows", expectedRows).put("key_events", keyEvents).put("editors", editors)
                .put("password_checks", new JSONArray(new String[] {"unicode-paste", "copy-blocked", "cut-blocked", "public-state-isolation"}))
                .put("clipboard_restored", true).put("event_authority", "Instrumentation-injected clipboard/undo shortcuts; programmatic InputConnection seed, selection, controlled reset and sibling perturbation; soft keyboard suppressed").toString());
            result.putString("stream", "JADIX_NATIVE_EDITING_PASS");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            result.putString("stream", "JADIX_NATIVE_EDITING_FAIL: " + android.util.Log.getStackTraceString(error));
            finish(Activity.RESULT_CANCELED, result);
        } finally {
            MainActivity current = activity;
            if (clipboardCaptured || current != null) {
                runOnMainSync(() -> {
                    if (clipboardCaptured) { if (originalClip == null) clipboard.clearPrimaryClip(); else clipboard.setPrimaryClip(originalClip); }
                    if (current != null) current.finish();
                });
            }
        }
    }
}
