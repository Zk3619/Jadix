package dev.jadix.nativeui;

import android.content.Context;
import android.graphics.*;
import android.view.View;
import android.view.MotionEvent;
import android.widget.FrameLayout;

/** Applies node masks and per-corner clipping to the complete child subtree on API 26+. */
class NativeFrame extends FrameLayout {
    NativeFrame(Context context) { super(context); }

    NativeView owner;
    void cancelNativeTouch() {
        long now=android.os.SystemClock.uptimeMillis();
        MotionEvent event=MotionEvent.obtain(now,now,MotionEvent.ACTION_CANCEL,0,0,0);
        try { dispatchCapturedTouch(event); } finally { event.recycle(); }
    }
    private void dispatchCapturedTouch(MotionEvent event) {
        if(touchTarget!=null)dispatchTo(touchTarget,event,true);
        touchTarget=null;disallowIntercept=false;
    }
    @Override protected void onDetachedFromWindow() {
        cancelNativeTouch();super.onDetachedFromWindow();
    }

    private View touchTarget;
    private boolean disallowIntercept;
    @Override public void requestDisallowInterceptTouchEvent(boolean value) {
        disallowIntercept=value;super.requestDisallowInterceptTouchEvent(value);
    }
    private boolean dispatchTo(View child,MotionEvent event,boolean cancel) {
        Matrix inverse=NativeInteraction.childMatrix(this,child);if(inverse==null)return false;
        MotionEvent local=MotionEvent.obtain(event);local.transform(inverse);
        if(cancel||child.getParent()!=this||child.getVisibility()!=VISIBLE||"none".equals(NativeInteraction.pointerEvents(child)))local.setAction(MotionEvent.ACTION_CANCEL);
        else if(!(child instanceof android.view.ViewGroup)) {
            if(local.getActionMasked()==MotionEvent.ACTION_UP&&!NativeInteraction.inside(child,local.getX(),local.getY(),true))local.setAction(MotionEvent.ACTION_CANCEL);
            if(NativeInteraction.inside(child,local.getX(),local.getY(),true)) {
            // Platform widgets test their original bounds. Clamp only the copied
            // sample; ancestor scrollers still see the real drag coordinates.
            float x=Math.max(0,Math.min(child.getWidth()-1,local.getX()));
            float y=Math.max(0,Math.min(child.getHeight()-1,local.getY()));
            local.offsetLocation(x-local.getX(),y-local.getY());
            }
        }
        try{return child.dispatchTouchEvent(local);}finally{local.recycle();}
    }
    @Override public boolean dispatchTouchEvent(MotionEvent event) {
        int action=event.getActionMasked();
        if(action==MotionEvent.ACTION_DOWN){touchTarget=null;disallowIntercept=false;}
        if("none".equals(NativeInteraction.pointerEvents(this))) {
            if(touchTarget!=null)dispatchTo(touchTarget,event,true);touchTarget=null;return false;
        }
        boolean intercepted=!disallowIntercept&&onInterceptTouchEvent(event);
        if(touchTarget!=null&&intercepted){dispatchTo(touchTarget,event,true);touchTarget=null;}
        if(action==MotionEvent.ACTION_DOWN&&!intercepted) {
            for(View child:NativeInteraction.frontToBack(this)) {
                Matrix inverse=NativeInteraction.childMatrix(this,child);if(inverse==null)continue;
                float[] local={event.getX(),event.getY()};inverse.mapPoints(local);
                if(NativeInteraction.deepest(child,local[0],local[1])!=null&&dispatchTo(child,event,false)){touchTarget=child;return true;}
            }
        }
        boolean handled=touchTarget!=null?dispatchTo(touchTarget,event,false)
            :!"box-none".equals(NativeInteraction.pointerEvents(this))&&onTouchEvent(event);
        if(action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_CANCEL){touchTarget=null;disallowIntercept=false;}
        return handled;
    }

    @Override public void onVisibilityAggregated(boolean visible) {
        super.onVisibilityAggregated(visible);
        NativeMotion.refreshSubtree(this);
    }
    @Override protected void onWindowVisibilityChanged(int visibility) {
        super.onWindowVisibilityChanged(visibility);
        NativeMotion.refreshSubtree(this);
    }

    @Override protected boolean drawChild(Canvas canvas, View child, long time) {
        NativeSurface surface=NativeSurface.from(child.getBackground());
        if (surface==null) return drawNativeChild(canvas,child,time);
        surface.setBounds(0,0,child.getWidth(),child.getHeight());
        boolean clipped=surface.spec.optBoolean("clip"), masked=surface.masks.length>0;
        if (!clipped&&!masked) return drawNativeChild(canvas,child,time);
        Matrix matrix=new Matrix(child.getMatrix());
        matrix.postTranslate(child.getLeft()-getScrollX(),child.getTop()-getScrollY());
        int saved=surface.beginComposite(canvas,matrix);
        boolean result=drawNativeChild(canvas,child,time);
        surface.endComposite(canvas,matrix,saved);
        return result;
    }
    private boolean drawNativeChild(Canvas canvas,View child,long time) {
        Matrix matrix=new Matrix(child.getMatrix());matrix.postTranslate(child.getLeft()-getScrollX(),child.getTop()-getScrollY());
        if(NativeEffects.drawRaster(canvas,child,matrix))return true;
        if(!canvas.isHardwareAccelerated()&&child instanceof android.view.TextureView){
            int save=canvas.save();canvas.concat(matrix);canvas.saveLayerAlpha(null,Math.round(child.getAlpha()*255));
            NativeCapture.drawTexture(canvas,child);canvas.restoreToCount(save);return true;
        }
        return super.drawChild(canvas,child,time);
    }
}
