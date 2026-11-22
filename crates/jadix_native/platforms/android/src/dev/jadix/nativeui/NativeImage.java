package dev.jadix.nativeui;

import android.graphics.*;
import android.os.Build;
import android.widget.ImageView;
import org.json.*;

final class NativeImage extends ImageView {
    private final NativeView root;
    private String source;
    private long generation;
    private JSONObject style = new JSONObject();
    private android.graphics.drawable.Drawable image;
    private void playback() {
        if(image instanceof android.graphics.drawable.Animatable) {
            android.graphics.drawable.Animatable animation=(android.graphics.drawable.Animatable)image;
            if(isAttachedToWindow()&&isShown()&&getWindowVisibility()==VISIBLE&&android.animation.ValueAnimator.areAnimatorsEnabled())animation.start();else animation.stop();
        }
    }
    private AutoCloseable loading, preview;
    private void cancelLoads() {
        for(AutoCloseable request:new AutoCloseable[]{loading,preview}) if(request!=null) try { request.close(); } catch(Exception ignored) {}
        loading=null; preview=null;
    }
    NativeImage(NativeView root) { super(root.getContext()); this.root = root; setScaleType(ScaleType.MATRIX); }
    void update(JSONObject node) throws JSONException {
        style = node.getJSONObject("style");
        setContentDescription(node.isNull("name") ? node.optString("alt") : node.optString("name"));
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
        String next = node.getString("source");
        if (next.equals(source)) { matrix(); return; }
        animate().cancel();
        cancelLoads();
        if(image instanceof android.graphics.drawable.Animatable)((android.graphics.drawable.Animatable)image).stop();
        JSONObject appearance=style.optJSONObject("view"); setAlpha(appearance==null?1:(float)appearance.optDouble("opacity",1));
        source = next; long expected = ++generation;
        String id = (String) getTag();
        if (id == null) return; // Measurement must not launch asynchronous work.
        root.post(() -> { if (expected == generation && root.owns(id, this)) root.emit(id, NativeView.payload("imageLoadStart")); });
        image = null; setImageDrawable(null);
        String placeholder = style.optString("image_placeholder", "");
        if (!placeholder.isEmpty() && !"null".equals(placeholder)) preview=root.resources.image(placeholder, bitmap -> {
            if (expected == generation && image == null && root.owns(id, this)) { setImageBitmap(bitmap); matrix(); }
        });
        loading=root.resources.drawable(next, bitmap -> {
            if (expected != generation || !root.owns(id, this)) return;
            image = bitmap;
            if(preview!=null) { try { preview.close(); } catch(Exception ignored) {} preview=null; }
            if (bitmap != null) {
                setImageDrawable(bitmap); matrix(); playback();
                try { root.emit(id, NativeView.payload("imageLoad").put("width", bitmap.getIntrinsicWidth()).put("height", bitmap.getIntrinsicHeight())); }
                catch (JSONException error) { throw new IllegalStateException(error); }
                long duration = style.optLong("image_fade_ms", 0);
                JSONObject current=style.optJSONObject("view");float opacity=current==null?1:(float)current.optDouble("opacity",1);
                if (duration > 0 && android.animation.ValueAnimator.areAnimatorsEnabled()) { setAlpha(0); animate().alpha(opacity).setDuration(Math.min(duration, 10000)).start(); }
            } else {
                if (getDrawable() == null) {
                    setImageResource(android.R.drawable.ic_menu_report_image);
                    matrix();
                }
                try { root.emit(id, NativeView.payload("imageError").put("message", "Image could not be loaded")); }
                catch (JSONException error) { throw new IllegalStateException(error); }
            }
            if (Build.VERSION.SDK_INT >= 30) setStateDescription(bitmap == null ? "Image unavailable" : null);
            root.emit(id, NativeView.payload("imageLoadEnd"));
        });
    }
    private void matrix() {
        if (getDrawable() == null || getWidth() == 0 || getHeight() == 0) return;
        float w = getDrawable().getIntrinsicWidth(), h = getDrawable().getIntrinsicHeight();
        if (w <= 0 || h <= 0) return;
        float sx = getWidth() / w, sy = getHeight() / h;
        String fit = style.optString("image_fit", "contain");
        if ("none".equals(fit)) sx = sy = 1;
        else if ("cover".equals(fit)) sx = sy = Math.max(sx, sy);
        else if (!"fill".equals(fit)) sx = sy = "scale_down".equals(fit) ? Math.min(1, Math.min(sx, sy)) : Math.min(sx, sy);
        JSONObject position = style.optJSONObject("image_position");
        JSONArray fraction = position == null ? null : position.optJSONArray("fraction"), offset = position == null ? null : position.optJSONArray("offset");
        float density = getResources().getDisplayMetrics().density;
        Matrix matrix = new Matrix(); matrix.setScale(sx, sy);
        matrix.postTranslate((getWidth()-w*sx) * (float) (fraction == null ? .5 : fraction.optDouble(0, .5)) + (float) (offset == null ? 0 : offset.optDouble(0)) * density,
            (getHeight()-h*sy) * (float) (fraction == null ? .5 : fraction.optDouble(1, .5)) + (float) (offset == null ? 0 : offset.optDouble(1)) * density);
        setImageMatrix(matrix);
    }
    @Override protected void onSizeChanged(int w, int h, int oldw, int oldh) { super.onSizeChanged(w,h,oldw,oldh); matrix(); }
    @Override protected void onAttachedToWindow() { super.onAttachedToWindow(); playback(); }
    @Override protected void onDetachedFromWindow() { if(image instanceof android.graphics.drawable.Animatable)((android.graphics.drawable.Animatable)image).stop(); super.onDetachedFromWindow(); }
    @Override protected void onVisibilityChanged(android.view.View changed,int visibility) { super.onVisibilityChanged(changed,visibility); playback(); }
    @Override protected void onWindowVisibilityChanged(int visibility) { super.onWindowVisibilityChanged(visibility); playback(); }
    void dispose() { generation++; source = null; style = new JSONObject(); cancelLoads(); animate().cancel(); if(image instanceof android.graphics.drawable.Animatable)((android.graphics.drawable.Animatable)image).stop(); setImageDrawable(null); image = null; }
}
