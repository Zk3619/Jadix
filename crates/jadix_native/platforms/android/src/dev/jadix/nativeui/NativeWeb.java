package dev.jadix.nativeui;

import android.graphics.Bitmap;
import android.net.Uri;
import android.webkit.*;
import org.json.*;
import java.io.ByteArrayInputStream;
import java.util.*;
import java.util.function.Consumer;

/** Native WebView with document-scoped message ports and bounded, awaitable evaluation. */
public final class NativeWeb extends WebView implements AutoCloseable {
    private final NativeView root;
    private final NativeHosted.Events events;
    private volatile Set<String> origins = Collections.emptySet();
    private String content = "";
    private volatile long generation;
    private volatile boolean closed;
    private boolean rendererGone;
    private final Object resourceLock = new Object();
    private final String resourceOwner;
    private WebMessagePort port;
    private final HashMap<Long, Evaluation> evaluations = new HashMap<>();
    private boolean bridgeReady;
    private boolean documentReady;
    private static final class Evaluation {
        final Consumer<String> success, failure;
        Runnable timeout;
        Evaluation(Consumer<String> success, Consumer<String> failure) { this.success = success; this.failure = failure; }
    }
    private long nextEvaluation;

    NativeWeb(NativeView root, String owner, NativeHosted.Events events) {
        super(root.getContext()); this.root = root; this.events = events; this.resourceOwner = owner;
        getSettings().setJavaScriptEnabled(true);
        getSettings().setDomStorageEnabled(true);
        getSettings().setAllowFileAccess(false); getSettings().setAllowContentAccess(false);
        getSettings().setAllowFileAccessFromFileURLs(false); getSettings().setAllowUniversalAccessFromFileURLs(false);
        getSettings().setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
        setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) { return !allowed(request.getUrl()); }
            @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                Uri uri = request.getUrl();
                if ("app".equals(uri.getScheme()) || "asset".equals(uri.getScheme())) {
                    if ("app".equals(uri.getScheme()) && !"bundle".equals(uri.getAuthority())) return applicationResource(request);
                    if (!"GET".equals(request.getMethod()) && !"HEAD".equals(request.getMethod())) return denied();
                    try { String mime = java.net.URLConnection.guessContentTypeFromName(uri.getPath()); return new WebResourceResponse(mime == null ? "application/octet-stream" : mime, "UTF-8", "HEAD".equals(request.getMethod()) ? new ByteArrayInputStream(new byte[0]) : root.resources.open(uri.toString())); }
                    catch (Exception error) { return denied(); }
                }
                if (!allowed(uri)) return denied();
                return null;
            }
            @Override public void onPageStarted(WebView view, String url, Bitmap favicon) {
                if (closed) return;
                invalidateDocument("Web document navigated"); events.emit("loading", true); events.emit("navigation", url);
            }
            @Override public void onPageFinished(WebView view, String url) {
                if (closed || !allowed(Uri.parse(url))) return;
                documentReady = true; root.scheduleWebPump("");
                events.emit("loading", false); connect(url);
            }
            @Override public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) { if (!closed && request.isForMainFrame()) { events.emit("loading", false); events.emit("error", error.getDescription().toString()); } }
            @Override public void onReceivedSslError(WebView view, android.webkit.SslErrorHandler handler, android.net.http.SslError error) { handler.cancel(); if (!closed) events.emit("error", "Web TLS verification failed"); }
            @Override public boolean onRenderProcessGone(WebView view, RenderProcessGoneDetail detail) {
                if (closed) return true;
                rendererGone=true;
                events.emit("loading",false); events.emit("error","Web renderer exited");
                if(getParent() instanceof NativeHosted)((NativeHosted)getParent()).rendererGone(NativeWeb.this);
                else close();
                return true;
            }
        });
        setWebChromeClient(new WebChromeClient() {
            @Override public void onPermissionRequest(PermissionRequest request) { request.deny(); }
            @Override public boolean onCreateWindow(WebView view, boolean dialog, boolean gesture, android.os.Message result) { return false; }
        });
        setDownloadListener((url, userAgent, disposition, mime, size) -> { if (!closed) events.emit("error", "Web downloads require an application handler"); });
    }
    private WebResourceResponse applicationResource(WebResourceRequest request) {
        String session = root.webResourceSession(); long token, id;
        if (session == null) return denied();
        synchronized (resourceLock) {
            if (closed) return denied();
            token = generation;
            id = NativeRuntime.requestWebResource(session, resourceOwner + ":" + token, request.getUrl().toString(), request.getMethod());
        }
        if (id == 0) return denied();
        Object[] reply = NativeRuntime.awaitWebResource(session, id);
        if (reply == null || closed || token != generation) return denied();
        String[] type = ((String)reply[1]).split(";", 2);
        byte[] bytes = "HEAD".equals(request.getMethod()) ? new byte[0] : (byte[])reply[2];
        int status = Integer.parseInt((String)reply[0]);
        return new WebResourceResponse(type[0].trim(), "UTF-8", status, "Application resource", Collections.singletonMap("Content-Type", (String)reply[1]), new ByteArrayInputStream(bytes));
    }
    private static String origin(Uri uri) {
        if (!"https".equals(uri.getScheme()) || uri.getHost() == null || uri.getUserInfo() != null) return "";
        return "https://" + uri.getHost().toLowerCase(Locale.ROOT) + (uri.getPort() < 0 || uri.getPort() == 443 ? "" : ":" + uri.getPort());
    }
    private boolean allowed(Uri uri) {
        String scheme = uri.getScheme();
        return "about".equals(scheme) && "about:blank".equals(uri.toString())
            || "data".equals(scheme) && uri.toString().startsWith("data:text/html")
            || "app".equals(scheme) || "asset".equals(scheme)
            || !origin(uri).isEmpty() && origins.contains(origin(uri));
    }
    private static WebResourceResponse denied() { return new WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden", Collections.emptyMap(), new ByteArrayInputStream(new byte[0])); }
    void configure(JSONObject specification) throws JSONException {
        root.requireUiThread(); if (closed) return;
        String next = specification.toString(); if (next.equals(content)) return;
        HashSet<String> permitted = new HashSet<>(); JSONArray values = specification.getJSONArray("origins");
        for (int i = 0; i < values.length(); i++) { String value = origin(Uri.parse(values.getString(i))); if (!value.isEmpty()) permitted.add(value); }
        if (!specification.isNull("url")) { String value = origin(Uri.parse(specification.getString("url"))); if (!value.isEmpty()) permitted.add(value); }
        String base = specification.isNull("base_url") ? "https://jadix.invalid/" : specification.getString("base_url");
        if (specification.isNull("url")) { String value = origin(Uri.parse(base)); if (!value.isEmpty()) permitted.add(value); }
        origins = Collections.unmodifiableSet(permitted); content = next; invalidateDocument("Web content replaced");
        if (!specification.isNull("url")) loadUrl(specification.getString("url"));
        else loadDataWithBaseURL(base, specification.getString("html"), "text/html", "UTF-8", null);
    }
    private void connect(String url) {
        if (port != null) { port.close(); port = null; }
        Uri uri = Uri.parse(url); String value = origin(uri);
        // Opaque local documents do not get a wildcard bridge. Use a declared HTTPS base URL.
        if (value.isEmpty()) return;
        WebMessagePort[] pair = createWebMessageChannel(); port = pair[0]; long token = generation;
        port.setWebMessageCallback(new WebMessagePort.WebMessageCallback() {
            @Override public void onMessage(WebMessagePort source, WebMessage message) {
                String data = message.getData(); if (closed || token != generation || data == null || data.length() > 8 * 1024 * 1024) return;
                try {
                    JSONObject decoded = new JSONObject(data);
                    switch (decoded.optString("kind")) {
                        case "ready": bridgeReady = true; break;
                        case "evaluation": {
                            long id = decoded.getLong("id");
                            Evaluation pending = takeEvaluation(id); if (pending == null) break;
                            if (!(decoded.opt("value") instanceof String) || !(decoded.opt("ok") instanceof Boolean)) {
                                pending.failure.accept("Invalid web evaluation result"); break;
                            }
                            String value = decoded.getString("value");
                            if (value.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 1024 * 1024) pending.failure.accept("Web evaluation result exceeds budget");
                            else if (decoded.optBoolean("ok")) pending.success.accept(value); else pending.failure.accept(value);
                            break;
                        }
                        case "message": if (data.getBytes(java.nio.charset.StandardCharsets.UTF_8).length <= 16 * 1024) events.emit("message", decoded.opt("value")); break;
                    }
                }
                catch (JSONException error) { events.emit("error", "Invalid web message JSON"); }
            }
        });
        evaluateJavascript("window.addEventListener('message',function jadixPort(e){if(e.data==='jadix-connect'&&e.ports.length){window.removeEventListener('message',jadixPort);const p=e.ports[0];const send=v=>p.postMessage(JSON.stringify(v));window.jadix={postMessage:v=>send({kind:'message',value:v})};window.__jadixEvaluate=(id,script)=>{Promise.resolve().then(()=>(0,eval)(script)).then(v=>{let s=JSON.stringify(v);if(s===undefined)s='null';if(s.length>1048576)throw Error('Web evaluation result exceeds budget');send({kind:'evaluation',id,ok:true,value:s});}).catch(e=>send({kind:'evaluation',id,ok:false,value:String(e).slice(0,4096)}));};send({kind:'ready'});window.dispatchEvent(new Event('jadix-ready'));}});", result -> {
            if (!closed && token == generation) postWebMessage(new WebMessage("jadix-connect", new WebMessagePort[]{pair[1]}), Uri.parse(value)); else pair[1].close();
        });
    }
    public void evaluate(String script, Consumer<String> success, Consumer<String> failure) {
        root.requireUiThread(); Objects.requireNonNull(success); Objects.requireNonNull(failure);
        if (closed || !bridgeReady || script == null || script.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 256 * 1024 || evaluations.size() >= 64 || getUrl() == null || !allowed(Uri.parse(getUrl()))) { failure.accept("Web evaluation unavailable"); return; }
        if (nextEvaluation == 9007199254740991L) { failure.accept("Web evaluation identifier exhausted"); return; }
        long id = ++nextEvaluation; Evaluation pending = new Evaluation(success, failure);
        pending.timeout = () -> { Evaluation expired = takeEvaluation(id); if (expired != null) expired.failure.accept("Web evaluation timed out"); };
        evaluations.put(id, pending); postDelayed(pending.timeout, 10000);
        try { evaluateJavascript("window.__jadixEvaluate(" + id + "," + JSONObject.quote(script) + ")", null); }
        catch (RuntimeException error) { Evaluation failed = takeEvaluation(id); if (failed != null) failed.failure.accept("Web evaluation could not be started"); }
    }
    private Evaluation takeEvaluation(long id) {
        Evaluation pending = evaluations.remove(id);
        if (pending != null) removeCallbacks(pending.timeout);
        return pending;
    }
    boolean ready() { return !closed && documentReady; }
    boolean evaluationReady() { return !closed && bridgeReady; }
    // The shared WebEvaluationSession owns Promise polling, cancellation and deadlines.
    void evaluateRaw(String script, Consumer<String> success, Consumer<String> failure) {
        root.requireUiThread();
        if (!ready()) { failure.accept("Web document is not ready"); return; }
        long token = generation;
        evaluateJavascript(script, value -> {
            if (closed || token != generation) failure.accept("Web document changed");
            else if (value == null || value.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 1024 * 1024 + 4096) failure.accept("Web evaluation result exceeds budget");
            else success.accept(value);
        });
    }
    private void invalidateDocument(String reason) {
        root.scheduleWebPump("");
        synchronized (resourceLock) {
            String session = root.webResourceSession();
            if (session != null) NativeRuntime.cancelWebResources(session, resourceOwner + ":" + generation);
            generation++;
        }
        bridgeReady = false; documentReady = false; if (port != null) port.close(); port = null;
        ArrayList<Evaluation> pending = new ArrayList<>(evaluations.values()); evaluations.clear();
        for (Evaluation evaluation : pending) removeCallbacks(evaluation.timeout);
        // A client callback must not strand the remaining promises or native WebView.
        for (Evaluation evaluation : pending) {
            try { evaluation.failure.accept(reason); }
            catch (RuntimeException error) { android.util.Log.e("JadixWeb", "Evaluation cancellation callback failed", error); }
        }
    }
    @Override public void close() {
        root.requireUiThread();
        if (closed) return; closed = true; invalidateDocument(rendererGone ? "Web renderer exited" : "Web view disposed");
        if(getParent() instanceof android.view.ViewGroup)((android.view.ViewGroup)getParent()).removeView(this);
        if(!rendererGone)stopLoading();
        setWebChromeClient(null); destroy();
    }
}
