package dev.jadix.nativeui;

import android.graphics.Bitmap;
import android.graphics.drawable.*;
import android.text.*;
import android.text.style.ImageSpan;
import android.text.style.URLSpan;
import android.view.View;
import android.view.inputmethod.*;
import android.widget.EditText;
import org.json.*;
import java.nio.charset.StandardCharsets;
import java.util.*;

/** Native styled text, with shared HTML sanitization and lossless image attachments. */
final class NativeRichEditor extends EditText implements NativeHosted.Surface {
    private final NativeView root;
    private final NativeHosted.Events events;
    private final IdentityHashMap<Drawable, JSONObject> attachments = new IdentityHashMap<>();
    private final IdentityHashMap<Drawable, Bitmap> decoded = new IdentityHashMap<>();
    private final IdentityHashMap<ImageSpan, AutoCloseable> requests = new IdentityHashMap<>();
    private final ArrayDeque<ImageSpan> waiting = new ArrayDeque<>();
    private long imageBudgetBytes;
    private JSONObject deferred;
    private long revision, generation;
    private boolean updating, closed, changeQueued, editable;
    private String controlled = "";

    NativeRichEditor(NativeView root, NativeHosted.Events events) {
        super(root.getContext()); this.root = root; this.events = events;
        NativeSelection.install(this);
        setGravity(android.view.Gravity.TOP | android.view.Gravity.START);
        setInputType(android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE);
        setFilters(new InputFilter[] {(source, start, end, dest, dstart, dend) -> {
            if (updating) return null;
            SpannableStringBuilder candidate = new SpannableStringBuilder(dest);
            candidate.replace(dstart, dend, source, start, end);
            if (candidate.toString().getBytes(StandardCharsets.UTF_8).length > 1024 * 1024
                    || exportHtml(candidate).getBytes(StandardCharsets.UTF_8).length > 1024 * 1024) {
                post(() -> { if (!closed) events.emit("error", "Rich text document exceeds 1 MiB"); });
                return dest.subSequence(dstart, dend);
            }
            return null;
        }});
        addTextChangedListener(new TextWatcher() {
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}
            public void onTextChanged(CharSequence s, int start, int before, int count) { queueChange(); }
            public void afterTextChanged(Editable value) { if (!updating) pruneAttachments(); }
        });
        setOnFocusChangeListener((view, focused) -> emit("focus", object("focused", focused)));
    }
    public View view() { return this; }
    private boolean composing() { return BaseInputConnection.getComposingSpanStart(getText()) >= 0; }
    public void update(JSONObject value) throws JSONException {
        if (closed || value.getLong("revision") < revision) return;
        editable = value.optBoolean("editable");
        setKeyListener(editable ? android.text.method.TextKeyListener.getInstance() : null);
        setTextIsSelectable(true); setCursorVisible(editable);
        if (composing()) { deferred = value; return; }
        updating = true;
        try {
            revision = value.getLong("revision");
            String html = value.getString("html");
            if (!html.equals(controlled)) {
                JSONObject plan = (JSONObject)NativeRuntime.call(NativeRuntime.object("html").put("html", html));
                JSONArray images = plan.getJSONArray("images"); cancelImages(); ++generation;
                imageBudgetBytes = plan.getLong("imageBudgetBytes");
                SpannableStringBuilder parsed = new SpannableStringBuilder(NativeRichMarkup.parse(plan));
                // The shared plan uses inert links so OS HTML importers never fetch resources.
                for (URLSpan link : parsed.getSpans(0, parsed.length(), URLSpan.class)) {
                    String source = link.getURL();
                    if (!source.matches("jadix-attachment:[0-9]+")) continue;
                    int index;
                    try { index = Integer.parseInt(source.substring(source.indexOf(':') + 1)); } catch (NumberFormatException error) { continue; }
                    JSONObject image = images.optJSONObject(index); if (image == null) continue;
                    Drawable placeholder = new ColorDrawable(android.graphics.Color.TRANSPARENT);
                    setImageBounds(placeholder, image, null); attachments.put(placeholder, image);
                    int start = parsed.getSpanStart(link), end = parsed.getSpanEnd(link); parsed.removeSpan(link);
                    parsed.setSpan(new ImageSpan(placeholder, source), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
                }
                setText(parsed); controlled = html;
                for (ImageSpan span : getText().getSpans(0, length(), ImageSpan.class)) {
                    if (attachments.containsKey(span.getDrawable())) waiting.add(span);
                }
                loadImages();
            }
            JSONArray range = value.optJSONArray("selection");
            if (range != null) setSelection(boundary(range.optInt(0)), boundary(range.optInt(1)));
        } finally { updating = false; }
    }
    private void cancelImages() {
        for (AutoCloseable request : requests.values()) try { request.close(); } catch (Exception ignored) {}
        requests.clear(); waiting.clear(); attachments.clear(); decoded.clear();
    }
    private void pruneAttachments() {
        Set<Drawable> live = Collections.newSetFromMap(new IdentityHashMap<Drawable, Boolean>());
        for (ImageSpan span : getText().getSpans(0, length(), ImageSpan.class)) live.add(span.getDrawable());
        attachments.keySet().retainAll(live); decoded.keySet().retainAll(live);
        for (Iterator<Map.Entry<ImageSpan, AutoCloseable>> it = requests.entrySet().iterator(); it.hasNext();) {
            Map.Entry<ImageSpan, AutoCloseable> entry = it.next();
            if (getText().getSpanStart(entry.getKey()) < 0) { try { entry.getValue().close(); } catch (Exception ignored) {} it.remove(); }
        }
        waiting.removeIf(span -> getText().getSpanStart(span) < 0); loadImages();
    }
    private void loadImages() {
        while (!closed && requests.size() < 4 && !waiting.isEmpty()) {
            ImageSpan span = waiting.remove(); JSONObject image = attachments.get(span.getDrawable());
            if (image == null || getText().getSpanStart(span) < 0) continue;
            long token = generation;
            requests.put(span, root.resources.image(image.optString("source"), bitmap -> {
                if (closed || token != generation) return;
                requests.remove(span);
                if (bitmap != null && getText().getSpanStart(span) >= 0) {
                    Set<Bitmap> unique = Collections.newSetFromMap(new IdentityHashMap<Bitmap, Boolean>());
                    unique.addAll(decoded.values()); long bytes = 0;
                    for (Bitmap retained : unique) bytes += retained.getAllocationByteCount();
                    if (unique.contains(bitmap) || bytes + bitmap.getAllocationByteCount() <= imageBudgetBytes) resolve(span, image, bitmap);
                    else events.emit("error", "Rich text image budget exceeded");
                } else if (bitmap == null && getText().getSpanStart(span) >= 0) events.emit("error", "Rich text image could not be decoded");
                loadImages();
            }));
        }
    }
    private int boundary(int offset) throws JSONException {
        JSONArray boundaries = NativeRuntime.boundaries(getText().toString());
        int clamped = Math.max(0, Math.min(offset, length())), result = 0;
        for (int i = 0; i < boundaries.length(); i++) { int next = boundaries.getInt(i); if (next > clamped) break; result = next; }
        return result;
    }
    private void setImageBounds(Drawable drawable, JSONObject image, Bitmap bitmap) throws JSONException {
        JSONArray size = (JSONArray)NativeRuntime.call(NativeRuntime.object("imageDimensions")
            .put("width", image.opt("width")).put("height", image.opt("height"))
            .put("natural", bitmap == null ? JSONObject.NULL : new JSONArray().put(bitmap.getWidth()).put(bitmap.getHeight())));
        float density = getResources().getDisplayMetrics().density;
        drawable.setBounds(0, 0, (int)Math.ceil(size.getDouble(0) * density), (int)Math.ceil(size.getDouble(1) * density));
    }
    private void resolve(ImageSpan span, JSONObject image, Bitmap bitmap) {
        try {
            Drawable drawable = new BitmapDrawable(getResources(), bitmap); setImageBounds(drawable, image, bitmap);
            Editable text = getText(); int start = text.getSpanStart(span), end = text.getSpanEnd(span); if (start < 0) return;
            updating = true;
            try { text.removeSpan(span); text.setSpan(new ImageSpan(drawable, span.getSource()), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE); }
            finally { updating = false; }
            attachments.remove(span.getDrawable()); attachments.put(drawable, image); decoded.put(drawable, bitmap); requestLayout(); invalidate();
        } catch (JSONException error) { events.emit("error", "Invalid rich text image dimensions"); }
    }
    private String exportHtml() { return exportHtml(getText()); }
    private String exportHtml(Spanned text) {
        if (NativeRichMarkup.contains(text)) return NativeRichMarkup.export(text, span -> {
            JSONObject attachment = attachments.get(span.getDrawable());
            return attachment == null ? null : attachment.optString("html");
        }, getResources().getDisplayMetrics().density);
        SpannableStringBuilder copy = new SpannableStringBuilder(text);
        TreeMap<Integer, ImageSpan> ordered = new TreeMap<>(Collections.reverseOrder());
        for (ImageSpan span : copy.getSpans(0, copy.length(), ImageSpan.class)) ordered.put(copy.getSpanStart(span), span);
        LinkedHashMap<String, String> replacements = new LinkedHashMap<>();
        for (Map.Entry<Integer, ImageSpan> entry : ordered.entrySet()) {
            ImageSpan span = entry.getValue(); JSONObject attachment = attachments.get(span.getDrawable());
            if (attachment == null) continue;
            String marker = "jadiximage" + UUID.randomUUID().toString().replace("-", "");
            int start = copy.getSpanStart(span), end = copy.getSpanEnd(span); copy.removeSpan(span); copy.replace(start, end, marker);
            replacements.put(marker, attachment.optString("html"));
        }
        String html = Html.toHtml(copy, Html.TO_HTML_PARAGRAPH_LINES_CONSECUTIVE);
        for (Map.Entry<String, String> replacement : replacements.entrySet()) html = html.replace(replacement.getKey(), replacement.getValue());
        return html;
    }
    private void queueChange() {
        if (updating || closed || !editable || changeQueued) return;
        changeQueued = true;
        post(() -> {
            changeQueued = false; if (closed || updating || composing()) return;
            String html = exportHtml();
            if (html.getBytes(StandardCharsets.UTF_8).length <= 1024 * 1024) { controlled = html; emit("change", object("html", html)); }
            else {
                String previous = controlled; int start = getSelectionStart(), end = getSelectionEnd(); controlled = "\u0000";
                try { update(new JSONObject().put("html", previous).put("revision", revision).put("editable", editable));
                    setSelection(Math.max(0, Math.min(start, length())), Math.max(0, Math.min(end, length())));
                } catch (JSONException error) { controlled = previous; }
                events.emit("error", "Rich text document exceeds 1 MiB");
            }
            flushDeferred();
        });
    }
    private void flushDeferred() {
        if (deferred == null || composing()) return;
        JSONObject next = deferred; deferred = null;
        try { update(next); } catch (JSONException error) { events.emit("error", "Invalid rich text update"); }
    }
    @Override protected void onSelectionChanged(int start, int end) {
        super.onSelectionChanged(start, end);
        if (events != null && start >= 0 && end >= 0) emit("selection", object("range", new JSONArray().put(Math.min(start, end)).put(Math.max(start, end))));
    }
    @Override public InputConnection onCreateInputConnection(EditorInfo info) {
        InputConnection connection = super.onCreateInputConnection(info); if (connection == null) return null;
        return new InputConnectionWrapper(connection, false) {
            @Override public boolean finishComposingText() { boolean result = super.finishComposingText(); queueChange(); post(() -> flushDeferred()); return result; }
            @Override public boolean commitText(CharSequence text, int position) { boolean result = super.commitText(text, position); queueChange(); return result; }
        };
    }
    private static JSONObject object(String key, Object value) {
        try { return new JSONObject().put(key, value); } catch (JSONException error) { throw new IllegalStateException(error); }
    }
    private void emit(String event, JSONObject data) {
        if (!closed && !updating) { try { events.emit(event, data.put("revision", revision)); } catch (JSONException error) { throw new IllegalStateException(error); } }
    }
    public void close() { closed = true; generation++; deferred = null; cancelImages(); clearFocus(); }
}
