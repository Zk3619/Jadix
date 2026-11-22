package dev.jadix.nativeui;

import android.content.Context;
import android.util.TypedValue;
import android.view.MotionEvent;
import android.view.ViewGroup;

/** Retains styled button content while the button owns focus and activation. */
final class NativeContentButton extends NativeFrame {
    NativeContentButton(Context context) {
        super(context);
        setClickable(true);
        setFocusable(true);
        setDescendantFocusability(ViewGroup.FOCUS_BLOCK_DESCENDANTS);
        TypedValue background = new TypedValue();
        if (context.getTheme().resolveAttribute(android.R.attr.selectableItemBackground, background, true)
                && background.resourceId != 0) setBackgroundResource(background.resourceId);
    }

    @Override public CharSequence getAccessibilityClassName() { return "android.widget.Button"; }
    @Override public boolean onInterceptTouchEvent(MotionEvent event) { return true; }
    @Override public boolean onTouchEvent(MotionEvent event) {
        return isEnabled() && super.onTouchEvent(event);
    }
}
