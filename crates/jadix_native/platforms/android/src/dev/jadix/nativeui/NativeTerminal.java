package dev.jadix.nativeui;

import android.content.*;
import android.graphics.Typeface;
import android.text.*;
import android.text.style.*;
import android.view.*;
import android.view.inputmethod.*;
import android.widget.EditText;
import org.json.*;
import java.nio.charset.StandardCharsets;

/** Android input/display adapter; the shared Rust screen owns all VT state. */
final class NativeTerminal extends EditText implements NativeHosted.Surface {
    private final NativeHosted.Events events;
    private String session, inputRevision, dimensions = "";
    private boolean closed, applicationCursor, bracketedPaste, composing;
    private JSONObject pending;
    NativeTerminal(Context context, NativeHosted.Events events) {
        super(context); this.events = events;
        NativeSelection.install(this);
        setTypeface(Typeface.MONOSPACE); setTextSize(12); setSingleLine(false);
        setHorizontallyScrolling(true); setTextIsSelectable(true); setKeyListener(null);
        setFocusableInTouchMode(true); setContentDescription("Terminal");
    }
    public View view() { return this; }
    public void update(JSONObject value) throws JSONException {
        if (closed) return;
        int columns = value.optInt("columns", 80), rows = value.optInt("rows", 24);
        if (columns < 1 || columns > 512 || rows < 1 || rows > 256) throw new IllegalArgumentException("Invalid terminal dimensions");
        int history = Math.max(0, Math.min(value.optInt("scrollback", 10000), 1000000 / columns - rows));
        JSONObject spec = new JSONObject().put("columns", columns).put("rows", rows).put("scrollback", history);
        String next = spec.toString();
        if (session == null) {
            JSONObject result = NativeRuntime.terminal(NativeRuntime.object("create").put("specification", spec));
            session = result.getString("id"); display(result.getJSONObject("frame")); dimensions = next;
        } else if (!next.equals(dimensions)) {
            display(NativeRuntime.terminal(NativeRuntime.object("configure").put("id", session).put("specification", spec)));
            dimensions = next;
            JSONObject size = new JSONObject().put("columns", columns).put("rows", rows);
            post(() -> { if (!closed) events.emit("resize", size); });
        }
        if (value.has("command")) throw new IllegalArgumentException("Terminal requires an application-owned transport");
        String revision = value.optString("input_revision", "");
        if (!revision.isEmpty() && !revision.equals(inputRevision) && value.has("input")) {
            byte[] bytes = value.getString("input").getBytes(StandardCharsets.UTF_8);
            if (bytes.length > 65536) throw new IllegalArgumentException("Terminal chunk exceeds budget");
            JSONArray data = new JSONArray(); for (byte b : bytes) data.put(b & 255);
            display(NativeRuntime.terminal(NativeRuntime.object("feed").put("id", session).put("data", data)));
            inputRevision = revision;
            post(() -> { if (!closed) events.emit("output", object("data", data)); });
        }
    }
    private static JSONObject object(String name, Object value) {
        try { return new JSONObject().put(name, value); } catch (JSONException error) { throw new IllegalStateException(error); }
    }
    private void send(String text) {
        if (!closed && text.getBytes(StandardCharsets.UTF_8).length <= 65536) events.emit("input", object("bytes", text));
    }
    private void display(JSONObject frame) throws JSONException {
        pending = frame; applicationCursor = frame.optBoolean("application_cursor"); bracketedPaste = frame.optBoolean("bracketed_paste");
        JSONArray replies = frame.getJSONArray("replies");
        for (int i = 0; i < replies.length(); i++) { String reply = replies.getString(i); post(() -> send(reply)); }
        render();
    }
    private void render() throws JSONException {
        if (pending == null || composing || closed) return;
        JSONObject frame = pending; pending = null; SpannableStringBuilder text = new SpannableStringBuilder();
        JSONArray runs = frame.getJSONArray("runs");
        for (int i = 0; i < runs.length(); i++) {
            JSONObject run = runs.getJSONObject(i), style = run.getJSONObject("style");
            int start = text.length(); text.append(run.getString("text")); int end = text.length(); if (start == end) continue;
            int foreground = style.isNull("foreground") ? 0xffeeeeee : 0xff000000 | style.getInt("foreground");
            int background = style.isNull("background") ? 0xff202020 : 0xff000000 | style.getInt("background");
            if (style.optBoolean("inverse")) { int swap = foreground; foreground = background; background = swap; }
            text.setSpan(new ForegroundColorSpan(foreground), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            text.setSpan(new BackgroundColorSpan(background), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            text.setSpan(new StyleSpan((style.optBoolean("bold") ? Typeface.BOLD : 0) | (style.optBoolean("italic") ? Typeface.ITALIC : 0)), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            if (style.optBoolean("underline")) text.setSpan(new UnderlineSpan(), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        }
        int start = getSelectionStart(), end = getSelectionEnd(); boolean selection = start >= 0 && start != end;
        boolean follows = getScrollY() + getHeight() >= computeVerticalScrollRange() - getLineHeight() * 2;
        setText(text); int cursor = Math.max(0, Math.min(frame.optInt("cursor_utf16"), text.length()));
        setSelection(selection ? Math.min(start, text.length()) : cursor, selection ? Math.min(end, text.length()) : cursor);
        setCursorVisible(frame.optBoolean("cursor_visible", true));
        if (follows && !selection) bringPointIntoView(cursor);
    }
    @Override public boolean onCheckIsTextEditor() { return true; }
    @Override public InputConnection onCreateInputConnection(EditorInfo out) {
        out.inputType = android.text.InputType.TYPE_CLASS_TEXT | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        out.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI | EditorInfo.IME_ACTION_NONE;
        return new BaseInputConnection(this, true) {
            final Editable input = new SpannableStringBuilder();
            @Override public Editable getEditable() { return input; }
            @Override public boolean setComposingText(CharSequence text, int position) { composing = true; return super.setComposingText(text, position); }
            @Override public boolean commitText(CharSequence text, int position) { input.clear(); composing = false; send(text.toString()); flush(); return true; }
            @Override public boolean finishComposingText() { String text = input.toString(); input.clear(); composing = false; if (!text.isEmpty()) send(text); flush(); return true; }
            @Override public boolean deleteSurroundingText(int before, int after) {
                if (composing) return super.deleteSurroundingText(before, after);
                for (int i = 0; i < Math.min(256, Math.max(0, before)); i++) send("\u007f"); return true;
            }
            @Override public boolean sendKeyEvent(KeyEvent event) { return dispatchKeyEvent(event); }
            private void flush() { try { render(); } catch (JSONException error) { events.emit("error", error.toString()); } }
        };
    }
    @Override public boolean onKeyDown(int code, KeyEvent event) {
        if (composing) return super.onKeyDown(code, event);
        String key = null;
        switch (code) {
            case KeyEvent.KEYCODE_DPAD_UP: key = "up"; break; case KeyEvent.KEYCODE_DPAD_DOWN: key = "down"; break;
            case KeyEvent.KEYCODE_DPAD_LEFT: key = "left"; break; case KeyEvent.KEYCODE_DPAD_RIGHT: key = "right"; break;
            case KeyEvent.KEYCODE_MOVE_HOME: key = "home"; break; case KeyEvent.KEYCODE_MOVE_END: key = "end"; break;
            case KeyEvent.KEYCODE_PAGE_UP: key = "page_up"; break; case KeyEvent.KEYCODE_PAGE_DOWN: key = "page_down"; break;
            case KeyEvent.KEYCODE_INSERT: key = "insert"; break; case KeyEvent.KEYCODE_FORWARD_DEL: key = "delete"; break;
            case KeyEvent.KEYCODE_DEL: key = "backspace"; break; case KeyEvent.KEYCODE_ENTER: key = "return"; break;
            case KeyEvent.KEYCODE_ESCAPE: key = "escape"; break; case KeyEvent.KEYCODE_TAB: key = "tab"; break;
            default: if (code >= KeyEvent.KEYCODE_F1 && code <= KeyEvent.KEYCODE_F12) key = "f" + (code - KeyEvent.KEYCODE_F1 + 1);
        }
        try {
            if (key != null) { Object value = NativeRuntime.call(NativeRuntime.object("terminalKey").put("key", key).put("shift", event.isShiftPressed()).put("alt", event.isAltPressed()).put("control", event.isCtrlPressed()).put("application_cursor", applicationCursor)); if (value instanceof String) { send((String)value); return true; } }
            int unicode = event.getUnicodeChar(event.getMetaState() & ~KeyEvent.META_CTRL_MASK & ~KeyEvent.META_ALT_MASK);
            if (event.isCtrlPressed() && unicode >= 64 && unicode <= 127) { send(Character.toString((char)(Character.toUpperCase(unicode) & 31))); return true; }
            if (unicode > 0 && !event.isMetaPressed()) { send((event.isAltPressed() ? "\u001b" : "") + new String(Character.toChars(unicode))); return true; }
        } catch (JSONException error) { events.emit("error", error.toString()); }
        return super.onKeyDown(code, event);
    }
    @Override public boolean onTextContextMenuItem(int id) {
        if (id == android.R.id.paste || id == android.R.id.pasteAsPlainText) {
            android.content.ClipboardManager clipboard = (android.content.ClipboardManager)getContext().getSystemService(Context.CLIPBOARD_SERVICE);
            ClipData clip = clipboard.getPrimaryClip(); if (clip != null && clip.getItemCount() > 0) {
                String text = clip.getItemAt(0).coerceToText(getContext()).toString();
                if (text.getBytes(StandardCharsets.UTF_8).length <= 65524) send(bracketedPaste ? "\u001b[200~" + text + "\u001b[201~" : text);
            } return true;
        }
        return id == android.R.id.cut ? false : super.onTextContextMenuItem(id);
    }
    public void close() {
        if (closed) return; closed = true; pending = null;
        if (session != null) {
            try { NativeRuntime.terminal(NativeRuntime.object("close").put("id", session)); }
            catch (JSONException error) { throw new IllegalStateException(error); }
            finally { session = null; }
        }
    }
}
