package dev.jadix.nativeui;

import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import java.io.File;
import java.nio.file.Files;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONObject;

public class NotesChecks extends Instrumentation {
    protected final java.util.ArrayList<NotesActivity> opened = new java.util.ArrayList<>();
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    protected void require(boolean condition, String message) { if (!condition) throw new AssertionError(message); }
    protected void ui(Runnable action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
    }
    protected JSONObject state(NotesActivity activity) {
        try { return new JSONObject(activity.content.state()); } catch (Exception error) { throw new AssertionError(error); }
    }
    protected View find(View view, String label, boolean field) {
        if (field && view instanceof EditText && label.contentEquals(((EditText)view).getHint())) return view;
        if (!field && view instanceof Button && label.contentEquals(((Button)view).getText())) return view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup)view;
            for (int index = 0; index < group.getChildCount(); index++) { View result = find(group.getChildAt(index), label, field); if (result != null) return result; }
        }
        return null;
    }
    protected void click(NotesActivity activity, String label) {
        View root = activity.content.activeDialog() == null ? activity.content : activity.content.activeDialog().getWindow().getDecorView();
        View button = find(root, label, false); require(button != null, "Missing button: " + label + " state=" + activity.content.state() + " callback=" + activity.content.takeError()); button.performClick();
    }
    protected void edit(NotesActivity activity, String hint, String text) {
        EditText field = (EditText)find(activity.content, hint, true); require(field != null, "Missing input: " + hint);
        field.requestFocus(); field.selectAll();
        android.view.inputmethod.InputConnection connection = field.onCreateInputConnection(new android.view.inputmethod.EditorInfo());
        require(connection != null, "Missing InputConnection");
        connection.commitText(text, 1);
    }
    protected NotesActivity open() {
        NotesActivity activity = (NotesActivity)startActivitySync(new Intent(getTargetContext(), NotesActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        opened.add(activity);
        waitForIdleSync(); return activity;
    }
    @Override public void onStart() {
        Bundle report = new Bundle();
        File document = new File(getTargetContext().getFilesDir(), "jadix-notes-v1.json");
        java.util.LinkedHashMap<File, byte[]> originals = new java.util.LinkedHashMap<>();
        boolean passed = false;
        try {
            for (String suffix : new String[] {"", ".new", ".bak"}) {
                File path = new File(document.getPath() + suffix);
                originals.put(path, path.exists() ? Files.readAllBytes(path.toPath()) : null);
            }
            NotesActivity first = open();
            ui(() -> {
                edit(first, "Notebook JSON for export or import", "{\"schema\":1,\"next_id\":\"1\",\"notes\":[]}");
                click(first, "Import notebook"); click(first, "Confirm");
                edit(first, "Title / 标题", "Android 中文😀");
                edit(first, "Write a note / 正文", "First line\n第二行😀");
                click(first, "Save note");
                require(!state(first).optBoolean("storage_pending"), "Durable save acknowledged");
                click(first, "New note");
                edit(first, "Title / 标题", "Second"); edit(first, "Write a note / 正文", "safe"); click(first, "Save note");
                click(first, "Open: Android 中文😀");
                require(state(first).optJSONObject("draft").optString("body").equals("First line\n第二行😀"), "Unicode reopen");
                edit(first, "Write a note / 正文", "unsaved");
                click(first, "Open: Second");
                require(state(first).optString("selected").equals("1"), "Dirty navigation guarded");
                click(first, "Revert draft");
                click(first, "Delete note"); click(first, "Cancel");
                require(state(first).optJSONObject("archive").optJSONArray("notes").length() == 2, "Delete cancel");
                click(first, "Export notebook");
                edit(first, "Notebook JSON for export or import", "{}"); click(first, "Import notebook"); click(first, "Confirm");
                require(state(first).optJSONObject("archive").optJSONArray("notes").length() == 2, "Invalid import preserved notes");
                edit(first, "Write a note / 正文", "recreated unsaved draft");
            });
            ActivityMonitor monitor = addMonitor(NotesActivity.class.getName(), null, false);
            ui(first::recreate);
            NotesActivity recreated = (NotesActivity)waitForMonitorWithTimeout(monitor, 10000);
            removeMonitor(monitor);
            require(recreated != null, "Activity recreation completed");
            opened.add(recreated);
            waitForIdleSync();
            ui(() -> {
                require(state(recreated).optJSONObject("draft").optString("body").equals("recreated unsaved draft"), "Matching durable archive restores unsaved draft");
                require(state(recreated).optBoolean("dirty"), "Recovered draft remains unsaved");
                require(state(recreated).optJSONObject("archive").optJSONArray("notes").length() == 2, "Recreation preserves saved documents");
                recreated.finish();
            });
            waitForIdleSync();
            NotesActivity second = open();
            ui(() -> {
                require(state(second).optJSONObject("archive").optJSONArray("notes").length() == 2, "Normal finish/relaunch preserved documents");
                require(state(second).isNull("selected"), "Fresh editor separate from documents");
                click(second, "Open: Second");
                click(second, "Delete note"); click(second, "Confirm");
                require(state(second).optJSONObject("archive").optJSONArray("notes").length() == 1, "Confirmed delete");
                second.finish();
            });
            waitForIdleSync();
            NoteDocumentStore writer = new NoteDocumentStore(getTargetContext()); writer.read();
            NoteDocumentStore stale = new NoteDocumentStore(getTargetContext()); stale.read();
            writer.write("{\"schema\":1,\"next_id\":\"1\",\"notes\":[]}", false);
            try { stale.write("{}", false); throw new AssertionError("Stale writer accepted"); } catch (java.io.IOException expected) {}
            String good = writer.read();
            try { writer.write(new String(new char[NoteDocumentStore.MAX_BYTES+1]).replace('\0','x'), false); throw new AssertionError("Oversized storage accepted"); } catch (java.io.IOException expected) {}
            require(writer.read().equals(good), "Rejected write preserved prior file");
            Files.write(document.toPath(), new byte[] {(byte)0xc3,(byte)0x28});
            NotesActivity corrupt = open();
            ui(() -> {
                require(state(corrupt).optString("status").contains("Storage failed"), "Corruption surfaced");
                edit(corrupt, "Title / 标题", "Must not overwrite"); click(corrupt, "Save note");
                require(state(corrupt).optBoolean("storage_pending"), "Corrupt-file save refused");
            });
            require(Files.readAllBytes(document.toPath()).length == 2, "Corrupt bytes preserved");
            ui(() -> {
                edit(corrupt, "Notebook JSON for export or import", "{\"schema\":1,\"next_id\":\"1\",\"notes\":[]}");
                click(corrupt, "Import notebook"); click(corrupt, "Confirm");
                require(!state(corrupt).optBoolean("storage_pending"), "Explicit replacement recovered storage");
                require(!state(corrupt).optBoolean("replacement_confirmed"), "Replacement authorization consumed");
                corrupt.finish();
            });
            waitForIdleSync();
            report.putString("checks", "Unicode InputConnection multi-document CRUD; dirty guard; delete cancel/confirm; invalid import; recreate with unsaved draft; normal finish/relaunch durable documents; stale writer; size limit; corrupt UTF-8 preservation and explicit replacement");
            passed = true;
        } catch (Throwable error) { report.putString("failure", android.util.Log.getStackTraceString(error)); }
        finally {
            try {
                ui(() -> { for (NotesActivity activity : opened) if (!activity.isFinishing()) activity.finish(); });
                waitForIdleSync();
                for (java.util.Map.Entry<File, byte[]> entry : originals.entrySet()) {
                    if (entry.getValue() == null) Files.deleteIfExists(entry.getKey().toPath());
                    else Files.write(entry.getKey().toPath(), entry.getValue());
                }
            } catch (Exception error) { passed = false; report.putString("failure", "Could not restore original test documents: " + error); }
        }
        if (passed) report.putString("status", "JADIX_NOTES_CHECKS_PASS");
        finish(passed ? ActivityResult.OK : ActivityResult.FAILED, report);
    }
    private static final class ActivityResult { static final int OK = -1; static final int FAILED = 0; }
}
