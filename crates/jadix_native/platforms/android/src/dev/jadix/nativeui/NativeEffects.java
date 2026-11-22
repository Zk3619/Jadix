package dev.jadix.nativeui;

import android.graphics.*;
import android.os.Build;
import android.view.View;
import org.json.*;

/** Adapts the ordered shared filter plan to Android's rendering primitives. */
final class NativeEffects {
    private static final java.util.WeakHashMap<View,String> signatures=new java.util.WeakHashMap<>();
    private static final java.util.WeakHashMap<View,NativeBackdrop> backdrops=new java.util.WeakHashMap<>();
    private static final java.util.WeakHashMap<View,RasterPlan> rasters=new java.util.WeakHashMap<>();
    private static final class RasterPlan {
        final JSONArray filters,matrices;final float density;final String blend;final NativeCapture.Watch watch;
        RasterPlan(View view,JSONArray f,JSONArray m,float d,String b){filters=f;matrices=m;density=d;blend=b;watch=new NativeCapture.Watch(view);}
    }
    static NativeBackdrop backdrop(View view) { return backdrops.get(view); }
    static void dispose(View view) {
        signatures.remove(view);RasterPlan raster=rasters.remove(view);if(raster!=null)raster.watch.close(); NativeBackdrop backdrop=backdrops.remove(view); if(backdrop!=null)backdrop.release();
    }
    static void apply(View view, JSONObject style, float density) throws JSONException {
        JSONObject appearance = style.optJSONObject("view");
        JSONArray filters = appearance == null ? null : appearance.optJSONArray("filters");
        if (filters == null) filters = new JSONArray();
        String blend = appearance == null ? "normal" : appearance.optString("blend_mode","normal");
        JSONArray behind=appearance==null?null:appearance.optJSONArray("backdrop_filters");
        if(behind==null)behind=new JSONArray();
        {
            NativeBackdrop backdrop=backdrops.get(view);
            if(behind.length()==0) { if(backdrop!=null){backdrops.remove(view);backdrop.release();} }
            else {
                if(backdrop==null){backdrop=new NativeBackdrop(view);backdrops.put(view,backdrop);}
                backdrop.configure(behind,appearance.optJSONObject("backdrop_progressive"),density);
            }
        }
        String signature=filters.toString()+":"+blend+":"+style.optDouble("image_blur_radius")+":"+density;
        if(signature.equals(signatures.get(view)))return;
        signatures.put(view,signature);
        RasterPlan oldRaster=rasters.remove(view);if(oldRaster!=null)oldRaster.watch.close();
        JSONArray matrices = filters.length()==0?new JSONArray():(JSONArray)NativeRuntime.call(NativeRuntime.object("filterMatrices").put("filters",filters));
        Paint layer = new Paint();
        applyBlend(layer,blend);
        if (Build.VERSION.SDK_INT >= 31) {
            Hardware.apply(view,filters,matrices,density,(float)style.optDouble("image_blur_radius")*density);
        } else {
            boolean spatial=style.optDouble("image_blur_radius")>0;
            for(int i=0;i<filters.length();i++)if(matrices.isNull(i))spatial=true;
            if(spatial) {
                JSONArray ordered=new JSONArray(),colors=new JSONArray();
                if(style.optDouble("image_blur_radius")>0){ordered.put(new JSONObject().put("kind","blur").put("value",style.optDouble("image_blur_radius")));colors.put(JSONObject.NULL);}
                for(int i=0;i<filters.length();i++){ordered.put(filters.get(i));colors.put(matrices.get(i));}
                rasters.put(view,new RasterPlan(view,ordered,colors,density,blend));
            } else rasters.remove(view);
            ColorMatrix combined=new ColorMatrix(); boolean colored=false;
            for (int i=0;i<matrices.length();i++) if (!matrices.isNull(i)) {
                combined.postConcat(new ColorMatrix(matrix(matrices.getJSONArray(i)))); colored=true;
            }
            if (colored&&!spatial) layer.setColorFilter(new ColorMatrixColorFilter(combined));
        }
        boolean layered=layer.getColorFilter()!=null || !"normal".equals(blend);
        view.setLayerType(layered?View.LAYER_TYPE_HARDWARE:View.LAYER_TYPE_NONE,layered?layer:null);
    }

    static boolean drawRaster(Canvas canvas,View view,Matrix transform) {
        return drawRaster(canvas,view,transform,true);
    }
    static boolean drawRaster(Canvas canvas,View view,Matrix transform,boolean opacity) {
        RasterPlan plan=rasters.get(view);if(plan==null)return false;
        float padding=NativeRaster.padding(plan.filters,plan.density);
        Bitmap bitmap=NativeCapture.content(view,padding);if(bitmap==null)return false;
        try {
            float scale=bitmap.getWidth()/(view.getWidth()+2*padding);
            bitmap=NativeRaster.filter(bitmap,plan.filters,plan.matrices,plan.density*scale);
            Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG|Paint.FILTER_BITMAP_FLAG);applyBlend(paint,plan.blend);if(opacity)paint.setAlpha(Math.round(view.getAlpha()*255));
            int saved=canvas.save();canvas.concat(transform);
            canvas.drawBitmap(bitmap,null,new RectF(-padding,-padding,view.getWidth()+padding,view.getHeight()+padding),paint);
            canvas.restoreToCount(saved);return true;
        }catch(JSONException error){throw new IllegalArgumentException("Invalid raster filter",error);}
        finally { if(!bitmap.isRecycled())bitmap.recycle(); }
    }

    static float[] matrix(JSONArray values) {
        float[] matrix=new float[20];
        for(int i=0;i<20;i++)matrix[i]=(float)values.optDouble(i)*(i%5==4?255:1);
        return matrix;
    }
    private static void applyBlend(Paint paint,String blend) {
        if (Build.VERSION.SDK_INT >= 29) ModernBlend.apply(paint,blend);
        else switch (blend) {
            case "multiply": paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.MULTIPLY)); break;
            case "screen": paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.SCREEN)); break;
            case "darken": paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.DARKEN)); break;
            case "lighten": paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.LIGHTEN)); break;
            case "plus_lighter": paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.ADD)); break;
        }
    }
    private static final class ModernBlend {
        static void apply(Paint paint,String value){paint.setBlendMode(blend(value));}
    private static BlendMode blend(String value) {
        switch(value) {
            case "multiply":return BlendMode.MULTIPLY; case "screen":return BlendMode.SCREEN;
            case "overlay":return BlendMode.OVERLAY; case "darken":return BlendMode.DARKEN;
            case "lighten":return BlendMode.LIGHTEN; case "color_dodge":return BlendMode.COLOR_DODGE;
            case "color_burn":return BlendMode.COLOR_BURN; case "hard_light":return BlendMode.HARD_LIGHT;
            case "soft_light":return BlendMode.SOFT_LIGHT; case "difference":return BlendMode.DIFFERENCE;
            case "exclusion":return BlendMode.EXCLUSION; case "hue":return BlendMode.HUE;
            case "saturation":return BlendMode.SATURATION; case "color":return BlendMode.COLOR;
            case "luminosity":return BlendMode.LUMINOSITY; case "plus_darker":return BlendMode.DARKEN;
            case "plus_lighter":return BlendMode.PLUS; default:return BlendMode.SRC_OVER;
        }
    }
    }
    static final class Hardware {
        static void apply(View view,JSONArray filters,JSONArray matrices,float density,float imageBlur) throws JSONException {
            view.setRenderEffect(effect(filters,matrices,density,imageBlur));
        }
    static RenderEffect effect(JSONArray filters,JSONArray matrices,float density,float imageBlur) throws JSONException {
            RenderEffect effect = null;
            
            if (imageBlur > 0) effect = blur(effect,imageBlur);
            for (int i=0;i<filters.length();i++) {
                JSONObject filter=filters.getJSONObject(i);
                if (!matrices.isNull(i)) effect=color(effect,new ColorMatrixColorFilter(matrix(matrices.getJSONArray(i))));
                else if ("blur".equals(filter.optString("kind"))) effect=blur(effect,(float)filter.optDouble("value")*density);
                else if ("drop_shadow".equals(filter.optString("kind"))) {
                    JSONObject shadow=filter.getJSONObject("value"); JSONArray offset=shadow.optJSONArray("offset");
                    RenderEffect source=effect==null?RenderEffect.createOffsetEffect(0,0):effect;
                    RenderEffect cast=color(source,new BlendModeColorFilter(NativeAppearance.color(shadow.optJSONArray("color"),0),BlendMode.SRC_IN));
                    cast=blur(cast,(float)shadow.optDouble("blur")*density/2);
                    cast=RenderEffect.createOffsetEffect(offset==null?0:(float)offset.optDouble(0)*density,offset==null?0:(float)offset.optDouble(1)*density,cast);
                    effect=RenderEffect.createBlendModeEffect(cast,source,BlendMode.SRC_OVER);
                }
            }
        return effect;
    }

    private static RenderEffect color(RenderEffect input,ColorFilter filter) {
        return input==null?RenderEffect.createColorFilterEffect(filter):RenderEffect.createColorFilterEffect(filter,input);
    }
    private static RenderEffect blur(RenderEffect input,float radius) {
        if (radius<=0)return input;
        return input==null?RenderEffect.createBlurEffect(radius,radius,Shader.TileMode.DECAL):RenderEffect.createBlurEffect(radius,radius,input,Shader.TileMode.DECAL);
    }
    }
}
