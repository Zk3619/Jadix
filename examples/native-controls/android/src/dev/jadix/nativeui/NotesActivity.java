package dev.jadix.nativeui;

import android.app.Activity;
import android.os.Bundle;
import org.json.JSONObject;

public final class NotesActivity extends Activity {
    static { System.loadLibrary("jadix_native_controls"); }
    static native String nativeStorageResult(String token, String error);
    public NativeView content;
    private NoteDocumentStore documents;
    private NativeStateStore recovery;
    private String persisted;
    private boolean blocked;
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        content = new NativeView(this);
        setContentView(content);
        content.mount(0, "");
        documents = new NoteDocumentStore(this);
        try {
            String document = documents.read();
            if (document != null) content.restore(document);
        } catch (Exception error) {
            blocked = true;
            nativeStorageResult(content.token(), "Stored notebook unavailable; export a backup or confirm Import to replace it");
            android.util.Log.e("JadixNotes", "Preserved unavailable notebook", error);
        }
        try {
            persisted = new JSONObject(content.state()).getJSONObject("archive").toString();
            recovery = new NativeStateStore(this, saved == null ? null : saved.getString("notesRecoveryId"));
            recovery.activate();
            if (saved != null && saved.containsKey("notesRecoveryId")) {
                String state = recovery.read();
                String archive = new JSONObject(state).getJSONObject("archive").toString();
                if (archive.equals(persisted) && !blocked) content.restore(state);
                else nativeStorageResult(content.token(), "Draft recovery conflicts with saved notebook; saved documents retained");
            }
        } catch (Exception error) {
            nativeStorageResult(content.token(), "Draft recovery unavailable; saved documents retained");
        }
        content.setStateListener(this::persist);
    }
    private void persist(String state) {
        try {
            JSONObject value = new JSONObject(state);
            String document = value.getJSONObject("archive").toString();
            if (document.equals(persisted) && !value.getBoolean("storage_pending")) return;
            boolean replace = value.getBoolean("replacement_confirmed");
            if (blocked && !replace) throw new java.io.IOException("Stored notebook unavailable; export a backup or confirm Import");
            documents.write(document, replace);
            persisted = document;
            blocked = false;
            nativeStorageResult(content.token(), null);
        } catch (Exception error) {
            nativeStorageResult(content.token(), error.getMessage());
            android.util.Log.e("JadixNotes", "Notebook save failed", error);
        }
    }
    @Override protected void onSaveInstanceState(Bundle saved) {
        try {
            recovery.write(content.state());
            saved.putString("notesRecoveryId", recovery.id);
        } catch (Exception error) { android.util.Log.e("JadixNotes", "Draft recovery save failed", error); }
        super.onSaveInstanceState(saved);
    }
    @Override protected void onDestroy() {
        content.setStateListener(null);
        content.close();
        if (recovery != null) { if (isFinishing()) recovery.delete(); recovery.close(); }
        super.onDestroy();
    }
}
