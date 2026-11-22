package dev.jadix.nativeui;

import android.view.*;
import android.widget.*;
import org.json.*;
import java.util.ArrayList;
import java.util.function.Consumer;

/** Both axes share one document and logical coordinate system. */
final class NativeScroll extends ScrollView {
    final FrameLayout document;
    final HorizontalScrollView horizontal;
    private final FrameLayout scaled;
    private final NativeView root;
    private final ScaleGestureDetector pinch;
    private double zoom = 1, minimum = 1, maximum = 1;
    private boolean zoomGestures, configured, pending, disposed;
    String keyboardTaps="never", keyboardDismissMode="none";
    private float touchX,touchY;
    private boolean scrollEnabled=true, stopping, flungX, flungY, dragged;
    private JSONObject snap, refresh;
    private JSONArray indicatorInset;
    private boolean showX=true,showY=true,pulling,refreshing,momentum;
    private float pull;
    private android.animation.ValueAnimator pullAnimation;
    private final android.graphics.Paint overlayPaint=new android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG);
    private void activity(String phase,Double distance) {
        if(disposed||!(getTag() instanceof String))return;String id=(String)getTag();
        try {JSONObject value=new JSONObject().put("phase",phase);if(distance!=null)value.put("distance",distance);
            root.emit(id,NativeView.payload("scrollActivity").put("activity",value));
        }catch(JSONException error){throw new IllegalStateException(error);}
    }
    private void beginScrollDrag(){if(!dragged){dragged=true;activity("dragStart",null);}}
    private void endDrag(){if(dragged){dragged=false;activity("dragEnd",null);}}
    private void beginMomentum(){endDrag();if(!momentum){momentum=true;activity("momentumStart",null);}removeCallbacks(settled);postDelayed(settled,180);}
    private void setPull(float value) {pull=value;horizontal.setTranslationY(value);invalidate();}
    private void animatePull(float value) {
        if(pullAnimation!=null)pullAnimation.cancel();
        if(!android.animation.ValueAnimator.areAnimatorsEnabled()){setPull(value);return;}
        pullAnimation=android.animation.ValueAnimator.ofFloat(pull,value);pullAnimation.setDuration(200);
        pullAnimation.addUpdateListener(a->{if(!disposed)setPull((Float)a.getAnimatedValue());});pullAnimation.start();
    }
    private boolean canPull(MotionEvent event) {
        if(!scrollEnabled||refresh==null||refreshing||zoom!=1||getScrollY()!=0||event.getPointerCount()!=1)return false;
        View hit=NativeInteraction.deepest(document,(event.getX()+horizontal.getScrollX())/(float)zoom,(event.getY()+getScrollY())/(float)zoom);
        for(View item=hit;item!=null&&item!=document;item=item.getParent() instanceof View?(View)item.getParent():null)
            if(item instanceof NativeScroll||item.canScrollVertically(-1))return false;
        return true;
    }
    @Override public void onDrawForeground(android.graphics.Canvas canvas) {
        super.onDrawForeground(canvas);
        int saved=canvas.save();canvas.translate(getScrollX(),getScrollY());
        if(indicatorInset!=null) {
            float d=root.density(),thickness=Math.max(2,2*d),top=(float)indicatorInset.optDouble(0)*d,
                right=(float)indicatorInset.optDouble(1)*d,bottom=(float)indicatorInset.optDouble(2)*d,left=(float)indicatorInset.optDouble(3)*d;
            overlayPaint.setStyle(android.graphics.Paint.Style.FILL);overlayPaint.setColor(0x88666666);
            float contentH=document.getHeight()*(float)zoom,contentW=document.getWidth()*(float)zoom;
            if(showY&&contentH>getHeight()) {float track=Math.max(0,getHeight()-top-bottom),thumb=Math.min(track,Math.max(18*d,track*getHeight()/contentH));float y=top+(track-thumb)*getScrollY()/Math.max(1,contentH-getHeight());canvas.drawRoundRect(getWidth()-right-thickness,y,getWidth()-right,y+thumb,thickness,thickness,overlayPaint);}
            if(showX&&contentW>getWidth()) {float track=Math.max(0,getWidth()-left-right),thumb=Math.min(track,Math.max(18*d,track*getWidth()/contentW));float x=left+(track-thumb)*horizontal.getScrollX()/Math.max(1,contentW-getWidth());canvas.drawRoundRect(x,getHeight()-bottom-thickness,x+thumb,getHeight()-bottom,thickness,thickness,overlayPaint);}
        }
        if(pull>0) {float d=root.density(),radius=10*d,cx=getWidth()/2f,cy=pull/2;overlayPaint.setStyle(android.graphics.Paint.Style.STROKE);overlayPaint.setStrokeWidth(2*d);overlayPaint.setColor(0xff5577bb);float start=refreshing?(android.os.SystemClock.uptimeMillis()%1000)*.36f:-90;
            float sweep=refreshing?270:Math.min(360,pull/(float)(refresh==null?64:refresh.optDouble("threshold",64))/root.density()*360);
            canvas.drawArc(cx-radius,cy-radius,cx+radius,cy+radius,start,sweep,false,overlayPaint);if(refreshing)postInvalidateOnAnimation();
        }
        canvas.restoreToCount(saved);
    }
    private boolean snapping() {return snap!=null&&(snap.optBoolean("paging")||snap.optDouble("interval",0)>0||(snap.optJSONArray("offsets")!=null&&snap.optJSONArray("offsets").length()>0));}
    private double snapTarget(boolean x,double velocity) {
        try {Object value=NativeRuntime.call(NativeRuntime.object("scrollSnap").put("policy",snap)
            .put("offset",x?offsetX():offsetY()).put("velocity",velocity/root.density()/zoom)
            .put("viewport",x?viewportWidth():viewportHeight())
            .put("content",(x?document.getLayoutParams().width:document.getLayoutParams().height)/root.density()));
            return ((Number)value).doubleValue();
        }catch(JSONException error){throw new IllegalStateException(error);}
    }
    @Override public void fling(int velocity) {
        if(stopping){super.fling(velocity);return;}
        if(!scrollEnabled)return;flungY=true;beginMomentum();
        if(snapping())scroll(offsetX(),snapTarget(false,velocity),true);else super.fling(velocity);
    }
    @Override public boolean onInterceptTouchEvent(MotionEvent event) {boolean intercept=scrollEnabled&&super.onInterceptTouchEvent(event);if(intercept&&event.getActionMasked()==MotionEvent.ACTION_MOVE)beginScrollDrag();return intercept;}
    @Override public boolean onTouchEvent(MotionEvent event) {if(scrollEnabled&&event.getActionMasked()==MotionEvent.ACTION_MOVE)beginScrollDrag();return scrollEnabled&&super.onTouchEvent(event);}
    private String target;
    private long sampleTime;
    private double lastX, lastY, velocityX, velocityY;
    private final ArrayList<Consumer<JSONObject>> observers = new ArrayList<>();
    private final Runnable settled = this::settled;
    private void settled() {
        if (disposed) return;
        velocityX = velocityY = 0;
        if(momentum){momentum=false;activity("momentumEnd",null);}
        root.materialize();
    }
    NativeScroll(NativeView root) {
        super(root.getContext()); this.root = root;
        setFillViewport(false); setClipChildren(true);
        horizontal = new HorizontalScrollView(getContext()) {
            @Override public void fling(int velocity) {
                if(stopping){super.fling(velocity);return;}
                if(!scrollEnabled)return;flungX=true;beginMomentum();
                if(snapping())scroll(snapTarget(true,velocity),offsetY(),true);else super.fling(velocity);
            }
            @Override public boolean onInterceptTouchEvent(MotionEvent event){boolean intercept=scrollEnabled&&super.onInterceptTouchEvent(event);if(intercept&&event.getActionMasked()==MotionEvent.ACTION_MOVE)beginScrollDrag();return intercept;}
            @Override public boolean onTouchEvent(MotionEvent event){if(scrollEnabled&&event.getActionMasked()==MotionEvent.ACTION_MOVE)beginScrollDrag();return scrollEnabled&&super.onTouchEvent(event);}
        }; horizontal.setFillViewport(false);
        scaled = new NativeFrame(getContext()); document = new NativeFrame(getContext());
        document.setClipChildren(false); document.setPivotX(0); document.setPivotY(0);
        scaled.addView(document, new FrameLayout.LayoutParams(0,0));
        horizontal.addView(scaled, new HorizontalScrollView.LayoutParams(0,0));
        addView(horizontal, new ScrollView.LayoutParams(LayoutParams.MATCH_PARENT, 0));
        horizontal.setOnScrollChangeListener((view,x,y,oldx,oldy) -> changed());
        setOnScrollChangeListener((view,x,y,oldx,oldy) -> changed());
        pinch = new ScaleGestureDetector(getContext(), new ScaleGestureDetector.SimpleOnScaleGestureListener() {
            @Override public boolean onScale(ScaleGestureDetector detector) {
                if (!zoomGestures) return false;
                setZoom(zoom * detector.getScaleFactor(), detector.getFocusX(), detector.getFocusY()); return true;
            }
        });
    }
    void configure(JSONObject style) throws JSONException {
        boolean enabled=style.optBoolean("scroll_enabled",true);
        if(scrollEnabled&&!enabled)stop();scrollEnabled=enabled;snap=style.optJSONObject("scroll_snap");
        keyboardTaps=style.optString("keyboard_taps","never");
        keyboardDismissMode=style.optString("keyboard_dismiss_mode","none");
        refresh=style.optJSONObject("scroll_refresh");
        boolean nextRefreshing=refresh!=null&&refresh.optBoolean("refreshing");
        if(nextRefreshing!=refreshing){refreshing=nextRefreshing;animatePull(refreshing?(float)refresh.optDouble("inset",56)*root.density():0);}
        if(refresh==null&&pull>0){pulling=false;animatePull(0);}
        indicatorInset=style.optJSONArray("scrollbar_inset");
        JSONArray bars=style.optJSONArray("scrollbars");showY=bars==null||bars.optBoolean(1,true);showX=bars==null||bars.optBoolean(0,true);
        setVerticalScrollBarEnabled(showY&&indicatorInset==null);horizontal.setHorizontalScrollBarEnabled(showX&&indicatorInset==null);
        int mode = style.optBoolean("scroll_bounces", true) ? OVER_SCROLL_IF_CONTENT_SCROLLS : OVER_SCROLL_NEVER;
        setOverScrollMode(mode); horizontal.setOverScrollMode(mode);
        JSONObject limits = style.optJSONObject("scroll_zoom");
        minimum = limits == null ? 1 : limits.optDouble("min", 1);
        maximum = limits == null ? 1 : limits.optDouble("max", 1);
        zoomGestures = limits != null && limits.optBoolean("gestures", true);
        double next = configured ? zoom : limits == null ? 1 : limits.optDouble("initial", 1);
        configured = true; setZoom(next, 0, 0);
        JSONArray nextTarget = style.optJSONArray("scroll_to");
        String key = nextTarget == null ? null : nextTarget.toString();
        if (!java.util.Objects.equals(key, target)) {
            target = key;
            if(nextTarget!=null)post(()->{if(!disposed&&java.util.Objects.equals(key,target))scroll(nextTarget.optDouble(0),nextTarget.optDouble(1),"smooth".equals(style.optString("scroll_behavior")));});
        }
    }
    void extent() {
        int width = (int)Math.ceil(getWidth()/zoom), height = (int)Math.ceil(getHeight()/zoom);
        if(getTag() instanceof String){double[] all=root.documentExtent((String)getTag());width=Math.max(width,(int)Math.ceil(all[0]*root.density()));height=Math.max(height,(int)Math.ceil(all[1]*root.density()));}
        FrameLayout.LayoutParams dp = (FrameLayout.LayoutParams) document.getLayoutParams();
        if (dp.width != width || dp.height != height) { dp.width=width; dp.height=height; document.setLayoutParams(dp); }
        int scaledWidth = (int)Math.ceil(width*zoom), scaledHeight = (int)Math.ceil(height*zoom);
        ViewGroup.LayoutParams sp = scaled.getLayoutParams();
        if (sp.width != scaledWidth || sp.height != scaledHeight) { sp.width=scaledWidth; sp.height=scaledHeight; scaled.setLayoutParams(sp); }
        ViewGroup.LayoutParams hp = horizontal.getLayoutParams();
        if (hp.height != scaledHeight) { hp.height=scaledHeight; horizontal.setLayoutParams(hp); }
    }
    double offsetX() { return horizontal.getScrollX() / root.density() / zoom; }
    double offsetY() { return getScrollY() / root.density() / zoom; }
    double viewportWidth() { return getWidth() / root.density() / zoom; }
    double viewportHeight() { return getHeight() / root.density() / zoom; }
    void scroll(double x, double y, boolean smooth) {
        if (disposed || !Double.isFinite(x) || !Double.isFinite(y)) return;
        int px = (int)Math.round(Math.max(0,x)*root.density()*zoom), py=(int)Math.round(Math.max(0,y)*root.density()*zoom);
        if (smooth && android.animation.ValueAnimator.areAnimatorsEnabled()) { horizontal.smoothScrollTo(px,0); smoothScrollTo(0,py); }
        else { horizontal.scrollTo(px,0); scrollTo(0,py); }
    }
    void stop() {
        long now = android.os.SystemClock.uptimeMillis();
        MotionEvent cancel = MotionEvent.obtain(now,now,MotionEvent.ACTION_CANCEL,0,0,0);
        super.onTouchEvent(cancel); horizontal.onTouchEvent(cancel); cancel.recycle();
        // A zero-distance fling aborts both platform OverScroller animations.
        stopping=true;
        try{fling(0); horizontal.fling(0); scroll(offsetX(), offsetY(), false);}
        finally{stopping=false;}
        if(dragged){dragged=false;activity("dragEnd",null);}
        if(momentum){momentum=false;activity("momentumEnd",null);}
        if(pulling){pulling=false;animatePull(0);}
    }
    void setZoom(double value, float x, float y) {
        if (!Double.isFinite(value) || minimum <= 0 || maximum < minimum) return;
        double next = Math.max(minimum, Math.min(maximum, value)); if (next == zoom) return;
        double logicalX=(horizontal.getScrollX()+x)/zoom, logicalY=(getScrollY()+y)/zoom;
        zoom=next; document.setScaleX((float)zoom); document.setScaleY((float)zoom); extent();
        post(() -> { if(disposed)return;horizontal.scrollTo((int)(logicalX*zoom-x),0); scrollTo(0,(int)(logicalY*zoom-y)); changed(); });
        String id=(String)getTag();
        if (id != null) root.post(() -> {
            if (!root.owns(id,this)) return;
            try { root.emit(id,NativeView.payload("scrollZoom").put("scale",zoom).put("x",offsetX()).put("y",offsetY()).put("width",viewportWidth()).put("height",viewportHeight())); }
            catch (JSONException error) { throw new IllegalStateException(error); }
        });
    }
    JSONObject state() {
        try { return new JSONObject().put("offset", new JSONArray().put(offsetX()).put(offsetY()))
            .put("viewport", new JSONArray().put(viewportWidth()).put(viewportHeight()))
            .put("content", new JSONArray().put(document.getLayoutParams().width/root.density()).put(document.getLayoutParams().height/root.density())).put("zoom", zoom); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }
    JSONObject viewport() throws JSONException {
        return new JSONObject().put("offset", new JSONArray().put(offsetX()).put(offsetY()))
            .put("size",new JSONArray().put(viewportWidth()).put(viewportHeight()))
            .put("velocity",new JSONArray().put(velocityX).put(velocityY)).put("elapsed",Math.max(0,(android.os.SystemClock.uptimeMillis()-sampleTime)/1000.0));
    }
    AutoCloseable observe(Consumer<JSONObject> listener) {
        if (disposed || observers.size() >= 64) throw new IllegalStateException("Scroll observer budget exceeded");
        observers.add(listener); listener.accept(state()); return () -> { root.requireUiThread(); observers.remove(listener); };
    }
    private void changed() {
        if (disposed || pending) return; pending=true;
        postOnAnimation(() -> {
            pending=false; if (disposed) return;
            long now=android.os.SystemClock.uptimeMillis(); double x=offsetX(),y=offsetY(),dt=(now-sampleTime)/1000.0;
            velocityX=sampleTime > 0 && dt > 0 ? (x-lastX)/dt : 0; velocityY=sampleTime > 0 && dt > 0 ? (y-lastY)/dt : 0;
            lastX=x;lastY=y;sampleTime=now;
            String id=(String)getTag();
            if (id != null) try { root.emit(id,NativeView.payload("scroll").put("x",x).put("y",y).put("viewport",new JSONArray().put(viewportWidth()).put(viewportHeight()))); }
            catch (JSONException error) { throw new IllegalStateException(error); }
            for (Consumer<JSONObject> listener : new ArrayList<>(observers)) listener.accept(state());
            invalidate();root.materialize(); removeCallbacks(settled); postDelayed(settled, 180);
        });
    }
    @Override public boolean dispatchTouchEvent(MotionEvent event) {
        int action=event.getActionMasked();
        if(action==MotionEvent.ACTION_DOWN){
            if(momentum){momentum=false;activity("momentumEnd",null);}touchX=event.getX();touchY=event.getY();flungX=flungY=dragged=pulling=false;
        }
        float dx=event.getX()-touchX,dy=event.getY()-touchY,slop=ViewConfiguration.get(getContext()).getScaledTouchSlop();
        if(action==MotionEvent.ACTION_MOVE&&!pulling&&dy>slop&&dy>Math.abs(dx)&&canPull(event)) {
            MotionEvent cancel=MotionEvent.obtain(event);cancel.setAction(MotionEvent.ACTION_CANCEL);super.dispatchTouchEvent(cancel);cancel.recycle();
            pulling=true;if(getParent()!=null)getParent().requestDisallowInterceptTouchEvent(true);if(pullAnimation!=null)pullAnimation.cancel();
        }
        if(pulling) {
            if(action==MotionEvent.ACTION_MOVE) {setPull((float)Math.min(8192*root.density(),Math.max(0,dy-slop)*.5));activity("pull",(double)pull/root.density());}
            if(action==MotionEvent.ACTION_POINTER_DOWN||action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_CANCEL) {
                pulling=false;if(getParent()!=null)getParent().requestDisallowInterceptTouchEvent(false);boolean start=false;
                try {start=Boolean.TRUE.equals(NativeRuntime.call(NativeRuntime.object("refreshRelease").put("policy",refresh).put("distance",pull/root.density()).put("cancelled",action!=MotionEvent.ACTION_UP)));}catch(JSONException error){throw new IllegalStateException(error);}
                if(start){refreshing=true;animatePull((float)refresh.optDouble("inset",56)*root.density());activity("refresh",null);
                    post(()->{if(!disposed&&refreshing&&(refresh==null||!refresh.optBoolean("refreshing"))){refreshing=false;animatePull(0);}});
                }else animatePull(0);
            }
            return true;
        }
        if(action==MotionEvent.ACTION_MOVE&&!"none".equals(keyboardDismissMode)
                &&Math.hypot(event.getX()-touchX,event.getY()-touchY)>ViewConfiguration.get(getContext()).getScaledTouchSlop())NativeKeyboard.hide(this);
        if (zoomGestures&&scrollEnabled) pinch.onTouchEvent(event);
        boolean result=super.dispatchTouchEvent(event);
        if(action==MotionEvent.ACTION_UP&&dragged&&scrollEnabled&&snapping()) {
            double x=flungX?offsetX():snapTarget(true,0),y=flungY?offsetY():snapTarget(false,0);
            // Do not replace a platform fling/snap already started on the other axis.
            if(!flungX&&!flungY)scroll(x,y,true);
            else if(!flungX)horizontal.smoothScrollTo((int)Math.round(x*root.density()*zoom),0);
            else if(!flungY)smoothScrollTo(0,(int)Math.round(y*root.density()*zoom));
        }
        if((action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_CANCEL)&&dragged){dragged=false;activity("dragEnd",null);}
        return result;
    }
    @Override protected void onSizeChanged(int w,int h,int ow,int oh) { super.onSizeChanged(w,h,ow,oh); if (document != null) { extent(); changed(); } }
    void dispose() { stop(); disposed=true;if(pullAnimation!=null)pullAnimation.cancel();removeCallbacks(settled);observers.clear(); }
}
