package dev.jadix.nativeui;

import android.view.*;

/** Dialog and popup content use the same input contract as the main native root. */
class NativeEventFrame extends NativeFrame {
    private final NativeInteraction interactions;
    private final NativeKeyboard keyboard;
    NativeEventFrame(NativeView root) {
        super(root.getContext());owner=root;
        interactions=new NativeInteraction(root,this);keyboard=new NativeKeyboard(this);
    }
    @Override public boolean dispatchTouchEvent(MotionEvent event) {
        MotionEvent filtered=keyboard.filter(event,interactions.longPressed());
        try{return interactions.touch(filtered)||super.dispatchTouchEvent(filtered);}
        finally{if(filtered!=event)filtered.recycle();}
    }
    @Override public boolean dispatchKeyEvent(KeyEvent event){return interactions.key(event)||super.dispatchKeyEvent(event);}
    @Override public void requestDisallowInterceptTouchEvent(boolean value) {
        if (value && interactions != null) interactions.childClaimed();
        super.requestDisallowInterceptTouchEvent(value);
    }
    @Override public boolean dispatchHoverEvent(MotionEvent event){interactions.hover(event);return super.dispatchHoverEvent(event);}
    @Override public boolean dispatchGenericMotionEvent(MotionEvent event){return interactions.generic(event)||super.dispatchGenericMotionEvent(event);}
    void cancelInput(){interactions.close();keyboard.clear();cancelNativeTouch();}
    @Override protected void onDetachedFromWindow(){cancelInput();super.onDetachedFromWindow();}
}
