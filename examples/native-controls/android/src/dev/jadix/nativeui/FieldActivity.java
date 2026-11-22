package dev.jadix.nativeui;

import android.app.Activity;
import android.os.Bundle;
import android.util.AtomicFile;
import java.io.*;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;

/** Independent form product; platform input and accessibility acceptance is separate. */
public final class FieldActivity extends Activity {
    static { System.loadLibrary("jadix_native_controls"); }
    private static native String nativeStorageResult(String token, String error);
    private NativeView content;
    private AtomicFile storage;
    private String observed;
    private boolean blocked;

    private String read() throws IOException {
        try (FileInputStream file = storage.openRead(); ByteArrayOutputStream bytes = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096]; int length;
            while ((length = file.read(buffer)) != -1) {
                if (bytes.size() + length > 65536) throw new IOException("Field checkpoint exceeds 64 KiB");
                bytes.write(buffer, 0, length);
            }
            return StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(bytes.toByteArray())).toString();
        } catch (FileNotFoundException error) {
            if (storage.getBaseFile().exists()) throw error;
            return null;
        }
    }

    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        content = new NativeView(this); setContentView(content); content.mount(0, "");
        storage = new AtomicFile(new File(getFilesDir(), "field-v1.json"));
        try { observed = read(); if (observed != null) content.restore(observed); }
        catch (Exception error) { blocked = true; nativeStorageResult(content.token(), "Stored request unavailable; original preserved"); }
        content.setStateListener(this::persist);
        content.handleIntent(getIntent(), saved == null);
        content.installBackHandler(this);
    }
    @Override protected void onNewIntent(android.content.Intent intent) {
        super.onNewIntent(intent); setIntent(intent); content.handleIntent(intent, false);
    }


    private void persist(String state) {
        if (state.equals(observed)) return;
        FileOutputStream file = null;
        try {
            if (blocked) throw new IOException("Stored request unavailable; original preserved");
            String actual = read();
            if (observed == null ? actual != null : !observed.equals(actual)) throw new IOException("Request changed; reload before saving");
            byte[] bytes = state.getBytes(StandardCharsets.UTF_8);
            if (bytes.length > 65536) throw new IOException("Field checkpoint exceeds 64 KiB");
            file = storage.startWrite(); file.write(bytes); storage.finishWrite(file); file = null;
            observed = state; nativeStorageResult(content.token(), null);
        } catch (Exception error) {
            if (file != null) storage.failWrite(file);
            nativeStorageResult(content.token(), error.getMessage());
        }
    }

    @Override protected void onSaveInstanceState(Bundle saved) { persist(content.state()); super.onSaveInstanceState(saved); }
    @Override public void onBackPressed() { content.requestClose(this::finish); }
    @Override protected void onDestroy() {
        content.setStateListener(null); content.close(); super.onDestroy();
    }
}
