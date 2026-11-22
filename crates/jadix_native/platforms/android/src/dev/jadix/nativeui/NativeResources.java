package dev.jadix.nativeui;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Typeface;
import android.net.Uri;
import android.util.Base64;
import android.util.LruCache;
import java.io.*;
import java.net.HttpURLConnection;
import java.net.URL;
import java.security.MessageDigest;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.Consumer;

/** Root-owned bounded resource work; completion always returns to the UI thread. */
final class NativeResources implements AutoCloseable {
    static final int MAX_BYTES = 8 * 1024 * 1024;
    private final NativeView root;
    private final ThreadPoolExecutor worker = new ThreadPoolExecutor(2, 4, 30, TimeUnit.SECONDS,
        new ArrayBlockingQueue<>(32), task -> { Thread thread = new Thread(task, "jadix-resources"); thread.setDaemon(true); return thread; });
    private final LruCache<String, Cached> cache = new LruCache<String, Cached>(MAX_BYTES) {
        protected int sizeOf(String key, Cached image) { return image.asset.size(); }
    };
    private static final class Cached {
        final ImageAsset asset; final long expires;
        Cached(ImageAsset asset, long age) { this.asset=asset; expires=android.os.SystemClock.elapsedRealtime()+age; }
    }
    private static final class ImageAsset {
        final Bitmap bitmap; final byte[] animation;
        ImageAsset(Bitmap bitmap, byte[] animation) { this.bitmap=bitmap; this.animation=animation; }
        int size() { return bitmap.getAllocationByteCount()+(animation==null?0:animation.length); }
    }
    private static final class ImageJob {
        final ArrayList<Subscription> waiting = new ArrayList<>();
        volatile boolean cancelled; volatile HttpURLConnection connection; volatile InputStream stream;
        FutureTask<Void> task; boolean started;
        void cancel() {
            cancelled=true;
            if(task!=null) task.cancel(true);
            HttpURLConnection active=connection; if(active!=null) active.disconnect();
            InputStream input=stream; if(input!=null) try { input.close(); } catch(IOException ignored) {}
        }
    }
    private static final class Subscription implements AutoCloseable {
        final Consumer<ImageAsset> completion; volatile boolean cancelled; Runnable release;
        Subscription(Consumer<ImageAsset> completion) { this.completion=completion; }
        void deliver(ImageAsset asset) { if(!cancelled) { cancelled=true; completion.accept(asset); } }
        public void close() { if(!cancelled) { cancelled=true; if(release!=null) release.run(); } }
    }
    private final LinkedHashMap<String, ImageJob> pending = new LinkedHashMap<>();
    private final ThreadLocal<ImageJob> decoding = new ThreadLocal<>();
    private int imageBindings=32, imagePending=4, imageSourceBytes=768*1024-24, imageEntries=32, subscribers;
    private long imageCacheAge=30000;
    private final HashMap<String, Typeface> fontBytes = new HashMap<>(), fonts = new HashMap<>();
    private final ConcurrentHashMap<String, File> assetRoots = new ConcurrentHashMap<>();
    private long fontByteCount;
    private volatile boolean closed;
    NativeResources(NativeView root) { this.root = root; }
    NativeResources reopen() {
        NativeResources replacement = new NativeResources(root); replacement.assetRoots.putAll(assetRoots);
        replacement.imageBindings=imageBindings; replacement.imagePending=imagePending; replacement.imageSourceBytes=imageSourceBytes;
        replacement.imageEntries=imageEntries; replacement.imageCacheAge=imageCacheAge; replacement.cache.resize(cache.maxSize()); return replacement;
    }
    void configureImages(org.json.JSONObject limits) throws org.json.JSONException {
        ArrayList<ImageJob> cancelled=new ArrayList<>(pending.values());
        pending.clear(); subscribers=0; cache.evictAll();
        imageBindings=limits.getInt("bindings"); imagePending=limits.getInt("pending"); imageSourceBytes=limits.getInt("sourceBytes");
        imageEntries=limits.getInt("cacheEntries"); imageCacheAge=limits.getLong("cacheAgeMs"); cache.resize(Math.max(1,limits.getInt("cacheBytes")));
        if(limits.getInt("cacheBytes")==0) imageEntries=0;
        for(ImageJob job:cancelled) job.cancel();
        for(ImageJob job:cancelled) for(Subscription subscription:job.waiting) subscription.deliver(null);
    }

    void registerAssetRoot(String name, File directory) throws IOException {
        if (!name.matches("[A-Za-z0-9_-]{1,64}") || !directory.isDirectory()) throw new IllegalArgumentException("Invalid asset root");
        assetRoots.put(name, directory.getCanonicalFile());
    }
    Typeface font(String family, int style) {
        for (String candidate : family.split(",")) {
            String name = candidate.replace("\"", "").replace("'", "").trim();
            Typeface registered = fonts.get(name);
            if (registered != null) return Typeface.create(registered, style);
        }
        String first = family.split(",")[0].replace("\"", "").replace("'", "").trim();
        return Typeface.create("system-ui".equals(first) ? "sans-serif" : first, style);
    }
    void registerFont(String family, byte[] bytes) throws Exception {
        if (closed || family == null || family.isEmpty() || family.length() > 128 || bytes == null || bytes.length == 0 || bytes.length > 16 * 1024 * 1024)
            throw new IllegalArgumentException("Invalid native font");
        String digest = Base64.encodeToString(MessageDigest.getInstance("SHA-256").digest(bytes), Base64.NO_WRAP);
        Typeface face = fontBytes.get(digest);
        if (face == null) {
            if (fontBytes.size() >= 64 || fontByteCount + bytes.length > 32 * 1024 * 1024) throw new IllegalStateException("Native font budget exceeded");
            File temporary = File.createTempFile("jadix-font-", ".font", root.getContext().getCacheDir());
            try {
                try (OutputStream output = new FileOutputStream(temporary)) { output.write(bytes); }
                face = new Typeface.Builder(temporary).build();
                if (face == null) throw new IllegalArgumentException("Invalid font data");
                fontBytes.put(digest, face);
                fontByteCount += bytes.length;
            } finally { temporary.delete(); }
        }
        if (fonts.size() >= 128 && !fonts.containsKey(family)) throw new IllegalStateException("Native font family budget exceeded");
        if (fonts.put(family, face) != face) root.resourcesChanged();
    }

    File local(Uri uri) throws IOException {
        File base;
        String relative;
        if ("asset".equals(uri.getScheme()) || "app".equals(uri.getScheme())) {
            base = assetRoots.get(uri.getHost());
            if (base == null) throw new IOException("Unknown asset root");
            relative = uri.getPath();
            while (relative != null && relative.startsWith("/")) relative = relative.substring(1);
        } else if ("file".equals(uri.getScheme())) {
            File file = new File(uri.getPath()).getCanonicalFile();
            for (File allowed : new File[] {root.getContext().getFilesDir(), root.getContext().getCacheDir()}) {
                if (file.getPath().startsWith(allowed.getCanonicalPath() + File.separator)) return file;
            }
            for (File allowed : assetRoots.values()) if (file.getPath().startsWith(allowed.getPath() + File.separator)) return file;
            throw new IOException("File is outside registered roots");
        } else throw new IOException("Unsupported local resource");
        if (relative == null || relative.isEmpty()) throw new IOException("Missing resource path");
        File file = new File(base, relative).getCanonicalFile();
        if (!file.getPath().startsWith(base.getCanonicalPath() + File.separator)) throw new IOException("Resource escaped its root");
        return file;
    }
    InputStream open(String source) throws IOException {
        return open(source, uri -> true, MAX_BYTES);
    }
    InputStream open(String source, java.util.function.Predicate<Uri> policy, long maximum) throws IOException {
        if (closed || maximum < 1) throw new IOException("Native resources unavailable");
        Uri uri = Uri.parse(source);
        if ("https".equals(uri.getScheme()) || "http".equals(uri.getScheme())) {
            if (!root.resourceAllowed(uri)) throw new IOException("Resource origin denied");
            // Redirect targets pass the same policy; never forward cookies or credentials.
            for (int redirects = 0; redirects <= 5; redirects++) {
                if (!root.resourceAllowed(uri) || !policy.test(uri)) throw new IOException("Redirect origin denied");
                HttpURLConnection connection = (HttpURLConnection) new URL(uri.toString()).openConnection();
                ImageJob job=decoding.get();
                if(job!=null) { job.connection=connection; if(job.cancelled) { connection.disconnect(); throw new InterruptedIOException(); } }
                connection.setConnectTimeout(10000); connection.setReadTimeout(10000); connection.setInstanceFollowRedirects(false);
                try {
                int status = connection.getResponseCode();
                if (status >= 300 && status < 400) {
                    String location = connection.getHeaderField("Location"); connection.disconnect();
                    if (location == null) throw new IOException("Missing redirect location");
                    uri = Uri.parse(new URL(new URL(uri.toString()), location).toString()); continue;
                }
                if (status < 200 || status >= 300 || connection.getContentLengthLong() > maximum) { connection.disconnect(); throw new IOException("Invalid resource response"); }
                return bounded(new FilterInputStream(connection.getInputStream()) { public void close() throws IOException { try { super.close(); } finally { connection.disconnect(); } } }, maximum);
                } catch (IOException | RuntimeException error) { connection.disconnect(); throw error; }
            }
            throw new IOException("Too many resource redirects");
        }
        if (!policy.test(uri)) throw new IOException("Resource origin denied");
        if ("asset".equals(uri.getScheme()) && uri.getHost() == null) {
            String path = uri.getPath();
            if (path == null || uri.getQuery() != null || uri.getFragment() != null || path.indexOf('\\') >= 0 || path.indexOf('\0') >= 0) throw new IOException("Invalid packaged asset path");
            while (path.startsWith("/")) path = path.substring(1);
            for (String part : path.split("/")) if (part.equals(".") || part.equals("..")) throw new IOException("Invalid packaged asset path");
            return bounded(root.getContext().getAssets().open(path), maximum);
        }
        File file = local(uri);
        if (file.length() > maximum) throw new IOException("Resource exceeds byte budget");
        return bounded(new FileInputStream(file), maximum);
    }
    private InputStream bounded(InputStream input, long maximum) {
        return new FilterInputStream(input) {
            long remaining = maximum;
            private void check(long count) throws IOException {
                if (closed || Thread.currentThread().isInterrupted()) throw new InterruptedIOException("Resource reading interrupted");
                if (count > remaining) throw new IOException("Resource exceeds byte budget");
                if (count > 0) remaining -= count;
            }
            @Override public int read() throws IOException { int value = in.read(); check(value < 0 ? 0 : 1); return value; }
            @Override public int read(byte[] bytes, int offset, int length) throws IOException {
                int count = in.read(bytes, offset, (int)Math.min(length, Math.min(Integer.MAX_VALUE, remaining + 1))); check(count); return count;
            }
            @Override public long skip(long count) throws IOException {
                long skipped = in.skip(Math.min(Math.max(0, count), remaining)); check(skipped); return skipped;
            }
        };
    }
    byte[] bytes(String source) throws IOException { return bytes(source, MAX_BYTES); }
    private byte[] bytes(String source, int maximum) throws IOException {
        if (source.startsWith("data:image/")) {
            int comma = source.indexOf(',');
            if (comma < 0 || comma > 128 || source.length() > maximum * 4 / 3 + 256 || !source.substring(0, comma).endsWith(";base64")) throw new IOException("Invalid image data URL");
            try { byte[] decoded=Base64.decode(source.substring(comma + 1), Base64.DEFAULT); if(decoded.length>maximum) throw new IOException("Image exceeds byte budget"); return decoded; }
            catch (IllegalArgumentException error) { throw new IOException("Invalid image data", error); }
        }
        try (InputStream input = open(source, uri -> true, maximum); ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            ImageJob job=decoding.get(); if(job!=null) { job.stream=input; if(job.cancelled) throw new InterruptedIOException(); }
            byte[] buffer = new byte[16384]; int count;
            while ((count = input.read(buffer)) != -1) {
                if (Thread.currentThread().isInterrupted()) throw new InterruptedIOException();
                if (output.size() + count > maximum) throw new IOException("Resource exceeds byte budget");
                output.write(buffer, 0, count);
            }
            return output.toByteArray();
        }
    }
    private ImageAsset decode(String source) throws IOException {
        byte[] bytes = bytes(source, imageSourceBytes);
        BitmapFactory.Options bounds = new BitmapFactory.Options(); bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(bytes, 0, bytes.length, bounds);
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
            int[] pixels = NativeRuntime.rasterizeSvg(bytes);
            if (pixels == null) throw new IOException("Invalid image encoding");
            Bitmap image = Bitmap.createBitmap(pixels, 2, pixels[0], pixels[0], pixels[1], Bitmap.Config.ARGB_8888);
            image.setDensity(android.util.DisplayMetrics.DENSITY_DEFAULT); return new ImageAsset(image,null);
        }
        if (bounds.outWidth > 4096 || bounds.outHeight > 4096 || (long) bounds.outWidth * bounds.outHeight > 4 * 1024 * 1024)
            throw new IOException("Invalid image dimensions");
        Bitmap image = BitmapFactory.decodeByteArray(bytes, 0, bytes.length);
        if (image == null) throw new IOException("Invalid image encoding");
        image.setDensity(android.util.DisplayMetrics.DENSITY_DEFAULT);
        boolean animated=false;
        if(android.os.Build.VERSION.SDK_INT>=28) {
            android.graphics.drawable.Drawable drawable=android.graphics.ImageDecoder.decodeDrawable(android.graphics.ImageDecoder.createSource(java.nio.ByteBuffer.wrap(bytes)),(decoder,info,input)->decoder.setAllocator(android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE));
            animated=drawable instanceof android.graphics.drawable.AnimatedImageDrawable;
        } else {
            android.graphics.Movie movie=android.graphics.Movie.decodeByteArray(bytes,0,bytes.length); animated=movie!=null&&movie.duration()>0;
        }
        return new ImageAsset(image,animated?bytes:null);
    }
    AutoCloseable image(String source, Consumer<Bitmap> completion) {
        return imageAsset(source,asset->completion.accept(asset==null?null:asset.bitmap));
    }
    private AutoCloseable imageAsset(String source, Consumer<ImageAsset> completion) {
        Subscription subscription=new Subscription(completion);
        if (closed) { subscription.close(); return subscription; }
        Cached cached = cache.get(source);
        if(cached!=null && cached.expires<=android.os.SystemClock.elapsedRealtime()) { cache.remove(source); cached=null; }
        if (cached != null) { final ImageAsset asset=cached.asset; root.post(() -> { if (!closed) subscription.deliver(asset); }); return subscription; }
        ImageJob waiting=pending.get(source);
        if(subscribers>=imageBindings) {
            root.post(() -> { if(!closed) subscription.deliver(null); }); return subscription;
        }
        final ImageJob job=waiting==null?new ImageJob():waiting;
        job.waiting.add(subscription); subscribers++;
        subscription.release=() -> root.post(() -> {
            if(pending.get(source)!=job || !job.waiting.remove(subscription)) return;
            subscribers--; if(job.waiting.isEmpty()) { pending.remove(source); job.cancel(); if(job.task!=null) worker.remove(job.task); startImages(); }
        });
        if(waiting!=null) return subscription;
        pending.put(source,job);
        job.task=new FutureTask<>(() -> {
            ImageAsset result = null;
            decoding.set(job);
            try { if(!job.cancelled) result = decode(source); } catch (IOException | RuntimeException error) { /* Delivered as an image error, never a host mutation. */ }
            finally { decoding.remove(); job.stream=null; job.connection=null; }
            final ImageAsset decoded = result;
            root.post(() -> {
                if (closed || job.cancelled || pending.get(source)!=job) return;
                pending.remove(source); subscribers-=job.waiting.size();
                if (decoded != null && imageEntries>0 && imageCacheAge>0 && decoded.size()<=cache.maxSize()) {
                    cache.put(source,new Cached(decoded,imageCacheAge));
                    while(cache.snapshot().size()>imageEntries) cache.remove(cache.snapshot().keySet().iterator().next());
                }
                for (Subscription callback : job.waiting) callback.deliver(decoded);
                job.waiting.clear(); startImages();
            });
            return null;
        });
        startImages();
        return subscription;
    }
    /** Queue distinct sources in subscription order; only pending decodes run at once. */
    private void startImages() {
        if(closed) return;
        int active=0; for(ImageJob job:pending.values()) if(job.started) active++;
        ArrayList<ImageJob> rejected=new ArrayList<>();
        for(Iterator<ImageJob> jobs=pending.values().iterator();jobs.hasNext()&&active<imagePending;) {
            ImageJob job=jobs.next(); if(job.started) continue;
            job.started=true;
            try { worker.execute(job.task); active++; }
            catch(RejectedExecutionException error) { jobs.remove(); subscribers-=job.waiting.size(); rejected.add(job); }
        }
        for(ImageJob job:rejected) root.post(()->{if(!closed)for(Subscription callback:job.waiting)callback.deliver(null);job.waiting.clear();});
    }
    AutoCloseable drawable(String source, Consumer<android.graphics.drawable.Drawable> completion) {
        final class Request implements AutoCloseable {
            volatile boolean cancelled; AutoCloseable subscription; FutureTask<Void> task;
            public void close() { cancelled=true; if(subscription!=null)try{subscription.close();}catch(Exception ignored){} if(task!=null){task.cancel(true);worker.remove(task);} }
        }
        Request request=new Request();
        request.subscription=imageAsset(source,asset->{
            if(request.cancelled || closed)return;
            if(asset==null) { completion.accept(null); return; }
            if(asset.animation==null) { completion.accept(new android.graphics.drawable.BitmapDrawable(root.getResources(),asset.bitmap)); return; }
            request.task=new FutureTask<>(()->{
                android.graphics.drawable.Drawable drawable=null;
                try {
                    if(android.os.Build.VERSION.SDK_INT>=28) drawable=android.graphics.ImageDecoder.decodeDrawable(android.graphics.ImageDecoder.createSource(java.nio.ByteBuffer.wrap(asset.animation)),(decoder,info,input)->decoder.setAllocator(android.graphics.ImageDecoder.ALLOCATOR_SOFTWARE));
                    else { android.graphics.Movie movie=android.graphics.Movie.decodeByteArray(asset.animation,0,asset.animation.length); if(movie!=null)drawable=new NativeAnimatedImage(movie); }
                } catch(IOException|RuntimeException ignored) {}
                final android.graphics.drawable.Drawable result=drawable;
                root.post(()->{if(!closed&&!request.cancelled)completion.accept(result);}); return null;
            });
            try { worker.execute(request.task); } catch(RejectedExecutionException error) { completion.accept(null); }
        });
        return request;
    }
    void execute(Runnable task) { if (closed) throw new RejectedExecutionException("Native resources disposed"); worker.execute(task); }
    public void close() { closed = true; for(ImageJob job:pending.values()) job.cancel(); worker.shutdownNow(); pending.clear(); subscribers=0; cache.evictAll(); fonts.clear(); fontBytes.clear(); fontByteCount = 0; }
}

