package dev.jadix.nativeui;

import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.view.Choreographer;
import android.view.MotionEvent;
import org.json.JSONObject;
import java.util.Locale;

/** A bounded diagnostic overlay backed by the root's shared Rust monitor. */
final class NativePerformance implements Choreographer.FrameCallback {
    private final NativeView root;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final android.widget.Button accessibilityControl;
    private String session, snapshot;
    private boolean scheduled, visible, changed;
    private long drawNanos, lastSample;
    private final RectF bounds = new RectF();
    private boolean expanded = true, tracking, moved;
    private int pointer = -1;
    private float x = 8, y = 8, downX, downY, originX, originY;
    NativePerformance(NativeView root) {
        this.root = root;
        accessibilityControl = new android.widget.Button(root.getContext());
        accessibilityControl.setBackgroundColor(android.graphics.Color.TRANSPARENT);
        accessibilityControl.setMinimumWidth(0); accessibilityControl.setMinimumHeight(0);
        accessibilityControl.setPadding(0, 0, 0, 0);
        accessibilityControl.setImportantForAccessibility(android.view.View.IMPORTANT_FOR_ACCESSIBILITY_YES);
        accessibilityControl.setOnClickListener(view -> toggle());
        updateAccessibility();
    }
    void bind(String session) { stop(); this.session = session; snapshot = null; lastSample = 0; updateAccessibility(); start(); }
    void start() {
        if (!scheduled && session != null && root.isAttachedToWindow() && root.getWindowVisibility() == android.view.View.VISIBLE) {
            scheduled = true; Choreographer.getInstance().postFrameCallback(this);
        }
    }
    void stop() { if (scheduled) Choreographer.getInstance().removeFrameCallback(this); scheduled = false; changed = false; drawNanos = 0; tracking = false; pointer = -1; }
    void setVisible(boolean visible) { this.visible = visible; tracking = false; pointer = -1; updateAccessibility(); root.invalidate(); }
    private void toggle() { expanded = !expanded; updateAccessibility(); root.invalidate(); }
    void updateAccessibility() {
        if (!visible || session == null) {
            if (accessibilityControl.getParent() == root) root.removeView(accessibilityControl);
            return;
        }
        if (accessibilityControl.getParent() == null) root.addView(accessibilityControl);
        accessibilityControl.bringToFront();
        layoutBounds(root.getResources().getDisplayMetrics().density);
        android.widget.FrameLayout.LayoutParams current = accessibilityControl.getLayoutParams() instanceof android.widget.FrameLayout.LayoutParams
            ? (android.widget.FrameLayout.LayoutParams)accessibilityControl.getLayoutParams() : null;
        int width = (int)Math.ceil(bounds.width()), height = (int)Math.ceil(bounds.height());
        int left = Math.round(bounds.left) - root.getPaddingLeft(), top = Math.round(bounds.top) - root.getPaddingTop();
        if (current == null || current.width != width || current.height != height || current.leftMargin != left || current.topMargin != top) {
            android.widget.FrameLayout.LayoutParams layout = new android.widget.FrameLayout.LayoutParams(width, height);
            layout.leftMargin = left; layout.topMargin = top; accessibilityControl.setLayoutParams(layout);
        }
        String description = "Performance, " + (expanded ? "expanded. Collapse" : "collapsed. Expand");
        if (snapshot != null) try {
            JSONObject sample = new JSONObject(snapshot);
            description += String.format(Locale.ROOT, ". %.0f frames per second, %d live nodes", sample.optDouble("fps"), sample.optInt("liveNodes"));
        } catch (org.json.JSONException ignored) { }
        if (!description.contentEquals(accessibilityControl.getContentDescription() == null ? "" : accessibilityControl.getContentDescription()))
            accessibilityControl.setContentDescription(description);
    }
    String snapshot() { return snapshot; }
    @Override public void doFrame(long frameTimeNanos) {
        scheduled = false;
        if (session == null || !root.isAttachedToWindow() || root.getWindowVisibility() != android.view.View.VISIBLE) return;
        boolean sample = lastSample == 0 || frameTimeNanos - lastSample >= 1_000_000_000L;
        double refresh = root.getDisplay() == null ? 0 : root.getDisplay().getRefreshRate();
        String result = NativeRuntime.performance(session, refresh, drawNanos / 1e9, changed, sample);
        changed = false; drawNanos = 0;
        if (sample) { lastSample = frameTimeNanos; snapshot = result; if (visible) { updateAccessibility(); root.invalidate(); } }
        start();
    }
    void draw(Canvas canvas, long nanos) {
        changed = true; drawNanos += Math.max(0, nanos);
        if (!visible || snapshot == null) return;
        int save = canvas.save();
        try {
            JSONObject sample = new JSONObject(snapshot), process = sample.getJSONObject("process");
            float density = root.getResources().getDisplayMetrics().density;
            layoutBounds(density);
            float left = bounds.left, top = bounds.top;
            canvas.clipRect(bounds);
            paint.setColor(0xee15202b); canvas.drawRoundRect(bounds, 6 * density, 6 * density, paint);
            paint.setColor(0xffeeeeee); paint.setTextSize(11 * density);
            if (!expanded) {
                canvas.drawText(String.format(Locale.ROOT, "%.0f FPS +", sample.optDouble("fps")), left + 6 * density, top + 16 * density, paint);
                return;
            }
            JSONObject heap = sample.optJSONObject("heap");
            String[] lines = {
                String.format(Locale.ROOT, "%.0f FPS  %.0f Hz  commits %d/s  −", sample.optDouble("fps"), sample.optDouble("refreshRate"), sample.optInt("commitsPerSecond")),
                String.format(Locale.ROOT, "Frame %.2f  p95 %.2f  JS %.2f  core %.2f ms", sample.getJSONObject("frame").optDouble("meanMs"), sample.getJSONObject("frame").optDouble("p95Ms"), sample.getJSONObject("js").optDouble("p95Ms"), sample.getJSONObject("core").optDouble("p95Ms")),
                "CPU " + counter(process, "process_percent", 1, "%") + "  UI " + counter(process, "ui_percent", 1, "%"),
                "RSS " + counter(process, "working_set", 1048576, " MiB") + "  heap " + (heap == null ? "—" : counter(heap, "allocatedBytes", 1048576, " MiB")),
                "Nodes " + sample.optInt("liveNodes") + "  retired " + sample.optInt("retiredNodes") + "  objects " + (heap == null ? "—" : heap.optLong("objects"))
            };
            for (int i = 0; i < lines.length; i++) canvas.drawText(lines[i], left + 6 * density, top + (13 + i * 14) * density, paint);
            org.json.JSONArray frames = sample.optJSONArray("framesMs");
            if (frames != null && frames.length() > 0) {
                int count = Math.min(120, frames.length()), start = frames.length() - count;
                double budget = sample.optDouble("refreshRate", 60); budget = budget > 0 ? 1000 / budget : 16.667;
                double scale = budget * 2;
                for (int i = start; i < frames.length(); i++) scale = Math.max(scale, frames.optDouble(i, 0));
                Path graph = new Path();
                for (int i = 0; i < count; i++) {
                    float gx = left + (6 + 276f * i / 119) * density;
                    float gy = top + (92 - (float)(18 * Math.max(0, frames.optDouble(start + i, 0)) / scale)) * density;
                    if (i == 0) graph.moveTo(gx, gy); else graph.lineTo(gx, gy);
                }
                paint.setStyle(Paint.Style.STROKE); paint.setStrokeWidth(density); paint.setColor(0xff59d9aa);
                canvas.drawPath(graph, paint); paint.setStyle(Paint.Style.FILL);
                paint.setColor(0xfffdcc66); paint.setStrokeWidth(density);
                canvas.drawLine(left + 6 * density, top + (92 - (float)(18 * budget / scale)) * density,
                    left + 282 * density, top + (92 - (float)(18 * budget / scale)) * density, paint);
            }
        } catch (org.json.JSONException ignored) { /* The next sample replaces incomplete data. */ }
        finally { paint.setStyle(Paint.Style.FILL); canvas.restoreToCount(save); }
    }
    private void layoutBounds(float density) {
        float width = (expanded ? 288 : 76) * density, height = (expanded ? 96 : 24) * density;
        float availableWidth = Math.max(0, root.getWidth() - root.getPaddingLeft() - root.getPaddingRight());
        float availableHeight = Math.max(0, root.getHeight() - root.getPaddingTop() - root.getPaddingBottom());
        x = Math.max(0, Math.min(x, Math.max(0, (availableWidth - width) / density)));
        y = Math.max(0, Math.min(y, Math.max(0, (availableHeight - height) / density)));
        bounds.set(root.getPaddingLeft() + x * density, root.getPaddingTop() + y * density,
            root.getPaddingLeft() + x * density + Math.min(width, availableWidth),
            root.getPaddingTop() + y * density + Math.min(height, availableHeight));
    }
    boolean touch(MotionEvent event) {
        if (!visible || snapshot == null) return false;
        float density = root.getResources().getDisplayMetrics().density; layoutBounds(density);
        if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
            if (!bounds.contains(event.getX(), event.getY())) return false;
            tracking = true; moved = false; pointer = event.getPointerId(0);
            downX = event.getX(); downY = event.getY(); originX = x; originY = y; return true;
        }
        if (!tracking) return false;
        if (event.getActionMasked() == MotionEvent.ACTION_POINTER_UP && event.getPointerId(event.getActionIndex()) == pointer) {
            tracking = false; pointer = -1; return true;
        }
        int index = event.findPointerIndex(pointer);
        if (index < 0 || event.getActionMasked() == MotionEvent.ACTION_CANCEL) { tracking = false; pointer = -1; return true; }
        float dx = event.getX(index) - downX, dy = event.getY(index) - downY;
        if (Math.hypot(dx, dy) > android.view.ViewConfiguration.get(root.getContext()).getScaledTouchSlop()) moved = true;
        if (moved) { x = originX + dx / density; y = originY + dy / density; }
        if (event.getActionMasked() == MotionEvent.ACTION_UP) { if (!moved) toggle(); tracking = false; pointer = -1; }
        if (moved) updateAccessibility();
        root.invalidate(); return true;
    }
    private static String counter(JSONObject process, String key, double divisor, String suffix) {
        return process.isNull(key) ? "—" : String.format(Locale.ROOT, "%.1f%s", process.optDouble(key) / divisor, suffix);
    }
}
