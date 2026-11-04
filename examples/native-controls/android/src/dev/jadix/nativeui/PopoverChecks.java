package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import android.widget.Button;
import android.widget.EditText;
import android.widget.PopupWindow;
import android.widget.ScrollView;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

public final class PopoverChecks extends Instrumentation {
    private MainActivity activity;
    private final ArrayList<String> checks = new ArrayList<>();
    private interface Checked { void run() throws Exception; }
    private void ui(Checked action) {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runOnMainSync(() -> { try { action.run(); } catch (Throwable error) { failure.set(error); } });
        if (failure.get() != null) throw new AssertionError(failure.get());
        waitForIdleSync();
    }
    private static void require(boolean value, String message) { if (!value) throw new AssertionError(message); }
    private JSONObject state() throws Exception { return new JSONObject(activity.content.state()); }
    private View named(View view, String label) {
        if (view instanceof Button && label.contentEquals(((Button)view).getText())) return view;
        if (view instanceof EditText && label.contentEquals(((EditText)view).getHint())) return view;
        if (view instanceof ViewGroup) for (int index=0;index<((ViewGroup)view).getChildCount();index++) {
            View found = named(((ViewGroup)view).getChildAt(index), label);
            if (found != null) return found;
        }
        return null;
    }
    private PopupWindow popup() { return activity.content.activePopover(); }
    private View popupControl(String label) { return named(popup().getContentView(), label); }
    private void opened(boolean expected) {
        for (int attempt=0;attempt<80;attempt++) {
            AtomicReference<Boolean> ready = new AtomicReference<>(false);
            ui(() -> ready.set(expected ? popup()!=null && popup().isShowing() && popup().getContentView().hasWindowFocus()
                    : popup()==null || !popup().isShowing()));
            if (ready.get()) return;
            android.os.SystemClock.sleep(50);
        }
        throw new AssertionError("native Popover showing="+expected);
    }
    private void tap(View view) {
        AtomicReference<android.graphics.Rect> rectangle = new AtomicReference<>();
        ui(() -> { android.graphics.Rect bounds = new android.graphics.Rect(); view.getLocalVisibleRect(bounds); int[] location=new int[2]; view.getLocationOnScreen(location); bounds.offset(location[0],location[1]); rectangle.set(bounds); });
        android.graphics.Rect bounds = rectangle.get();
        require(bounds != null && !bounds.isEmpty(), "touch target visible");
        long time=android.os.SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,bounds.exactCenterX(),bounds.exactCenterY(),0);
        MotionEvent up=MotionEvent.obtain(time,time+30,MotionEvent.ACTION_UP,bounds.exactCenterX(),bounds.exactCenterY(),0);
        try { sendPointerSync(down); sendPointerSync(up); } finally { down.recycle(); up.recycle(); }
        waitForIdleSync();
    }
    private void active(PopupWindow expected) {
        for (int attempt=0;attempt<80;attempt++) {
            AtomicReference<Boolean> ready=new AtomicReference<>(false);
            ui(() -> ready.set(popup()==expected && expected.isShowing() && expected.getContentView().hasWindowFocus()));
            if (ready.get()) return;
            android.os.SystemClock.sleep(50);
        }
        throw new AssertionError("expected retained native popup did not become active");
    }
    private View rootControl(String label) { return named(activity.content,label); }
    private void republish(long environment) {
        float density=activity.getResources().getDisplayMetrics().density;
        NativeView view=activity.content;
        NativeView.nativeResize(view.token(),(view.getWidth()-view.getPaddingLeft()-view.getPaddingRight())/density,
                (view.getHeight()-view.getPaddingTop()-view.getPaddingBottom())/density,environment);
    }
    private android.graphics.Rect bounds(View view) {
        int[] location=new int[2];view.getLocationOnScreen(location);
        return new android.graphics.Rect(location[0],location[1],location[0]+view.getWidth(),location[1]+view.getHeight());
    }
    private void insideViewport() {
        android.graphics.Rect viewport=new android.graphics.Rect();activity.content.getWindowVisibleDisplayFrame(viewport);
        require(viewport.intersect(bounds(activity.content)),"visible native viewport");
        require(viewport.contains(bounds(popup().getContentView())),"native panel stays inside visible viewport");
    }
    private void openProfile() { tap(rootControl("Edit profile")); opened(true); }
    private void scrollToControl(String label, boolean end) {
        ui(() -> {
            ScrollView scroll=(ScrollView)activity.content.findKind("scroll");scroll.setSmoothScrollingEnabled(false);
            View target=rootControl(label);target.setFocusableInTouchMode(true);target.requestFocus();
            scroll.scrollTo(0,end ? scroll.getChildAt(0).getHeight() : 0);
        });
        int previous=-1,stable=0;
        for(int attempt=0;attempt<80;attempt++) {
            AtomicReference<Integer> position=new AtomicReference<>();AtomicReference<Boolean> visible=new AtomicReference<>(false);
            ui(() -> {position.set(((ScrollView)activity.content.findKind("scroll")).getScrollY());
                visible.set(bounds(activity.content).contains(bounds(rootControl(label))));});
            stable=visible.get() && position.get()==previous ? stable+1 : 0;previous=position.get();
            if(stable>=3) return;android.os.SystemClock.sleep(50);
        }
        AtomicReference<String> details=new AtomicReference<>("");
        ui(() -> {ScrollView scroll=(ScrollView)activity.content.findKind("scroll");details.set("scroll="+scroll.getScrollY()+" height="+scroll.getHeight()+" child="+scroll.getChildAt(0).getHeight()+" root="+bounds(activity.content)+" target="+bounds(rootControl(label)));});
        throw new AssertionError("native scroll did not settle with visible control "+label+": "+details.get());
    }
    private void closePopup() { sendKeyDownUpSync(KeyEvent.KEYCODE_ESCAPE); opened(false); }
    private void screenshot() throws Exception {
        android.graphics.Bitmap bitmap=getUiAutomation().takeScreenshot();
        require(bitmap!=null,"screenshot available");
        try(FileOutputStream file=new FileOutputStream(new File(activity.getFilesDir(),"popover-native.png"))) {
            require(bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,file),"screenshot encoded");
        } finally { bitmap.recycle(); }
    }
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    @Override public void onStart() {
        Bundle result=new Bundle();
        try {
            activity=(MainActivity)startActivitySync(new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK|Intent.FLAG_ACTIVITY_CLEAR_TASK));
            waitForIdleSync();
            ui(() -> require(popup()==null && rootControl("Edit profile") instanceof Button,"closed native button page"));
            checks.add("initial controlled page uses native buttons and has no open PopupWindow");
            openProfile();
            AtomicReference<PopupWindow> retainedPopup=new AtomicReference<>();
            AtomicReference<EditText> retainedInput=new AtomicReference<>();
            ui(() -> {
                retainedPopup.set(popup());
                retainedInput.set((EditText)popupControl("Profile name"));
                require(retainedInput.get()!=null && retainedInput.get().hasFocus(),"first native editor focused");
                require(popup().isFocusable(),"focusable native PopupWindow");
            });
            checks.add("injected touch opens native PopupWindow and focuses retained EditText");
            ui(() -> {
                android.graphics.Rect trigger=bounds(rootControl("Edit profile")), panel=bounds(popup().getContentView());
                int gap=Math.round(4*activity.getResources().getDisplayMetrics().density);
                require(Math.abs(panel.left-trigger.left)<=2 && Math.abs(panel.top-trigger.bottom-gap)<=2,"native screen anchor and gap");
                insideViewport();
            });
            checks.add("native panel aligns with actual screen anchor and four logical pixel gap inside visible viewport");
            ui(() -> { retainedInput.get().setText("中文 😀 é"); retainedInput.get().setSelection(3,5); });
            ui(() -> require("中文 😀 é".equals(state().getString("value")),"Unicode native edit reaches Rust"));
            ui(() -> republish(9001));
            ui(() -> require(popup()==retainedPopup.get() && popupControl("Profile name")==retainedInput.get()
                    && retainedInput.get().getSelectionStart()==3 && retainedInput.get().getSelectionEnd()==5,"publication retains native editor selection and identity"));
            checks.add("programmatic native Unicode edit reaches Rust and publication retains popup/editor selection");
            ui(() -> {
                EditText field=retainedInput.get();String previous=state().getString("value");field.selectAll();
                InputConnection connection=field.onCreateInputConnection(new EditorInfo());require(connection!=null,"native popup InputConnection");
                connection.setComposingText("zhong",1);require(previous.equals(state().getString("value")),"preedit does not commit Rust value");
                int start=android.view.inputmethod.BaseInputConnection.getComposingSpanStart(field.getText());
                int end=android.view.inputmethod.BaseInputConnection.getComposingSpanEnd(field.getText());
                republish(9002);
                require(start==android.view.inputmethod.BaseInputConnection.getComposingSpanStart(field.getText())
                        && end==android.view.inputmethod.BaseInputConnection.getComposingSpanEnd(field.getText()),"native preedit retained across publication");
                connection.setComposingText("中",1);connection.commitText("中文😀",1);
                require("中文😀".equals(state().getString("value")),"native popup composition commits Unicode to Rust");
            });
            checks.add("native InputConnection keeps preedit local across publication and commits Chinese emoji to Rust");
            closePopup();
            ui(() -> require(rootControl("Edit profile").hasFocus(),"Escape restores trigger focus"));
            openProfile();
            sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);
            opened(false);
            checks.add("injected Escape and Back close controlled panels and restore the trigger");
            openProfile();
            tap(popupControl("More options"));
            opened(true);
            ui(() -> require(popup()!=retainedPopup.get() && popupControl("Apply option")!=null,"nested native popup"));
            sendKeyDownUpSync(KeyEvent.KEYCODE_ESCAPE);
            active(retainedPopup.get());
            ui(() -> require(popup()==retainedPopup.get() && popup().isShowing(),"Escape closes only child"));
            tap(popupControl("More options"));
            tap(popupControl("Apply option"));
            active(retainedPopup.get());
            ui(() -> require(state().getInt("count")==1 && popup()==retainedPopup.get(),"nested callback updates Rust and closes own context"));
            checks.add("nested PopupWindow Escape and contextual action preserve parent and update Rust");
            AtomicReference<Integer> before=new AtomicReference<>();
            ui(() -> { int[] location=new int[2]; popup().getContentView().getLocationOnScreen(location); before.set(location[1]);
                ((ScrollView)activity.content.findKind("scroll")).scrollBy(0,30); });
            android.os.SystemClock.sleep(150);
            ui(() -> { int[] location=new int[2]; popup().getContentView().getLocationOnScreen(location);
                require(Math.abs(location[1]-(before.get()-30))<=2,"popup follows actual native scroll"); });
            checks.add("native scroll repositions the retained popup with its anchor");
            ui(() -> rootControl("Reset name").performClick());
            ui(() -> require("Reset 中文 😀".contentEquals(retainedInput.get().getText()),"external Rust value reaches retained input"));
            tap(popupControl("Save profile"));
            opened(false);
            ui(() -> ((ScrollView)activity.content.findKind("scroll")).scrollTo(0,0));
            openProfile();
            ui(() -> require(popupControl("Profile name")==retainedInput.get(),"close/reopen retains editor"));
            checks.add("external controlled reset and close/reopen preserve native editor identity");
            tap(rootControl("Background action"));
            opened(false);
            ui(() -> require(activity.content.takeError()==null,"outside close produces no rejected event"));
            checks.add("injected outside pointer closes the nonmodal controlled panel");
            openProfile();
            ui(() -> rootControl("Other panel").performClick());
            opened(true);
            ui(() -> require(popup()!=retainedPopup.get() && popupControl("Other field")!=null && retainedPopup.get().isShowing(),"independent native panel opening order"));
            sendKeyDownUpSync(KeyEvent.KEYCODE_ESCAPE);active(retainedPopup.get());closePopup();
            checks.add("programmatic independent panel opening preserves prior window and Escape closes the newest panel only");
            scrollToControl("Bottom panel",true);
            tap(rootControl("Bottom panel"));opened(true);
            ui(() -> {insideViewport();require(bounds(popup().getContentView()).bottom<=bounds(rootControl("Bottom panel")).top,"bottom edge flips panel above trigger");});
            closePopup();
            scrollToControl("Edit profile",false);
            checks.add("native bottom edge flips the panel above its scrolled trigger and remains inside viewport");
            openProfile();
            screenshot();
            AtomicReference<String> oldRoot=new AtomicReference<>();
            AtomicReference<View> oldTrigger=new AtomicReference<>();
            ui(() -> {oldRoot.set(activity.content.token());oldTrigger.set(rootControl("Edit profile"));
                activity.content.close();activity.content.mount(0,"");oldTrigger.get().performClick();retainedInput.get().setText("stale");});
            opened(false);
            ui(() -> {require(state().getInt("count")==0 && state().getString("value").isEmpty(),"fresh root isolated");
                require(!retainedPopup.get().isShowing(),"disposed native popup removed");
                boolean rejected=false;try{NativeView.nativeState(oldRoot.get());}catch(IllegalStateException expected){rejected=true;}require(rejected,"old root rejected");});
            checks.add("unmount disposes native windows and stale views cannot mutate replacement root");
            for(int index=0;index<10;index++) {openProfile();ui(() -> {activity.content.close();activity.content.mount(0,"");});opened(false);}
            ui(() -> require(activity.content.takeError()==null,"ten popup lifecycles remain usable"));
            checks.add("ten open/unmount/remount cycles release PopupWindow and keep replacement roots usable");
            ui(() -> rootControl("Background action").performClick());openProfile();
            AtomicReference<PopupWindow> retiredWindow=new AtomicReference<>();AtomicReference<String> retiredRoot=new AtomicReference<>();
            ui(() -> {EditText field=(EditText)popupControl("Profile name");field.selectAll();field.onCreateInputConnection(new EditorInfo()).commitText("重建😀",1);
                retiredWindow.set(popup());retiredRoot.set(activity.content.token());});
            ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);ui(() -> activity.recreate());
            Activity replacement=monitor.waitForActivityWithTimeout(10000);removeMonitor(monitor);require(replacement instanceof MainActivity,"Activity recreated");
            activity=(MainActivity)replacement;opened(false);
            ui(() -> {require(!retiredWindow.get().isShowing(),"Activity recreation removes old native popup");
                require(state().getInt("count")==1 && "重建😀".equals(state().getString("value")),"recreation restores Rust business state");
                boolean rejected=false;try{NativeView.nativeState(retiredRoot.get());}catch(IllegalStateException expected){rejected=true;}require(rejected,"recreated root rejects retired token");});
            openProfile();ui(() -> require("重建😀".contentEquals(((EditText)popupControl("Profile name")).getText()),"restored business value reaches fresh native popup"));closePopup();
            checks.add("Activity recreation removes open native window, restores Rust business state and rejects the retired root");
            JSONObject report=new JSONObject().put("passed",true).put("checks",new JSONArray(checks))
                    .put("event_authority","instrumented native views, injected pointer/keys; programmatic edits/scroll/resize explicitly described");
            try(FileOutputStream file=new FileOutputStream(new File(activity.getFilesDir(),"popover-checks.json"))) {file.write(report.toString(2).getBytes(StandardCharsets.UTF_8));}
            ui(() -> activity.finish());
            result.putString("stream","JADIX_POPOVER_CHECKS_PASS: scenarios="+checks.size()+"; popup_checked=true; nested_checked=true; scroll_checked=true; disposal_checked=true; composition_checked=true; geometry_checked=true; recreate_checked=true;\n");
            finish(Activity.RESULT_OK,result);
        }catch(Throwable error){
            try{screenshot();}catch(Throwable ignored){}
            AtomicReference<String> diagnostics=new AtomicReference<>("");
            try { ui(() -> {
                java.lang.reflect.Field descriptors=NativeView.class.getDeclaredField("descriptors");descriptors.setAccessible(true);
                java.util.Map<?,?> nodes=(java.util.Map<?,?>)descriptors.get(activity.content);
                JSONArray panels=new JSONArray();for(Object value:nodes.values()){JSONObject node=(JSONObject)value;if("popover".equals(node.optString("kind")))panels.put(node);}
                java.lang.reflect.Field order=NativeView.class.getDeclaredField("popoverOrder");order.setAccessible(true);
                diagnostics.set("completed="+new JSONArray(checks)+"; order="+order.get(activity.content)+"; panels="+panels+"; error="+activity.content.takeError());
            });}catch(Throwable ignored){}
            result.putString("stream","JADIX_POPOVER_CHECKS_FAIL: "+android.util.Log.getStackTraceString(error)+"\n"+diagnostics.get());finish(Activity.RESULT_CANCELED,result);
        }
    }
}
