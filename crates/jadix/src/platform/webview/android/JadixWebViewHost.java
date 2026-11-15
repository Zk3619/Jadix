package org.jadix.webview;

import android.app.Activity;
import android.graphics.Bitmap;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;

/** Include this source in the Android application. Works with NativeActivity
 * and GameActivity; it does not replace Jadix's Activity or native SurfaceView. */
public final class JadixWebViewHost {
    private static final ConcurrentHashMap<String, Entry> entries = new ConcurrentHashMap<>();
    private static String key(Activity activity, String id) { return System.identityHashCode(activity) + ":" + id; }
    private static final class Entry {
        final FrameLayout clip;
        final WebView web;
        volatile int status = 0;
        volatile long documentState = 0;
        long generation = 0;
        final ConcurrentHashMap<Long, Evaluation> evaluations = new ConcurrentHashMap<>();
        String url, html;
        Entry(Activity activity) {
            clip = new FrameLayout(activity);
            clip.setClipChildren(true);
            clip.setClipToPadding(true);
            web = new WebView(activity);
            web.getSettings().setJavaScriptEnabled(true);
            web.getSettings().setDomStorageEnabled(true);
            web.setWebViewClient(new WebViewClient() {
                @Override public void onPageStarted(WebView view, String url, Bitmap icon) {
                    status = 0; documentState = (++generation) << 2; evaluations.clear();
                }
                @Override public void onPageFinished(WebView view, String url) {
                    if (status >= 0) { status = 1; documentState = (generation << 2) | 1; }
                }
                @Override public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                    if (request.isForMainFrame()) { status = -1; documentState = (generation << 2) | 2; evaluations.clear(); }
                }
            });
            clip.addView(web);
            ViewGroup content = activity.findViewById(android.R.id.content);
            content.addView(clip);
        }
    }
    public static void sync(Activity activity, String id, String url, String html,
            int x, int y, int width, int height, int cx, int cy, int cw, int ch, boolean visible) {
        activity.runOnUiThread(() -> {
            if (activity.isFinishing() || activity.isDestroyed()) return;
            Entry e = entries.computeIfAbsent(key(activity, id), ignored -> new Entry(activity));
            FrameLayout.LayoutParams outer = new FrameLayout.LayoutParams(Math.max(1,cw), Math.max(1,ch));
            outer.leftMargin = cx; outer.topMargin = cy;
            e.clip.setLayoutParams(outer);
            FrameLayout.LayoutParams inner = new FrameLayout.LayoutParams(Math.max(1,width), Math.max(1,height));
            inner.leftMargin = x-cx; inner.topMargin = y-cy;
            e.web.setLayoutParams(inner);
            e.clip.setVisibility(visible ? View.VISIBLE : View.INVISIBLE);
            if (!Objects.equals(e.url, url) || !Objects.equals(e.html, html)) {
                e.url = url; e.html = html; e.status = 0;
                if (html != null) e.web.loadDataWithBaseURL(null, html, "text/html", "UTF-8", null);
                else e.web.loadUrl(url);
            }
        });
    }
    public static void visible(Activity activity, String id, boolean visible) {
        activity.runOnUiThread(() -> {
            Entry e = entries.get(key(activity,id));
            if (e != null) e.clip.setVisibility(visible ? View.VISIBLE : View.INVISIBLE);
        });
    }
    public static int status(Activity activity, String id) {
        Entry e = entries.get(key(activity,id)); return e == null ? 0 : e.status;
    }
    private static final class Evaluation { volatile String result; }
    public static long documentState(Activity activity, String id) {
        Entry e = entries.get(key(activity,id)); return e == null ? 0 : e.documentState;
    }
    public static void evaluate(Activity activity, String id, long serial, String script) {
        // Bind the exact entry before posting; removal/replacement cannot execute
        // an old request against a new WebView with the same public ID.
        Entry e = entries.get(key(activity,id));
        if (e == null) return;
        long generation = e.documentState >>> 2;
        if (serial != 0 && e.evaluations.size() >= 64) return;
        Evaluation evaluation = new Evaluation();
        if (serial != 0) e.evaluations.put(serial, evaluation);
        activity.runOnUiThread(() -> {
            if (entries.get(key(activity,id)) != e || (e.documentState >>> 2) != generation) {
                evaluation.result = "{\"status\":\"error\",\"code\":\"aborted\"}";
                return;
            }
            if (serial == 0) { e.web.evaluateJavascript(script, null); return; }
            if (e.evaluations.get(serial) != evaluation) return;
            e.web.evaluateJavascript(script, result -> {
                if (e.evaluations.get(serial) == evaluation && (e.documentState >>> 2) == generation) {
                    evaluation.result = result != null && result.length() <= 1052672 ? result : "{\"status\":\"error\",\"code\":\"too-large\"}";
                }
            });
        });
    }
    public static String takeEvaluation(Activity activity, String id, long serial, boolean cancel) {
        Entry e = entries.get(key(activity,id));
        if (e == null) return "{\"status\":\"error\",\"code\":\"aborted\"}";
        Evaluation evaluation = cancel ? e.evaluations.remove(serial) : e.evaluations.get(serial);
        if (cancel) return null;
        if (evaluation == null) return "{\"status\":\"error\",\"code\":\"aborted\"}";
        String result = evaluation.result;
        if (result != null) e.evaluations.remove(serial, evaluation);
        return result;
    }
    public static void remove(Activity activity, String id) {
        activity.runOnUiThread(() -> {
            Entry e = entries.remove(key(activity,id));
            if (e == null) return;
            e.evaluations.clear();
            ViewGroup parent = (ViewGroup)e.clip.getParent();
            if (parent != null) parent.removeView(e.clip);
            e.web.stopLoading(); e.web.destroy();
        });
    }
    private JadixWebViewHost() {}
}
