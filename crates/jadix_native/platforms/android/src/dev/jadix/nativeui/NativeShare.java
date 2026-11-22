package dev.jadix.nativeui;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.ClipData;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.UUID;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Adapts the shared share lifecycle to Android's system chooser. */
public final class NativeShare implements AutoCloseable {
    private static final HashMap<String, WeakReference<NativeShare>> ACTIVE = new HashMap<>();
    private final NativeServices services;
    private final String token = UUID.randomUUID().toString();
    private String id;
    private PendingIntent selected;
    private NativePermissions bridge;
    private int request;
    NativeShare(NativeServices services) { this.services = services; ACTIVE.put(token, new WeakReference<>(this)); }

    void command(JSONObject command) throws JSONException {
        finish(NativeServices.object("status", "replaced"), null);
        id = command.getString("id");
        try {
            bridge = services.activityBridge();
            if (bridge == null) throw new IllegalStateException("Native activity unavailable");
            JSONArray files = command.getJSONArray("files");
            if (files.length() > 64) throw new IllegalArgumentException("Share attachment budget exceeded");
            ArrayList<Uri> attachments = new ArrayList<>();
            String mime = null;
            for (int i = 0; i < files.length(); i++) {
                Uri uri = NativeShareProvider.grant(services.root, token, files.getString(i));
                attachments.add(uri);
                String next = services.root.getContext().getContentResolver().getType(uri);
                mime = mime == null ? next : mime.equals(next) ? mime : "*/*";
            }
            Intent send = new Intent(attachments.size() > 1 ? Intent.ACTION_SEND_MULTIPLE : Intent.ACTION_SEND);
            send.setType(mime == null ? "text/plain" : mime);
            if (!command.isNull("subject")) send.putExtra(Intent.EXTRA_SUBJECT, command.getString("subject"));
            String text = command.isNull("text") ? "" : command.getString("text");
            if (!command.isNull("url")) text += (text.isEmpty() ? "" : "\n") + command.getString("url");
            if (!text.isEmpty()) send.putExtra(Intent.EXTRA_TEXT, text);
            if (!attachments.isEmpty()) {
                if (attachments.size() == 1) send.putExtra(Intent.EXTRA_STREAM, attachments.get(0));
                else send.putParcelableArrayListExtra(Intent.EXTRA_STREAM, attachments);
                ClipData clip = ClipData.newUri(services.root.getContext().getContentResolver(), "Attachments", attachments.get(0));
                for (int i = 1; i < attachments.size(); i++) clip.addItem(new ClipData.Item(attachments.get(i)));
                send.setClipData(clip); send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            }
            Context context = services.root.getContext();
            Intent callback = new Intent(context, ChosenReceiver.class).setData(new Uri.Builder().scheme("jadix-share").authority(token).appendPath(id).build());
            int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
            selected = PendingIntent.getBroadcast(context, 0, callback, flags);
            String launched = id;
            request = bridge.launch(Intent.createChooser(send, null, selected.getIntentSender()), result ->
                services.main.postDelayed(() -> { if (launched.equals(id)) finish(NativeServices.object("status", "dismissed"), null); }, 1000));
        } catch (RuntimeException | java.io.IOException | JSONException error) {
            finish(null, "Android share failed: " + error.getClass().getSimpleName());
        }
    }
    private void finish(JSONObject outcome, String error) {
        if (id == null) return;
        String previous = id; id = null;
        if (bridge != null) bridge.cancelActivity(request);
        if (selected != null) { selected.cancel(); selected = null; }
        services.shareResult(previous, outcome, error);
    }
    public void close() {
        finish(NativeServices.object("status", "dismissed"), null);
        ACTIVE.remove(token); NativeShareProvider.revoke(services.root.getContext(), token);
    }
    public static final class ChosenReceiver extends BroadcastReceiver {
        @Override public void onReceive(Context context, Intent intent) {
            Uri uri = intent.getData(); if (uri == null) return;
            WeakReference<NativeShare> reference = ACTIVE.get(uri.getHost());
            NativeShare share = reference == null ? null : reference.get();
            if (share == null || share.id == null || !share.id.equals(uri.getLastPathSegment())) return;
            ComponentName component = intent.getParcelableExtra(Intent.EXTRA_CHOSEN_COMPONENT);
            if (component != null) share.finish(NativeServices.object("status", "shared", "service", component.flattenToString()), null);
        }
    }
}
