package dev.jadix.nativeui;

import android.app.Activity;
import android.os.Bundle;

public final class MainActivity extends Activity {
    static { System.loadLibrary("jadix_native_controls"); }
    public NativeView content;
    public String stateError;

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        content = new NativeView(this);
        setContentView(content);
        content.mount(0, "");
        if (saved != null) {
            try {
                if (saved.containsKey("nativeStateError")) throw new IllegalStateException("previous checkpoint failed");
                String state = saved.getString("nativeState");
                if (state == null) throw new IllegalStateException("checkpoint missing");
                content.restore(state);
            } catch (Exception error) {
                content.close();
                content.mount(0, "");
                stateError = error.toString();
                android.util.Log.e("JadixNativeState", "Checkpoint unavailable; opened a new page", error);
                android.widget.Toast.makeText(this, "Checkpoint unavailable; opened a new page", android.widget.Toast.LENGTH_LONG).show();
            }
        }
    }

    @Override protected void onSaveInstanceState(Bundle saved) {
        try {
            String state = content.state();
            if (state.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 128 * 1024) {
                throw new IllegalStateException("checkpoint too large");
            }
            saved.putString("nativeState", state);
        } catch (Exception error) {
            saved.putBoolean("nativeStateError", true);
            stateError = error.toString();
            android.util.Log.e("JadixNativeState", "Could not save the latest page state", error);
        }
        super.onSaveInstanceState(saved);
    }

    @Override protected void onDestroy() {
        content.close();
        super.onDestroy();
    }
}
