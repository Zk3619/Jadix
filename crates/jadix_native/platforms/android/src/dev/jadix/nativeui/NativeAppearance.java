package dev.jadix.nativeui;

import android.graphics.*;
import android.graphics.drawable.*;
import android.os.Build;
import android.text.*;
import android.text.style.*;
import android.view.*;
import android.widget.*;
import org.json.*;

/** Presentation of the shared style contract; layout remains owned by the core. */
final class NativeAppearance {
    private static final java.util.WeakHashMap<View,Defaults> defaults=new java.util.WeakHashMap<>();
    private static final class Defaults {
        final Drawable background;
        final android.content.res.ColorStateList backgroundTint,buttonTint,hint;
        final int highlight;
        final Drawable cursor;
        final boolean clipChildren,clipOutline;
        final float elevation;
        View.OnLayoutChangeListener geometry;
        Defaults(View view) {
            background=view.getBackground(); backgroundTint=view.getBackgroundTintList();
            buttonTint=view instanceof CompoundButton?((CompoundButton)view).getButtonTintList():null;
            hint=view instanceof TextView?((TextView)view).getHintTextColors():null;
            highlight=view instanceof TextView?((TextView)view).getHighlightColor():0;
            cursor=Build.VERSION.SDK_INT>=29&&view instanceof TextView?((TextView)view).getTextCursorDrawable():null;
            clipChildren=view instanceof ViewGroup&&((ViewGroup)view).getClipChildren();
            clipOutline=view.getClipToOutline(); elevation=view.getElevation();
        }
    }
    private static Defaults defaults(View view) { return defaults.computeIfAbsent(view,Defaults::new); }
    static void dispose(View view) {
        NativeMotion.dispose(view);
        NativeEffects.dispose(view);
        Defaults initial=defaults.remove(view);
        if (initial != null) {
            if (initial.geometry != null) view.removeOnLayoutChangeListener(initial.geometry);
            view.setBackground(initial.background); view.setBackgroundTintList(initial.backgroundTint);
            view.setClipToOutline(initial.clipOutline); view.setElevation(initial.elevation);
            if (view instanceof ViewGroup) ((ViewGroup)view).setClipChildren(initial.clipChildren);
            if (view instanceof CompoundButton) ((CompoundButton)view).setButtonTintList(initial.buttonTint);
            if (view instanceof TextView) {
                ((TextView)view).setHintTextColor(initial.hint);
                ((TextView)view).setHighlightColor(initial.highlight);
            }
        }
        view.clearAnimation();
    }
    static int color(JSONArray rgba, int fallback) { return rgba == null ? fallback : Color.argb(rgba.optInt(3,255),rgba.optInt(0),rgba.optInt(1),rgba.optInt(2)); }
    static void view(View view, JSONObject style, float density) throws JSONException {
        NativeMotion.apply(view,style,density);
    }
    static void paint(View view, JSONObject style, float density) throws JSONException {
        JSONObject appearance=style.optJSONObject("view");
        Defaults initial=defaults(view);
        if(initial.geometry!=null){view.removeOnLayoutChangeListener(initial.geometry);initial.geometry=null;}
        NativeEffects.apply(view,style,density);
        view.setAlpha((float)(appearance == null ? 1 : appearance.optDouble("opacity",1)));
        view.setVisibility(appearance != null && appearance.optBoolean("hidden") ? View.INVISIBLE : View.VISIBLE);
        if (appearance == null) {
            transform(view,null,density); view.setBackground(initial.background);view.setBackgroundTintList(initial.backgroundTint);
            view.setClipToOutline(initial.clipOutline);view.setElevation(initial.elevation);view.setPointerIcon(null);
            if(view instanceof ViewGroup)((ViewGroup)view).setClipChildren(initial.clipChildren);
            if(view instanceof ImageView)((ImageView)view).setImageTintList(null);
            if(view instanceof CompoundButton)((CompoundButton)view).setButtonTintList(initial.buttonTint);
            return;
        }
        if (view instanceof ViewGroup) ((ViewGroup)view).setClipChildren(appearance.optBoolean("clip"));
        transform(view,appearance,density);
        initial.geometry=(v,l,t,r,b,ol,ot,or,ob)->transform(v,appearance,density);
        view.addOnLayoutChangeListener(initial.geometry);
        JSONArray corners=appearance.optJSONArray("corners"), widths=appearance.optJSONArray("border_widths"), layers=appearance.optJSONArray("backgrounds");
        boolean decorated=!appearance.isNull("background") || layers!=null && layers.length()>0
            || nonzero(corners) || nonzero(widths) || appearance.optDouble("outline_width")>0
            || appearance.optJSONArray("shadows")!=null && appearance.optJSONArray("shadows").length()>0
            || appearance.optJSONArray("masks")!=null && appearance.optJSONArray("masks").length()>0
            || NativeEffects.backdrop(view)!=null;
        if (decorated) {
            boolean nativeFill=appearance.isNull("background") && (layers==null || layers.length()==0)
                && !nonzero(corners) && !nonzero(widths) && initial.background!=null;
            NativeSurface surface=new NativeSurface(appearance,nativeFill ? Color.TRANSPARENT : color(style.optJSONArray("background"),Color.TRANSPARENT),density);
            surface.backdrop=NativeEffects.backdrop(view);
            view.setBackgroundTintList(null);
            view.setBackground(nativeFill ? new LayerDrawable(new Drawable[]{surface,initial.background}) : surface);
            view.setClipToOutline(false);
        } else {
            view.setBackground(initial.background);
            int background=color(style.optJSONArray("background"),Color.TRANSPARENT);
            view.setBackgroundTintList(Color.alpha(background)==0 ? initial.backgroundTint : android.content.res.ColorStateList.valueOf(background));
            view.setClipToOutline(initial.clipOutline);
        }
        JSONArray tint=appearance.optJSONArray("tint");
        if (view instanceof ImageView) ((ImageView)view).setImageTintList(tint==null?null:android.content.res.ColorStateList.valueOf(color(tint,0)));
        view.setElevation(0);
        if(view instanceof CompoundButton)((CompoundButton)view).setButtonTintList(appearance.isNull("accent")?initial.buttonTint:android.content.res.ColorStateList.valueOf(color(appearance.optJSONArray("accent"),0)));
        String cursor=appearance.optString("cursor","default"); int pointer=PointerIcon.TYPE_DEFAULT;
        switch(cursor) { case "pointer":pointer=PointerIcon.TYPE_HAND;break;case "text":pointer=PointerIcon.TYPE_TEXT;break;case "crosshair":pointer=PointerIcon.TYPE_CROSSHAIR;break;case "wait":case "progress":pointer=PointerIcon.TYPE_WAIT;break;case "help":pointer=PointerIcon.TYPE_HELP;break;case "move":pointer=PointerIcon.TYPE_ALL_SCROLL;break;case "zoom_in":pointer=PointerIcon.TYPE_ZOOM_IN;break;case "zoom_out":pointer=PointerIcon.TYPE_ZOOM_OUT;break;case "grab":pointer=PointerIcon.TYPE_GRAB;break;case "grabbing":pointer=PointerIcon.TYPE_GRABBING;break;case "not_allowed":pointer=PointerIcon.TYPE_NO_DROP;break;case "resize_horizontal":pointer=PointerIcon.TYPE_HORIZONTAL_DOUBLE_ARROW;break;case "resize_vertical":pointer=PointerIcon.TYPE_VERTICAL_DOUBLE_ARROW;break;case "resize_north_east_south_west":pointer=PointerIcon.TYPE_TOP_RIGHT_DIAGONAL_DOUBLE_ARROW;break;case "resize_north_west_south_east":pointer=PointerIcon.TYPE_TOP_LEFT_DIAGONAL_DOUBLE_ARROW;break;case "none":pointer=PointerIcon.TYPE_NULL;break; }
        view.setPointerIcon(PointerIcon.getSystemIcon(view.getContext(),pointer));
    }
    private static boolean nonzero(JSONArray values) { if(values!=null)for(int i=0;i<values.length();i++)if(values.optDouble(i)!=0)return true;return false; }
    static void transform(View view,JSONObject appearance,float density) { transform(view,appearance,density,null); }
    static void transform(View view,JSONObject appearance,float density,float[] keyframe) {
        try {
            JSONArray motion=new JSONArray();for(float value:keyframe==null?new float[]{0,1,1,0}:keyframe)motion.put(value);
            JSONObject plan=(JSONObject)NativeRuntime.call(NativeRuntime.object("surfaceTransform")
                .put("appearance",appearance==null?new JSONObject():appearance)
                .put("size",new JSONArray().put(view.getWidth()/density).put(view.getHeight()/density)).put("motion",motion));
            JSONArray origin=plan.getJSONArray("origin");view.setPivotX((float)origin.getDouble(0)*density);view.setPivotY((float)origin.getDouble(1)*density);
            if(Build.VERSION.SDK_INT>=29) {
                view.setTranslationX(0);view.setTranslationY(0);view.setScaleX(1);view.setScaleY(1);view.setRotation(0);view.setRotationX(0);view.setRotationY(0);
                JSONArray values=plan.getJSONArray("matrix");float[] matrix=new float[9];
                for(int i=0;i<9;i++)matrix[i]=(float)(values.getDouble(i)*(i==2||i==5?density:i==6||i==7?1/density:1));
                Matrix projected=new Matrix();projected.setValues(matrix);view.setAnimationMatrix(projected);
            } else {
                JSONArray translation=plan.getJSONArray("translation"),scale=plan.getJSONArray("scale"),rotation=plan.getJSONArray("rotation");
                view.setTranslationX((float)translation.getDouble(0)*density);view.setTranslationY((float)translation.getDouble(1)*density);
                view.setScaleX((float)scale.getDouble(0));view.setScaleY((float)scale.getDouble(1));
                view.setRotationX((float)rotation.getDouble(0));view.setRotationY((float)rotation.getDouble(1));view.setRotation((float)rotation.getDouble(2));
            }
            boolean hidden=appearance!=null&&(appearance.optBoolean("hidden")||appearance.optBoolean("backface_hidden")&&plan.optBoolean("backface"));
            view.setVisibility(hidden?View.INVISIBLE:View.VISIBLE);
        }catch(JSONException error){throw new IllegalArgumentException("Invalid surface transform",error);}
    }
    static void text(TextView text, JSONObject node,float density) throws JSONException {
        Defaults initial=defaults(text);
        JSONObject style=node.getJSONObject("style"), appearance=style.optJSONObject("text");
        if (appearance == null) appearance=new JSONObject();
        if (!(text instanceof android.widget.EditText) && "text".equals(node.optString("kind"))) {
            text.setTextIsSelectable(appearance.optBoolean("selectable", false));
            NativeSelection.install(text);
        }
        int fontStyle=(style.optInt("font_weight",400)>=600?Typeface.BOLD:0)|(appearance.optBoolean("italic")?Typeface.ITALIC:0);
        text.setTypeface(Typeface.create(text.getTypeface(),fontStyle));
        text.setLetterSpacing((float)(appearance.optDouble("letter_spacing",0)*density/Math.max(1,text.getTextSize())));
        text.setTextDirection("rtl".equals(appearance.optString("direction"))?View.TEXT_DIRECTION_RTL:"ltr".equals(appearance.optString("direction"))?View.TEXT_DIRECTION_LTR:View.TEXT_DIRECTION_FIRST_STRONG);
        int gravity=Gravity.START; String align=appearance.optString("align","start");
        switch(align) { case "center":gravity=Gravity.CENTER_HORIZONTAL;break;case "end":gravity=Gravity.END;break;case "right":gravity=Gravity.RIGHT;break;case "left":gravity=Gravity.LEFT;break; }
        String vertical=appearance.optString("vertical_align","top"); gravity|="center".equals(vertical)?Gravity.CENTER_VERTICAL:"bottom".equals(vertical)?Gravity.BOTTOM:Gravity.TOP;
        text.setGravity(gravity); text.setJustificationMode("justify".equals(align)?android.text.Layout.JUSTIFICATION_MODE_INTER_WORD:android.text.Layout.JUSTIFICATION_MODE_NONE);
        text.setLineSpacing((float)(appearance.has("line_height")?appearance.optDouble("line_height")*density-text.getTextSize():0),(float)appearance.optDouble("line_height_multiple",1));
        text.setMaxLines(style.isNull("max_lines")?Integer.MAX_VALUE:style.optInt("max_lines",Integer.MAX_VALUE));
        String overflow=style.optString("text_overflow","wrap");
        text.setEllipsize("ellipsis".equals(overflow)?TextUtils.TruncateAt.END:"ellipsis_start".equals(overflow)?TextUtils.TruncateAt.START:"ellipsis_middle".equals(overflow)?TextUtils.TruncateAt.MIDDLE:null);
        int flags=text.getPaintFlags() & ~(Paint.UNDERLINE_TEXT_FLAG|Paint.STRIKE_THRU_TEXT_FLAG);
        if (appearance.optBoolean("underline")) flags|=Paint.UNDERLINE_TEXT_FLAG;
        if (appearance.optBoolean("strikethrough")) flags|=Paint.STRIKE_THRU_TEXT_FLAG;
        text.setPaintFlags(flags);
        JSONObject shadow=appearance.optJSONObject("shadow"); JSONArray offset=shadow==null?null:shadow.optJSONArray("offset");
        text.setShadowLayer(shadow==null?0:(float)shadow.optDouble("blur")*density,offset==null?0:(float)offset.optDouble(0)*density,offset==null?0:(float)offset.optDouble(1)*density,shadow==null?0:color(shadow.optJSONArray("color"),0));
        text.setHintTextColor(appearance.isNull("placeholder_color")?initial.hint:android.content.res.ColorStateList.valueOf(color(appearance.optJSONArray("placeholder_color"),Color.GRAY)));
        text.setHighlightColor(appearance.isNull("selection_color")?initial.highlight:color(appearance.optJSONArray("selection_color"),0));
        if (Build.VERSION.SDK_INT>=29 && text instanceof EditText) {
            if(appearance.isNull("caret_color"))text.setTextCursorDrawable(initial.cursor);
            else { GradientDrawable caret=new GradientDrawable();caret.setColor(color(appearance.optJSONArray("caret_color"),Color.BLACK));caret.setSize(Math.max(1,(int)(2*density)),(int)text.getTextSize());text.setTextCursorDrawable(caret); }
        }
        JSONArray variants=appearance.optJSONArray("variants"); StringBuilder features=new StringBuilder();
        if (variants!=null) for(int i=0;i<variants.length();i++) { String feature=null; switch(variants.optString(i)) {case "small_caps":feature="smcp";break;case "oldstyle_numbers":feature="onum";break;case "lining_numbers":feature="lnum";break;case "tabular_numbers":feature="tnum";break;case "proportional_numbers":feature="pnum";break;} if(feature!=null) {if(features.length()>0)features.append(',');features.append("'").append(feature).append("' 1");} }
        text.setFontFeatureSettings(features.length()==0?null:features.toString());
        if (text instanceof EditText) return;
        String source=node.optString("text",""); String display=(String)NativeRuntime.call(NativeRuntime.object("displayText").put("text",source).put("appearance",appearance));
        SpannableString spans=new SpannableString(display);
        JSONArray inline=style.optJSONArray("inline_styles");
        if (inline!=null && source.equals(display)) for(int i=0;i<inline.length();i++) {
            JSONObject part=inline.getJSONObject(i);JSONArray range=part.getJSONArray("range");int start=utf16(source,range.getInt(0)),end=utf16(source,range.getInt(1));
            if(start>=end)continue;
            if(!part.isNull("foreground"))spans.setSpan(new ForegroundColorSpan(color(part.optJSONArray("foreground"),0)),start,end,Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            if(!part.isNull("background"))spans.setSpan(new BackgroundColorSpan(color(part.optJSONArray("background"),0)),start,end,Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            if(part.optBoolean("bold")||part.optBoolean("italic"))spans.setSpan(new StyleSpan((part.optBoolean("bold")?Typeface.BOLD:0)|(part.optBoolean("italic")?Typeface.ITALIC:0)),start,end,Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            if(part.optBoolean("underline"))spans.setSpan(new UnderlineSpan(),start,end,Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            if(part.optBoolean("strikethrough"))spans.setSpan(new StrikethroughSpan(),start,end,Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        }
        if(!text.getText().toString().equals(display)||inline!=null||(text.getText() instanceof Spanned && ((Spanned)text.getText()).getSpans(0,text.length(),CharacterStyle.class).length>0))text.setText(spans);
    }
    private static int utf16(String text,int bytes) { int offset=0,count=0;while(offset<text.length()&&count<bytes){int cp=text.codePointAt(offset);count+=cp<128?1:cp<2048?2:cp<65536?3:4;offset+=Character.charCount(cp);}return offset; }
}
