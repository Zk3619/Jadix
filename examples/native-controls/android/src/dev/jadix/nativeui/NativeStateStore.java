package dev.jadix.nativeui;

import android.content.Context;
import android.util.AtomicFile;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import java.util.HashMap;
import java.util.Map;

final class NativeStateStore implements AutoCloseable {
    static final int MAX_BYTES = 2 * 1024 * 1024;
    static final int MAX_FILES = 16;
    static final long MAX_TOTAL_BYTES = 16L * 1024 * 1024;
    static final long MAX_AGE_MILLIS = 7L * 24 * 60 * 60 * 1000;
    private static final Object LOCK = new Object();
    private static final Map<String, Integer> ACTIVE = new HashMap<>();
    final String id;
    private final AtomicFile file;
    private final File directory;
    private boolean active;

    NativeStateStore(Context context, String savedId) {
        id = savedId == null ? UUID.randomUUID().toString() : savedId;
        if (!UUID.fromString(id).toString().equals(id)) throw new IllegalArgumentException("invalid saved state identity");
        directory = context.getFilesDir();
        file = new AtomicFile(base(id));
    }

    void write(String state) throws IOException {
        byte[] bytes = state.getBytes(StandardCharsets.UTF_8);
        if (bytes.length > MAX_BYTES) throw new IOException("saved state too large");
        synchronized (LOCK) {
            Map<String, Entry> entries = inventory();
            long now = System.currentTimeMillis();
            for (Map.Entry<String, Entry> entry : entries.entrySet()) {
                String identity = entry.getKey();
                long modified = entry.getValue().modified;
                if (!identity.equals(id) && !ACTIVE.containsKey(base(identity).getAbsolutePath())
                        && now > modified && now - modified > MAX_AGE_MILLIS) {
                    deleteFiles(base(identity));
                    for (String suffix : new String[] {"", ".new", ".bak"}) {
                        if (new File(base(identity).getPath() + suffix).exists()) throw new IOException("cannot expire saved state");
                    }
                }
            }
            entries = inventory();
            long total = 0;
            for (Entry entry : entries.values()) {
                if (entry.bytes > MAX_TOTAL_BYTES + MAX_BYTES - total) throw new IOException("saved state cache quota exceeded");
                total += entry.bytes;
            }
            Entry existing = entries.get(id);
            if (entries.size() + (existing == null ? 1 : 0) > MAX_FILES
                    || total - (existing == null ? 0 : existing.bytes) + bytes.length > MAX_TOTAL_BYTES
                    || total + bytes.length > MAX_TOTAL_BYTES + MAX_BYTES) {
                throw new IOException("saved state cache quota exceeded");
            }
            FileOutputStream stream = null;
            try {
                stream = file.startWrite();
                stream.write(bytes);
                file.finishWrite(stream);
                stream = null;
            } finally {
                if (stream != null) file.failWrite(stream);
            }
        }
    }

    String read() throws IOException {
        synchronized (LOCK) {
            verify(base(id));
            verify(new File(base(id).getPath() + ".new"));
            verify(new File(base(id).getPath() + ".bak"));
            try (FileInputStream stream = file.openRead(); ByteArrayOutputStream bytes = new ByteArrayOutputStream()) {
                byte[] buffer = new byte[8192];
                int length;
                while ((length = stream.read(buffer)) != -1) {
                    if (bytes.size() > MAX_BYTES - length) throw new IOException("saved state too large");
                    bytes.write(buffer, 0, length);
                }
                return StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(bytes.toByteArray())).toString();
            }
        }
    }

    void activate() {
        synchronized (LOCK) {
            if (!active) {
                String key = base(id).getAbsolutePath();
                ACTIVE.put(key, ACTIVE.containsKey(key) ? ACTIVE.get(key) + 1 : 1);
                active = true;
            }
        }
    }

    @Override public void close() {
        synchronized (LOCK) {
            if (active) {
                String key = base(id).getAbsolutePath();
                int count = ACTIVE.get(key);
                if (count == 1) ACTIVE.remove(key); else ACTIVE.put(key, count - 1);
                active = false;
            }
        }
    }

    void delete() { synchronized (LOCK) { deleteFiles(base(id)); } }

    private static void deleteFiles(File base) {
        new AtomicFile(base).delete();
        new File(base.getPath() + ".new").delete();
    }

    private File base(String identity) { return new File(directory, "native-state-" + identity + ".json"); }

    private void verify(File candidate) throws IOException {
        if (!directory.getCanonicalFile().equals(candidate.getCanonicalFile().getParentFile())
                || candidate.exists() && !candidate.isFile()) throw new IOException("invalid saved state file");
    }

    private Map<String, Entry> inventory() throws IOException {
        File[] files = directory.listFiles();
        if (files == null) throw new IOException("cannot enumerate saved state cache");
        Map<String, Entry> entries = new HashMap<>();
        for (File candidate : files) {
            String name = candidate.getName();
            if (name.endsWith(".new") || name.endsWith(".bak")) name = name.substring(0, name.length() - 4);
            if (!name.startsWith("native-state-") || !name.endsWith(".json")) continue;
            String identity = name.substring(13, name.length() - 5);
            try { if (!UUID.fromString(identity).toString().equals(identity)) continue; }
            catch (IllegalArgumentException error) { continue; }
            verify(candidate);
            Entry entry = entries.get(identity);
            if (entry == null) { entry = new Entry(); entries.put(identity, entry); }
            long length = candidate.length();
            if (length > Long.MAX_VALUE - entry.bytes) throw new IOException("saved state cache size overflow");
            entry.bytes += length;
            entry.modified = Math.max(entry.modified, candidate.lastModified());
        }
        return entries;
    }

    private static final class Entry { long bytes; long modified; }
}
