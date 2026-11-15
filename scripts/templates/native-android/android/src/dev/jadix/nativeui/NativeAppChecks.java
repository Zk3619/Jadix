package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import org.json.JSONObject;

public final class NativeAppChecks extends Instrumentation {
    private String nonce;
    private Throwable failure;
    private EditText originalEditor;

    @Override public void onCreate(Bundle arguments) {
        nonce = arguments == null ? null : arguments.getString("jadix.nonce");
        super.onCreate(arguments);
        start();
    }

    private void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    @Override public void onStart() {
        Bundle report = new Bundle();
        MainActivity activity = null;
        try {
            require(nonce != null && nonce.matches("[0-9a-f]{32}"), "fresh run nonce required");
            activity = (MainActivity) startActivitySync(new Intent(getTargetContext(), MainActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            waitForIdleSync();
            MainActivity selected = activity;
            runOnMainSync(() -> {
                try {
                    originalEditor = (EditText) selected.content.findKind("input");
                    require(originalEditor != null, "native editor mounted");
                    originalEditor.requestFocus();
                    originalEditor.setText("独立工程😀");
                    originalEditor.setSelection(2, 6);
                } catch (Throwable error) { failure = error; }
            });
            waitForIdleSync();
            if (failure != null) throw new AssertionError("native editing failed", failure);
            runOnMainSync(() -> {
                try {
                    NativeView view = selected.content;
                    String token = view.token();
                    EditText editor = originalEditor;
                    ((Button) view.findKind("button")).performClick();
                    ((CheckBox) view.findKind("checkbox")).performClick();
                    JSONObject state = new JSONObject(view.state());
                    require(state.getInt("count") == 1 && state.getString("value").equals("独立工程😀")
                        && state.getBoolean("checked"), "native callbacks round trip to Rust: " + state + "; error=" + view.takeError());
                    require(view.findKind("input") == editor && editor.hasFocus()
                        && editor.getSelectionStart() == 2 && editor.getSelectionEnd() == 6,
                        "editor identity, focus and selection retained");
                    String checkpoint = view.state();
                    JSONObject invalid = new JSONObject(checkpoint).put("checked", "invalid");
                    boolean rejected = false;
                    try { view.restore(invalid.toString()); } catch (IllegalStateException expected) { rejected = true; }
                    require(rejected && view.state().equals(checkpoint), "invalid checkpoint leaves state intact");
                    view.close();
                    boolean retired = false;
                    try { NativeView.nativeState(token); } catch (IllegalStateException expected) { retired = true; }
                    require(retired, "retired root rejected");
                    view.mount(0, "");
                    view.restore(checkpoint);
                    require(!token.equals(view.token()) && view.state().equals(checkpoint), "new root restores checkpoint");
                    require(((EditText) view.findKind("input")).getText().toString().equals("独立工程😀"), "restored native editor");
                    require(view.takeError() == null, "no retained native callback error");
                } catch (Throwable error) { failure = error; }
            });
            if (failure != null) throw new AssertionError("native application probe failed", failure);
            report.putString("jadix.native.project", "PASS:" + nonce);
            report.putString("jadix.package", getTargetContext().getPackageName());
        } catch (Throwable error) {
            report.putString("jadix.native.project", "FAIL:" + error);
            android.util.Log.e("JadixNativeProject", "Application probe failed", error);
        } finally {
            if (activity != null) {
                MainActivity selected = activity;
                runOnMainSync(selected::finish);
            }
        }
        finish(report.getString("jadix.native.project", "").equals("PASS:" + nonce)
            ? Activity.RESULT_OK : Activity.RESULT_CANCELED, report);
    }
}
