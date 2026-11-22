package dev.jadix.nativeui;

import android.graphics.Matrix;
import android.graphics.Region;
import android.os.SystemClock;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.util.*;

/** Passive raw touch observation and opt-in gesture ownership; editors keep their IME pipeline. */
final class NativeInteraction {
    private final NativeView root;
    private final NativeFrame surface;
    private final ArrayList<String> path=new ArrayList<>();
    private String press,pan,pinch,rotation;
    private final ArrayList<String> hovered = new ArrayList<>();
    private MotionEvent lastHover;
    private boolean transforming;
    private boolean panning,panFailed,claimed,longPressed;
    private boolean secondary;
    private float downX,downY,lastX,lastY;
    private double distance,angle,scale=1,turn,priorScale=1,priorTurn;
    private long lastTime,generation;
    private MotionEvent latest;
    NativeInteraction(NativeView root){this(root,root);}
    NativeInteraction(NativeView root,NativeFrame surface){this.root=root;this.surface=surface;}
    boolean longPressed(){return longPressed;}
    static JSONObject modifiers(int meta) throws JSONException {return new JSONObject().put("shift",(meta&KeyEvent.META_SHIFT_ON)!=0).put("control",(meta&KeyEvent.META_CTRL_ON)!=0).put("alt",(meta&KeyEvent.META_ALT_ON)!=0).put("meta",(meta&KeyEvent.META_META_ON)!=0).put("capsLock",(meta&KeyEvent.META_CAPS_LOCK_ON)!=0);}
    private JSONObject policy(String id){JSONObject node=root.descriptor(id);return node==null?new JSONObject():node.optJSONObject("interaction")==null?new JSONObject():node.optJSONObject("interaction");}
    private void send(String id,String kind,String phase,MotionEvent event,JSONObject fields){if(id==null || ("press".equals(kind) && !policy(id).optBoolean("press")))return;try{JSONObject value=new JSONObject().put("kind",kind).put("phase",phase).put("position",new JSONArray().put(event.getX()/root.density()).put(event.getY()/root.density())).put("timestamp",event.getEventTime()/1000.0).put("modifiers",modifiers(event.getMetaState()));
        float[] local=localPosition(id,event);
        if(local!=null)value.put("localPosition",new JSONArray().put(local[0]/root.density()).put(local[1]/root.density()));
        if(fields!=null)for(Iterator<String> keys=fields.keys();keys.hasNext();){String key=keys.next();value.put(key,fields.get(key));}root.emit(id,NativeView.payload("interaction").put("event",value));}catch(JSONException error){throw new IllegalStateException(error);}}
    private float[] localPosition(String id,MotionEvent event) {
        ArrayList<View> chain=new ArrayList<>(); View view=root.nodeView(id);
        while(view!=surface) {
            if(view==null||!(view.getParent() instanceof ViewGroup))return null;
            chain.add(view);view=(View)view.getParent();
        }
        float[] point={event.getX(),event.getY()};
        for(int i=chain.size()-1;i>=0;i--) {
            View child=chain.get(i);Matrix inverse=childMatrix((ViewGroup)child.getParent(),child);
            if(inverse==null)return null;inverse.mapPoints(point);
        }
        return point;
    }
    private JSONObject panFields(MotionEvent event) throws JSONException {double dt=Math.max(.001,(event.getEventTime()-lastTime)/1000.0);return new JSONObject().put("translation",new JSONArray().put((event.getX()-downX)/root.density()).put((event.getY()-downY)/root.density())).put("velocity",new JSONArray().put(Math.max(-1e6,Math.min(1e6,(event.getX()-lastX)/root.density()/dt))).put(Math.max(-1e6,Math.min(1e6,(event.getY()-lastY)/root.density()/dt))));}
    private ArrayList<String> hit(MotionEvent event){
        ArrayList<String> result=new ArrayList<>();
        View target=deepest(surface,event.getX(),event.getY());
        for(View item=target;item!=null;item=item.getParent() instanceof View?(View)item.getParent():null){
            if(item.getTag() instanceof String&&root.descriptor((String)item.getTag())!=null
                    && !"box-none".equals(pointerEvents(item)))result.add((String)item.getTag());
            if(item==surface)break;
        }
        return result;
    }
    static JSONObject interaction(View view) {
        if (!(view.getTag() instanceof String)) return new JSONObject();
        for(View item=view;item!=null;item=item.getParent() instanceof View?(View)item.getParent():null) {
            NativeView owner=item instanceof NativeView?(NativeView)item:item instanceof NativeFrame?((NativeFrame)item).owner:null;
            if(owner!=null) {
                JSONObject node=owner.descriptor((String)view.getTag());
                JSONObject value=node==null?null:node.optJSONObject("interaction");
                return value==null?new JSONObject():value;
            }
        }
        return new JSONObject();
    }
    static String pointerEvents(View view) { return interaction(view).optString("pointerEvents","auto"); }
    static boolean inside(View view,float x,float y,boolean slop) {
        JSONArray edges=slop?interaction(view).optJSONArray("hitSlop"):null;
        float density=view.getResources().getDisplayMetrics().density;
        return x>=-(edges==null?0:edges.optDouble(3)*density) && y>=-(edges==null?0:edges.optDouble(0)*density)
            && x<view.getWidth()+(edges==null?0:edges.optDouble(1)*density)
            && y<view.getHeight()+(edges==null?0:edges.optDouble(2)*density);
    }
    static Matrix childMatrix(ViewGroup parent,View child) {
        Matrix result=new Matrix(child.getMatrix());
        result.postTranslate(child.getLeft()-parent.getScrollX(),child.getTop()-parent.getScrollY());
        Matrix inverse=new Matrix();return result.invert(inverse)?inverse:null;
    }
    static ArrayList<View> frontToBack(ViewGroup group) {
        ArrayList<View> children=new ArrayList<>();
        for(int i=0;i<group.getChildCount();i++)children.add(group.getChildAt(i));
        // Stable ordering preserves insertion order when elevations are equal.
        children.sort((a,b)->Float.compare(a.getZ(),b.getZ()));
        Collections.reverse(children);return children;
    }
    static View deepest(View view,float x,float y) {
        if(view.getVisibility()!=View.VISIBLE||"none".equals(pointerEvents(view)))return null;
        boolean bounds=inside(view,x,y,false);
        NativeSurface surface=NativeSurface.from(view.getBackground());
        if(surface!=null&&surface.spec.optBoolean("clip")) {
            surface.setBounds(0,0,view.getWidth(),view.getHeight());
            Region shape=new Region();shape.setPath(surface.shape,new Region(0,0,view.getWidth(),view.getHeight()));
            if(!shape.contains((int)Math.floor(x),(int)Math.floor(y)))return null;
        }
        if(view instanceof ViewGroup) {
            ViewGroup group=(ViewGroup)view;
            if(bounds||!group.getClipChildren()) for(View child:frontToBack(group)) {
                Matrix inverse=childMatrix(group,child);if(inverse==null)continue;
                float[] local={x,y};inverse.mapPoints(local);
                View found=deepest(child,local[0],local[1]);if(found!=null)return found;
            }
        }
        return !"box-none".equals(pointerEvents(view))&&inside(view,x,y,true)?view:null;
    }
    private String owner(String family){for(String id:path){JSONObject node=root.descriptor(id);if(node==null)continue;JSONObject p=policy(id);if(("press".equals(family)&&(p.optBoolean("press")||!p.isNull("longPressMillis")))||("pan".equals(family)&&!p.isNull("pan"))||p.optBoolean(family))return id;String kind=node.optString("kind");if(Arrays.asList("input","textarea","hosted","button","checkbox","switch","select").contains(kind))break;}return null;}
    private void claim(){if(!claimed){claimed=true;surface.cancelNativeTouch();surface.requestDisallowInterceptTouchEvent(true);}}
    void childClaimed() {
        if (claimed || latest == null) return;
        cancelPress(latest);
        try {
            if (panning) send(pan,"pan","cancelled",latest,panFields(latest));
            if (transforming) {
                send(pinch,"pinch","cancelled",latest,new JSONObject().put("value",scale).put("rate",0));
                send(rotation,"rotation","cancelled",latest,new JSONObject().put("value",turn).put("rate",0));
            }
        } catch (JSONException error) { throw new IllegalStateException(error); }
        pan=pinch=rotation=null; panFailed=true; panning=transforming=false;
    }
    boolean touch(MotionEvent event){
        if(event.getActionMasked()==MotionEvent.ACTION_DOWN && (event.getButtonState()&MotionEvent.BUTTON_SECONDARY)!=0){secondary=contextMenu(event);if(secondary)return true;}
        if(secondary){if(event.getActionMasked()==MotionEvent.ACTION_UP||event.getActionMasked()==MotionEvent.ACTION_CANCEL)secondary=false;return true;}
        int action=event.getActionMasked();if(action==MotionEvent.ACTION_DOWN){cancel();path.addAll(hit(event));press=owner("press");pan=owner("pan");pinch=owner("pinch");rotation=owner("rotation");downX=lastX=event.getX();downY=lastY=event.getY();lastTime=event.getEventTime();panFailed=false;panning=false;claimed=false;longPressed=false;send(press,"press","started",event,null);long token=++generation;int timeout=policy(press).optInt("longPressMillis",0);if(timeout>0)root.postDelayed(()->{if(generation!=token||latest==null||press==null||Math.hypot(latest.getX()-downX,latest.getY()-downY)>ViewConfiguration.get(root.getContext()).getScaledTouchSlop())return;longPressed=true;claim();if(policy(press).optBoolean("press"))send(press,"press","cancelled",latest,null);send(press,"longPress","started",latest,null);},timeout);}
        if(latest!=null)latest.recycle();latest=MotionEvent.obtain(event);
        try{
            raw(event);
            if(action==MotionEvent.ACTION_POINTER_DOWN&&event.getPointerCount()==2){cancelPress(event);generation++;if(panning){send(pan,"pan","cancelled",event,panFields(event));panning=false;}panFailed=true;transforming=pinch!=null||rotation!=null;distance=span(event);angle=angle(event);scale=priorScale=1;turn=priorTurn=0;if(pinch!=null||rotation!=null)claim();send(pinch,"pinch","started",event,new JSONObject().put("value",1).put("rate",0));send(rotation,"rotation","started",event,new JSONObject().put("value",0).put("rate",0));}
            if(action==MotionEvent.ACTION_MOVE){
                if(event.getPointerCount()==1&&pan!=null&&!panFailed){String decision=(String)NativeRuntime.call(NativeRuntime.object("pan").put("axis",policy(pan).optString("pan","both")).put("activation",policy(pan).optJSONObject("panActivation")==null?new JSONObject():policy(pan).optJSONObject("panActivation")).put("delta",new JSONArray().put((event.getX()-downX)/root.density()).put((event.getY()-downY)/root.density())));if(!panning&&"fail".equals(decision))panFailed=true;if(!panning&&"activate".equals(decision)){panning=true;claim();cancelPress(event);send(pan,"pan","started",event,panFields(event));}else if(panning)send(pan,"pan","changed",event,panFields(event));}
                if(transforming&&event.getPointerCount()>=2){double dt=Math.max(.001,(event.getEventTime()-lastTime)/1000.0);scale=Math.max(.000001,Math.min(1e6,span(event)/Math.max(.001,distance)));double delta=angle(event)-angle;while(delta>Math.PI)delta-=2*Math.PI;while(delta< -Math.PI)delta+=2*Math.PI;turn=Math.max(-1e6,Math.min(1e6,turn+delta));angle=angle(event);send(pinch,"pinch","changed",event,new JSONObject().put("value",scale).put("rate",bounded((scale-priorScale)/dt)));send(rotation,"rotation","changed",event,new JSONObject().put("value",turn).put("rate",bounded((turn-priorTurn)/dt)));priorScale=scale;priorTurn=turn;}
                if(press!=null && (Math.hypot(event.getX()-downX,event.getY()-downY)>ViewConfiguration.get(root.getContext()).getScaledTouchSlop() || !insidePress(event))){cancelPress(event);}
            }
            if(transforming&&action==MotionEvent.ACTION_POINTER_UP&&event.getPointerCount()==2){send(pinch,"pinch","ended",event,new JSONObject().put("value",scale).put("rate",0));send(rotation,"rotation","ended",event,new JSONObject().put("value",turn).put("rate",0));pinch=rotation=null;transforming=false;}
            boolean consumed=claimed;
            if(action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_CANCEL){String phase=action==MotionEvent.ACTION_CANCEL?"cancelled":"ended";if(panning)send(pan,"pan",phase,event,panFields(event));if(transforming){send(pinch,"pinch",phase,event,new JSONObject().put("value",scale).put("rate",0));send(rotation,"rotation",phase,event,new JSONObject().put("value",turn).put("rate",0));transforming=false;}if(press!=null)send(press,longPressed?"longPress":"press",insidePress(event)?phase:"cancelled",event,null);generation++;path.clear();press=pan=pinch=rotation=null;panning=claimed=longPressed=false;surface.requestDisallowInterceptTouchEvent(false);}
            lastX=event.getX();lastY=event.getY();lastTime=event.getEventTime();return consumed;
        }catch(JSONException error){throw new IllegalStateException(error);}
    }
    private boolean insidePress(MotionEvent event) {
        View view=press==null?null:root.nodeView(press);
        float[] local=press==null?null:localPosition(press,event);
        return view!=null && local!=null && inside(view,local[0],local[1],true);
    }
    private void cancelPress(MotionEvent event) { if(press!=null)send(press,longPressed?"longPress":"press","cancelled",event,null);press=null;generation++;longPressed=false; }
    private double span(MotionEvent event){return Math.hypot(event.getX(1)-event.getX(0),event.getY(1)-event.getY(0));}
    private double angle(MotionEvent event){return Math.atan2(event.getY(1)-event.getY(0),event.getX(1)-event.getX(0));}
    static JSONArray tilt(MotionEvent event, int pointer) throws JSONException {
        int tool = event.getToolType(pointer);
        double x = 0, y = 0;
        if (tool == MotionEvent.TOOL_TYPE_STYLUS || tool == MotionEvent.TOOL_TYPE_ERASER) {
            double inclination = event.getAxisValue(MotionEvent.AXIS_TILT, pointer);
            double orientation = event.getOrientation(pointer);
            if (Double.isFinite(inclination) && Double.isFinite(orientation)) {
                double tangent = Math.tan(Math.max(0, Math.min(Math.PI / 2, inclination)));
                x = Math.toDegrees(Math.atan(Math.sin(orientation) * tangent));
                y = Math.toDegrees(Math.atan(-Math.cos(orientation) * tangent));
            }
        }
        return new JSONArray().put(x).put(y);
    }
    private void raw(MotionEvent event)throws JSONException{int action=event.getActionMasked(),changed=event.getActionIndex();boolean ending=action==MotionEvent.ACTION_UP||action==MotionEvent.ACTION_POINTER_UP||action==MotionEvent.ACTION_CANCEL;JSONArray touches=new JSONArray(),active=new JSONArray();for(int i=0;i<Math.min(10,event.getPointerCount());i++){JSONObject contact=new JSONObject().put("id",event.getPointerId(i)+1).put("position",new JSONArray().put(event.getX(i)/root.density()).put(event.getY(i)/root.density())).put("force",Math.max(0,Math.min(1,event.getPressure(i)))).put("tilt",tilt(event,i)).put("kind",(event.getToolType(i)==MotionEvent.TOOL_TYPE_STYLUS||event.getToolType(i)==MotionEvent.TOOL_TYPE_ERASER)?"pencil":event.getToolType(i)==MotionEvent.TOOL_TYPE_MOUSE?"indirect":"direct");if(action==MotionEvent.ACTION_MOVE||action==MotionEvent.ACTION_CANCEL||i==changed)touches.put(contact);if(action!=MotionEvent.ACTION_CANCEL&&!(ending&&i==changed))active.put(contact);}String phase=action==MotionEvent.ACTION_DOWN||action==MotionEvent.ACTION_POINTER_DOWN?"started":action==MotionEvent.ACTION_CANCEL?"cancelled":ending?"ended":"changed";for(String id:new ArrayList<>(path))if(policy(id).optBoolean("touch"))send(id,"touch",phase,event,new JSONObject().put("touches",touches).put("activeTouches",active));}
    void cancel(){generation++;if(latest!=null){try{if(panning)send(pan,"pan","cancelled",latest,panFields(latest));if(press!=null)send(press,longPressed?"longPress":"press","cancelled",latest,null);if(pinch!=null&&transforming)send(pinch,"pinch","cancelled",latest,new JSONObject().put("value",scale).put("rate",0));if(rotation!=null&&transforming)send(rotation,"rotation","cancelled",latest,new JSONObject().put("value",turn).put("rate",0));MotionEvent cancelled=MotionEvent.obtain(latest);cancelled.setAction(MotionEvent.ACTION_CANCEL);try{raw(cancelled);}finally{cancelled.recycle();}}catch(JSONException error){throw new IllegalStateException(error);}latest.recycle();latest=null;}path.clear();press=pan=pinch=rotation=null;panning=claimed=transforming=longPressed=false;surface.requestDisallowInterceptTouchEvent(false);}
    private static double bounded(double value) { return Math.max(-1e6,Math.min(1e6,value)); }
    private boolean contextMenu(MotionEvent event) {
        for(String id:hit(event)) if(policy(id).optBoolean("contextMenu")) {
            cancel(); send(id,"contextMenu","started",event,null); return true;
        }
        return false;
    }
    boolean generic(MotionEvent event) {
        if(event.getActionMasked()==MotionEvent.ACTION_BUTTON_RELEASE && event.getActionButton()==MotionEvent.BUTTON_SECONDARY){boolean consumed=secondary;secondary=false;return consumed;}
        if(event.getActionMasked()!=MotionEvent.ACTION_BUTTON_PRESS || event.getActionButton()!=MotionEvent.BUTTON_SECONDARY)return false;
        if(secondary)return true;
        secondary=contextMenu(event);return secondary;
    }
    void hover(MotionEvent event) {
        ArrayList<String> next=new ArrayList<>();
        if(event.getActionMasked()!=MotionEvent.ACTION_HOVER_EXIT) for(String id:hit(event)) if(policy(id).optBoolean("hover")) next.add(id);
        for(String id:new ArrayList<>(hovered)) if(!next.contains(id)) send(id,"hover","ended",event,null);
        for(String id:next) send(id,"hover",hovered.contains(id)?"changed":"started",event,null);
        hovered.clear();hovered.addAll(next);
        if(lastHover!=null)lastHover.recycle();lastHover=MotionEvent.obtain(event);
    }
    void removing(String id) {
        if(path.contains(id))cancel();
        if(hovered.remove(id)&&lastHover!=null)send(id,"hover","ended",lastHover,null);
    }
    void close() {
        cancel();
        if(lastHover!=null){for(String id:new ArrayList<>(hovered))send(id,"hover","ended",lastHover,null);lastHover.recycle();lastHover=null;}
        hovered.clear();
    }
    boolean key(KeyEvent event){if(event.getAction()!=KeyEvent.ACTION_DOWN&&event.getAction()!=KeyEvent.ACTION_UP)return false;View focus=surface.findFocus();if(focus==null)return false;int usage=usage(event.getKeyCode());if(usage==0)return false;String key=keyName(event);try{JSONObject modifiers=modifiers(event.getMetaState());for(View item=focus;item!=null;item=item.getParent() instanceof View?(View)item.getParent():null){if(!(item.getTag() instanceof String))continue;String id=(String)item.getTag();JSONObject node=root.descriptor(id),keyboard=node==null?null:node.optJSONObject("keyboard");if(keyboard==null)continue;JSONArray shortcuts=keyboard.optJSONArray("shortcuts");if(event.getAction()==KeyEvent.ACTION_DOWN&&event.getRepeatCount()==0&&shortcuts!=null)for(int i=0;i<shortcuts.length();i++){JSONObject shortcut=shortcuts.getJSONObject(i),wanted=shortcut.optJSONObject("modifiers");boolean match=key.equals(shortcut.optString("key"));for(String name:new String[]{"shift","control","alt","meta","capsLock"})match&=modifiers.optBoolean(name)==(wanted!=null&&wanted.optBoolean(name));if(match){root.emit(id,NativeView.payload("shortcut").put("id",shortcut.getString("id")));return true;}}if(keyboard.optBoolean(event.getAction()==KeyEvent.ACTION_DOWN?"keyDown":"keyUp"))root.emit(id,NativeView.payload("key").put("event",new JSONObject().put("phase",event.getAction()==KeyEvent.ACTION_DOWN?"down":"up").put("key",key).put("code",usage).put("modifiers",modifiers).put("repeat",event.getRepeatCount()>0)));}return false;}catch(JSONException error){throw new IllegalStateException(error);}}
    private static int usage(int key){if(key>=KeyEvent.KEYCODE_A&&key<=KeyEvent.KEYCODE_Z)return key-KeyEvent.KEYCODE_A+4;if(key>=KeyEvent.KEYCODE_1&&key<=KeyEvent.KEYCODE_9)return key-KeyEvent.KEYCODE_1+30;if(key>=KeyEvent.KEYCODE_F1&&key<=KeyEvent.KEYCODE_F12)return key-KeyEvent.KEYCODE_F1+58;switch(key){case KeyEvent.KEYCODE_0:return 39;case KeyEvent.KEYCODE_ENTER:return 40;case KeyEvent.KEYCODE_ESCAPE:return 41;case KeyEvent.KEYCODE_DEL:return 42;case KeyEvent.KEYCODE_TAB:return 43;case KeyEvent.KEYCODE_SPACE:return 44;case KeyEvent.KEYCODE_MENU:return 101;case KeyEvent.KEYCODE_MOVE_HOME:return 74;case KeyEvent.KEYCODE_PAGE_UP:return 75;case KeyEvent.KEYCODE_FORWARD_DEL:return 76;case KeyEvent.KEYCODE_MOVE_END:return 77;case KeyEvent.KEYCODE_PAGE_DOWN:return 78;case KeyEvent.KEYCODE_DPAD_RIGHT:return 79;case KeyEvent.KEYCODE_DPAD_LEFT:return 80;case KeyEvent.KEYCODE_DPAD_DOWN:return 81;case KeyEvent.KEYCODE_DPAD_UP:return 82;case KeyEvent.KEYCODE_MINUS:return 45;case KeyEvent.KEYCODE_EQUALS:return 46;case KeyEvent.KEYCODE_LEFT_BRACKET:return 47;case KeyEvent.KEYCODE_RIGHT_BRACKET:return 48;case KeyEvent.KEYCODE_BACKSLASH:return 49;case KeyEvent.KEYCODE_SEMICOLON:return 51;case KeyEvent.KEYCODE_APOSTROPHE:return 52;case KeyEvent.KEYCODE_GRAVE:return 53;case KeyEvent.KEYCODE_COMMA:return 54;case KeyEvent.KEYCODE_PERIOD:return 55;case KeyEvent.KEYCODE_SLASH:return 56;case KeyEvent.KEYCODE_CAPS_LOCK:return 57;case KeyEvent.KEYCODE_CTRL_LEFT:return 224;case KeyEvent.KEYCODE_SHIFT_LEFT:return 225;case KeyEvent.KEYCODE_ALT_LEFT:return 226;case KeyEvent.KEYCODE_META_LEFT:return 227;case KeyEvent.KEYCODE_CTRL_RIGHT:return 228;case KeyEvent.KEYCODE_SHIFT_RIGHT:return 229;case KeyEvent.KEYCODE_ALT_RIGHT:return 230;case KeyEvent.KEYCODE_META_RIGHT:return 231;default:return 0;}}
    private static String keyName(KeyEvent event){if(event.getKeyCode()>=KeyEvent.KEYCODE_F1&&event.getKeyCode()<=KeyEvent.KEYCODE_F12)return "F"+(event.getKeyCode()-KeyEvent.KEYCODE_F1+1);switch(event.getKeyCode()){case KeyEvent.KEYCODE_SHIFT_LEFT:case KeyEvent.KEYCODE_SHIFT_RIGHT:return "Shift";case KeyEvent.KEYCODE_CTRL_LEFT:case KeyEvent.KEYCODE_CTRL_RIGHT:return "Control";case KeyEvent.KEYCODE_ALT_LEFT:case KeyEvent.KEYCODE_ALT_RIGHT:return "Alt";case KeyEvent.KEYCODE_META_LEFT:case KeyEvent.KEYCODE_META_RIGHT:return "Meta";case KeyEvent.KEYCODE_CAPS_LOCK:return "CapsLock";case KeyEvent.KEYCODE_MENU:return "ContextMenu";case KeyEvent.KEYCODE_ENTER:return "Enter";case KeyEvent.KEYCODE_ESCAPE:return "Escape";case KeyEvent.KEYCODE_DEL:return "Backspace";case KeyEvent.KEYCODE_TAB:return "Tab";case KeyEvent.KEYCODE_MOVE_HOME:return "Home";case KeyEvent.KEYCODE_MOVE_END:return "End";case KeyEvent.KEYCODE_PAGE_UP:return "PageUp";case KeyEvent.KEYCODE_PAGE_DOWN:return "PageDown";case KeyEvent.KEYCODE_FORWARD_DEL:return "Delete";case KeyEvent.KEYCODE_DPAD_LEFT:return "ArrowLeft";case KeyEvent.KEYCODE_DPAD_RIGHT:return "ArrowRight";case KeyEvent.KEYCODE_DPAD_UP:return "ArrowUp";case KeyEvent.KEYCODE_DPAD_DOWN:return "ArrowDown";default:int value=event.getUnicodeChar(event.getMetaState()&~(KeyEvent.META_CTRL_MASK|KeyEvent.META_META_MASK));return value>0?new String(Character.toChars(value)):KeyEvent.keyCodeToString(event.getKeyCode());}}
}
