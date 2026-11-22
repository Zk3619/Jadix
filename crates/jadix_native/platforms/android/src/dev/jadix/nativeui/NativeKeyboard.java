package dev.jadix.nativeui;

import android.content.Context;
import android.graphics.Rect;
import android.os.Build;
import android.view.*;
import android.view.inputmethod.InputMethodManager;
import android.widget.*;

/** Decide keyboard-only taps before either native widgets or shared responders see UP. */
final class NativeKeyboard {
    private final NativeFrame surface;
    private NativeScroll scroll;
    private View target;
    private float x,y;
    private long down;
    private boolean moved;
    NativeKeyboard(NativeFrame surface){this.surface=surface;}
    void clear(){scroll=null;target=null;}
    MotionEvent filter(MotionEvent event,boolean longPressed) {
        int action=event.getActionMasked();
        if(action==MotionEvent.ACTION_DOWN){
            clear();moved=false;x=event.getX();y=event.getY();down=event.getEventTime();
            target=NativeInteraction.deepest(surface,x,y);
            View focus=surface.getRootView().findFocus();
            if(focus==null||!focus.onCheckIsTextEditor()||!keyboardVisible(focus))return event;
            for(View item=target;item!=null;item=item.getParent() instanceof View?(View)item.getParent():null){
                if(item.onCheckIsTextEditor()){clear();return event;}
                if(item instanceof NativeScroll&&!"always".equals(((NativeScroll)item).keyboardTaps))scroll=(NativeScroll)item;
                if(item==surface)break;
            }
        } else if(action==MotionEvent.ACTION_MOVE||action==MotionEvent.ACTION_POINTER_DOWN){
            moved|=event.getPointerCount()>1||Math.hypot(event.getX()-x,event.getY()-y)>ViewConfiguration.get(surface.getContext()).getScaledTouchSlop();
        } else if(action==MotionEvent.ACTION_UP){
            NativeScroll selected=scroll;
            boolean tap=!moved&&!longPressed;
            for(View item=target;item!=null&&item!=surface;item=item.getParent() instanceof View?(View)item.getParent():null)
                if(item.isLongClickable()&&event.getEventTime()-down>=ViewConfiguration.getLongPressTimeout())tap=false;
            boolean handled=false;
            for(View item=target;item!=null&&item!=surface;item=item.getParent() instanceof View?(View)item.getParent():null)
                handled|=item.isEnabled()&&(item.isClickable()||item.isLongClickable()||item.onCheckIsTextEditor()||item instanceof SeekBar);
            // Shared gestures are handled taps too, including handlers on plain boxes.
            for(View item=target;item!=null&&item!=surface;item=item.getParent() instanceof View?(View)item.getParent():null){
                org.json.JSONObject policy=NativeInteraction.interaction(item);
                handled|=policy.optBoolean("press")||policy.optInt("longPressMillis")>0;
            }
            clear();
            if(selected!=null&&tap&&("never".equals(selected.keyboardTaps)||!handled)){
                hide(selected);
                if("never".equals(selected.keyboardTaps)){
                    MotionEvent cancel=MotionEvent.obtain(event);cancel.setAction(MotionEvent.ACTION_CANCEL);return cancel;
                }
            }
        } else if(action==MotionEvent.ACTION_CANCEL)clear();
        return event;
    }
    private static boolean keyboardVisible(View focus){
        if(Build.VERSION.SDK_INT>=30){WindowInsets insets=focus.getRootWindowInsets();return insets!=null&&insets.isVisible(WindowInsets.Type.ime());}
        Rect visible=new Rect();focus.getWindowVisibleDisplayFrame(visible);
        return focus.getRootView().getHeight()-visible.height()>100*focus.getResources().getDisplayMetrics().density;
    }
    static void hide(View view){
        View focus=view.getRootView().findFocus();if(focus==null)return;
        InputMethodManager manager=(InputMethodManager)view.getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        if(manager!=null)manager.hideSoftInputFromWindow(focus.getWindowToken(),0);
        focus.clearFocus();
    }
}
