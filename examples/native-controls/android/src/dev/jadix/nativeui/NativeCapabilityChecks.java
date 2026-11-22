package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.text.SpannableStringBuilder;
import android.text.Spanned;
import android.text.style.StyleSpan;
import android.widget.FrameLayout;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;
import org.json.JSONArray;
import org.json.JSONObject;

/** Device checks for OS HTML import, WebView Promise evaluation and real media decoding. */
public final class NativeCapabilityChecks extends Instrumentation {
    private String token;
    private FrameLayout stage;
    private NativeRichEditor editor;
    private NativeWeb web;
    private NativeMedia media;
    private NativeServices audioServices;
    private android.media.AudioManager focusManager;
    private android.media.AudioFocusRequest competingFocus;
    private File sound;
    private final JSONArray checks = new JSONArray();
    private interface Action { void run() throws Exception; }

    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments); token = arguments == null ? null : arguments.getString("jadix.token"); start();
    }
    private static void require(boolean condition, String message) { if (!condition) throw new AssertionError(message); }
    private void ui(Action action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
        waitForIdleSync();
    }
    private void until(String message, BooleanSupplier condition) {
        long deadline = SystemClock.uptimeMillis() + 15000; boolean[] passed = {false};
        do { ui(() -> passed[0] = condition.getAsBoolean()); if (passed[0]) return; SystemClock.sleep(20); }
        while (SystemClock.uptimeMillis() < deadline);
        throw new AssertionError(message);
    }
    private JSONObject plan(String html) throws Exception {
        return (JSONObject) NativeRuntime.call(NativeRuntime.object("html").put("html", html));
    }
    private void markup(NativeView root) {
        ui(() -> {
            String[] documents = {
                "<p><b>中文😀</b> <i>العربية</i><u> link</u></p>",
                "<div><p></p><p><span style=\"color:#123456;background-color:#abcdef\">text</span></p></div>",
                "<ul><li>first</li><li><b>second</b></li></ul><blockquote>quote</blockquote>",
                "<pre>  a\n b</pre><h2>heading</h2><p><a href=\"https://example.invalid/a?b=1&amp;c=2\">safe</a></p>"
            };
            for (String input : documents) {
                JSONObject parsed = plan(input);
                String output = NativeRichMarkup.export(NativeRichMarkup.parse(parsed), image -> null, 1);
                require(plan(output).getString("html").equals(parsed.getString("html")), "Native HTML round trip changed markup: " + output);
            }
            JSONObject sanitized = plan("<p onclick=\"bad()\">safe<script>bad()</script><a href=\"javascript:bad()\">link</a></p>");
            String clean = sanitized.getString("html");
            require(!clean.contains("onclick") && !clean.contains("javascript:") && !clean.contains("<script"), "Unsafe HTML survived shared sanitizer");
            AtomicReference<String> changed = new AtomicReference<>();
            editor = new NativeRichEditor(root, (event, value) -> { if (event.equals("change")) changed.set(((JSONObject)value).optString("html")); });
            stage.addView(editor, new FrameLayout.LayoutParams(-1, 220));
            editor.update(new JSONObject().put("html", "<p>中文😀</p>").put("revision", 1).put("editable", true));
            SpannableStringBuilder inserted = new SpannableStringBuilder(" pasted");
            inserted.setSpan(new StyleSpan(android.graphics.Typeface.BOLD_ITALIC), 0, inserted.length(), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            editor.getText().append(inserted);
            // The posted change must export native pasted styles as well as imported markup.
            editor.post(() -> {
                String html = changed.get();
                if (html == null || !html.contains("font-weight:bold") || !html.contains("font-style:italic") || !html.contains("中文😀")) changed.set("FAILED:" + html);
            });
            editor.setTag(changed);
            checks.put("shared-html/native-round-trip");
        });
        until("Native rich edit did not export pasted formatting", () -> {
            @SuppressWarnings("unchecked") AtomicReference<String> changed = (AtomicReference<String>)editor.getTag();
            require(changed.get() == null || !changed.get().startsWith("FAILED:"), "Native pasted formatting was lost: " + changed.get());
            return changed.get() != null;
        });
        ui(() -> { editor.close(); stage.removeView(editor); editor = null; checks.put("native-rich-edit/pasted-style"); });
    }
    // Isolated real root; inspect recognizer ownership without dispatching fixture IDs to the business app.
    @SuppressWarnings("unchecked")
    private static void selectionNode(NativeView root, String id, JSONObject descriptor, android.view.View view) {
        ((java.util.Map<String, JSONObject>)field(root, "descriptors")).put(id, descriptor);
        ((java.util.Map<String, android.view.View>)field(root, "views")).put(id, view);
    }
    private void pointer(long down, int action, float x, float y) {
        android.view.MotionEvent event = android.view.MotionEvent.obtain(down, SystemClock.uptimeMillis(), action, x, y, 0);
        event.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
        try { require(getUiAutomation().injectInputEvent(event, true), "OS touch injection failed"); } finally { event.recycle(); }
    }
    private void selection(NativeView root) {
        getUiAutomation(); // Connect before timestamping the first touch sample.
        android.accessibilityservice.AccessibilityServiceInfo accessibility = getUiAutomation().getServiceInfo();
        accessibility.flags |= android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS;
        getUiAutomation().setServiceInfo(accessibility);
        NativeView[] fixture = {null}; android.widget.TextView[] text = {null};
        android.content.ClipboardManager[] service = {null};
        ui(() -> service[0] = (android.content.ClipboardManager)getTargetContext().getSystemService(android.content.Context.CLIPBOARD_SERVICE));
        android.content.ClipboardManager clipboard = service[0];
        android.content.ClipData[] prior = {null};
        ui(() -> prior[0] = clipboard.getPrimaryClip());
        try {
            for (boolean rich : new boolean[] {false, true}) {
                ui(() -> {
                    NativeView view = new NativeView(root.getContext()); fixture[0] = view;
                    view.setTag("ancestor"); view.setBackgroundColor(android.graphics.Color.WHITE);
                    selectionNode(view, "ancestor", new JSONObject().put("kind", "box").put("interaction", new JSONObject().put("press", true)), view);
                    stage.addView(view, new FrameLayout.LayoutParams(-1, -1));
                    android.widget.TextView field;
                    if (rich) {
                        editor = new NativeRichEditor(view, (event, value) -> {});
                        editor.update(new JSONObject().put("html", "<p>alpha bravo charlie delta</p>").put("revision", 1).put("editable", true));
                        editor.setShowSoftInputOnFocus(false); field = editor;
                    } else { field = new android.widget.TextView(root.getContext()); field.setText("alpha bravo charlie delta"); field.setTextIsSelectable(true); NativeSelection.install(field); }
                    text[0] = field; field.setTag("selection-text"); field.setTextSize(24); field.setTextColor(android.graphics.Color.BLACK);
                    field.setPadding(24, 24, 24, 24);
                    selectionNode(view, "selection-text", new JSONObject().put("kind", rich ? "hosted" : "text"), field);
                    FrameLayout.LayoutParams position = new FrameLayout.LayoutParams(-1, (int)(180 * root.density())); position.topMargin = (int)(90 * root.density());
                    view.addView(field, position); field.requestFocus();
                });
                until("Selection fixture did not lay out", () -> text[0].getLayout() != null && text[0].getWidth() > 0);
                float[] points = new float[4];
                ui(() -> {
                    android.widget.TextView field = text[0]; int[] origin = new int[2]; field.getLocationOnScreen(origin);
                    android.text.Layout layout = field.getLayout();
                    points[0] = origin[0] + field.getTotalPaddingLeft() + layout.getPrimaryHorizontal(2);
                    points[1] = origin[1] + field.getTotalPaddingTop() + (layout.getLineTop(0) + layout.getLineBottom(0)) / 2f;
                });
                long down = SystemClock.uptimeMillis(); pointer(down, android.view.MotionEvent.ACTION_DOWN, points[0], points[1]);
                ui(() -> require(rich ? field(field(fixture[0], "interactions"), "press") == null : "ancestor".equals(field(field(fixture[0], "interactions"), "press")), "Initial press ownership differs from native text boundary"));
                SystemClock.sleep(android.view.ViewConfiguration.getLongPressTimeout() + 200);
                pointer(down, android.view.MotionEvent.ACTION_UP, points[0], points[1]);
                until("OS long press did not select the word", () -> text[0].getSelectionStart() == 0 && text[0].getSelectionEnd() >= 5 && text[0].getSelectionEnd() < 19);
                SystemClock.sleep(350);
                ui(() -> {
                    android.widget.TextView field = text[0]; int[] origin = new int[2]; field.getLocationOnScreen(origin);
                    android.content.res.TypedArray theme = field.getContext().obtainStyledAttributes(new int[] {android.R.attr.textSelectHandleRight});
                    android.graphics.drawable.Drawable handle = theme.getDrawable(0); theme.recycle();
                    require(handle != null, "OS selection handle drawable missing");
                    android.text.Layout layout = field.getLayout();
                    // Android's smart selection may extend the word after starting ActionMode.
                    int end = field.getSelectionEnd();
                    require(field.getSelectionStart() == 0 && end >= 5 && end < 19, "Unexpected OS smart selection");
                    points[0] = origin[0] + field.getTotalPaddingLeft() + layout.getPrimaryHorizontal(end) + handle.getIntrinsicWidth() / 4f;
                    points[1] = origin[1] + field.getTotalPaddingTop() + layout.getLineBottom(0) + handle.getIntrinsicHeight() / 2f;
                    points[2] = origin[0] + field.getTotalPaddingLeft() + layout.getPrimaryHorizontal(19) + handle.getIntrinsicWidth() / 4f;
                    points[3] = points[1];
                });
                android.graphics.Bitmap selectionImage = getUiAutomation().takeScreenshot();
                try (FileOutputStream capture = new FileOutputStream(new File(getTargetContext().getFilesDir(), "selection-" + rich + ".png"))) {
                    selectionImage.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, capture);
                } catch (java.io.IOException error) { throw new AssertionError(error); } finally { selectionImage.recycle(); }
                down = SystemClock.uptimeMillis(); pointer(down, android.view.MotionEvent.ACTION_DOWN, points[0], points[1]);
                for (int step = 1; step <= 12; step++) { SystemClock.sleep(25); pointer(down, android.view.MotionEvent.ACTION_MOVE, points[0] + (points[2] - points[0]) * step / 12, points[1]); }
                SystemClock.sleep(200); pointer(down, android.view.MotionEvent.ACTION_UP, points[2], points[3]);
                until("Dragging the OS end handle did not extend selection (rich=" + rich + ", coordinates=" + java.util.Arrays.toString(points) + ")", () -> text[0].getSelectionStart() == 0 && text[0].getSelectionEnd() >= 17);
                String[] selected = {null}; ui(() -> {
                    selected[0] = text[0].getText().subSequence(text[0].getSelectionStart(), text[0].getSelectionEnd()).toString();
                    require(field(field(fixture[0], "interactions"), "press") == null, "Selection retained ancestor press ownership");
                });
                boolean copied = false; long menuDeadline = SystemClock.uptimeMillis() + 5000;
                while (!copied && SystemClock.uptimeMillis() < menuDeadline) {
                    for (android.view.accessibility.AccessibilityWindowInfo window : getUiAutomation().getWindows()) {
                        android.view.accessibility.AccessibilityNodeInfo contents = window.getRoot();
                        if (contents != null) {
                            for (android.view.accessibility.AccessibilityNodeInfo item : contents.findAccessibilityNodeInfosByText("Copy")) {
                                if (item.isClickable() && ("Copy".equalsIgnoreCase(String.valueOf(item.getText())) || "Copy".equalsIgnoreCase(String.valueOf(item.getContentDescription())))) copied |= item.performAction(android.view.accessibility.AccessibilityNodeInfo.ACTION_CLICK);
                                item.recycle();
                            }
                            contents.recycle();
                        }
                        window.recycle();
                    }
                    if (!copied) SystemClock.sleep(100);
                }
                require(copied, "OS Copy action unavailable (rich=" + rich + ")");
                until("OS Copy did not publish selected text", () -> clipboard.hasPrimaryClip() && selected[0].contentEquals(clipboard.getPrimaryClip().getItemAt(0).coerceToText(getTargetContext())));
                ui(() -> { if (editor != null) { editor.close(); editor = null; } fixture[0].close(); stage.removeView(fixture[0]); fixture[0] = null; });
            }
            checks.put("selection/os-long-press-handle-copy-ancestor-handoff");
        } finally {
            ui(() -> { if (editor != null) { editor.close(); editor = null; } if (fixture[0] != null) { fixture[0].close(); stage.removeView(fixture[0]); } if (prior[0] != null) clipboard.setPrimaryClip(prior[0]); else if (android.os.Build.VERSION.SDK_INT >= 28) clipboard.clearPrimaryClip(); else clipboard.setPrimaryClip(android.content.ClipData.newPlainText("", "")); });
        }
    }
    private JSONObject document(String html) throws Exception {
        return new JSONObject().put("url", JSONObject.NULL).put("base_url", "https://jadix.invalid/").put("origins", new JSONArray()).put("html", html);
    }
    private void browser(NativeView root) {
        AtomicReference<String> value = new AtomicReference<>(), failure = new AtomicReference<>();
        ui(() -> { web = new NativeWeb(root, "capability-" + token, (event, data) -> {}); stage.addView(web, new FrameLayout.LayoutParams(-1, 320)); web.configure(document("<html><body>Jadix Promise fixture</body></html>")); });
        until("WebView message bridge did not become ready", () -> web.evaluationReady());
        ui(() -> web.evaluate("new Promise(resolve=>setTimeout(()=>resolve({text:'中文😀',value:42}),30))", value::set, failure::set));
        until("Web Promise did not resolve", () -> { require(failure.get() == null, "Web evaluation failed: " + failure.get()); return value.get() != null; });
        ui(() -> { JSONObject reply = new JSONObject(value.get()); require(reply.getInt("value") == 42 && reply.getString("text").equals("中文😀"), "Web Promise result changed"); checks.put("web/async-promise"); });
        value.set(null); failure.set(null);
        ui(() -> web.evaluate("Promise.reject(new Error('expected rejection'))", value::set, failure::set));
        until("Web Promise rejection was not delivered", () -> failure.get() != null);
        require(failure.get().contains("expected rejection") && value.get() == null, "Web rejection resolved incorrectly");
        failure.set(null);
        ui(() -> { web.evaluate("new Promise(()=>{})", value::set, failure::set); web.configure(document("<html><body>replacement</body></html>")); });
        require(failure.get() != null && value.get() == null, "Navigation did not cancel pending Promise");
        until("Replacement WebView document did not become ready", () -> web.evaluationReady());
        failure.set(null);
        ui(() -> { web.evaluate("new Promise(()=>{})", value::set, failure::set); web.close(); });
        require(failure.get() != null && value.get() == null, "Disposal did not cancel pending Promise");
        ui(() -> { web.close(); web = null; checks.put("web/rejection-navigation-disposal"); });
    }
    private File wave() throws Exception {
        int frames = 48000, bytes = frames * 2;
        ByteBuffer data = ByteBuffer.allocate(44 + bytes).order(ByteOrder.LITTLE_ENDIAN);
        data.put("RIFF".getBytes(StandardCharsets.US_ASCII)).putInt(36 + bytes).put("WAVEfmt ".getBytes(StandardCharsets.US_ASCII));
        data.putInt(16).putShort((short)1).putShort((short)1).putInt(48000).putInt(96000).putShort((short)2).putShort((short)16);
        data.put("data".getBytes(StandardCharsets.US_ASCII)).putInt(bytes);
        for (int frame = 0; frame < frames; frame++) data.putShort((short)(Math.sin(2 * Math.PI * 440 * frame / 48000) * 256));
        File file = File.createTempFile("jadix-capability-", ".wav", getTargetContext().getCacheDir());
        try (FileOutputStream stream = new FileOutputStream(file)) { stream.write(data.array()); }
        return file;
    }
    private void playback(NativeView root) throws Exception {
        sound = wave(); ArrayList<String> events = new ArrayList<>(); AtomicReference<String> error = new AtomicReference<>();
        JSONObject specification = new JSONObject().put("source", sound.toURI().toString()).put("audio", true).put("paused", false)
            .put("muted", true).put("volume", .25).put("rate", 1).put("looping", false).put("controls", false)
            .put("seek", JSONObject.NULL).put("seek_revision", 0).put("progress_interval_ms", 50);
        ui(() -> { media = new NativeMedia(root, (event, data) -> { events.add(event); if (event.equals("error")) error.set(String.valueOf(data)); }); stage.addView(media, new FrameLayout.LayoutParams(240, 120)); media.configure(specification); });
        until("Native MediaPlayer did not decode WAV to completion", () -> { require(error.get() == null, "Media decode failed: " + error.get()); return events.contains("ended"); });
        ui(() -> {
            require(media.getDuration() >= 900 && media.getDuration() <= 1100 && media.getAudioSessionId() > 0, "Decoded media metadata invalid");
            specification.put("seek", .1).put("seek_revision", 1).put("looping", true).put("rate", 1.25);
            media.configure(specification);
        });
        until("Controlled seek after completion did not resume playback", () -> media.isPlaying());
        SystemClock.sleep(1100);
        ui(() -> { require(media.isPlaying(), "Looped playback stopped"); media.pause(); require(!media.isPlaying(), "Native pause failed"); media.start(); require(media.isPlaying(), "Native resume failed"); media.close(); require(!media.isPlaying(), "Disposed player still active"); checks.put("media/decode-ended-seek-rate-loop-pause-dispose"); });
        int count = events.size(); SystemClock.sleep(300);
        ui(() -> { require(events.size() == count, "Disposed media delivered a late event"); stage.removeView(media); media = null; });
    }
    // Readback stays in instrumentation; production audio retains its actual OS callbacks.
    private static Object field(Object owner, String name) {
        try { java.lang.reflect.Field field = owner.getClass().getDeclaredField(name); field.setAccessible(true); return field.get(owner); }
        catch (ReflectiveOperationException error) { throw new AssertionError(error); }
    }
    private Object stream(int id) { return ((java.util.Map<?, ?>)field(audioServices.audio, "players")).get(id); }
    private android.media.MediaPlayer player(int id) { Object stream = stream(id); require(stream != null, "Audio stream was removed"); return (android.media.MediaPlayer)field(stream, "player"); }
    private void audio(int id, String action, Object... entries) throws Exception {
        JSONObject command = NativeServices.object("id", id, "action", action);
        for (int i = 0; i < entries.length; i += 2) command.put((String)entries[i], entries[i + 1]);
        audioServices.command(NativeServices.object("service", "audio", "command", command));
    }
    private void audioServices(NativeView root) throws Exception {
        byte[] encoded = java.nio.file.Files.readAllBytes(sound.toPath());
        JSONObject source = NativeServices.object("type", "bytes", "base64", android.util.Base64.encodeToString(encoded, android.util.Base64.NO_WRAP));
        JSONObject settings = NativeServices.object("volume", .1, "muted", true, "looping", true, "rate", 1);
        ui(() -> { audioServices = new NativeServices(root); audio(700001, "load", "source", source); });
        until("Sound preload did not decode PCM", () -> {
            Object sound = ((java.util.Map<?, ?>)field(audioServices.audio, "sounds")).get(700001);
            return sound != null && field(sound, "pcm") != null;
        });
        ui(() -> {
            NativeSoundMixer mixer = (NativeSoundMixer)field(audioServices.audio, "mixer");
            synchronized (mixer) {
                for (int i = 0; i < 40; i++) audio(700001, "play", "volume", .1, "rate", 1);
                Object[] voices = (Object[])field(mixer, "voices"); int active = 0;
                for (Object voice : voices) if (field(voice, "pcm") != null) active++;
                require(voices.length == 32 && active == 32, "Sound overlap exceeded or failed the 32-voice pool");
                audio(700001, "stop");
                for (Object voice : voices) require(field(voice, "pcm") == null, "Sound stop retained a voice");
            }
            audio(700001, "unload"); require(((Number)field(audioServices.audio, "decodedBytes")).intValue() == 0, "Unloaded PCM retained its quota");
            checks.put("sound/bytes-decode-overlap-32-voices-stop-unload");
            audio(700002, "open", "source", source, "settings", settings); audio(700002, "preload");
        });
        until("AudioPlayer preload did not finish", () -> Boolean.TRUE.equals(field(stream(700002), "ready")));
        ui(() -> {
            require(!player(700002).isPlaying(), "Preload started audio");
            settings.put("rate", 1.25); audio(700002, "set", "settings", settings);
            require(!player(700002).isPlaying(), "Setting rate started paused audio");
            audio(700002, "seek", "position", .2); audio(700002, "resume");
        });
        until("AudioPlayer did not resume", () -> player(700002).isPlaying());
        ui(() -> {
            require(Math.abs(player(700002).getPlaybackParams().getSpeed() - 1.25) < .01 && player(700002).isLooping(), "Audio settings were not applied");
            focusManager = (android.media.AudioManager)root.getContext().getSystemService(android.content.Context.AUDIO_SERVICE);
            competingFocus = new android.media.AudioFocusRequest.Builder(android.media.AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
                .setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).build())
                .setOnAudioFocusChangeListener(change -> {}).build();
            require(focusManager.requestAudioFocus(competingFocus) == android.media.AudioManager.AUDIOFOCUS_REQUEST_GRANTED, "Competing OS focus request denied");
        });
        until("OS transient focus loss did not pause audio", () -> !player(700002).isPlaying() && Boolean.TRUE.equals(field(audioServices.audio, "transientLoss")));
        ui(() -> { focusManager.abandonAudioFocusRequest(competingFocus); competingFocus = null; });
        until("OS focus gain did not resume audio", () -> player(700002).isPlaying());
        ui(() -> {
            audio(700002, "pause"); require(!player(700002).isPlaying(), "Explicit audio pause failed");
            Object stream = stream(700002); audio(700002, "close");
            require(stream(700002) == null && field(stream, "player") == null && field(stream, "bytes") == null, "Closed stream retained decoder/source");
            audioServices.close(); audioServices.close(); audioServices = null;
            checks.put("audio/preload-paused-rate-seek-loop-os-focus-dispose");
        });
    }
    private void syntheticTilt() {
        ui(() -> {
            android.view.MotionEvent.PointerProperties pointer = new android.view.MotionEvent.PointerProperties();
            pointer.id = 4;
            android.view.MotionEvent.PointerCoords coordinates = new android.view.MotionEvent.PointerCoords();
            coordinates.pressure = .5f;
            coordinates.setAxisValue(android.view.MotionEvent.AXIS_TILT, (float)Math.PI / 4);
            for (int tool : new int[] {android.view.MotionEvent.TOOL_TYPE_STYLUS, android.view.MotionEvent.TOOL_TYPE_ERASER,
                    android.view.MotionEvent.TOOL_TYPE_FINGER, android.view.MotionEvent.TOOL_TYPE_MOUSE}) {
                pointer.toolType = tool;
                for (float orientation : new float[] {0, (float)Math.PI / 2}) {
                    coordinates.orientation = orientation;
                    android.view.MotionEvent event = android.view.MotionEvent.obtain(1, 2, android.view.MotionEvent.ACTION_MOVE, 1,
                        new android.view.MotionEvent.PointerProperties[] {pointer}, new android.view.MotionEvent.PointerCoords[] {coordinates},
                        0, 0, 1, 1, 0, 0, android.view.InputDevice.SOURCE_STYLUS, 0);
                    try {
                        JSONArray tilt = NativeInteraction.tilt(event, 0);
                        boolean pen = tool == android.view.MotionEvent.TOOL_TYPE_STYLUS || tool == android.view.MotionEvent.TOOL_TYPE_ERASER;
                        double x = pen && orientation != 0 ? 45 : 0, y = pen && orientation == 0 ? -45 : 0;
                        require(Math.abs(tilt.getDouble(0) - x) < .001 && Math.abs(tilt.getDouble(1) - y) < .001,
                            "Synthetic MotionEvent tilt projection changed: " + tilt);
                    } finally { event.recycle(); }
                }
            }
            checks.put("touch/synthetic-motion-event-tilt");
        });
    }
    @SuppressWarnings("unchecked")
    private void notificationPermission(NativeView root) {
        ui(() -> {
            NativeServices services = new NativeServices(root);
            try {
                java.util.ArrayDeque<JSONObject> events = (java.util.ArrayDeque<JSONObject>)field(services, "events");
                services.system.command(new JSONObject().put("id", 701).put("action", "permission").put("request", false));
                JSONObject baseline = events.removeLast().getJSONObject("event");
                require(!"unknown".equals(baseline.getString("authorization")) && baseline.isNull("error"), "OS authorization query failed");
                java.lang.reflect.Method permission = NativeSystem.class.getDeclaredMethod("permission", int.class, String.class);
                permission.setAccessible(true);
                for (String error : new String[] {"failed", "denied", "unavailable"}) {
                    permission.invoke(services.system, 702, error);
                    JSONObject reply = events.removeLast().getJSONObject("event");
                    require(baseline.getString("authorization").equals(reply.getString("authorization")) && error.equals(reply.getString("error")), "Request error lost OS authorization");
                }
                NativeView detachedRoot = new NativeView(root.getContext().getApplicationContext());
                NativeServices detached = new NativeServices(detachedRoot);
                try {
                    int[] completed = {0};
                    detached.permission("dev.jadix.permission.UNAVAILABLE", (granted, error) -> {
                        completed[0]++;
                        require(!granted && "unavailable".equals(error), "Missing Activity must report unavailable");
                    });
                    require(completed[0] == 1, "Permission request did not complete exactly once");
                } finally { detached.close(); detachedRoot.close(); }
                checks.put("notification/request-error-preserves-os-authorization");
            } finally { services.close(); }
        });
    }
    private void environment(NativeView root) {
        ui(() -> {
            JSONObject snapshot = new JSONObject(root.environmentSnapshot(""));
            require(snapshot.getJSONObject("locale").getJSONArray("languages").length() > 0, "Locale language preferences missing");
            if (android.os.Build.VERSION.SDK_INT >= 36) {
                android.view.accessibility.AccessibilityManager manager = (android.view.accessibility.AccessibilityManager)root.getContext().getSystemService(android.content.Context.ACCESSIBILITY_SERVICE);
                require(snapshot.getBoolean("high_contrast") == manager.isHighContrastTextEnabled(), "Environment differs from OS contrast preference");
            } else require(snapshot.isNull("high_contrast"), "Unsupported contrast preference must remain unknown");
            NativeView embedded = new NativeView(root.getContext());
            try {
                embedded.setLocale(java.util.Locale.forLanguageTag("ar-EG"));
                embedded.setFontScale(1.75f);
                JSONObject overridden = new JSONObject(embedded.environmentSnapshot(""));
                require(overridden.getJSONObject("locale").getString("locale").equals("ar-EG")
                    && overridden.getString("layout_direction").equals("right-to-left")
                    && overridden.getDouble("font_scale") == 1.75, "Host environment overrides missing");
                require(root.environmentSnapshot("").equals(snapshot.toString()), "Embedded overrides escaped into another root");
                for (float invalid : new float[] {Float.NaN, Float.POSITIVE_INFINITY, -1, 0.25f, 5}) {
                    boolean rejected = false;
                    try { embedded.setFontScale(invalid); } catch (IllegalArgumentException expected) { rejected = true; }
                    require(rejected && new JSONObject(embedded.environmentSnapshot("")).getDouble("font_scale") == 1.75,
                        "Invalid font scale changed environment");
                }
                embedded.setLocale(null); embedded.setFontScale(0);
                JSONObject restored = new JSONObject(embedded.environmentSnapshot(""));
                require(restored.getJSONObject("locale").toString().equals(snapshot.getJSONObject("locale").toString())
                    && restored.getString("layout_direction").equals(snapshot.getString("layout_direction"))
                    && restored.getDouble("font_scale") == snapshot.getDouble("font_scale"), "Host environment reset did not follow OS");
            } finally { embedded.close(); }
            checks.put("environment/os-locale-contrast");
        });
    }

    @SuppressWarnings("unchecked")
    private void authentication(NativeView root) {
        NativeServices[] service = {null}; NativeView[] fixture = {null};
        ArrayList<Intent> launched = new ArrayList<>(); ArrayList<JSONObject> replies = new ArrayList<>();
        try {
            ui(() -> {
                android.content.Context context = new android.content.ContextWrapper(root.getContext()) {
                    @Override public void startActivity(Intent intent) { launched.add(new Intent(intent)); }
                };
                fixture[0] = new NativeView(context); service[0] = new NativeServices(fixture[0]);
                NativeServices services = service[0];
                java.util.ArrayDeque<JSONObject> events = (java.util.ArrayDeque<JSONObject>)field(services, "events");
                JSONObject request = new JSONObject().put("url", "https://example.invalid/auth").put("callback_scheme", "unregistered-jadix-check").put("ephemeral", true);
                services.system.command(new JSONObject().put("id", 801).put("action", "open_auth_session").put("request", request));
                require(launched.isEmpty() && events.removeLast().getJSONObject("event").getString("error").equals("unavailable"), "Undeclared callback protocol launched authentication");
                request.put("callback_scheme", "jadix-check");
                services.system.command(new JSONObject().put("id", 802).put("action", "open_auth_session").put("request", request));
                require(launched.size() == 1 && events.isEmpty(), "Ephemeral authentication was rejected");
                Intent tab = launched.get(0);
                require(tab.hasCategory(Intent.CATEGORY_BROWSABLE) && tab.getDataString().equals("https://example.invalid/auth")
                    && tab.hasExtra("android.support.customtabs.extra.SESSION")
                    && tab.getBooleanExtra("androidx.browser.customtabs.extra.ENABLE_EPHEMERAL_BROWSING", false), "Custom Tabs protocol extras missing");
                NativePermissions bridge = services.activityBridge();
                bridge.onPause(); bridge.onResume();
                require(!services.system.intent(new Intent(Intent.ACTION_VIEW, android.net.Uri.parse("unrelated://callback"))), "Unrelated callback completed authentication");
                require(services.system.intent(new Intent(Intent.ACTION_VIEW, android.net.Uri.parse("JADIX-CHECK://callback?code=中文😀"))), "Registered callback was ignored");
                JSONObject completed = events.removeLast();
                require(completed.getInt("id") == 802 && completed.getJSONObject("event").getJSONObject("outcome").getString("status").equals("completed"), "Authentication did not complete");
                require(field(bridge, "returned") == null && !services.system.intent(new Intent(Intent.ACTION_VIEW, android.net.Uri.parse("jadix-check://callback"))), "Completed callback was retained or replayed");
                services.system.command(new JSONObject().put("id", 803).put("action", "open_auth_session").put("request", request));
                services.system.command(new JSONObject().put("id", 804).put("action", "open_auth_session").put("request", request));
                JSONObject replaced = events.removeLast();
                require(replaced.getInt("id") == 803 && replaced.getJSONObject("event").getJSONObject("outcome").getString("status").equals("cancelled"), "Superseded authentication was not cancelled");
                bridge.onPause(); bridge.onResume();
                // Retain only the cancellation fixture's events across the posted resume callback.
                services.main.removeCallbacksAndMessages(null);
            });
            until("Returning from authentication did not cancel exactly once", () -> {
                java.util.ArrayDeque<JSONObject> events = (java.util.ArrayDeque<JSONObject>)field(service[0], "events");
                if (events.isEmpty()) return false;
                replies.addAll(events); events.clear(); return true;
            });
            ui(() -> {
                require(replies.size() == 1 && replies.get(0).getInt("id") == 804
                    && replies.get(0).getJSONObject("event").getJSONObject("outcome").getString("status").equals("cancelled"), "Return callback duplicated or used a stale session");
                NativeServices services = service[0];
                JSONObject request = new JSONObject().put("url", "https://example.invalid/auth").put("callback_scheme", "jadix-check");
                services.system.command(new JSONObject().put("id", 805).put("action", "open_auth_session").put("request", request));
                services.system.command(new JSONObject().put("id", 804).put("action", "close_auth_session"));
                require(field(services.system, "auth").equals(805), "Stale close cancelled a newer session");
                services.system.command(new JSONObject().put("id", 805).put("action", "close_auth_session"));
                Intent returned = launched.get(launched.size() - 1);
                require(returned.getComponent() != null && (returned.getFlags() & (Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP)) == (Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP), "Closing authentication did not return to the host");
                require(field(services.activityBridge(), "returned") == null && field(services.system, "auth").equals(0), "Closing authentication retained lifecycle ownership");
                checks.put("auth/custom-tab-ephemeral-callback-lifecycle");
            });
        } finally { ui(() -> { if (service[0] != null) service[0].close(); if (fixture[0] != null) fixture[0].close(); }); }
    }

    @Override public void onStart() {
        Bundle result = new Bundle(); MainActivity activity = null; boolean passed = false;
        try {
            require(token != null && UUID.fromString(token).toString().equals(token), "Fresh UUID token required");
            activity = (MainActivity)startActivitySync(new Intent(getTargetContext(), MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
            MainActivity current = activity;
            until("Activity did not become visible", () -> current.content.hasWindowFocus() && current.content.getHeight() > 0);
            ui(() -> { stage = new FrameLayout(current); current.addContentView(stage, new FrameLayout.LayoutParams(-1, -1)); });
            markup(current.content); selection(current.content); browser(current.content); playback(current.content); audioServices(current.content); syntheticTilt(); notificationPermission(current.content); environment(current.content); authentication(current.content);
            result.putString("jadix_capabilities", new JSONObject().put("schema", 1).put("token", token).put("passed", true)
                .put("api", android.os.Build.VERSION.SDK_INT).put("checks", checks)
                .put("authority", "Android instrumentation, shared Rust HTML policy, OS HTML importer, WebView JavaScript, MediaPlayer/PCM decoding and OS audio focus; programmatic actions, injected selection touch, OS Copy and internal readback").toString());
            result.putString("stream", "JADIX_NATIVE_CAPABILITIES_PASS"); passed = true;
        } catch (Throwable error) { result.putString("stream", "JADIX_NATIVE_CAPABILITIES_FAIL: " + android.util.Log.getStackTraceString(error)); }
        finally {
            MainActivity current = activity;
            try {
                ui(() -> { if (competingFocus != null) focusManager.abandonAudioFocusRequest(competingFocus); if (audioServices != null) audioServices.close(); if (editor != null) editor.close(); if (web != null) web.close(); if (media != null) media.close(); if (stage != null && stage.getParent() instanceof android.view.ViewGroup) ((android.view.ViewGroup)stage.getParent()).removeView(stage); if (current != null) current.finish(); });
            } catch (Throwable error) {
                passed = false; result.remove("jadix_capabilities");
                result.putString("stream", result.getString("stream", "") + "\nCleanup failed: " + android.util.Log.getStackTraceString(error));
            }
            if (sound != null) sound.delete();
        }
        finish(passed ? Activity.RESULT_OK : Activity.RESULT_CANCELED, result);
    }
}
