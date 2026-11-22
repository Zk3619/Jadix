package dev.jadix.nativeui;

import android.graphics.*;
import org.json.*;

/** API 26 fallback for the shared ordered filter plan. Captures are pixel-bounded. */
final class NativeRaster {
    static Bitmap filter(Bitmap source, JSONArray filters, JSONArray matrices, float scale) throws JSONException {
        Bitmap current=source;
        for(int i=0;i<filters.length();i++) {
            JSONObject filter=filters.getJSONObject(i);
            if(!matrices.isNull(i)) {
                Bitmap next=Bitmap.createBitmap(current.getWidth(),current.getHeight(),Bitmap.Config.ARGB_8888);
                Paint paint=new Paint();paint.setColorFilter(new ColorMatrixColorFilter(NativeEffects.matrix(matrices.getJSONArray(i))));
                new Canvas(next).drawBitmap(current,0,0,paint);current.recycle();current=next;
            } else if("blur".equals(filter.optString("kind"))) {
                current=blur(current,(float)filter.optDouble("value")*scale);
            } else if("drop_shadow".equals(filter.optString("kind"))) {
                JSONObject shadow=filter.getJSONObject("value");JSONArray offset=shadow.optJSONArray("offset");
                Bitmap cast=Bitmap.createBitmap(current.getWidth(),current.getHeight(),Bitmap.Config.ARGB_8888);
                Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG);
                paint.setColorFilter(new PorterDuffColorFilter(NativeAppearance.color(shadow.optJSONArray("color"),0),PorterDuff.Mode.SRC_IN));
                new Canvas(cast).drawBitmap(current,0,0,paint);
                cast=blur(cast,(float)shadow.optDouble("blur")*scale/2);
                Bitmap next=Bitmap.createBitmap(current.getWidth(),current.getHeight(),Bitmap.Config.ARGB_8888);
                Canvas canvas=new Canvas(next);paint.setColorFilter(null);
                canvas.drawBitmap(cast,offset==null?0:(float)offset.optDouble(0)*scale,offset==null?0:(float)offset.optDouble(1)*scale,paint);
                canvas.drawBitmap(current,0,0,paint);cast.recycle();current.recycle();current=next;
            }
        }
        return current;
    }
    static float padding(JSONArray filters,float density) {
        float padding=0;
        for(int i=0;i<filters.length();i++) {
            JSONObject f=filters.optJSONObject(i);if(f==null)continue;
            if("blur".equals(f.optString("kind")))padding+=3*(float)f.optDouble("value")*density;
            else if("drop_shadow".equals(f.optString("kind"))) {
                JSONObject s=f.optJSONObject("value");if(s==null)continue;JSONArray o=s.optJSONArray("offset");
                padding+=1.5f*(float)s.optDouble("blur")*density+(o==null?0:Math.max(Math.abs((float)o.optDouble(0)),Math.abs((float)o.optDouble(1)))*density);
            }
        }
        return Math.min(256,padding);
    }
    private static Bitmap blur(Bitmap source,float sigma) {
        if(sigma<0.5f)return source;
        int width=source.getWidth(),height=source.getHeight();
        float factor=Math.min(1,Math.min(32/sigma,(float)Math.sqrt(1_048_576.0/((double)width*height))));
        int w=Math.max(1,Math.round(width*factor)),h=Math.max(1,Math.round(height*factor));
        Bitmap work=Bitmap.createScaledBitmap(source,w,h,true);
        int[] pixels=new int[w*h],scratch=new int[w*h];work.getPixels(pixels,0,w,0,0,w,h);
        if(work!=source)work.recycle();
        for(int i=0;i<pixels.length;i++) {
            int c=pixels[i],a=c>>>24;pixels[i]=a<<24|((c>>>16&255)*a/255)<<16|((c>>>8&255)*a/255)<<8|(c&255)*a/255;
        }
        // Three box passes approximate a Gaussian with variance r*(r+1).
        int radius=Math.max(1,Math.round((float)Math.sqrt(sigma*sigma*factor*factor+0.25f)-0.5f));
        for(int pass=0;pass<3;pass++){box(pixels,scratch,w,h,radius,true);box(scratch,pixels,w,h,radius,false);}
        for(int i=0;i<pixels.length;i++) {
            int c=pixels[i],a=c>>>24;
            pixels[i]=a==0?0:a<<24|Math.min(255,(c>>>16&255)*255/a)<<16|Math.min(255,(c>>>8&255)*255/a)<<8|Math.min(255,(c&255)*255/a);
        }
        Bitmap result=Bitmap.createBitmap(w,h,Bitmap.Config.ARGB_8888);result.setPixels(pixels,0,w,0,0,w,h);
        if(w!=width||h!=height) { Bitmap enlarged=Bitmap.createScaledBitmap(result,width,height,true);result.recycle();result=enlarged; }
        source.recycle();return result;
    }
    private static void box(int[] input,int[] output,int width,int height,int radius,boolean horizontal) {
        int lines=horizontal?height:width,length=horizontal?width:height,step=horizontal?1:width,divisor=2*radius+1;
        for(int line=0;line<lines;line++) {
            int base=horizontal?line*width:line;long a=0,r=0,g=0,b=0;
            for(int x=-radius;x<=radius;x++)if(x>=0&&x<length){int c=input[base+x*step];a+=c>>>24;r+=c>>>16&255;g+=c>>>8&255;b+=c&255;}
            for(int x=0;x<length;x++) {
                output[base+x*step]=(int)(a/divisor)<<24|(int)(r/divisor)<<16|(int)(g/divisor)<<8|(int)(b/divisor);
                int remove=x-radius,add=x+radius+1;
                if(remove>=0){int c=input[base+remove*step];a-=c>>>24;r-=c>>>16&255;g-=c>>>8&255;b-=c&255;}
                if(add<length){int c=input[base+add*step];a+=c>>>24;r+=c>>>16&255;g+=c>>>8&255;b+=c&255;}
            }
        }
    }
    private NativeRaster() { }
}
