package dev.jadix.nativeui;

import android.graphics.*;
import android.graphics.drawable.Drawable;
import org.json.*;

/** Background geometry is rebuilt only when the shared style or bounds change. */
final class NativeSurface extends Drawable {
    final JSONObject spec;
    final int fallback;
    final float density;
    final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    final Path shape = new Path();
    final RectF rect = new RectF();
    final float[] radii = new float[8];
    Shader[] backgrounds = new Shader[0], masks = new Shader[0];
    int alpha = 255;
    ColorFilter filter;
    NativeBackdrop backdrop;

    NativeSurface(JSONObject spec, int fallback, float density) {
        this.spec = spec; this.fallback = fallback; this.density = density;
    }

    @Override protected void onBoundsChange(Rect bounds) {
        rect.set(0, 0, bounds.width(), bounds.height());
        JSONArray corners = spec.optJSONArray("corners");
        float[] values = new float[4];
        for (int i = 0; i < 4; i++) values[i] = corners == null ? 0 : (float)corners.optDouble(i) * density;
        float fit = 1;
        for (int i = 0; i < 4; i++) {
            float sum = values[i] + values[(i + 1) % 4];
            if (sum > 0) fit = Math.min(fit, (i % 2 == 0 ? rect.width() : rect.height()) / sum);
        }
        for (int i = 0; i < 4; i++) radii[2 * i] = radii[2 * i + 1] = values[i] * Math.max(0, fit);
        shape.set(path(rect, radii));
        backgrounds = shaders(spec.optJSONArray("backgrounds"));
        masks = shaders(spec.optJSONArray("masks"));
    }

    Path path(RectF bounds, float[] rounded) {
        Path path = new Path();
        if (bounds.isEmpty()) return path;
        if (!spec.optBoolean("continuous_corners")) { path.addRoundRect(bounds, rounded, Path.Direction.CW); return path; }
        try {
            JSONArray corners = new JSONArray(); for (float radius : rounded) corners.put(radius);
            JSONArray segments = (JSONArray)NativeRuntime.call(NativeRuntime.object("roundedOutline")
                .put("bounds",new JSONArray().put(bounds.left).put(bounds.top).put(bounds.width()).put(bounds.height()))
                .put("radii",corners).put("continuous",true));
            for (int i=0;i<segments.length();i++) {
                JSONObject segment=segments.getJSONObject(i); JSONArray p=segment.optJSONArray("points");
                switch(segment.getString("op")) {
                    case "move": path.moveTo((float)p.getDouble(0),(float)p.getDouble(1)); break;
                    case "line": path.lineTo((float)p.getDouble(0),(float)p.getDouble(1)); break;
                    case "cubic": path.cubicTo((float)p.getDouble(0),(float)p.getDouble(1),(float)p.getDouble(2),(float)p.getDouble(3),(float)p.getDouble(4),(float)p.getDouble(5)); break;
                    case "close": path.close(); break;
                }
            }
            return path;
        } catch(JSONException error) { throw new IllegalArgumentException("Invalid rounded outline",error); }
    }

    static NativeSurface from(android.graphics.drawable.Drawable drawable) {
        if (drawable instanceof NativeSurface) return (NativeSurface)drawable;
        if (drawable instanceof android.graphics.drawable.LayerDrawable) {
            android.graphics.drawable.LayerDrawable layers=(android.graphics.drawable.LayerDrawable)drawable;
            for (int i=0;i<layers.getNumberOfLayers();i++) { NativeSurface surface=from(layers.getDrawable(i)); if(surface!=null)return surface; }
        }
        return null;
    }

    int beginComposite(Canvas canvas,Matrix matrix) {
        int save=masks.length>0?canvas.saveLayer(null,null):canvas.save();
        if(spec.optBoolean("clip")){Path path=new Path(shape);path.transform(matrix);canvas.clipPath(path);}
        return save;
    }
    void endComposite(Canvas canvas,Matrix matrix,int save) {
        if(masks.length>0){
            Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.DST_IN));
            int mask=canvas.saveLayer(null,paint);paint.setXfermode(null);canvas.concat(matrix);
            for(int i=masks.length-1;i>=0;i--){paint.setShader(masks[i]);canvas.drawRect(rect,paint);}
            canvas.restoreToCount(mask);
        }
        canvas.restoreToCount(save);
    }

    private Shader[] shaders(JSONArray layers) {
        if (layers == null || rect.isEmpty()) return new Shader[0];
        Shader[] result = new Shader[layers.length()];
        for (int i = 0; i < result.length; i++) result[i] = NativeGradient.shader(layers.optJSONObject(i), rect.width(), rect.height(), density);
        return result;
    }

    @Override public void draw(Canvas canvas) {
        if (rect.isEmpty()) return;
        int save = canvas.save(); canvas.translate(getBounds().left, getBounds().top);
        NativeShadow.draw(canvas,this,false);
        if(backdrop!=null) {
            int clip=canvas.save();canvas.clipPath(shape);backdrop.draw(canvas,alpha);canvas.restoreToCount(clip);
        }
        paint.reset(); paint.setAntiAlias(true); paint.setColorFilter(filter);
        paint.setColor(NativeAppearance.color(spec.optJSONArray("background"), fallback));
        paint.setAlpha(Color.alpha(paint.getColor()) * alpha / 255); canvas.drawPath(shape, paint);
        paint.setColor(Color.WHITE); paint.setAlpha(alpha);
        for (int i = backgrounds.length - 1; i >= 0; i--) {
            paint.setShader(backgrounds[i]); canvas.drawPath(shape, paint);
        }
        paint.setShader(null);
        NativeShadow.draw(canvas,this,true);
        borders(canvas);
        outline(canvas);
        canvas.restoreToCount(save);
    }

    private void borders(Canvas canvas) {
        JSONArray widths = spec.optJSONArray("border_widths"), colors = spec.optJSONArray("border_colors");
        if (widths == null) return;
        float[] w = new float[4]; for (int i = 0; i < 4; i++) w[i] = Math.max(0, (float)widths.optDouble(i) * density);
        Path inner = new Path(); RectF inset = new RectF(w[3], w[0], rect.right - w[1], rect.bottom - w[2]);
        float[] r = {Math.max(0,radii[0]-w[3]),Math.max(0,radii[1]-w[0]),Math.max(0,radii[2]-w[1]),Math.max(0,radii[3]-w[0]),
            Math.max(0,radii[4]-w[1]),Math.max(0,radii[5]-w[2]),Math.max(0,radii[6]-w[3]),Math.max(0,radii[7]-w[2])};
        if (!inset.isEmpty()) inner.set(path(inset, r));
        int clip = canvas.save(); canvas.clipPath(shape); canvas.clipOutPath(inner);
        float width = rect.width(), height = rect.height();
        float[][] sides = {{0,0,width,0,width-w[1],w[0],w[3],w[0]}, {width,0,width,height,width-w[1],height-w[2],width-w[1],w[0]},
            {width,height,0,height,w[3],height-w[2],width-w[1],height-w[2]}, {0,height,0,0,w[3],w[0],w[3],height-w[2]}};
        String style = spec.optString("border_style", "solid");
        for (int i = 0; i < 4; i++) {
            if (w[i] <= 0) continue;
            paint.setColor(colors == null ? Color.BLACK : NativeAppearance.color(colors.optJSONArray(i), 0));
            paint.setAlpha(Color.alpha(paint.getColor()) * alpha / 255);
            Path wedge = new Path(); float[] points = sides[i]; wedge.moveTo(points[0],points[1]);
            for (int n = 2; n < 8; n += 2) wedge.lineTo(points[n],points[n+1]); wedge.close();
            int side = canvas.save(); canvas.clipPath(wedge);
            if ("solid".equals(style)) { paint.setStyle(Paint.Style.FILL); canvas.drawRect(rect, paint); }
            else {
                // The shared rounded perimeter supplies curved dashes at the corners.
                paint.setStyle(Paint.Style.STROKE); paint.setStrokeWidth(w[i] * 2); stroke(style, w[i]);
                canvas.drawPath(shape, paint); paint.setPathEffect(null);
            }
            canvas.restoreToCount(side);
        }
        paint.setStyle(Paint.Style.FILL); canvas.restoreToCount(clip);
    }

    private void stroke(String style, float width) {
        paint.setStrokeCap("dotted".equals(style) ? Paint.Cap.ROUND : Paint.Cap.BUTT);
        paint.setPathEffect("solid".equals(style) ? null : "dotted".equals(style)
            ? new DashPathEffect(new float[]{Math.max(.01f,width*.01f),width*2},0)
            : new DashPathEffect(new float[]{width*3,width*3},0));
    }

    private void outline(Canvas canvas) {
        float width = (float)spec.optDouble("outline_width") * density;
        if (width <= 0) return;
        float offset = (float)spec.optDouble("outline_offset") * density + width / 2;
        RectF bounds = new RectF(rect); bounds.inset(-offset, -offset);
        if (bounds.isEmpty()) return;
        float[] rounded = radii.clone(); for (int i = 0; i < rounded.length; i++) rounded[i] = Math.max(0, rounded[i] + offset);
        Path path = path(bounds, rounded);
        paint.setStyle(Paint.Style.STROKE); paint.setStrokeWidth(width);
        paint.setColor(NativeAppearance.color(spec.optJSONArray("outline_color"), 0)); paint.setAlpha(Color.alpha(paint.getColor()) * alpha / 255);
        stroke(spec.optString("outline_style","solid"), width); canvas.drawPath(path, paint); paint.setPathEffect(null);
    }

    @Override public void getOutline(Outline outline) {
        if (rect.isEmpty()) { outline.setEmpty(); return; }
        Path path = new Path(shape); path.offset(getBounds().left,getBounds().top);
        if (android.os.Build.VERSION.SDK_INT >= 30) outline.setPath(path);
        else outline.setConvexPath(path);
    }
    @Override public void setAlpha(int value) { alpha = value; invalidateSelf(); }
    @Override public void setColorFilter(ColorFilter value) { filter = value; invalidateSelf(); }
    @Override public int getOpacity() { return PixelFormat.TRANSLUCENT; }
}
