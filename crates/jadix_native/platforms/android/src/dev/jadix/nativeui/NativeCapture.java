package dev.jadix.nativeui;

import android.graphics.*;
import android.view.*;

/** Bounded snapshots in target-local coordinates, including native texture leaves. */
final class NativeCapture {
    private static final java.util.Set<View> content=new java.util.HashSet<>(),backdrops=new java.util.HashSet<>();
    private static long activePixels;
    private static final java.util.WeakHashMap<View,Textures> textures=new java.util.WeakHashMap<>();
    private static final java.util.WeakHashMap<TextureView,Long> revisions=new java.util.WeakHashMap<>();
    static final class Watch implements View.OnAttachStateChangeListener,AutoCloseable {
        private final java.lang.ref.WeakReference<View> view;
        private Textures source;
        Watch(View value){view=new java.lang.ref.WeakReference<>(value);value.addOnAttachStateChangeListener(this);if(value.isAttachedToWindow())onViewAttachedToWindow(value);}
        public void onViewAttachedToWindow(View value){
            onViewDetachedFromWindow(value);View root=value.getRootView();source=textures.get(root);
            if(source==null){source=new Textures(root);textures.put(root,source);}source.readers.put(this,Boolean.TRUE);
        }
        public void onViewDetachedFromWindow(View value){if(source!=null){source.readers.remove(this);if(source.readers.isEmpty())source.close();source=null;}}
        public void close(){View value=view.get();if(value!=null){value.removeOnAttachStateChangeListener(this);onViewDetachedFromWindow(value);}}
    }
    private static final class Texture {
        Bitmap bitmap;long revision=Long.MIN_VALUE;
    }
    private static final class Textures implements ViewTreeObserver.OnPreDrawListener {
        final java.lang.ref.WeakReference<View> root;
        final java.util.WeakHashMap<Watch,Boolean> readers=new java.util.WeakHashMap<>();
        final java.util.WeakHashMap<TextureView,Texture> frames=new java.util.WeakHashMap<>();
        ViewTreeObserver observer;
        Textures(View value){root=new java.lang.ref.WeakReference<>(value);observer=value.getViewTreeObserver();observer.addOnPreDrawListener(this);}
        public boolean onPreDraw(){View value=root.get();if(value==null||readers.isEmpty())close();else prepare(value);return true;}
        void prepare(View value){
            java.util.Set<TextureView> retained=new java.util.HashSet<>();collect(value,retained,new long[]{4_194_304});
            frames.keySet().retainAll(retained);
        }
        void collect(View value,java.util.Set<TextureView> retained,long[] budget){
            if(value.getVisibility()!=View.VISIBLE||budget[0]<=0)return;
            if(value instanceof TextureView){
                TextureView texture=(TextureView)value;int width=value.getWidth(),height=value.getHeight();
                if(!texture.isAvailable()||width<=0||height<=0)return;
                float scale=(float)Math.min(1,Math.sqrt(Math.min(1_048_576,budget[0])/(double)width/height));
                int w=Math.max(1,(int)(width*scale)),h=Math.max(1,(int)(height*scale));budget[0]-=(long)w*h;retained.add(texture);
                Texture frame=frames.get(texture);if(frame==null){frame=new Texture();frames.put(texture,frame);}
                long revision=revisions.getOrDefault(texture,-1L);
                if(frame.bitmap!=null&&frame.bitmap.getWidth()==w&&frame.bitmap.getHeight()==h&&revision==frame.revision&&(revision>=0||!value.isDirty()))return;
                // TextureView forbids getBitmap during draw. Read back in pre-draw,
                // and publish immutable frames so retained display lists stay valid.
                try {Bitmap bitmap=texture.getBitmap(w,h);if(bitmap!=null){frame.bitmap=bitmap;frame.revision=revision;}}
                catch(IllegalStateException ignored){frames.remove(texture);}
            }else if(value instanceof ViewGroup){ViewGroup group=(ViewGroup)value;for(int i=0;i<group.getChildCount();i++)collect(group.getChildAt(i),retained,budget);}
        }
        void close(){if(observer!=null&&observer.isAlive())observer.removeOnPreDrawListener(this);observer=null;View value=root.get();if(value!=null&&textures.get(value)==this)textures.remove(value);frames.clear();}
    }
    static void changed(TextureView view){
        revisions.put(view,revisions.getOrDefault(view,0L)+1);
        Textures source=textures.get(view.getRootView());if(source!=null)view.getRootView().postInvalidateOnAnimation();
    }
    static void prepare(View view){Textures source=textures.get(view.getRootView());if(source!=null&&content.isEmpty()&&backdrops.isEmpty())source.prepare(view.getRootView());}
    static boolean drawTexture(Canvas canvas,View view){
        if(canvas.isHardwareAccelerated()||!(view instanceof TextureView))return false;
        Textures source=textures.get(view.getRootView());Texture frame=source==null?null:source.frames.get(view);
        if(frame!=null&&frame.bitmap!=null)canvas.drawBitmap(frame.bitmap,null,new Rect(0,0,view.getWidth(),view.getHeight()),new Paint(Paint.FILTER_BITMAP_FLAG));
        return true;
    }
    static Bitmap content(View view,float padding) {
        return capture(view,padding,false);
    }
    static Bitmap backdrop(View view,float padding) {
        return capture(view,padding,true);
    }
    private static Bitmap capture(View view,float padding,boolean backdrop) {
        int w=view.getWidth(),h=view.getHeight();
        java.util.Set<View> active=backdrop?backdrops:content;
        if(w<=0||h<=0||active.contains(view)||content.size()+backdrops.size()>=8||activePixels>=16_777_216)return null;
        double paddedW=w+2.0*padding,paddedH=h+2.0*padding;
        float scale=(float)Math.min(1,Math.sqrt(Math.min(4_194_304.0,16_777_216-activePixels)/(paddedW*paddedH)));
        Bitmap bitmap=Bitmap.createBitmap(Math.max(1,(int)Math.ceil(paddedW*scale)),Math.max(1,(int)Math.ceil(paddedH*scale)),Bitmap.Config.ARGB_8888);
        Canvas canvas=new Canvas(bitmap);canvas.scale(scale,scale);canvas.translate(padding,padding);
        long pixels=(long)bitmap.getWidth()*bitmap.getHeight();activePixels+=pixels;active.add(view);
        try {
            if(!backdrop){if(!drawTexture(canvas,view))view.draw(canvas);}
            else {
                java.util.ArrayList<View> chain=new java.util.ArrayList<>();
                for(View current=view;current!=null;current=current.getParent() instanceof View?(View)current.getParent():null)chain.add(0,current);
                if(chain.size()>1){Matrix target=new Matrix(),inverse=new Matrix(),root=new Matrix();view.transformMatrixToGlobal(target);
                    if(target.invert(inverse)){chain.get(0).transformMatrixToGlobal(root);canvas.concat(inverse);canvas.concat(root);behind(canvas,chain,0);}}
            }
        } catch(RuntimeException|Error error) {bitmap.recycle();throw error;}
        finally {active.remove(view);activePixels-=pixels;}
        return bitmap;
    }
    static void behind(Canvas canvas,java.util.List<View> chain,int depth) {
        ViewGroup group=(ViewGroup)chain.get(depth);View next=chain.get(depth+1);
        if(group.getBackground()!=null)group.getBackground().draw(canvas);
        java.util.ArrayList<View> children=new java.util.ArrayList<>();
        for(int i=0;i<group.getChildCount();i++)children.add(group.getChildAt(i));
        children.sort(java.util.Comparator.comparingDouble(View::getZ));
        for(View child:children) {
            if(child.getVisibility()!=View.VISIBLE&&child!=next)continue;
            int save=canvas.save();
            if(group.getClipChildren())canvas.clipRect(0,0,group.getWidth(),group.getHeight());
            if(group.getClipToPadding())canvas.clipRect(group.getPaddingLeft(),group.getPaddingTop(),group.getWidth()-group.getPaddingRight(),group.getHeight()-group.getPaddingBottom());
            canvas.translate(child.getLeft()-group.getScrollX(),child.getTop()-group.getScrollY());
            canvas.concat(child.getMatrix());
            if(child.getAlpha()<1)canvas.saveLayerAlpha(null,Math.round(child.getAlpha()*255));
            NativeSurface surface=NativeSurface.from(child.getBackground());int composite=0;
            if(surface!=null){surface.setBounds(0,0,child.getWidth(),child.getHeight());composite=surface.beginComposite(canvas,new Matrix());}
            if(child==next) {
                if(depth+2<chain.size())behind(canvas,chain,depth+1);
            }else if(!NativeEffects.drawRaster(canvas,child,new Matrix(),false)&&!drawTexture(canvas,child))child.draw(canvas);
            if(surface!=null)surface.endComposite(canvas,new Matrix(),composite);
            canvas.restoreToCount(save);if(child==next)return;
        }
    }
    private NativeCapture() { }
}
