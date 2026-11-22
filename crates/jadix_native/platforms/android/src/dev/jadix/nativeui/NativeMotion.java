package dev.jadix.nativeui;

import android.animation.*;
import android.view.View;
import org.json.*;

/** Android schedules frames; the shared animation kernel resolves timing and channels. */
final class NativeMotion implements View.OnAttachStateChangeListener {
    private static final java.util.WeakHashMap<View,NativeMotion> states=new java.util.WeakHashMap<>();
    private final java.lang.ref.WeakReference<View> owner;
    private JSONObject visible,target,style;
    private String keyframes;
    private ValueAnimator transition,animation;
    private float density;
    private float[] keyframe;
    private boolean active;
    private NativeMotion(View view) { owner=new java.lang.ref.WeakReference<>(view); view.addOnAttachStateChangeListener(this); }
    static void apply(View view,JSONObject style,float density) throws JSONException {
        NativeMotion state=states.computeIfAbsent(view,NativeMotion::new); state.update(view,style,density);
    }
    static void dispose(View view) {
        NativeMotion state=states.remove(view);
        if(state!=null) { state.cancel();view.removeOnAttachStateChangeListener(state); }
    }
    static void refreshSubtree(View root) {
        for(java.util.Map.Entry<View,NativeMotion> entry:new java.util.ArrayList<>(states.entrySet())) {
            View view=entry.getKey(); if(view==null)continue;
            for(View ancestor=view;ancestor!=null;ancestor=ancestor.getParent() instanceof View?(View)ancestor.getParent():null) {
                if(ancestor!=root)continue;
                NativeMotion state=entry.getValue();
                if(state.style!=null&&state.active!=state.enabled(view))try{state.update(view,state.style,state.density);}catch(JSONException error){throw new IllegalStateException(error);}
                break;
            }
        }
    }
    private boolean enabled(View view) {
        if (!ValueAnimator.areAnimatorsEnabled() || !view.isAttachedToWindow()
                || view.getWindowVisibility()!=View.VISIBLE || target!=null&&target.optBoolean("hidden")) return false;
        for(android.view.ViewParent parent=view.getParent();parent instanceof View;parent=parent.getParent())
            if(((View)parent).getVisibility()!=View.VISIBLE) return false;
        return true;
    }
    private void cancel() { if(transition!=null){transition.cancel();transition=null;}if(animation!=null){animation.cancel();animation=null;} }
    private void update(View view,JSONObject next,float scale) throws JSONException {
        if(transition!=null) { transition.cancel();transition=null; }
        style=next; density=scale; target=next.optJSONObject("view");
        JSONObject from=visible;
        visible=target==null?null:new JSONObject(target.toString());
        boolean enabled=enabled(view);active=enabled;
        JSONObject plan=enabled&&from!=null&&target!=null?(JSONObject)nullable(NativeRuntime.call(NativeRuntime.object("appearanceMotion").put("from",from).put("to",target))):null;
        if(plan!=null&&plan.getJSONArray("channels").length()>0&&plan.optDouble("duration")>0) {
            final JSONArray channels=plan.getJSONArray("channels"),curve=plan.getJSONArray("curve");
            transition=ValueAnimator.ofFloat(0,1);transition.setInterpolator(new android.view.animation.LinearInterpolator());
            transition.setDuration(Math.max(1,Math.round(plan.getDouble("duration"))));transition.setStartDelay(Math.round(plan.optDouble("delay")));
            transition.addUpdateListener(value->{
                View current=owner.get();if(current==null)return;
                try {
                    double t=sample(curve,(float)value.getAnimatedValue()); visible=new JSONObject(target.toString());
                    for(int i=0;i<channels.length();i++) {
                        JSONObject channel=channels.getJSONObject(i);JSONArray path=channel.getJSONArray("path");Object container=visible;
                        for(int j=0;j<path.length()-1;j++)container=container instanceof JSONObject?((JSONObject)container).get(path.getString(j)):((JSONArray)container).get(Integer.parseInt(path.getString(j)));
                        double v=channel.getDouble("from")+(channel.getDouble("to")-channel.getDouble("from"))*t;
                        v=Math.max(channel.getDouble("minimum"),Math.min(channel.getDouble("maximum"),v));
                        Object number=channel.optBoolean("integral")?Long.valueOf(Math.round(v)):(Object)Double.valueOf(v);
                        String leaf=path.getString(path.length()-1);
                        if(container instanceof JSONObject)((JSONObject)container).put(leaf,number);else ((JSONArray)container).put(Integer.parseInt(leaf),number);
                    }
                    paint(current);
                } catch(JSONException error) { throw new IllegalStateException(error); }
            });
            // Render the visible starting state throughout a transition delay.
            visible=new JSONObject(target.toString());
            for(int i=0;i<channels.length();i++) {
                JSONObject channel=channels.getJSONObject(i);JSONArray path=channel.getJSONArray("path");Object container=visible;
                for(int j=0;j<path.length()-1;j++)container=container instanceof JSONObject?((JSONObject)container).get(path.getString(j)):((JSONArray)container).get(Integer.parseInt(path.getString(j)));
                Object number=channel.get("from");if(channel.optBoolean("integral"))number=Long.valueOf(Math.round(channel.getDouble("from")));
                String leaf=path.getString(path.length()-1);if(container instanceof JSONObject)((JSONObject)container).put(leaf,number);else((JSONArray)container).put(Integer.parseInt(leaf),number);
            }
            paint(view);transition.start();
        } else paint(view);
        JSONObject spec=target==null?null:target.optJSONObject("animation");String signature=enabled&&spec!=null?spec.toString():null;
        if(!java.util.Objects.equals(signature,keyframes)) {
            if(animation!=null){animation.cancel();animation=null;} keyframes=signature;
            if(signature!=null&&!spec.optString("iterations").equals("0")) {
                JSONArray curve=(JSONArray)NativeRuntime.call(NativeRuntime.object("keyframeMotion").put("animation",spec));
                animation=ValueAnimator.ofFloat(0,1);animation.setInterpolator(new android.view.animation.LinearInterpolator());
                animation.setDuration(Math.max(1,Math.round(spec.getDouble("duration"))));animation.setStartDelay(Math.round(spec.optDouble("delay")));
                animation.setRepeatCount(spec.isNull("iterations")?ValueAnimator.INFINITE:(int)Math.min(Integer.MAX_VALUE,Math.max(0,spec.optLong("iterations",1)-1)));
                final double fps=spec.optDouble("frame_rate",240);final long[] last={Long.MIN_VALUE};
                animation.addUpdateListener(value->{View current=owner.get();if(current==null)return;long time=android.os.SystemClock.uptimeMillis();if(last[0]!=Long.MIN_VALUE&&time-last[0]<1000/fps&&(float)value.getAnimatedValue()!=1)return;last[0]=time;
                    try {float phase=(float)value.getAnimatedValue();float index=phase*(curve.length()-1);int left=Math.min(curve.length()-2,(int)index);float fraction=index-left;JSONArray a=curve.getJSONArray(left),b=curve.getJSONArray(left+1);keyframe=new float[4];for(int i=0;i<4;i++)keyframe[i]=(float)(a.getDouble(i)+(b.getDouble(i)-a.getDouble(i))*fraction);paint(current);
                    }catch(JSONException error){throw new IllegalStateException(error);}
                });animation.addListener(new AnimatorListenerAdapter(){@Override public void onAnimationEnd(Animator animator){keyframe=null;View current=owner.get();if(current!=null)try{paint(current);}catch(JSONException error){throw new IllegalStateException(error);}}});animation.start();
            }
        }
    }
    private static Object nullable(Object value) { return value==JSONObject.NULL?null:value; }
    private static double sample(JSONArray curve,float progress) { float index=progress*(curve.length()-1);int left=Math.min(curve.length()-2,(int)index);return curve.optDouble(left)+(curve.optDouble(left+1)-curve.optDouble(left))*(index-left); }
    private void paint(View view) throws JSONException {
        JSONObject frame=new JSONObject(style.toString());frame.put("view",visible==null?JSONObject.NULL:visible);NativeAppearance.paint(view,frame,density);
        if(keyframe!=null) { NativeAppearance.transform(view,visible,density,keyframe);view.setAlpha(Math.max(0,Math.min(1,view.getAlpha()*keyframe[2]))); }
    }
    public void onViewAttachedToWindow(View view) { if(style!=null)try{update(view,style,density);}catch(JSONException error){throw new IllegalStateException(error);} }
    public void onViewDetachedFromWindow(View view) { cancel();keyframes=null;active=false; }
}
