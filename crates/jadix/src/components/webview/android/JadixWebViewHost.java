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
        String url, html;
        Entry(Activity activity) {
            clip = new FrameLayout(activity);
            clip.setClipChildren(true);
            clip.setClipToPadding(true);
            web = new WebView(activity);
            web.getSettings().setJavaScriptEnabled(true);
            web.getSettings().setDomStorageEnabled(true);
            web.setWebViewClient(new WebViewClient() {
                @Override public void onPageStarted(WebView view, String url, Bitmap icon) { status = 0; }
                @Override public void onPageFinished(WebView view, String url) { if (status >= 0) status = 1; }
                @Override public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                    if (request.isForMainFrame()) status = -1;
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
    public static void remove(Activity activity, String id) {
        activity.runOnUiThread(() -> {
            Entry e = entries.remove(key(activity,id));
            if (e == null) return;
            ViewGroup parent = (ViewGroup)e.clip.getParent();
            if (parent != null) parent.removeView(e.clip);
            e.web.stopLoading(); e.web.destroy();
        });
    }
    private JadixWebViewHost() {}
}
