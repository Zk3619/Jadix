package dev.jadix.nativeui;

import android.content.Context;
import android.util.AtomicFile;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.Objects;

final class NoteDocumentStore {
    static final int MAX_BYTES = 512 * 1024;
    private static final Object LOCK = new Object();
    private final AtomicFile file;
    private final File directory;
    private String observed;
    NoteDocumentStore(Context context) {
        directory = context.getFilesDir();
        file = new AtomicFile(new File(directory, "jadix-notes-v1.json"));
    }
    private void validatePaths() throws IOException {
        File base = file.getBaseFile();
        for (String suffix : new String[] {"", ".new", ".bak"}) {
            File path = new File(base.getPath() + suffix);
            if (!path.getCanonicalFile().getParentFile().equals(directory.getCanonicalFile()) || (path.exists() && !path.isFile()))
                throw new IOException("Invalid notebook storage path");
        }
    }
    private String current() throws IOException {
        validatePaths();
        File base = file.getBaseFile();
        if (!base.exists() && !new File(base.getPath() + ".bak").exists()) return null;
        try (FileInputStream stream = file.openRead(); ByteArrayOutputStream bytes = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[8192];
            int count;
            while ((count = stream.read(buffer)) != -1) {
                if (bytes.size() + count > MAX_BYTES) throw new IOException("Notebook exceeds 512 KiB");
                bytes.write(buffer, 0, count);
            }
            return StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes.toByteArray())).toString();
        }
    }
    String read() throws IOException { synchronized (LOCK) { observed = current(); return observed; } }
    void write(String document, boolean replaceConfirmed) throws IOException {
        byte[] bytes = document.getBytes(StandardCharsets.UTF_8);
        if (bytes.length > MAX_BYTES) throw new IOException("Notebook exceeds 512 KiB");
        synchronized (LOCK) {
            validatePaths();
            if (!replaceConfirmed && !Objects.equals(current(), observed)) throw new IOException("Notebook changed in another window; export and reopen, or confirm Import");
            FileOutputStream stream = null;
            try {
                stream = file.startWrite();
                stream.write(bytes);
                file.finishWrite(stream);
                observed = current();
                if (!document.equals(observed)) throw new IOException("Notebook write verification failed");
            } catch (IOException error) {
                if (stream != null) file.failWrite(stream);
                throw error;
            }
        }
    }
}
