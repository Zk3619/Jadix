package dev.jadix.nativeui;

import android.os.Bundle;
import android.os.SystemClock;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewTreeObserver;
import android.widget.Button;
import android.widget.EditText;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Iterator;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;
import org.json.JSONObject;

public final class NotesScaleChecks extends NotesChecks {
    private Bundle arguments;
    @Override public void onCreate(Bundle value) { arguments = value; super.onCreate(value); }

    private boolean same(Object expected, Object actual) {
        try {
            if (expected instanceof JSONObject && actual instanceof JSONObject) {
                JSONObject left = (JSONObject)expected, right = (JSONObject)actual;
                if (left.length() != right.length()) return false;
                Iterator<String> keys = left.keys();
                while (keys.hasNext()) { String key = keys.next(); if (!right.has(key) || !same(left.get(key), right.get(key))) return false; }
                return true;
            }
            if (expected instanceof JSONArray && actual instanceof JSONArray) {
                JSONArray left = (JSONArray)expected, right = (JSONArray)actual;
                if (left.length() != right.length()) return false;
                for (int index = 0; index < left.length(); index++) if (!same(left.get(index), right.get(index))) return false;
                return true;
            }
            return expected.equals(actual);
        } catch (Exception error) { throw new AssertionError(error); }
    }

    private int openButtons(View view) {
        int count = view instanceof Button && ((Button)view).getText().toString().startsWith("Open: Load ") ? 1 : 0;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup)view;
            for (int index = 0; index < group.getChildCount(); index++) count += openButtons(group.getChildAt(index));
        }
        return count;
    }

    @Override public void onStart() {
        Bundle report = new Bundle();
        File document = new File(getTargetContext().getFilesDir(), "jadix-notes-v1.json");
        java.util.LinkedHashMap<File, byte[]> originals = new java.util.LinkedHashMap<>();
        boolean passed = false;
        try {
            String name = arguments.getString("fixture", "");
            require(name.matches("jadix-notes-scale-[0-9a-f]{32}\\.json"), "Invalid fixture name");
            File fixturePath = new File(getTargetContext().getCacheDir(), name).getCanonicalFile();
            require(fixturePath.getParentFile().equals(getTargetContext().getCacheDir().getCanonicalFile()), "Fixture escaped cache");
            require(Files.size(fixturePath.toPath()) == 512 * 1024 - 2048, "Expected near-limit fixture");
            String source = new String(Files.readAllBytes(fixturePath.toPath()), StandardCharsets.UTF_8);
            JSONObject fixture = new JSONObject(source);
            for (String suffix : new String[] {"", ".new", ".bak"}) {
                File path = new File(document.getPath() + suffix);
                originals.put(path, path.exists() ? Files.readAllBytes(path.toPath()) : null);
            }
            NotesActivity first = open();
            ui(() -> {
                edit(first, "Notebook JSON for export or import", source);
                click(first, "Import notebook"); click(first, "Confirm");
                require(same(fixture, state(first).optJSONObject("archive")), "Imported fixture archive");
                require(!state(first).optBoolean("storage_pending") && openButtons(first.content) == 128, "128 real buttons and acknowledged storage");
            });
            byte[] before = Files.readAllBytes(document.toPath());
            ui(() -> {
                click(first, "New note"); edit(first, "Title / 标题", "Overflow"); edit(first, "Write a note / 正文", "Retained draft"); click(first, "Save note");
                require(state(first).optString("status").contains("full") && state(first).optBoolean("dirty"), "129th note rejected with retained draft");
                require(same(fixture, state(first).optJSONObject("archive")), "Full rejection atomic");
                click(first, "Revert draft"); click(first, "Open: Load 001 中文😀");
                edit(first, "Write a note / 正文", new String(new char[32768]).replace('\0', 'm')); click(first, "Save note");
                require(state(first).optString("status").contains("512 KiB") && same(fixture, state(first).optJSONObject("archive")), "Aggregate rejection atomic");
                click(first, "Revert draft");
            });
            require(java.util.Arrays.equals(before, Files.readAllBytes(document.toPath())), "Capacity failures preserve durable bytes");
            JSONArray dispatch = new JSONArray(), draws = new JSONArray();
            EditText retained = (EditText)find(first.content, "Write a note / 正文", true);
            for (int index = 0; index < 30; index++) {
                CountDownLatch drawn = new CountDownLatch(1);
                long[] timing = new long[3];
                ui(() -> {
                    ViewTreeObserver.OnDrawListener observer = new ViewTreeObserver.OnDrawListener() {
                        @Override public void onDraw() {
                            if (timing[2] != 0) return;
                            timing[2] = SystemClock.elapsedRealtimeNanos(); drawn.countDown();
                            first.content.post(() -> first.content.getViewTreeObserver().removeOnDrawListener(this));
                        }
                    };
                    first.content.getViewTreeObserver().addOnDrawListener(observer);
                    retained.requestFocus(); retained.setSelection(retained.length());
                    timing[0] = SystemClock.elapsedRealtimeNanos();
                    retained.onCreateInputConnection(new android.view.inputmethod.EditorInfo()).commitText("x", 1);
                    timing[1] = SystemClock.elapsedRealtimeNanos();
                });
                require(drawn.await(10, TimeUnit.SECONDS), "Native input produced a draw callback");
                waitForIdleSync();
                dispatch.put((timing[1] - timing[0]) / 1000000.0); draws.put((timing[2] - timing[0]) / 1000000.0);
            }
            ui(() -> {
                require(find(first.content, "Write a note / 正文", true) == retained && state(first).optBoolean("dirty"), "Edits retain native editor and Rust draft");
                click(first, "Revert draft");
                for (int index = 1; index <= 30; index++) {
                    click(first, String.format(java.util.Locale.ROOT, "Open: Load %03d 中文😀", index));
                    require(state(first).optString("selected").equals(Integer.toString(index)), "Opened saved identity");
                }
                require(find(first.content, "Write a note / 正文", true) == retained && openButtons(first.content) == 128, "Open actions retain editor and row count");
                click(first, "Open: Load 001 中文😀");
                edit(first, "Write a note / 正文", "near-limit recreated draft 中文😀");
            });
            ActivityMonitor monitor = addMonitor(NotesActivity.class.getName(), null, false);
            ui(first::recreate);
            NotesActivity recreated = (NotesActivity)waitForMonitorWithTimeout(monitor, 10000);
            removeMonitor(monitor);
            require(recreated != null, "Large notebook recreation completed"); opened.add(recreated); waitForIdleSync();
            ui(() -> {
                require(same(fixture, state(recreated).optJSONObject("archive")), "Recreate preserves large archive");
                require(state(recreated).optJSONObject("draft").optString("body").equals("near-limit recreated draft 中文😀") && state(recreated).optBoolean("dirty"), "Recreate restores unsaved Rust draft");
                recreated.finish();
            });
            waitForIdleSync();
            NotesActivity fresh = open();
            ui(() -> {
                require(same(fixture, state(fresh).optJSONObject("archive")) && state(fresh).isNull("selected"), "Normal finish/relaunch preserves large document with fresh draft");
                click(fresh, "Open: Load 128 中文😀"); click(fresh, "Delete note"); click(fresh, "Confirm");
                click(fresh, "New note"); edit(fresh, "Title / 标题", "Replacement 129"); edit(fresh, "Write a note / 正文", "Identity not reused 中文😀"); click(fresh, "Save note");
                require(state(fresh).optString("selected").equals("129") && state(fresh).optJSONObject("archive").optJSONArray("notes").length() == 128, "Deleted identity not reused at capacity");
                click(fresh, "Export notebook");
                try { require(same(new JSONObject(((EditText)find(fresh.content, "Notebook JSON for export or import", true)).getText().toString()), state(fresh).optJSONObject("archive")), "Large export equals Rust archive"); } catch (Exception error) { throw new AssertionError(error); }
            });
            JSONObject metrics = new JSONObject(); metrics.put("input_dispatch_ms", dispatch); metrics.put("input_to_draw_callback_ms", draws); metrics.put("notes", 128); metrics.put("fixture_bytes", source.getBytes(StandardCharsets.UTF_8).length);
            report.putString("metrics", metrics.toString());
            report.putString("checks", "InputConnection near-limit import; 128 real rows; count/aggregate rejection; 30 edits and 30 opens; actual draw callbacks; large archive/draft recreate; normal finish/relaunch; non-reused identity and export");
            passed = true;
        } catch (Throwable error) { report.putString("failure", android.util.Log.getStackTraceString(error)); }
        finally {
            try {
                ui(() -> { for (NotesActivity activity : opened) if (!activity.isFinishing()) activity.finish(); }); waitForIdleSync();
                for (java.util.Map.Entry<File, byte[]> entry : originals.entrySet()) {
                    if (entry.getValue() == null) Files.deleteIfExists(entry.getKey().toPath()); else Files.write(entry.getKey().toPath(), entry.getValue());
                }
            } catch (Exception error) { passed = false; report.putString("failure", "Could not restore original test documents: " + error); }
        }
        if (passed) report.putString("status", "JADIX_NOTES_SCALE_PASS");
        finish(passed ? -1 : 0, report);
    }
}
