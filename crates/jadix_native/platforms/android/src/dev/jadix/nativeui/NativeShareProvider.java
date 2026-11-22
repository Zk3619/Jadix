package dev.jadix.nativeui;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.os.SystemClock;
import android.provider.OpenableColumns;
import android.webkit.MimeTypeMap;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;

/** Expiring read-only attachment grants. Never exposes a filesystem path to recipients. */
public final class NativeShareProvider extends ContentProvider {
    private static final HashMap<Uri, Lease> LEASES = new HashMap<>();
    private static final class Lease {
        final String owner; final File file; final long expires = SystemClock.elapsedRealtime() + 600000;
        Lease(String owner, File file) { this.owner = owner; this.file = file; }
    }
    static synchronized Uri grant(NativeView root, String owner, String path) throws IOException {
        prune(root.getContext(), null);
        if (LEASES.size() >= 256) throw new IOException("Share attachment budget exceeded");
        File file = root.resources.local(Uri.fromFile(new File(path)));
        if (!file.isFile() || !file.canRead()) throw new FileNotFoundException("Share attachment unavailable");
        Uri uri = new Uri.Builder().scheme("content").authority(root.getContext().getPackageName() + ".jadix.share")
            .appendPath(owner).appendPath(UUID.randomUUID().toString()).appendPath(file.getName()).build();
        LEASES.put(uri, new Lease(owner, file)); return uri;
    }
    static synchronized void revoke(Context context, String owner) { prune(context, owner); }
    private static void prune(Context context, String owner) {
        Iterator<Map.Entry<Uri, Lease>> iterator = LEASES.entrySet().iterator();
        while (iterator.hasNext()) {
            Map.Entry<Uri, Lease> entry = iterator.next();
            if (entry.getValue().expires <= SystemClock.elapsedRealtime() || entry.getValue().owner.equals(owner)) {
                context.revokeUriPermission(entry.getKey(), Intent.FLAG_GRANT_READ_URI_PERMISSION); iterator.remove();
            }
        }
    }
    private synchronized File attachment(Uri uri) throws FileNotFoundException {
        synchronized (NativeShareProvider.class) {
            prune(getContext(), null);
            Lease lease = LEASES.get(uri);
            if (lease == null) throw new FileNotFoundException("Share grant expired");
            return lease.file;
        }
    }
    @Override public boolean onCreate() { return true; }
    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) throw new FileNotFoundException("Share grants are read-only");
        return ParcelFileDescriptor.open(attachment(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }
    @Override public String getType(Uri uri) {
        try {
            String name = attachment(uri).getName(); int dot = name.lastIndexOf('.');
            String type = dot < 0 ? null : MimeTypeMap.getSingleton().getMimeTypeFromExtension(name.substring(dot + 1).toLowerCase(Locale.ROOT));
            return type == null ? "application/octet-stream" : type;
        } catch (FileNotFoundException error) { return null; }
    }
    @Override public Cursor query(Uri uri, String[] projection, String selection, String[] arguments, String order) {
        try {
            File file = attachment(uri);
            String[] columns = projection == null ? new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE} : projection;
            if (columns.length > 2) throw new IllegalArgumentException("Unsupported share projection");
            MatrixCursor cursor = new MatrixCursor(columns, 1); Object[] row = new Object[columns.length];
            for (int i = 0; i < columns.length; i++) {
                if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) row[i] = file.getName();
                else if (OpenableColumns.SIZE.equals(columns[i])) row[i] = file.length();
            }
            cursor.addRow(row); return cursor;
        } catch (FileNotFoundException error) { return null; }
    }
    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException("Read-only share provider"); }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] arguments) { throw new UnsupportedOperationException("Read-only share provider"); }
    @Override public int delete(Uri uri, String selection, String[] arguments) { throw new UnsupportedOperationException("Read-only share provider"); }
}
