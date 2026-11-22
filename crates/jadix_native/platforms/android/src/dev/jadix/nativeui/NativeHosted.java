package dev.jadix.nativeui;

import android.app.AlertDialog;
import android.app.DatePickerDialog;
import android.content.res.Configuration;
import android.content.Context;
import android.graphics.*;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.util.Calendar;
import java.util.Locale;
import java.util.TimeZone;

/** Generic hosted surfaces consume the same validated specification as other hosts. */
public final class NativeHosted extends NativeFrame implements AutoCloseable {
    public interface Surface extends AutoCloseable {
        View view();
        void update(JSONObject properties) throws Exception;
        @Override void close();
    }
    public interface Events { void emit(String event, Object data); }
    public interface Factory { Surface create(Context context, Events events) throws Exception; }
    final FrameLayout content;
    private final NativeView root;
    private JSONObject specification = new JSONObject();
    private String identity = "", snapshot = "";
    private View control;
    private NativeShaderComposition shaderComposition;
    NativeWeb web() { return control instanceof NativeWeb ? (NativeWeb)control : null; }
    void rendererGone(NativeWeb web) {
        if(control==web) { removeView(web); control=null; snapshot=""; }
        web.close();
    }
    private Surface custom;
    private long generation;
    private boolean updating;
    private PopupMenu menu;
    private AlertDialog dialog;
    private DatePickerDialog dateDialog;
    private String dateLocale = "";

    NativeHosted(NativeView root) {
        super(root.getContext()); this.root = root;
        content = new NativeFrame(getContext()) {
            @Override protected void dispatchDraw(Canvas canvas) {
                if(shaderComposition==null||!shaderComposition.suppress())super.dispatchDraw(canvas);
            }
        }; addView(content, new LayoutParams(-1, -1));
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
    }
    void configure(JSONObject node) throws JSONException {
        JSONObject next = node.getJSONObject("specification");
        String nextIdentity = next.getString("type") + ":" + next.optString("kind");
        if (!identity.equals(nextIdentity)) { close(); identity = nextIdentity; }
        specification = next;
        String encoded = next.toString();
        if (snapshot.equals(encoded) && control != null) return;
        snapshot = encoded; updating = true;
        if (menu != null) { menu.dismiss(); menu = null; }
        if (dateDialog != null) { dateDialog.dismiss(); dateDialog = null; }
        try {
            switch (next.getString("type")) {
                case "menu_button": menuButton(); break;
                case "tab_bar": case "segments": choices(); break;
                case "date": date(); break;
                case "time": time(); break;
                case "color": color(); break;
                case "terminal":
                    if (getTag() == null) break;
                    if (!(control instanceof NativeTerminal)) install(new NativeTerminal(getContext(), this::emit));
                    ((NativeTerminal)control).update(next); break;
                case "rich_editor":
                    if (getTag() == null) break;
                    if (!(control instanceof NativeRichEditor)) install(new NativeRichEditor(root, this::emit));
                    ((NativeRichEditor)control).update(next); break;
                case "canvas":
                    if (!(control instanceof Drawing)) install(new Drawing(getContext()));
                    ((Drawing)control).commands = next.getJSONArray("commands"); control.invalidate(); break;
                case "web":
                    if (getTag() == null) break;
                    if (!(control instanceof NativeWeb)) install(new NativeWeb(root, getTag().toString(), this::emit));
                    ((NativeWeb)control).configure(next); break;
                case "media":
                    if (getTag() == null) break;
                    if (!(control instanceof NativeMedia)) install(new NativeMedia(root, this::emit));
                    ((NativeMedia)control).configure(next); break;
                case "shader":
                    if (getTag() == null) break;
                    if (!(control instanceof NativeShader)) {
                        NativeShader shader=new NativeShader(root,this::emit);install(shader);
                        shaderComposition=new NativeShaderComposition(this,content,shader);
                    }
                    shaderComposition.configure(next);((NativeShader)control).configure(next); break;
                case "custom": custom(); break;
                default: throw new IllegalArgumentException("Unknown hosted surface");
            }
        } catch (Exception error) { report(error); }
        finally { updating = false; }
    }
    void moveShader(View shader,boolean above) {
        if(above)bringChildToFront(shader);else bringChildToFront(content);
    }
    @Override protected void dispatchDraw(Canvas canvas) {
        super.dispatchDraw(canvas);if(shaderComposition!=null)shaderComposition.drawFallback(canvas);
    }
    private void install(View view) {
        if (control != null) removeView(control);
        control = view; addView(view, 0, new LayoutParams(-1, -1));
    }
    private void emit(String event, Object data) {
        if (updating || getTag() == null || !root.owns((String)getTag(), this)) return;
        try { root.emit((String)getTag(), NativeView.payload("hosted").put("event", event).put("data", data)); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }
    private void report(Exception error) {
        long token = generation;
        post(() -> { if (token == generation) emit("error", error.getMessage() == null ? error.getClass().getSimpleName() : error.getMessage()); });
    }
    private boolean enabled() { return specification.optBoolean("enabled", true); }
    private void menuButton() {
        if (!(control instanceof Button)) { Button button = new Button(getContext()); install(button); button.setOnClickListener(v -> openMenu()); }
        ((Button)control).setText(specification.optString("label")); control.setEnabled(enabled());
    }
    private void openMenu() {
        if (!enabled()) return;
        if (menu != null) menu.dismiss();
        menu = new PopupMenu(getContext(), control); long token = generation;
        try { menuItems(menu.getMenu(), specification.getJSONArray("items"), token); }
        catch (JSONException error) { throw new IllegalStateException(error); }
        menu.show();
    }
    private void menuItems(Menu destination, JSONArray items, long token) throws JSONException {
        for (int i = 0; i < items.length(); i++) {
            JSONObject item = items.getJSONObject(i); JSONArray children = item.optJSONArray("children");
            if (item.optBoolean("inline")) {
                if (item.optBoolean("enabled")) menuItems(destination, children, token);
                continue;
            }
            String label = item.getString("label");
            MenuItem nativeItem;
            if (children != null && children.length() > 0) {
                SubMenu submenu = destination.addSubMenu(label); nativeItem = submenu.getItem(); menuItems(submenu, children, token);
            } else {
                nativeItem = destination.add(label);
                nativeItem.setOnMenuItemClickListener(selected -> { if (generation == token && enabled()) emit("select", item.optString("id")); return true; });
            }
            nativeItem.setEnabled(item.optBoolean("enabled"));
            nativeItem.setCheckable(item.optBoolean("checked")).setChecked(item.optBoolean("checked"));
            nativeItem.setContentDescription(item.optString("accessibility_label", label));
            if (item.optBoolean("destructive")) {
                android.text.SpannableString title = new android.text.SpannableString(label);
                title.setSpan(new android.text.style.ForegroundColorSpan(Color.rgb(180, 30, 30)), 0, title.length(), 0); nativeItem.setTitle(title);
            }
            String accelerator = item.optString("accelerator", "");
            String[] chord = accelerator.toLowerCase(Locale.ROOT).split("\\+");
            if (chord.length > 0 && chord[chord.length - 1].length() == 1) {
                int modifiers = 0;
                for (String part : chord) { if (part.equals("ctrl") || part.equals("control")) modifiers |= KeyEvent.META_CTRL_ON; if (part.equals("alt")) modifiers |= KeyEvent.META_ALT_ON; if (part.equals("shift")) modifiers |= KeyEvent.META_SHIFT_ON; if (part.equals("meta") || part.equals("cmd")) modifiers |= KeyEvent.META_META_ON; }
                nativeItem.setAlphabeticShortcut(chord[chord.length - 1].charAt(0), modifiers);
            }
        }
    }
    private void choices() throws JSONException {
        RadioGroup group;
        if (!(control instanceof RadioGroup)) { group = new RadioGroup(getContext()); group.setOrientation(LinearLayout.HORIZONTAL); install(group); }
        else group = (RadioGroup)control;
        group.removeAllViews(); JSONArray items = specification.getJSONArray("items");
        group.setVisibility(specification.optBoolean("hidden") ? GONE : VISIBLE);
        for (int i = 0; i < items.length(); i++) {
            JSONObject item = items.getJSONObject(i); String id = item.getString("id"), label = item.getString("label");
            RadioButton button = new RadioButton(getContext()); button.setId(View.generateViewId());
            String badge = item.isNull("badge") ? "" : item.optString("badge").isEmpty() ? " •" : " " + item.optString("badge");
            button.setText(label + badge); button.setContentDescription(item.optString("accessibility_label", label) + badge);
            button.setEnabled(enabled() && item.optBoolean("enabled")); button.setChecked(id.equals(specification.optString("selected")));
            group.addView(button, new RadioGroup.LayoutParams(0, -1, 1));
            button.setOnClickListener(v -> emit("select", id));
        }
    }
    private static Calendar calendar(JSONObject date) {
        // Civil dates stay in the widget's local zone, including across DST changes.
        Calendar result = Calendar.getInstance(TimeZone.getDefault(), Locale.ROOT); result.clear();
        if (date != null) result.set(date.optInt("year"), date.optInt("month") - 1, date.optInt("day"), 12, 0, 0);
        else { Calendar today = Calendar.getInstance(); result.set(today.get(Calendar.YEAR), today.get(Calendar.MONTH), today.get(Calendar.DAY_OF_MONTH), 12, 0, 0); }
        return result;
    }
    private Context dateContext() {
        String tag = specification.optString("locale", "");
        if (tag.isEmpty()) return getContext();
        Configuration config = new Configuration(getResources().getConfiguration());
        config.setLocale(Locale.forLanguageTag(tag));
        return getContext().createConfigurationContext(config);
    }
    private long dateBound(String name, int year, int month, int day) {
        return calendar(specification.isNull(name) ? newDate(year, month, day) : specification.optJSONObject(name)).getTimeInMillis();
    }
    private void date() {
        Context context = dateContext();
        Calendar value = calendar(specification.optJSONObject("value"));
        long minimum = dateBound("minimum", 1, 1, 1), maximum = dateBound("maximum", 9999, 12, 31);
        value.setTimeInMillis(Math.max(minimum, Math.min(maximum, value.getTimeInMillis())));
        String locale = context.getResources().getConfiguration().getLocales().toLanguageTags();
        if ("calendar".equals(specification.optString("mode"))) {
            if (!(control instanceof CalendarView) || !locale.equals(dateLocale)) install(new CalendarView(context));
            CalendarView picker = (CalendarView)control;
            picker.setOnDateChangeListener(null);
            picker.setMinDate(calendar(newDate(1, 1, 1)).getTimeInMillis());
            picker.setMaxDate(calendar(newDate(9999, 12, 31)).getTimeInMillis());
            picker.setMinDate(minimum); picker.setMaxDate(maximum);
            picker.setDate(value.getTimeInMillis(), false, true); picker.setEnabled(enabled());
            picker.setOnDateChangeListener((view, year, month, day) -> emit("change", newDate(year, month + 1, day)));
        } else {
            if (!(control instanceof Button)) install(new Button(context));
            Button field = (Button)control;
            Locale display = context.getResources().getConfiguration().getLocales().get(0);
            String label = specification.isNull("value") ? "—" : java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM, display).format(value.getTime());
            field.setText(label); field.setContentDescription(label); field.setEnabled(enabled());
            field.setOnClickListener(view -> {
                if (!enabled()) return;
                if (dateDialog != null) dateDialog.dismiss();
                long token = generation;
                DatePickerDialog popup = new DatePickerDialog(dateContext(), (picker, year, month, day) -> {
                    if (token == generation) emit("change", newDate(year, month + 1, day));
                }, value.get(Calendar.YEAR), value.get(Calendar.MONTH), value.get(Calendar.DAY_OF_MONTH));
                dateDialog = popup;
                popup.getDatePicker().setMinDate(minimum); popup.getDatePicker().setMaxDate(maximum);
                popup.setOnDismissListener(ignored -> { if (dateDialog == popup) dateDialog = null; }); popup.show();
            });
        }
        dateLocale = locale;
    }
    private static JSONObject newDate(int year, int month, int day) {
        try { return new JSONObject().put("year", year).put("month", month).put("day", day); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }
    private void time() throws JSONException {
        LinearLayout row;
        if (!(control instanceof LinearLayout)) {
            row = new LinearLayout(getContext()); row.setOrientation(LinearLayout.HORIZONTAL);
            TimePicker picker = new TimePicker(getContext()); picker.setIs24HourView(android.text.format.DateFormat.is24HourFormat(getContext()));
            row.addView(picker, new LinearLayout.LayoutParams(0, -1, 1)); NumberPicker seconds = new NumberPicker(getContext()); seconds.setMinValue(0); seconds.setMaxValue(59); row.addView(seconds); install(row);
            picker.setOnTimeChangedListener((view, hour, minute) -> timeChanged(row)); seconds.setOnValueChangedListener((view, old, value) -> timeChanged(row));
        } else row = (LinearLayout)control;
        TimePicker picker = (TimePicker)row.getChildAt(0); NumberPicker seconds = (NumberPicker)row.getChildAt(1);
        JSONObject value = specification.getJSONObject("value"); picker.setHour(value.getInt("hour")); picker.setMinute(value.getInt("minute")); seconds.setValue(value.getInt("second"));
        picker.setEnabled(enabled()); seconds.setEnabled(enabled()); seconds.setVisibility(specification.optBoolean("seconds") ? VISIBLE : GONE);
    }
    private void timeChanged(LinearLayout row) {
        try { TimePicker picker = (TimePicker)row.getChildAt(0); emit("change", new JSONObject().put("hour", picker.getHour()).put("minute", picker.getMinute()).put("second", specification.optBoolean("seconds") ? ((NumberPicker)row.getChildAt(1)).getValue() : 0)); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }
    private void color() throws JSONException {
        if (!(control instanceof Button)) { Button button = new Button(getContext()); button.setText("Color"); button.setOnClickListener(v -> openColor()); install(button); }
        control.setEnabled(enabled()); JSONArray rgba = specification.getJSONArray("rgba");
        control.setBackgroundTintList(android.content.res.ColorStateList.valueOf(colorValue(rgba))); control.setContentDescription("Color " + rgba);
    }
    private void openColor() {
        if (!enabled()) return;
        LinearLayout channels = new LinearLayout(getContext()); channels.setOrientation(LinearLayout.VERTICAL); SeekBar[] sliders = new SeekBar[4];
        JSONArray rgba = specification.optJSONArray("rgba"); boolean alpha = specification.optBoolean("alpha");
        for (int i = 0; i < 4; i++) { sliders[i] = new SeekBar(getContext()); sliders[i].setMax(255); sliders[i].setProgress(rgba.optInt(i)); sliders[i].setContentDescription(new String[]{"Red", "Green", "Blue", "Alpha"}[i]); channels.addView(sliders[i]); if (i == 3 && !alpha) sliders[i].setVisibility(GONE); }
        long token = generation;
        dialog = new AlertDialog.Builder(getContext()).setTitle("Color").setView(channels).setNegativeButton(android.R.string.cancel, null).setPositiveButton(android.R.string.ok, (d, which) -> { if (token == generation) emit("change", new JSONArray().put(sliders[0].getProgress()).put(sliders[1].getProgress()).put(sliders[2].getProgress()).put(alpha ? sliders[3].getProgress() : 255)); }).create(); dialog.show();
    }
    private void custom() throws Exception {
        if (getTag() == null) return;
        if (custom == null) {
            Factory factory = root.hostedFactory(specification.getString("kind"));
            if (factory == null) throw new IllegalArgumentException("Unregistered hosted component: " + specification.getString("kind"));
            long token = generation;
            custom = factory.create(getContext(), (event, data) -> { root.requireUiThread(); if (token == generation) emit(event, data); });
            if (custom.view().getParent() != null) { custom.close(); custom = null; throw new IllegalArgumentException("Hosted factory must return a detached view"); }
            install(custom.view());
        }
        custom.update(specification.getJSONObject("properties"));
    }
    @Override public void close() {
        generation++; if (dateDialog != null) { dateDialog.dismiss(); dateDialog = null; } dateLocale = ""; if(shaderComposition!=null){shaderComposition.close();shaderComposition=null;} if (menu != null) menu.dismiss(); menu = null;
        if (dialog != null) dialog.dismiss(); dialog = null;
        if (control != null) removeView(control);
        if (custom != null) custom.close();
        else if (control instanceof AutoCloseable) try { ((AutoCloseable)control).close(); } catch (Exception ignored) { }
        custom = null; if (control != null) removeView(control); control = null; snapshot = "";
    }
    static int colorValue(JSONArray rgba) { return Color.argb(rgba.optInt(3,255), rgba.optInt(0), rgba.optInt(1), rgba.optInt(2)); }
    private final class Drawing extends View {
        JSONArray commands = new JSONArray(); final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        Drawing(Context context) { super(context); }
        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas); canvas.save(); canvas.scale(root.density(), root.density());
            try { for (int i = 0; i < commands.length(); i++) {
                JSONObject command = commands.getJSONObject(i); paint.setColor(colorValue(command.getJSONArray("color"))); paint.setStyle(Paint.Style.FILL);
                switch (command.getString("type")) {
                    case "fill": JSONArray r = command.getJSONArray("rect"); canvas.drawRect((float)r.getDouble(0),(float)r.getDouble(1),(float)(r.getDouble(0)+r.getDouble(2)),(float)(r.getDouble(1)+r.getDouble(3)),paint); break;
                    case "stroke": JSONArray points = command.getJSONArray("points"); Path path = new Path(); for (int j=0;j<points.length();j++) { JSONArray p=points.getJSONArray(j); if(j==0)path.moveTo((float)p.getDouble(0),(float)p.getDouble(1));else path.lineTo((float)p.getDouble(0),(float)p.getDouble(1)); } if(command.optBoolean("closed"))path.close(); paint.setStyle(Paint.Style.STROKE);paint.setStrokeWidth((float)command.getDouble("width"));canvas.drawPath(path,paint);break;
                    case "text": JSONArray p=command.getJSONArray("position");paint.setTypeface(root.resources.font("system-ui",Typeface.NORMAL));paint.setTextSize((float)command.getDouble("size"));canvas.drawText(command.getString("text"),(float)p.getDouble(0),(float)p.getDouble(1),paint);break;
                }
            }} catch(JSONException error) { report(error); } finally { canvas.restore(); }
        }
    }
}
