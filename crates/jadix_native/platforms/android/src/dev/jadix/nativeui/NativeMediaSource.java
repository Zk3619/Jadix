package dev.jadix.nativeui;

import android.media.MediaDataSource;
import java.io.*;
import java.util.concurrent.FutureTask;

/** Incremental disk-backed media: decoder seeks never retain the encoded movie in memory. */
final class NativeMediaSource extends MediaDataSource {
    private static final long MAX_BYTES = 1024L * 1024 * 1024;
    private final File file;
    private final RandomAccessFile cache;
    private final FutureTask<Void> transfer;
    private InputStream input;
    private long available;
    private boolean ended, closed;
    private IOException failure;

    NativeMediaSource(NativeView root, String source) throws IOException {
        file = File.createTempFile("jadix-media-", ".cache", root.getContext().getCacheDir());
        try { cache = new RandomAccessFile(file, "rw"); }
        catch (IOException error) { file.delete(); throw error; }
        transfer = new FutureTask<>(() -> {
            try (InputStream stream = root.resources.open(source, uri -> true, MAX_BYTES)) {
                synchronized (this) { if (closed) return null; input = stream; }
                byte[] bytes = new byte[64 * 1024];
                for (int count; (count = stream.read(bytes)) != -1;) {
                    synchronized (this) {
                        if (closed || Thread.currentThread().isInterrupted()) return null;
                        if (available + count > MAX_BYTES) throw new IOException("Media exceeds cache budget");
                        cache.seek(available); cache.write(bytes, 0, count); available += count; notifyAll();
                    }
                }
                synchronized (this) { ended = true; notifyAll(); }
            } catch (IOException | RuntimeException error) {
                synchronized (this) { if (!closed) failure = new IOException("Media transfer failed", error); notifyAll(); }
            } finally { synchronized (this) { input = null; } }
            return null;
        });
        try { root.resources.execute(transfer); }
        catch (RuntimeException error) { close(); throw error; }
    }

    @Override public synchronized int readAt(long position, byte[] buffer, int offset, int size) throws IOException {
        if (position < 0 || offset < 0 || size < 0 || offset > buffer.length - size) throw new IOException("Invalid media read");
        if (size == 0) return 0;
        while (!closed && failure == null && !ended && position >= available) {
            try { wait(); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); throw new InterruptedIOException(); }
        }
        if (closed) throw new IOException("Media source closed");
        if (failure != null) throw failure;
        if (position >= available) return -1;
        int count = (int)Math.min((long)size, available - position);
        cache.seek(position); return cache.read(buffer, offset, count);
    }

    @Override public synchronized long getSize() throws IOException {
        if (closed) throw new IOException("Media source closed");
        if (failure != null) throw failure;
        return ended ? available : -1;
    }

    @Override public void close() {
        InputStream stream;
        synchronized (this) {
            if (closed) return;
            closed = true; stream = input; notifyAll();
            try { cache.close(); } catch (IOException ignored) { }
        }
        transfer.cancel(true);
        if (stream != null) try { stream.close(); } catch (IOException ignored) { }
        file.delete();
    }
}
