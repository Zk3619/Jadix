package dev.jadix.nativeui;

import android.net.Uri;
import java.io.*;
import java.net.*;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Streaming HTTP, including error bodies, cancellation and redirect policy checks. */
final class NativeFetch implements AutoCloseable {
    private final NativeServices services;
    private final ThreadPoolExecutor worker = new ThreadPoolExecutor(4, 4, 30, TimeUnit.SECONDS,
        new ArrayBlockingQueue<>(64), task -> { Thread thread = new Thread(task, "jadix-http"); thread.setDaemon(true); return thread; });
    private final ConcurrentHashMap<Integer, Request> requests = new ConcurrentHashMap<>();
    private final ScheduledThreadPoolExecutor deadlines = new ScheduledThreadPoolExecutor(4, task -> {
        Thread thread = new Thread(task, "jadix-http-deadline"); thread.setDaemon(true); return thread;
    });
    NativeFetch(NativeServices services) { this.services = services; deadlines.setRemoveOnCancelPolicy(true); }
    void command(JSONObject command) throws JSONException {
        int id = command.getInt("id");
        if ("cancel".equals(command.getString("action"))) { Request request = requests.remove(id); if (request != null) request.cancel(); return; }
        Request request = new Request(id, command.getJSONObject("request"));
        if (requests.putIfAbsent(id, request) != null) throw new IllegalArgumentException("Duplicate HTTP request");
        try {
            request.timeout = deadlines.schedule(request::expire, Math.max(0, request.deadline - android.os.SystemClock.elapsedRealtime()), TimeUnit.MILLISECONDS);
            worker.execute(request);
        }
        catch (RejectedExecutionException error) { request.cancel(); requests.remove(id, request); services.event("fetch", id, NativeServices.object("type", "failed", "message", "HTTP worker limit exceeded")); }
    }
    private final class Request implements Runnable {
        final int id; final JSONObject spec;
        volatile boolean cancelled, expired;
        final long deadline;
        volatile ScheduledFuture<?> timeout;
        volatile HttpURLConnection connection;
        volatile Thread thread;
        Request(int id, JSONObject spec) {
            this.id = id; this.spec = spec;
            deadline = android.os.SystemClock.elapsedRealtime() + spec.optLong("timeout_ms", 30000);
        }
        void stopIO() {
            Thread currentThread = thread; if (currentThread != null) currentThread.interrupt();
            HttpURLConnection current = connection; if (current != null) current.disconnect();
        }
        void cancel() { cancelled = true; if (timeout != null) timeout.cancel(false); worker.remove(this); stopIO(); }
        void expire() {
            expired = true;
            if (worker.remove(this)) {
                if (requests.remove(id, this) && !cancelled && !services.closed)
                    services.event("fetch", id, NativeServices.object("type", "failed", "message", "HTTP deadline exceeded"));
            } else stopIO();
        }
        void check(long deadline) throws IOException {
            if (expired) throw new java.net.SocketTimeoutException("HTTP deadline exceeded");
            if (cancelled || services.closed || Thread.currentThread().isInterrupted()) throw new InterruptedIOException("HTTP request cancelled");
            if (android.os.SystemClock.elapsedRealtime() >= deadline) throw new java.net.SocketTimeoutException("HTTP deadline exceeded");
        }
        public void run() {
            thread = Thread.currentThread();
            try {
                URL url = new URL(spec.getString("url"));
                String method = spec.optString("method", "GET"); JSONArray headers = spec.getJSONArray("headers"), body = spec.getJSONArray("body");
                byte[] bytes = new byte[body.length()]; for (int i = 0; i < bytes.length; i++) bytes[i] = (byte) body.getInt(i);
                boolean credentials = true, bodyHeaders = true;
                for (int redirects = 0; ; redirects++) {
                    check(deadline);
                    if (!services.root.resourceAllowed(Uri.parse(url.toString()))) throw new IOException("HTTP origin denied");
                    connection = (HttpURLConnection) url.openConnection();
                    connection.setInstanceFollowRedirects(false);
                    int remaining = (int) Math.max(1, deadline - android.os.SystemClock.elapsedRealtime());
                    connection.setConnectTimeout(remaining); connection.setReadTimeout(remaining); connection.setRequestMethod(method);
                    for (int i = 0; i < headers.length(); i++) {
                        JSONArray pair = headers.getJSONArray(i); String name = pair.getString(0);
                        if ("host".equalsIgnoreCase(name) || "content-length".equalsIgnoreCase(name) || "connection".equalsIgnoreCase(name)) continue;
                        if (!bodyHeaders && ("content-type".equalsIgnoreCase(name) || "content-encoding".equalsIgnoreCase(name)
                            || "content-language".equalsIgnoreCase(name) || "content-location".equalsIgnoreCase(name))) continue;
                        if (!credentials && ("authorization".equalsIgnoreCase(name) || "cookie".equalsIgnoreCase(name) || "proxy-authorization".equalsIgnoreCase(name))) continue;
                        connection.addRequestProperty(name, pair.getString(1));
                    }
                    if (bytes.length != 0 && !"GET".equals(method) && !"HEAD".equals(method)) {
                        connection.setDoOutput(true); connection.setFixedLengthStreamingMode(bytes.length);
                        try (OutputStream output = connection.getOutputStream()) { output.write(bytes); }
                    }
                    int status = connection.getResponseCode(); check(deadline);
                    if (status == 301 || status == 302 || status == 303 || status == 307 || status == 308) {
                        if (redirects >= 5) throw new IOException("HTTP redirect limit exceeded");
                        String location = connection.getHeaderField("Location"); if (location == null) throw new IOException("Missing HTTP redirect location");
                        URL next = new URL(url, location);
                        credentials &= sameOrigin(url, next);
                        if (status == 303 && !"HEAD".equals(method) || (status == 301 || status == 302) && "POST".equals(method)) { method = "GET"; bytes = new byte[0]; bodyHeaders = false; }
                        connection.disconnect(); url = next; continue;
                    }
                    JSONArray responseHeaders = new JSONArray();
                    for (Map.Entry<String, List<String>> entry : connection.getHeaderFields().entrySet()) {
                        if (entry.getKey() != null) for (String value : entry.getValue()) responseHeaders.put(new JSONArray().put(entry.getKey()).put(value));
                    }
                    services.stream(id, NativeServices.object("type", "headers", "status", status, "url", url.toString(), "headers", responseHeaders));
                    long maximum = spec.getLong("max_response_bytes"), total = 0;
                    if (!"HEAD".equals(method) && status != 204 && status != 304) {
                        InputStream source = status >= 400 ? connection.getErrorStream() : connection.getInputStream();
                        if (source != null) try (InputStream input = source) {
                            byte[] chunk = new byte[16384]; int count;
                            while ((count = input.read(chunk)) != -1) {
                                check(deadline); total += count;
                                if (total > maximum) throw new IOException("HTTP response byte limit exceeded");
                                JSONArray data = new JSONArray(); for (int i = 0; i < count; i++) data.put(chunk[i] & 255);
                                services.stream(id, NativeServices.object("type", "chunk", "bytes", data));
                            }
                        }
                    }
                    check(deadline); services.event("fetch", id, NativeServices.object("type", "complete")); break;
                }
            } catch (Exception error) {
                if (!cancelled && !services.closed) services.event("fetch", id, NativeServices.object("type", "failed", "message", expired ? "HTTP deadline exceeded" : error instanceof InterruptedException ? "HTTP request cancelled" : error.toString()));
            } finally { if (timeout != null) timeout.cancel(false); if (connection != null) connection.disconnect(); requests.remove(id, this); Thread.interrupted(); }
        }
    }
    private static boolean sameOrigin(URL left, URL right) {
        return left.getProtocol().equalsIgnoreCase(right.getProtocol()) && left.getHost().equalsIgnoreCase(right.getHost())
            && (left.getPort() < 0 ? left.getDefaultPort() : left.getPort()) == (right.getPort() < 0 ? right.getDefaultPort() : right.getPort());
    }
    public void close() { for (Request request : requests.values()) request.cancel(); requests.clear(); worker.shutdownNow(); deadlines.shutdownNow(); }
}
