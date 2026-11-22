package dev.jadix.nativeui;

import android.animation.ValueAnimator;
import android.graphics.*;
import android.os.SystemClock;
import android.view.*;
import android.widget.FrameLayout;
import java.util.*;
import org.json.*;

/** Native capture and presentation ownership for the portable compositing contract. */
final class NativeShaderComposition implements NativeShader.Inputs, ViewTreeObserver.OnPreDrawListener, AutoCloseable {
    private final NativeHosted owner;
    private final FrameLayout content;
    private final NativeShader shader;
    private JSONObject spec=new JSONObject();
    private String type="background",value="";
    private Bitmap latest,from,backdrop;
    private float padding,fromPadding,progress=1;
    private long sequence,capturedAt=-1,started;
    private int duration;
    private boolean initialized,running,ready,failed,capturing,closed;
    private ViewTreeObserver observer;
    private NativeCapture.Watch textures;
    private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG|Paint.FILTER_BITMAP_FLAG);
    NativeShaderComposition(NativeHosted owner,FrameLayout content,NativeShader shader) {
        this.owner=owner;this.content=content;this.shader=shader;shader.inputs(this);
        owner.addOnAttachStateChangeListener(new View.OnAttachStateChangeListener(){
            public void onViewAttachedToWindow(View view){attach();}
            public void onViewDetachedFromWindow(View view){if(observer!=null&&observer.isAlive())observer.removeOnPreDrawListener(NativeShaderComposition.this);observer=null;}
        });
    }
    void configure(JSONObject next) {
        JSONObject composition=next.optJSONObject("composition");String nextType=composition==null?"background":composition.optString("type","background");
        String nextValue=composition==null?"":composition.optString("value");
        boolean changed=initialized&&type.equals("transition")&&nextType.equals(type)&&!value.equals(nextValue);
        // Finish an interrupted transition at its destination before taking the next source.
        if(changed){from=latest;fromPadding=padding;started=SystemClock.uptimeMillis();duration=composition.optInt("duration_ms",300);
            running=from!=null&&duration>0&&ValueAnimator.areAnimatorsEnabled();}
        else if(!nextType.equals(type)) {running=false;from=null;}
        spec=next;type=nextType;value=nextValue;initialized=true;ready=false;failed=false;
        if(!type.equals("background")&&textures==null)textures=new NativeCapture.Watch(owner);
        else if(type.equals("background")&&textures!=null){textures.close();textures=null;}
        padding=(float)next.optDouble("max_sample_offset",0)*owner.getResources().getDisplayMetrics().density;
        capturedAt=-1;progress=next.isNull("progress")?(running?0:1):(float)next.optDouble("progress",1);
        if(type.equals("transition")&&!next.isNull("progress"))running=progress<1;
        shader.setAlpha(type.equals("background")?1:0);
        if(type.equals("background"))owner.moveShader(shader,false);else owner.moveShader(shader,true);
        attach();content.invalidate();owner.invalidate();
    }
    private void attach() {
        ViewTreeObserver current=owner.getViewTreeObserver();if(observer==current)return;
        if(observer!=null&&observer.isAlive())observer.removeOnPreDrawListener(this);
        observer=current;if(!closed&&current.isAlive())current.addOnPreDrawListener(this);
    }
    boolean suppress() {return !capturing&&active()&&ready;}
    @Override public boolean active() {return !closed&&(type.equals("effect")||type.equals("transition")&&running);}
    private void advance() {
        if(!spec.isNull("progress")){progress=(float)spec.optDouble("progress",1);return;}
        if(running){progress=Math.min(1,(SystemClock.uptimeMillis()-started)/(float)Math.max(1,duration));
            if(progress>=1||!ValueAnimator.areAnimatorsEnabled()){running=false;ready=false;from=null;shader.setAlpha(0);content.invalidate();shader.refreshInputs();}}
    }
    @Override public Map<String,Bitmap> capture() {
        advance();
        if(capturedAt!=sequence) {
            capturedAt=sequence;capturing=true;
            try { NativeCapture.prepare(owner);latest=NativeCapture.content(content,padding);backdrop=active()?NativeCapture.backdrop(owner,padding):null; }
            finally {capturing=false;}
        }
        HashMap<String,Bitmap> result=new HashMap<>();result.put("content",latest);result.put("to",latest);
        result.put("from",from==null?latest:from);result.put("backdrop",backdrop);return result;
    }
    @Override public float progress() {return progress;}
    @Override public void presented(JSONObject current) {
        if(current!=spec||!active())return;boolean changed=!ready;ready=true;failed=false;shader.setAlpha(1);if(changed)content.invalidate();
    }
    @Override public void failed(JSONObject current) {
        if(current!=spec)return;ready=false;failed=true;shader.setAlpha(0);content.invalidate();owner.invalidate();
    }
    @Override public boolean onPreDraw() {
        if(closed)return true;
        sequence++;advance();
        if(owner.isShown()&&owner.getWidth()>0&&owner.getHeight()>0) {
            if(active()&&(content.isDirty()||running))shader.requestFrame();
            else if(type.equals("transition")&&content.isDirty())capture();
        }
        return true;
    }
    void drawFallback(Canvas canvas) {
        JSONObject mode=spec.optJSONObject("composition");
        if(!failed||!running||from==null||mode==null||mode.optString("fallback","fade").equals("none"))return;
        paint.setAlpha(Math.round((1-progress)*255));
        RectF destination=new RectF(-fromPadding,-fromPadding,owner.getWidth()+fromPadding,owner.getHeight()+fromPadding);
        canvas.drawBitmap(from,null,destination,paint);owner.postInvalidateOnAnimation();
    }
    @Override public void close() {
        closed=true;if(observer!=null&&observer.isAlive())observer.removeOnPreDrawListener(this);observer=null;
        if(textures!=null){textures.close();textures=null;}
        latest=from=backdrop=null;ready=false;running=false;shader.inputs(null);content.invalidate();
    }
}
