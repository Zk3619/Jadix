package dev.jadix.nativeui;

import android.graphics.Bitmap;
import android.graphics.SurfaceTexture;
import android.opengl.*;
import android.os.*;
import android.view.TextureView;
import java.nio.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import org.json.*;

/** Composited shader background. All EGL state belongs to one owned render thread. */
final class NativeShader extends TextureView implements AutoCloseable, TextureView.SurfaceTextureListener {
    private final NativeView root;
    private final NativeHosted.Events events;
    private final HandlerThread thread = new HandlerThread("Jadix shader");
    private final Handler renderer;
    private volatile JSONObject specification = new JSONObject();
    private volatile boolean closed;
    private final AtomicBoolean frameQueued = new AtomicBoolean();
    private volatile RenderInput latestFrame;
    interface Inputs {
        java.util.Map<String, Bitmap> capture();
        float progress();
        boolean active();
        void presented(JSONObject spec);
        void failed(JSONObject spec);
    }
    private Inputs composition;
    private volatile JSONObject presentedSpec,failedSpec;
    void inputs(Inputs value) { composition=value; }
    private final LinkedHashMap<String,Texture> capturedTextures=new LinkedHashMap<>();
    private static final class RenderInput {
        final JSONObject spec; final Texture[] textures; final Bitmap[] bitmaps; final float seconds, progress;
        RenderInput(JSONObject spec, Texture[] textures, float seconds, float progress) {
            this.spec=spec; this.textures=textures; this.seconds=seconds; this.progress=progress;
            bitmaps=new Bitmap[textures.length];for(int i=0;i<textures.length;i++)bitmaps[i]=textures[i].bitmap;
        }
    }
    private final Runnable renderFrame = new Runnable() { public void run() {
        RenderInput input=latestFrame;
        try { if(input!=null&&!closed)draw(input); }
        catch(RuntimeException error) { if(input!=null)report(input.spec,error); }
        finally {
            frameQueued.set(false);
            if(!closed&&latestFrame!=input&&frameQueued.compareAndSet(false,true))
                if(!renderer.post(this))frameQueued.set(false);
        }
    }};
    private boolean visible, ticking;
    private long accumulated, resumed;
    private final LinkedHashMap<String, Texture> textures = new LinkedHashMap<>();
    private volatile Texture[] textureSnapshot = new Texture[0];
    private final FloatBuffer vertices = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder()).asFloatBuffer();
    // Render-thread-only fields.
    private EGLDisplay display = EGL14.EGL_NO_DISPLAY;
    private EGLContext context = EGL14.EGL_NO_CONTEXT;
    private EGLSurface surface = EGL14.EGL_NO_SURFACE;
    private SurfaceTexture target;
    private int program, width, height;
    private String compiled = "";
    private boolean programReady;
    private String preparedKey = "";
    private JSONObject translated;
    private final int[] uniformBuffers = new int[2];
    private final ByteBuffer[] uniformBytes = new ByteBuffer[2];
    private final Runnable frame = new Runnable() { public void run() {
        if (closed || !ticking) return;
        requestFrame();
        postDelayed(this, Math.max(8, 1000 / Math.max(1, specification.optInt("frames_per_second", 60))));
    }};
    private static final class Texture {
        final String name, source;
        AutoCloseable request;
        volatile Bitmap bitmap;
        Bitmap uploaded;
        int handle;
        Texture(String name, String source) { this.name = name; this.source = source; }
    }
    NativeShader(NativeView root, NativeHosted.Events events) {
        super(root.getContext()); this.root = root; this.events = events;
        vertices.put(new float[]{-1,-1,1,-1,-1,1,1,1}).position(0);
        setOpaque(false); thread.start(); renderer = new Handler(thread.getLooper());
        setSurfaceTextureListener(this);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);setFocusable(false);setClickable(false);
    }
    void configure(JSONObject value) {
        updateClock(false); specification = value;failedSpec=null;
        JSONObject sources = value.optJSONObject("textures");
        HashSet<String> retained = new HashSet<>();
        if (sources != null) for (Iterator<String> names = sources.keys(); names.hasNext();) {
            String name = names.next(), source = sources.optString(name); retained.add(name);
            Texture old = textures.get(name);
            if (old != null && old.source.equals(source)) continue;
            if (old != null) removeTexture(old);
            Texture texture = new Texture(name, source); textures.put(name, texture);
            texture.request = root.resources.image(source, bitmap -> {
                if (closed || textures.get(name) != texture) return;
                texture.bitmap = bitmap; requestFrame();
            });
        }
        for (Iterator<Map.Entry<String, Texture>> entries = textures.entrySet().iterator(); entries.hasNext();) {
            Map.Entry<String, Texture> entry = entries.next();
            if (!retained.contains(entry.getKey())) { removeTexture(entry.getValue()); entries.remove(); }
        }
        JSONObject mode=value.optJSONObject("composition");String type=mode==null?"background":mode.optString("type");
        java.util.List<String> builtins=type.equals("effect")?Arrays.asList("content","backdrop"):
            type.equals("transition")?Arrays.asList("from","to","content","backdrop"):Collections.emptyList();
        for(Iterator<Map.Entry<String,Texture>> it=capturedTextures.entrySet().iterator();it.hasNext();) {
            Map.Entry<String,Texture> entry=it.next();if(!builtins.contains(entry.getKey())){removeTexture(entry.getValue());it.remove();}
        }
        for(String name:builtins)if(!capturedTextures.containsKey(name))capturedTextures.put(name,new Texture(name,""));
        ArrayList<Texture> all=new ArrayList<>(textures.values());all.addAll(capturedTextures.values());
        textureSnapshot=all.toArray(new Texture[0]); updateRunning(); requestFrame();
    }
    private void removeTexture(Texture texture) {
        try { if (texture.request != null) texture.request.close(); } catch (Exception ignored) { }
        renderer.post(() -> { if (texture.handle != 0 && current()) GLES20.glDeleteTextures(1, new int[]{texture.handle}, 0);
            texture.handle = 0; texture.uploaded = null; });
    }
    private void updateClock(boolean next) {
        long now = SystemClock.elapsedRealtime();
        if (ticking) accumulated += Math.max(0, now - resumed);
        ticking = next; resumed = now;
    }
    void refreshInputs() { updateRunning(); }
    private void updateRunning() {
        if (renderer == null || closed) return;
        visible = isAttachedToWindow() && isShown() && getWindowVisibility() == VISIBLE && isAvailable();
        boolean next = visible && failedSpec!=specification && (composition==null||capturedTextures.isEmpty()||composition.active()) && !specification.optBoolean("paused")
            && specification.optBoolean("animated", true) && specification.isNull("time_seconds");
        updateClock(next); removeCallbacks(frame);
        if (next) post(frame); else if (visible) requestFrame();
    }
    void requestFrame() {
        if (closed || !visible || failedSpec==specification || composition!=null&&!capturedTextures.isEmpty()&&!composition.active()) return;
        JSONObject current = specification; Texture[] inputs = textureSnapshot;
        float progress=current.isNull("progress")?1:(float)current.optDouble("progress",1);
        if(composition!=null&&!capturedTextures.isEmpty()) {
            try { java.util.Map<String,Bitmap> bitmaps=composition.capture();
                for(Texture texture:capturedTextures.values())texture.bitmap=bitmaps.get(texture.name);
                progress=composition.progress();
            }catch(RuntimeException error){report(current,error);return;}
        }
        double seconds = current.isNull("time_seconds") ? (accumulated + (ticking ? Math.max(0, SystemClock.elapsedRealtime()-resumed) : 0))/1000.0 : current.optDouble("time_seconds");
        latestFrame=new RenderInput(current,inputs,(float)seconds,progress);
        if(frameQueued.compareAndSet(false,true)&&!renderer.post(renderFrame))frameQueued.set(false);
    }
    @Override protected void onAttachedToWindow() { super.onAttachedToWindow(); updateRunning(); }
    @Override protected void onDetachedFromWindow() { updateClock(false); visible = false; removeCallbacks(frame); super.onDetachedFromWindow(); }
    @Override protected void onVisibilityChanged(android.view.View changed, int visibility) { super.onVisibilityChanged(changed, visibility); updateRunning(); }
    @Override protected void onWindowVisibilityChanged(int visibility) { super.onWindowVisibilityChanged(visibility); updateRunning(); }
    public void onSurfaceTextureAvailable(SurfaceTexture texture, int w, int h) {
        renderer.post(() -> { destroy(); if (closed) return; target = texture; width = w; height = h;failedSpec=null;
            try { initialize(); post(() -> updateRunning()); } catch (RuntimeException error) { report(specification, error); destroy(); } });
        updateRunning();
    }
    public void onSurfaceTextureSizeChanged(SurfaceTexture texture, int w, int h) {
        renderer.post(() -> { if (target == texture) { width = w; height = h; } }); requestFrame();
    }
    public boolean onSurfaceTextureDestroyed(SurfaceTexture texture) {
        updateClock(false); visible = false; removeCallbacks(frame);
        if(!renderer.post(() -> { if (target == texture) destroy(); texture.release(); }))texture.release(); return false;
    }
    public void onSurfaceTextureUpdated(SurfaceTexture texture) {
        NativeCapture.changed(this);
        JSONObject completed=presentedSpec;if(!closed&&completed==specification&&composition!=null)composition.presented(completed);
    }
    private void initialize() {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY);
        int[] version = new int[2];
        if (!EGL14.eglInitialize(display, version, 0, version, 1)) throw new IllegalStateException("EGL initialization failed");
        EGLConfig[] configs = new EGLConfig[1]; int[] count = new int[1];
        int[] attributes = {EGL14.EGL_RED_SIZE,8,EGL14.EGL_GREEN_SIZE,8,EGL14.EGL_BLUE_SIZE,8,EGL14.EGL_ALPHA_SIZE,8,
            EGL14.EGL_RENDERABLE_TYPE,0x40,EGL14.EGL_SURFACE_TYPE,EGL14.EGL_WINDOW_BIT,EGL14.EGL_NONE};
        if (!EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, count, 0) || count[0] == 0)
            throw new IllegalStateException("OpenGL ES 3 configuration unavailable");
        context = EGL14.eglCreateContext(display, configs[0], EGL14.EGL_NO_CONTEXT, new int[]{EGL14.EGL_CONTEXT_CLIENT_VERSION,3,EGL14.EGL_NONE},0);
        surface = EGL14.eglCreateWindowSurface(display, configs[0], target, new int[]{EGL14.EGL_NONE},0);
        if (!current()) throw new IllegalStateException("EGL surface unavailable");
    }
    private boolean current() {
        return display != EGL14.EGL_NO_DISPLAY && context != EGL14.EGL_NO_CONTEXT && surface != EGL14.EGL_NO_SURFACE
            && EGL14.eglMakeCurrent(display, surface, surface, context);
    }
    private static int compile(int type, String source) {
        int shader = GLES20.glCreateShader(type); GLES20.glShaderSource(shader, source); GLES20.glCompileShader(shader);
        int[] status = new int[1]; GLES20.glGetShaderiv(shader,GLES20.GL_COMPILE_STATUS,status,0);
        if (status[0] == 0) { String error = GLES20.glGetShaderInfoLog(shader); GLES20.glDeleteShader(shader); throw new IllegalArgumentException(error); }
        return shader;
    }
    private static String declaration(String name, JSONObject parameter) {
        String type = parameter.optString("type");
        if (type.equals("float") || type.equals("int") || type.equals("uint") || type.equals("mat4")) return "uniform "+type+" "+name+";\n";
        if (type.equals("vec4_array")) return "uniform vec4 "+name+"["+parameter.optJSONArray("value").length()+"];\n";
        return "uniform "+type+" "+name+";\n";
    }
    private static String inputKey(JSONObject spec) {
        StringBuilder key=new StringBuilder(spec.optString("language")).append(':').append(spec.optString("function")).append(':').append(spec.optString("source"));
        JSONObject parameters=spec.optJSONObject("parameters"),images=spec.optJSONObject("textures");
        TreeSet<String> names=new TreeSet<>();if(parameters!=null)for(Iterator<String> it=parameters.keys();it.hasNext();)names.add(it.next());
        for(String name:names){JSONObject parameter=parameters.optJSONObject(name);String type=parameter.optString("type");key.append('|').append(name).append(':').append(type);if(type.equals("vec4_array"))key.append(':').append(parameter.optJSONArray("value").length());}
        names.clear();if(images!=null)for(Iterator<String> it=images.keys();it.hasNext();)names.add(it.next());
        for(String name:names)key.append("|texture:").append(name);
        JSONObject mode=spec.optJSONObject("composition");key.append("|composition:").append(mode==null?"background":mode.optString("type"));return key.toString();
    }
    private String fragment(JSONObject spec, Texture[] inputs) {
        if(spec.optString("language").equals("wgsl")) {
            String key=inputKey(spec);
            if(!key.equals(preparedKey)) {
                preparedKey=key;translated=null;programReady=false;
                try {Object result=NativeRuntime.call(NativeRuntime.object("compileShader").put("specification",spec));
                    if(!(result instanceof JSONObject))throw new IllegalArgumentException("Invalid shader compilation response");translated=(JSONObject)result;
                }catch(JSONException error){throw new IllegalArgumentException(error);}
            }
            return translated==null?null:translated.optString("source");
        }
        translated=null;preparedKey="";
        String code = spec.optString("source");
        if (code.contains("void main(")) return code;
        StringBuilder source = new StringBuilder("#version 300 es\nprecision highp float;precision highp int;out vec4 jadix_color;\n#define gl_FragColor jadix_color\nuniform vec2 uResolution;uniform float uTime;uniform float uScale;uniform float uValues[256];\n");
        JSONObject parameters = spec.optJSONObject("parameters");
        if (parameters != null) for (Iterator<String> names = parameters.keys(); names.hasNext();) {
            String name = names.next(); source.append(declaration(name, parameters.optJSONObject(name)));
        }
        for (Texture texture : inputs) source.append("uniform sampler2D ").append(texture.name).append(";\n");
        return source.append(code).append("\nvoid main(){vec4 c=").append(spec.optString("function")).append("(gl_FragCoord.xy);jadix_color=vec4(c.rgb*c.a,c.a);}").toString();
    }
    private void draw(RenderInput input) {
        JSONObject spec=input.spec;Texture[] inputs=input.textures;float seconds=input.seconds;
        if (closed || specification != spec || width <= 0 || height <= 0 || target==null) return;
        if(!current())throw new IllegalStateException("EGL context lost");
        String fragment = fragment(spec, inputs);
        if(fragment==null)return;
        if (!compiled.equals(fragment)) {
            compiled = fragment; programReady=false; int vertex=0, pixel=0, candidate=0;
            try {
                boolean modern = fragment.startsWith("#version 300");
                vertex=compile(GLES20.GL_VERTEX_SHADER,modern ? "#version 300 es\nin vec2 aPosition;void main(){gl_Position=vec4(aPosition,0.,1.);}" : "attribute vec2 aPosition;void main(){gl_Position=vec4(aPosition,0.,1.);}");
                pixel=compile(GLES20.GL_FRAGMENT_SHADER,fragment); candidate=GLES20.glCreateProgram();
                GLES20.glAttachShader(candidate,vertex); GLES20.glAttachShader(candidate,pixel); GLES20.glLinkProgram(candidate);
                int[] status=new int[1]; GLES20.glGetProgramiv(candidate,GLES20.GL_LINK_STATUS,status,0);
                if(status[0]==0) throw new IllegalArgumentException(GLES20.glGetProgramInfoLog(candidate));
                if(program!=0)GLES20.glDeleteProgram(program);program=candidate;candidate=0;programReady=true;
            } catch (RuntimeException error) { report(spec,error); }
            finally { if(vertex!=0)GLES20.glDeleteShader(vertex);if(pixel!=0)GLES20.glDeleteShader(pixel);if(candidate!=0)GLES20.glDeleteProgram(candidate); }
        }
        if(!programReady)return;
        GLES20.glViewport(0,0,width,height); GLES20.glClearColor(0,0,0,0);GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT);
        if(program!=0) {
            GLES20.glUseProgram(program);int attribute=GLES20.glGetAttribLocation(program,"aPosition");
            if(attribute>=0){GLES20.glEnableVertexAttribArray(attribute);GLES20.glVertexAttribPointer(attribute,2,GLES20.GL_FLOAT,false,0,vertices);}
            GLES20.glUniform2f(GLES20.glGetUniformLocation(program,"uResolution"),width,height);
            GLES20.glUniform1f(GLES20.glGetUniformLocation(program,"uScale"),getResources().getDisplayMetrics().density);
            GLES20.glUniform1f(GLES20.glGetUniformLocation(program,"uTime"),seconds);
            JSONArray values=spec.optJSONArray("uniforms");
            // Clear removed array entries too; optimized-out locations are harmless (-1).
            for(int i=0;i<256;i++)GLES20.glUniform1f(GLES20.glGetUniformLocation(program,"uValues["+i+"]"),values==null?0:(float)values.optDouble(i,0));
            JSONObject parameters=spec.optJSONObject("parameters");
            if(parameters!=null)for(Iterator<String> names=parameters.keys();names.hasNext();){String name=names.next();uniform(name,parameters.optJSONObject(name));}
            if(translated!=null)bindBlocks(spec,seconds,input.progress);
            for(int i=0;i<inputs.length;i++)bind(inputs[i],input.bitmaps[i],i);
            GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP,0,4);if(attribute>=0)GLES20.glDisableVertexAttribArray(attribute);
        }
        if (!EGL14.eglSwapBuffers(display,surface)) { report(spec,new IllegalStateException("EGL swap failed")); destroy(); }
        else presentedSpec=spec;
    }
    private void bindBlocks(JSONObject spec,float seconds,float progress) {
        JSONArray blocks=translated.optJSONArray("blocks");JSONObject parameters=spec.optJSONObject("parameters");
        for(int i=0;blocks!=null&&i<blocks.length()&&i<2;i++) {
            JSONObject block=blocks.optJSONObject(i),offsets=block.optJSONObject("offsets");int size=block.optInt("size");
            if(size<=0||size>16384)throw new IllegalArgumentException("Invalid shader input block");
            int index=GLES30.glGetUniformBlockIndex(program,block.optString("name"));if(index==GLES30.GL_INVALID_INDEX)continue;
            ByteBuffer bytes=uniformBytes[i];
            if(bytes==null||bytes.capacity()!=size)uniformBytes[i]=bytes=ByteBuffer.allocateDirect(size).order(ByteOrder.nativeOrder());
            bytes.clear();for(int j=0;j<size;j++)bytes.put(j,(byte)0);
            if(block.optBoolean("builtin")) {
                float scale=getResources().getDisplayMetrics().density;
                bytes.putFloat(offsets.optInt("size"),width/scale);bytes.putFloat(offsets.optInt("size")+4,height/scale);
                bytes.putFloat(offsets.optInt("time"),seconds);bytes.putFloat(offsets.optInt("scale"),scale);
                bytes.putFloat(offsets.optInt("progress"),progress);
                bytes.putFloat(offsets.optInt("overflow"),(float)spec.optDouble("max_sample_offset",0));
                bytes.putFloat(offsets.optInt("pixels"),width);bytes.putFloat(offsets.optInt("pixels")+4,height);
            } else if(parameters!=null)for(Iterator<String> names=offsets.keys();names.hasNext();) {
                String name=names.next();JSONObject parameter=parameters.optJSONObject(name);if(parameter==null)continue;
                int offset=offsets.optInt(name);String type=parameter.optString("type");
                if(type.equals("int")||type.equals("uint")){bytes.putInt(offset,(int)parameter.optLong("value"));continue;}
                if(type.equals("float")){bytes.putFloat(offset,(float)parameter.optDouble("value"));continue;}
                JSONArray array=parameter.optJSONArray("value");if(array==null)continue;
                int count=type.equals("vec4_array")?array.length()*4:array.length();
                for(int j=0;j<count;j++)bytes.putFloat(offset+j*4,(float)(type.equals("vec4_array")?array.optJSONArray(j/4).optDouble(j%4):array.optDouble(j)));
            }
            if(uniformBuffers[i]==0){int[] handle=new int[1];GLES30.glGenBuffers(1,handle,0);uniformBuffers[i]=handle[0];}
            bytes.position(0);GLES30.glBindBuffer(GLES30.GL_UNIFORM_BUFFER,uniformBuffers[i]);
            GLES30.glBufferData(GLES30.GL_UNIFORM_BUFFER,size,bytes,GLES30.GL_DYNAMIC_DRAW);
            GLES30.glUniformBlockBinding(program,index,i);GLES30.glBindBufferBase(GLES30.GL_UNIFORM_BUFFER,i,uniformBuffers[i]);
        }
    }
    private void uniform(String name, JSONObject parameter) {
        int location=GLES20.glGetUniformLocation(program,name);if(location<0)return;
        String type=parameter.optString("type");
        if(type.equals("float")){GLES20.glUniform1f(location,(float)parameter.optDouble("value"));return;}
        if(type.equals("int")){GLES20.glUniform1i(location,parameter.optInt("value"));return;}
        if(type.equals("uint")){GLES30.glUniform1ui(location,(int)parameter.optLong("value"));return;}
        JSONArray array=parameter.optJSONArray("value");if(array==null)return;
        float[] data=new float[type.equals("vec4_array")?array.length()*4:array.length()];
        for(int i=0;i<data.length;i++)data[i]=(float)(type.equals("vec4_array")?array.optJSONArray(i/4).optDouble(i%4):array.optDouble(i));
        switch(type){case "vec2":GLES20.glUniform2fv(location,1,data,0);break;case "vec3":GLES20.glUniform3fv(location,1,data,0);break;
            case "vec4":GLES20.glUniform4fv(location,1,data,0);break;case "mat4":GLES20.glUniformMatrix4fv(location,1,false,data,0);break;
            case "vec4_array":GLES20.glUniform4fv(location,array.length(),data,0);break;}
    }
    private void bind(Texture texture,Bitmap bitmap,int slot) {
        if(texture.handle==0){int[] handle=new int[1];GLES20.glGenTextures(1,handle,0);texture.handle=handle[0];}
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0+slot);GLES20.glBindTexture(GLES20.GL_TEXTURE_2D,texture.handle);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_MIN_FILTER,GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_MAG_FILTER,GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_WRAP_S,GLES20.GL_CLAMP_TO_EDGE);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_WRAP_T,GLES20.GL_CLAMP_TO_EDGE);
        if(bitmap!=null&&bitmap!=texture.uploaded){GLUtils.texImage2D(GLES20.GL_TEXTURE_2D,0,bitmap,0);texture.uploaded=bitmap;}
        else if(bitmap==null&&texture.uploaded==null){ByteBuffer white=ByteBuffer.allocateDirect(4);white.putInt(texture.source.isEmpty()?0:-1).position(0);GLES20.glTexImage2D(GLES20.GL_TEXTURE_2D,0,GLES20.GL_RGBA,1,1,0,GLES20.GL_RGBA,GLES20.GL_UNSIGNED_BYTE,white);}
        GLES20.glUniform1i(GLES20.glGetUniformLocation(program,translated==null?texture.name:translated.optJSONObject("textures").optString(texture.name)),slot);
    }
    private void report(JSONObject spec,RuntimeException error){if(failedSpec==spec)return;failedSpec=spec;String message=String.valueOf(error.getMessage());if(message.length()>4096)message=message.substring(0,4096);final String text=message;post(()->{if(!closed&&specification==spec){presentedSpec=null;updateRunning();if(composition!=null)composition.failed(spec);events.emit("error",text);}});}
    private void destroy() {
        if(current()) { GLES30.glDeleteBuffers(2,uniformBuffers,0);uniformBuffers[0]=uniformBuffers[1]=0;if(program!=0)GLES20.glDeleteProgram(program);for(Texture texture:textureSnapshot){if(texture.handle!=0)GLES20.glDeleteTextures(1,new int[]{texture.handle},0);texture.handle=0;texture.uploaded=null;} }
        presentedSpec=null;program=0;compiled="";preparedKey="";translated=null;programReady=false;
        if(display!=EGL14.EGL_NO_DISPLAY){EGL14.eglMakeCurrent(display,EGL14.EGL_NO_SURFACE,EGL14.EGL_NO_SURFACE,EGL14.EGL_NO_CONTEXT);
            if(surface!=EGL14.EGL_NO_SURFACE)EGL14.eglDestroySurface(display,surface);if(context!=EGL14.EGL_NO_CONTEXT)EGL14.eglDestroyContext(display,context);EGL14.eglTerminate(display);}
        display=EGL14.EGL_NO_DISPLAY;context=EGL14.EGL_NO_CONTEXT;surface=EGL14.EGL_NO_SURFACE;target=null;
    }
    @Override public void close(){if(closed)return;closed=true;updateClock(false);removeCallbacks(frame);for(Texture texture:textures.values())try{if(texture.request!=null)texture.request.close();}catch(Exception ignored){}
        renderer.post(()->{destroy();thread.quitSafely();});textures.clear();capturedTextures.clear();composition=null;latestFrame=null;}
}
