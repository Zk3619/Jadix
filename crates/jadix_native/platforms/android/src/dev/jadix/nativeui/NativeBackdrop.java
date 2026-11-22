package dev.jadix.nativeui;

import android.graphics.*;
import android.view.*;
import org.json.*;

/** Records only ancestor backgrounds and preceding siblings, never the target itself. */
final class NativeBackdrop implements ViewTreeObserver.OnPreDrawListener, View.OnAttachStateChangeListener {
    private static boolean recording;
    private final java.lang.ref.WeakReference<View> owner;
    private final Hardware hardware;
    private JSONArray filters,matrices,samples;
    private JSONObject progressive;
    private float density;
    private boolean ownInvalidation;
    private String signature;
    private ViewTreeObserver observer;
    private final NativeCapture.Watch textures;

    NativeBackdrop(View view) {
        owner=new java.lang.ref.WeakReference<>(view);
        hardware=android.os.Build.VERSION.SDK_INT>=31?new Hardware():null;
        textures=hardware==null?new NativeCapture.Watch(view):null;
        view.addOnAttachStateChangeListener(this);
        if(view.isAttachedToWindow())onViewAttachedToWindow(view);
    }
    void configure(JSONArray filters,JSONObject progressive,float density) throws JSONException {
        String next=filters.toString()+":"+progressive+":"+density;
        if(next.equals(signature))return;
        this.filters=filters;this.progressive=progressive;this.density=density;
        matrices=(JSONArray)NativeRuntime.call(NativeRuntime.object("filterMatrices").put("filters",filters));
        samples=progressive==null?null:(JSONArray)NativeRuntime.call(NativeRuntime.object("progressiveFilters").put("profile",progressive).put("filters",filters));
        if(hardware!=null)hardware.configure(filters,matrices,samples,density);
        signature=next;
        View view=owner.get();if(view!=null)view.invalidate();
    }
    void release() {
        View view=owner.get();if(view!=null)view.removeOnAttachStateChangeListener(this);
        detach();discard();if(textures!=null)textures.close();
    }
    private void detach() {
        if(observer!=null&&observer.isAlive())observer.removeOnPreDrawListener(this);
        observer=null;
    }
    @Override public void onViewAttachedToWindow(View view) {
        detach();observer=view.getViewTreeObserver();observer.addOnPreDrawListener(this);
    }
    private void discard(){if(hardware!=null)hardware.discard();}
    @Override public void onViewDetachedFromWindow(View view) { detach();discard(); }
    @Override public boolean onPreDraw() {
        View view=owner.get();
        if(view==null){release();return true;}
        if(recording||!view.isShown()||view.getWidth()==0||view.getHeight()==0)return true;
        if(hardware==null) {
            // A backdrop depends on preceding siblings. One redraw joins their
            // damage; skip the redraw we scheduled so idle roots stay idle.
            if(ownInvalidation){ownInvalidation=false;return true;}
            ownInvalidation=true;view.invalidate();return true;
        }
        hardware.capture(view);
        return true;
    }
    void draw(Canvas canvas,int alpha) {
        View view=owner.get();if(view==null)return;
        if(hardware!=null&&hardware.canDraw(canvas))
            hardware.draw(canvas,alpha,view);
        else raster(canvas,alpha);
    }
    // Keep API 29/31 graphics types out of the class loaded on API 26.
    private static Shader mask(JSONObject sample,float height) throws JSONException {
        JSONArray stops=sample.getJSONArray("stops");int[] colors=new int[stops.length()];float[] positions=new float[stops.length()];
        for(int i=0;i<stops.length();i++){JSONArray stop=stops.getJSONArray(i);positions[i]=(float)stop.getDouble(0);colors[i]=Color.argb(Math.round((float)stop.getDouble(1)*255),255,255,255);}
        return new LinearGradient(0,0,0,height,colors,positions,Shader.TileMode.CLAMP);
    }
    private static void mask(Canvas canvas,Shader shader,float width,float height) {
        Paint paint=new Paint();paint.setShader(shader);paint.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.DST_IN));canvas.drawRect(0,0,width,height,paint);
    }
    private static final class Hardware {
        final RenderNode sharp=new RenderNode("jadix-backdrop-source");
        RenderNode[] nodes=new RenderNode[0];Shader[] masks=new Shader[0];JSONArray samples;
        float padding,height=-1;
        void configure(JSONArray filters,JSONArray matrices,JSONArray samples,float density) throws JSONException {
            discard();this.samples=samples;padding=NativeRaster.padding(filters,density);
            nodes=new RenderNode[samples==null?1:samples.length()];masks=new Shader[nodes.length];height=-1;
            for(int i=0;i<nodes.length;i++) {
                nodes[i]=new RenderNode("jadix-backdrop-sample");nodes[i].setClipToBounds(false);
                JSONObject sample=samples==null?null:samples.getJSONObject(i);
                nodes[i].setRenderEffect(NativeEffects.Hardware.effect(sample==null?filters:sample.getJSONArray("filters"),sample==null?matrices:sample.getJSONArray("matrices"),density,0));
            }
        }
        boolean canDraw(Canvas canvas){return canvas.isHardwareAccelerated()&&sharp.hasDisplayList();}
        void discard(){for(RenderNode node:nodes)node.discardDisplayList();sharp.discardDisplayList();}
        void capture(View view) {
            java.util.ArrayList<View> chain=new java.util.ArrayList<>();
            for(View current=view;current!=null;current=current.getParent() instanceof View?(View)current.getParent():null)chain.add(0,current);
            if(chain.size()<2)return;
            Matrix target=new Matrix(),inverse=new Matrix(),root=new Matrix();
            view.transformMatrixToGlobal(target);if(!target.invert(inverse))return;
            chain.get(0).transformMatrixToGlobal(root);
            int gutter=(int)Math.ceil(padding),w=view.getWidth()+2*gutter,h=view.getHeight()+2*gutter;
            boolean had=sharp.hasDisplayList();sharp.setPosition(0,0,w,h);
            RecordingCanvas canvas=sharp.beginRecording(w,h);recording=true;
            try {canvas.translate(gutter,gutter);canvas.concat(inverse);canvas.concat(root);NativeCapture.behind(canvas,chain,0);}
            finally {recording=false;sharp.endRecording();}
            try {
                for(int i=0;i<nodes.length;i++) {
                    nodes[i].setPosition(-gutter,-gutter,w-gutter,h-gutter);
                    canvas=nodes[i].beginRecording(w,h);canvas.drawRenderNode(sharp);nodes[i].endRecording();
                    if(samples!=null&&height!=view.getHeight())masks[i]=mask(samples.getJSONObject(i),view.getHeight());
                }
                height=view.getHeight();
            }catch(JSONException error){throw new IllegalArgumentException("Invalid progressive mask",error);}
            if(!had)view.invalidate();
        }
        void draw(Canvas canvas,int alpha,View view) {
            int save=canvas.saveLayerAlpha(null,alpha);
            Paint add=new Paint();add.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.ADD));
            for(int i=0;i<nodes.length;i++) {
                int layer=canvas.saveLayer(null,add);canvas.drawRenderNode(nodes[i]);
                if(samples!=null)mask(canvas,masks[i],view.getWidth(),view.getHeight());
                canvas.restoreToCount(layer);
            }
            canvas.restoreToCount(save);
        }
    }
    private void raster(Canvas canvas,int alpha) {
        View view=owner.get();if(view==null||filters==null)return;
        float padding=NativeRaster.padding(filters,density);
        Bitmap bitmap=NativeCapture.backdrop(view,padding);if(bitmap==null)return;
        try {
            float scale=bitmap.getWidth()/(view.getWidth()+2*padding);
            if(samples==null)bitmap=NativeRaster.filter(bitmap,filters,matrices,density*scale);
            else {
                Bitmap result=Bitmap.createBitmap(bitmap.getWidth(),bitmap.getHeight(),Bitmap.Config.ARGB_8888);
                Canvas composed=new Canvas(result);Paint add=new Paint();add.setXfermode(new PorterDuffXfermode(PorterDuff.Mode.ADD));
                try {
                    for(int i=0;i<samples.length();i++) {
                        JSONObject sample=samples.getJSONObject(i);
                        Bitmap filtered=NativeRaster.filter(bitmap.copy(Bitmap.Config.ARGB_8888,true),sample.getJSONArray("filters"),sample.getJSONArray("matrices"),density*scale);
                        try {
                            int layer=composed.saveLayer(null,add);composed.drawBitmap(filtered,0,0,null);
                            composed.translate(0,padding*scale);
                            mask(composed,mask(sample,view.getHeight()*scale),result.getWidth(),view.getHeight()*scale);
                            composed.restoreToCount(layer);
                        }finally {filtered.recycle();}
                    }
                }catch(RuntimeException|JSONException|Error error){result.recycle();throw error;}
                bitmap.recycle();bitmap=result;
            }
            Paint paint=new Paint(Paint.FILTER_BITMAP_FLAG);paint.setAlpha(alpha);
            canvas.drawBitmap(bitmap,null,new RectF(-padding,-padding,view.getWidth()+padding,view.getHeight()+padding),paint);
        }catch(JSONException error){throw new IllegalArgumentException("Invalid backdrop filter",error);}
        finally {if(!bitmap.isRecycled())bitmap.recycle();}
    }
}
