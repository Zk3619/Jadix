package dev.jadix.nativeui;

import android.graphics.*;
import android.util.LruCache;
import org.json.*;

/** Software rasterization makes colored, spread and inset box shadows consistent on API 26+. */
final class NativeShadow {
    private static final LruCache<String,Bitmap> cache=new LruCache<String,Bitmap>(16*1024*1024) {
        @Override protected int sizeOf(String key,Bitmap value){return value.getAllocationByteCount();}
    };
    static void draw(Canvas target,NativeSurface surface,boolean inset) {
        JSONArray shadows=surface.spec.optJSONArray("shadows");
        if(shadows==null||shadows.length()==0)return;
        float margin=0;
        for(int i=0;i<shadows.length();i++) {
            JSONObject s=shadows.optJSONObject(i); if(s==null||s.optBoolean("inset")!=inset)continue;
            JSONArray offset=s.optJSONArray("offset");
            margin=Math.max(margin,((float)s.optDouble("blur")*2+Math.max(0,(float)s.optDouble("spread"))
                +Math.max(offset==null?0:(float)Math.abs(offset.optDouble(0)),offset==null?0:(float)Math.abs(offset.optDouble(1))))*surface.density+2);
        }
        if(margin==0)return;
        float width=surface.rect.width()+margin*2,height=surface.rect.height()+margin*2;
        float scale=Math.min(1,Math.min(1024/width,1024/height));
        int w=Math.max(1,(int)Math.ceil(width*scale)),h=Math.max(1,(int)Math.ceil(height*scale));
        String key=surface.rect+":"+java.util.Arrays.toString(surface.radii)+":"+surface.spec.optBoolean("continuous_corners")+":"+shadows+":"+surface.density+":"+inset;
        Bitmap bitmap=cache.get(key);
        if(bitmap==null) {
            bitmap=Bitmap.createBitmap(w,h,Bitmap.Config.ARGB_8888);
            Canvas canvas=new Canvas(bitmap);canvas.scale(scale,scale);canvas.translate(margin,margin);
            Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
            if(inset)canvas.clipPath(surface.shape);
            for(int i=shadows.length()-1;i>=0;i--) {
                JSONObject s=shadows.optJSONObject(i);if(s==null||s.optBoolean("inset")!=inset)continue;
                float spread=(float)s.optDouble("spread")*surface.density,blur=(float)s.optDouble("blur")*surface.density/2;
                JSONArray offset=s.optJSONArray("offset");float x=offset==null?0:(float)offset.optDouble(0)*surface.density,y=offset==null?0:(float)offset.optDouble(1)*surface.density;
                RectF rect=new RectF(surface.rect);rect.inset(inset?spread:-spread,inset?spread:-spread);rect.offset(x,y);
                float[] radii=surface.radii.clone();for(int n=0;n<radii.length;n++)radii[n]=Math.max(0,radii[n]+(inset?-spread:spread));
                Path path=surface.path(rect,radii);
                if(inset){path.setFillType(Path.FillType.EVEN_ODD);path.addRect(-margin,-margin,width,height,Path.Direction.CW);}
                paint.setColor(NativeAppearance.color(s.optJSONArray("color"),0));paint.setMaskFilter(blur>0?new BlurMaskFilter(blur,BlurMaskFilter.Blur.NORMAL):null);
                canvas.drawPath(path,paint);
            }
            if(!inset){paint.setMaskFilter(null);paint.setColor(Color.BLACK);paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.DST_OUT));canvas.drawPath(surface.shape,paint);}
            cache.put(key,bitmap);
        }
        Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG|Paint.FILTER_BITMAP_FLAG);paint.setAlpha(surface.alpha);
        target.drawBitmap(bitmap,null,new RectF(-margin,-margin,width-margin,height-margin),paint);
    }
}
