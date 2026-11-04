package dev.jadix.nativeui;

import android.app.Activity;
import android.os.Bundle;
import android.os.Build;
import android.view.View;
import android.view.WindowInsetsController;

public final class MainActivity extends Activity {
    static { System.loadLibrary("jadix_native_controls"); }
    public NativeView content;
    NativeStateStore stateStore;
    String stateError;
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        content = new NativeView(this);
        setContentView(content);
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().getInsetsController().setSystemBarsAppearance(WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
                    WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS);
        } else {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR);
        }
        content.mount(0, "");
        try {
            stateStore = new NativeStateStore(this, saved == null ? null : saved.getString("nativeStateId"));
            stateStore.activate();
            if (saved != null) {
                if (saved.containsKey("nativeStateError")) throw new java.io.IOException("previous save failed");
                if (saved.containsKey("nativeStateId")) content.restore(stateStore.read());
                else if (saved.containsKey("nativeState")) content.restore(saved.getString("nativeState"));
                else if (saved.containsKey("count") || saved.containsKey("value")) {
                    org.json.JSONObject legacy = new org.json.JSONObject(content.state());
                    legacy.put("count", saved.getLong("count"));
                    legacy.put("value", saved.getString("value", ""));
                    content.restore(legacy.toString());
                }
            }
        } catch (Exception error) {
            if (stateStore != null) { stateStore.close(); stateStore.delete(); }
            content.close();
            content.mount(0, "");
            stateStore = new NativeStateStore(this, null);
            stateStore.activate();
            reportStateError("Saved state unavailable; opened a new page", error);
        }
    }
    @Override protected void onSaveInstanceState(Bundle saved) {
        try {
            stateStore.write(content.state());
            saved.putString("nativeStateId", stateStore.id);
        } catch (Exception error) {
            saved.putBoolean("nativeStateError", true);
            reportStateError("Could not save the latest page state", error);
        }
        super.onSaveInstanceState(saved);
    }
    private void reportStateError(String message, Exception error) {
        stateError = message;
        android.util.Log.e("JadixNativeState", message, error);
        android.widget.Toast.makeText(this, message, android.widget.Toast.LENGTH_LONG).show();
    }
    @Override protected void onDestroy() {
        content.close();
        if (isFinishing() && stateStore != null) stateStore.delete();
        if (stateStore != null) stateStore.close();
        super.onDestroy();
    }
}
