package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Application;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.res.Configuration;
import android.database.ContentObserver;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.Settings;
import java.util.Calendar;
import java.util.LinkedHashSet;
import java.util.Locale;
import java.util.TimeZone;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** UI-thread adapter for the shared, validated EnvironmentSnapshot contract. */
final class NativeEnvironment implements AutoCloseable, Application.ActivityLifecycleCallbacks {
    private final NativeView root;
    private final Activity activity;
    private boolean watching;
    private boolean receiverRegistered;
    private boolean lifecycleRegistered;
    private String lifecycle;
    private Locale hostLocale;
    private float hostFontScale;
    private Runnable stopContrast = () -> {};
    private final ContentObserver preferences = new ContentObserver(new Handler(Looper.getMainLooper())) {
        @Override public void onChange(boolean selfChange) { changed(); }
    };
    private final BroadcastReceiver broadcasts = new BroadcastReceiver() {
        @Override public void onReceive(Context context, Intent intent) { changed(); }
    };

    NativeEnvironment(NativeView root) {
        this.root = root;
        activity = NativePermissions.activity(root.getContext());
        lifecycle = root.getWindowVisibility() == android.view.View.VISIBLE ? "active" : "background";
    }

    void start() {
        if (watching) return;
        watching = true;
        try {
        IntentFilter filter = new IntentFilter();
        filter.addAction(Intent.ACTION_LOCALE_CHANGED);
        filter.addAction(Intent.ACTION_TIMEZONE_CHANGED);
        filter.addAction(Intent.ACTION_TIME_CHANGED);
        if (Build.VERSION.SDK_INT >= 33) root.getContext().registerReceiver(broadcasts, filter, Context.RECEIVER_NOT_EXPORTED);
        else root.getContext().registerReceiver(broadcasts, filter);
        receiverRegistered = true;
        root.getContext().getContentResolver().registerContentObserver(Settings.System.getUriFor(Settings.System.TIME_12_24), false, preferences);
        root.getContext().getContentResolver().registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, preferences);
        if (activity != null) { activity.getApplication().registerActivityLifecycleCallbacks(this); lifecycleRegistered = true; }
        if (Build.VERSION.SDK_INT >= 36) stopContrast = ModernContrast.watch(root.getContext(), this::changed);
        } catch (RuntimeException error) { close(); throw error; }
    }

    private void changed() { if (watching) root.environmentChanged(); }
    private void state(Activity candidate, String value) {
        if (candidate == activity && !lifecycle.equals(value)) { lifecycle = value; changed(); }
    }

    void setLocale(Locale locale) {
        root.requireUiThread();
        if (locale != null) {
            LinkedHashSet<String> languages = new LinkedHashSet<>();
            addLanguage(languages, locale);
            if (languages.isEmpty()) throw new IllegalArgumentException("Invalid locale");
        }
        if (!java.util.Objects.equals(hostLocale, locale)) { hostLocale = locale; root.environmentChanged(); }
    }

    void setFontScale(float scale) {
        root.requireUiThread();
        if (!Float.isFinite(scale) || (scale != 0 && (scale < 0.5f || scale > 4)))
            throw new IllegalArgumentException("Font scale must be 0 (system) or between 0.5 and 4");
        if (hostFontScale != scale) { hostFontScale = scale; root.environmentChanged(); }
    }

    String snapshot() {
        root.requireUiThread();
        Configuration configuration = root.getResources().getConfiguration();
        LinkedHashSet<String> languages = new LinkedHashSet<>();
        if (hostLocale != null) addLanguage(languages, hostLocale);
        if (Build.VERSION.SDK_INT >= 24) {
            android.os.LocaleList list = configuration.getLocales();
            for (int index = 0; index < list.size() && languages.size() < 16; index++) addLanguage(languages, list.get(index));
        } else addLanguage(languages, configuration.locale);
        if (languages.isEmpty()) addLanguage(languages, Locale.getDefault());
        if (languages.isEmpty()) languages.add("und");
        String language = languages.iterator().next();
        Locale locale = Locale.forLanguageTag(language);
        Calendar calendar = Calendar.getInstance(TimeZone.getDefault(), locale);
        try {
            JSONObject zone = new JSONObject().put("kind", "iana").put("id", TimeZone.getDefault().getID());
            // Java's custom GMT offsets are not IANA identifiers.
            String zoneId = zone.getString("id");
            if (zoneId.matches("GMT[+-]\\d{2}:\\d{2}")) zone.put("kind", "offset").put("id", zoneId.substring(3));
            JSONObject settings = new JSONObject().put("locale", language).put("languages", new JSONArray(languages))
                .put("time_zone", zone).put("hour_cycle", android.text.format.DateFormat.is24HourFormat(root.getContext()) ? "h23" : "h12")
                .put("first_day_of_week", ((calendar.getFirstDayOfWeek() + 5) % 7) + 1)
                .put("measurement_system", JSONObject.NULL).put("temperature_unit", JSONObject.NULL);
            int mode = configuration.uiMode & Configuration.UI_MODE_NIGHT_MASK;
            if (root.nativeAppearance().equals("dark")) mode = Configuration.UI_MODE_NIGHT_YES;
            else if (root.nativeAppearance().equals("light")) mode = Configuration.UI_MODE_NIGHT_NO;
            float scale = hostFontScale == 0 ? configuration.fontScale : hostFontScale;
            if (Float.isNaN(scale) || Float.isInfinite(scale)) scale = 1;
            return new JSONObject().put("platform", new JSONObject().put("name", "android").put("os", "android"))
                .put("locale", settings).put("color_scheme", mode == Configuration.UI_MODE_NIGHT_YES ? "dark" : mode == Configuration.UI_MODE_NIGHT_NO ? "light" : JSONObject.NULL)
                .put("layout_direction", (hostLocale == null ? configuration.getLayoutDirection() : android.text.TextUtils.getLayoutDirectionFromLocale(hostLocale)) == android.view.View.LAYOUT_DIRECTION_RTL ? "right-to-left" : "left-to-right")
                .put("high_contrast", Build.VERSION.SDK_INT >= 36 ? ModernContrast.read(root.getContext()) : JSONObject.NULL)
                .put("reduced_motion", Build.VERSION.SDK_INT >= 26 ? !android.animation.ValueAnimator.areAnimatorsEnabled() : JSONObject.NULL)
                .put("font_scale", Math.max(0.5f, Math.min(4, scale))).put("lifecycle", lifecycle).toString();
        } catch (JSONException error) { throw new IllegalStateException(error); }
    }

    private static void addLanguage(LinkedHashSet<String> languages, Locale locale) {
        if (locale == null) return;
        String value = locale.toLanguageTag();
        if (value.length() <= 85 && value.matches("[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*")) languages.add(value);
    }

    @Override public void close() {
        if (!watching) return;
        watching = false;
        stopContrast.run(); stopContrast = () -> {};
        if (receiverRegistered) { root.getContext().unregisterReceiver(broadcasts); receiverRegistered = false; }
        root.getContext().getContentResolver().unregisterContentObserver(preferences);
        if (lifecycleRegistered) { activity.getApplication().unregisterActivityLifecycleCallbacks(this); lifecycleRegistered = false; }
    }
    // Keep API 36 listener types out of classes loaded by Android 8 hosts.
    private static final class ModernContrast {
        private static android.view.accessibility.AccessibilityManager manager(Context context) {
            return (android.view.accessibility.AccessibilityManager)context.getSystemService(Context.ACCESSIBILITY_SERVICE);
        }
        static Object read(Context context) {
            android.view.accessibility.AccessibilityManager manager = manager(context);
            return manager == null ? JSONObject.NULL : manager.isHighContrastTextEnabled();
        }
        static Runnable watch(Context context, Runnable changed) {
            android.view.accessibility.AccessibilityManager manager = manager(context);
            if (manager == null) return () -> {};
            android.view.accessibility.AccessibilityManager.HighContrastTextStateChangeListener listener = enabled -> changed.run();
            manager.addHighContrastTextStateChangeListener(context.getMainExecutor(), listener);
            return () -> manager.removeHighContrastTextStateChangeListener(listener);
        }
    }

    @Override public void onActivityCreated(Activity candidate, Bundle saved) {}
    @Override public void onActivityStarted(Activity candidate) { state(candidate, "inactive"); }
    @Override public void onActivityResumed(Activity candidate) { state(candidate, "active"); }
    @Override public void onActivityPaused(Activity candidate) { state(candidate, "inactive"); }
    @Override public void onActivityStopped(Activity candidate) { state(candidate, "background"); }
    @Override public void onActivitySaveInstanceState(Activity candidate, Bundle saved) {}
    @Override public void onActivityDestroyed(Activity candidate) { state(candidate, "background"); if (candidate == activity) root.close(); }
}
