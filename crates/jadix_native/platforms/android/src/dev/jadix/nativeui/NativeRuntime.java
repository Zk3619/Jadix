package dev.jadix.nativeui;

import org.json.JSONArray;
import org.json.JSONObject;
import org.json.JSONTokener;
import org.json.JSONException;

/** Shared Rust policy entry point, registered by AndroidHost, not by the application. */
final class NativeRuntime {
    private NativeRuntime() {}
    private static native String policy(String request);
    private static native String terminal(String request);
    static native int[] rasterizeSvg(byte[] bytes);
    static native String performance(String session, double refreshRate, double workSeconds, boolean changed, boolean sample);
    static native int quit(String session, boolean begin);
    static native boolean pumpWeb(NativeView root, String session);
    static native void serviceEvents(String session, String events);
    static native long requestWebResource(String session, String owner, String url, String method);
    static native Object[] awaitWebResource(String session, long request);
    static native void cancelWebResources(String session, String owner);
    static native void pumpServices(NativeView root, String session);
    static native String windowWork(String session);
    static native void windowEvent(String session, String event);
    static native String createWindowRoot(String session, NativeView root, float width, float height, String content);
    static JSONObject terminal(JSONObject request) {
        try { Object value = new JSONTokener(terminal(request.toString())).nextValue(); return value instanceof JSONObject ? (JSONObject)value : null; }
        catch (JSONException error) { throw new IllegalArgumentException("Invalid terminal response", error); }
    }
    static Object call(JSONObject request) {
        try { return new JSONTokener(policy(request.toString())).nextValue(); }
        catch (JSONException error) { throw new IllegalArgumentException("Invalid native policy response", error); }
    }
    static JSONObject object(String operation) {
        try { return new JSONObject().put("operation", operation); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }
    static JSONArray boundaries(String text) {
        try { return (JSONArray) call(object("textBoundaries").put("text", text)); }
        catch (JSONException error) { throw new IllegalStateException(error); }
    }
}
