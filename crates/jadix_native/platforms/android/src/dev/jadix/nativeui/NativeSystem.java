package dev.jadix.nativeui;

import android.Manifest;
import android.app.*;
import android.content.*;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.view.HapticFeedbackConstants;
import java.util.*;
import org.json.*;

/** System services retain root identities; persisted notification clicks also work at cold start. */
final class NativeSystem implements AutoCloseable {
    private final NativeServices services;
    private final NotificationManager notifications;
    private final String tag = "jadix-" + UUID.randomUUID();
    private final HashSet<Integer> owned = new HashSet<>();
    private final String dismissAction;
    private int auth;
    private String callback;
    private NativePermissions authBridge;
    private final BroadcastReceiver dismiss = new BroadcastReceiver() {
        public void onReceive(Context context, Intent intent) {
            if (!dismissAction.equals(intent.getAction()) || !tag.equals(intent.getStringExtra("jadix.notification.root"))) return;
            int id = intent.getIntExtra("jadix.notification.id", 0);
            if (owned.remove(id)) services.event("desktop", id, NativeServices.object("type", "dismiss"));
        }
    };
    NativeSystem(NativeServices services) {
        this.services = services;
        notifications = (NotificationManager) services.root.getContext().getSystemService(Context.NOTIFICATION_SERVICE);
        dismissAction = services.root.getContext().getPackageName() + ".JADIX_NOTIFICATION_DISMISS." + tag;
        Context context = services.root.getContext();
        if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(dismiss, new IntentFilter(dismissAction), Context.RECEIVER_NOT_EXPORTED);
        else context.registerReceiver(dismiss, new IntentFilter(dismissAction));
    }
    private void reply(int id, String error) { services.event("reply", id, NativeServices.object("type", "complete", "error", error)); }
    private void authReply(int id, JSONObject outcome, String error) { services.event("reply", id, NativeServices.object("type", "auth_session", "outcome", outcome, "error", error)); }
    void command(JSONObject command) throws JSONException {
        int id = command.getInt("id");
        try {
            switch (command.getString("action")) {
                case "permission": {
                    if (Build.VERSION.SDK_INT >= 33 && command.optBoolean("request") && services.root.getContext().checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                        services.permission(Manifest.permission.POST_NOTIFICATIONS, (granted, error) -> {
                            String result = error;
                            if (result == null) {
                                try { services.root.getContext().getSharedPreferences("jadix-system", Context.MODE_PRIVATE).edit().putBoolean("notification-requested", true).apply(); }
                                catch (RuntimeException failure) { result = failure instanceof SecurityException ? "denied" : "failed"; }
                            }
                            permission(id, result);
                        });
                    } else permission(id, null);
                    break;
                }
                case "notify": notify(id, command.getJSONObject("notification")); break;
                case "cancel_notification": if (notifications != null) notifications.cancel(tag, id); owned.remove(id); break;
                case "badge": {
                    // Android launchers derive numeric badges from active notifications.
                    if (notifications == null) { reply(id, "unavailable"); break; }
                    String label = command.isNull("label") ? "" : command.getString("label");
                    if (label.isEmpty() || "0".equals(label)) { notifications.cancel(tag, 0); reply(id, null); break; }
                    int count;
                    try { count = Integer.parseInt(label); if (count < 0) throw new NumberFormatException(); }
                    catch (NumberFormatException error) { reply(id, "unavailable"); break; }
                    if (!notifications.areNotificationsEnabled()) { reply(id, "denied"); break; }
                    channel(false);
                    notifications.notify(tag, 0, builder(false).setContentTitle(services.root.getContext().getApplicationInfo().loadLabel(services.root.getContext().getPackageManager())).setNumber(count).setOnlyAlertOnce(true).build());
                    reply(id, null); break;
                }
                case "haptic": {
                    JSONObject feedback = command.getJSONObject("feedback"); int type;
                    switch (feedback.getString("type")) {
                        case "selection": type = HapticFeedbackConstants.CLOCK_TICK; break;
                        case "impact": type = "heavy".equals(feedback.optString("style")) ? HapticFeedbackConstants.LONG_PRESS : HapticFeedbackConstants.VIRTUAL_KEY; break;
                        default: type = Build.VERSION.SDK_INT >= 30 ? ("success".equals(feedback.optString("kind")) ? HapticFeedbackConstants.CONFIRM : HapticFeedbackConstants.REJECT) : HapticFeedbackConstants.LONG_PRESS;
                    }
                    boolean performed = services.root.performHapticFeedback(type);
                    reply(id, performed ? null : "unavailable"); break;
                }
                case "open_auth_session": {
                    JSONObject request = command.getJSONObject("request");
                    Activity activity = NativePermissions.activity(services.root.getContext());
                    String scheme = request.getString("callback_scheme");
                    if (activity == null || activity.isFinishing() || activity.isDestroyed()
                            || activity.getPackageManager().queryIntentActivities(new Intent(Intent.ACTION_VIEW, Uri.parse(scheme + "://callback"))
                                .addCategory(Intent.CATEGORY_BROWSABLE).setPackage(activity.getPackageName()), 0).isEmpty()) {
                        authReply(id, null, "unavailable"); break;
                    }
                    NativePermissions bridge = services.activityBridge();
                    if (bridge == null) { authReply(id, null, "unavailable"); break; }
                    if (auth != 0) { int previous = auth; clearAuth(); authReply(previous, NativeServices.object("status", "cancelled"), null); }
                    auth = id; callback = scheme; authBridge = bridge;
                    bridge.onReturn(() -> { if (auth == id) { clearAuth(); authReply(id, NativeServices.object("status", "cancelled"), null); } });
                    // Custom Tabs is a public Intent protocol; no AndroidX dependency is required.
                    // Ephemeral browsing is a preference honoured by supporting browsers.
                    Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(request.getString("url"))).addCategory(Intent.CATEGORY_BROWSABLE);
                    android.os.Bundle extras = new android.os.Bundle();
                    extras.putBinder("android.support.customtabs.extra.SESSION", null); intent.putExtras(extras);
                    if (request.optBoolean("ephemeral")) intent.putExtra("androidx.browser.customtabs.extra.ENABLE_EPHEMERAL_BROWSING", true);
                    try { services.root.getContext().startActivity(intent); }
                    catch (ActivityNotFoundException error) { clearAuth(); authReply(id, null, "unavailable"); }
                    break;
                }
                case "close_auth_session": if (auth == id) {
                    clearAuth();
                    Activity activity = NativePermissions.activity(services.root.getContext());
                    if (activity != null && !activity.isFinishing() && !activity.isDestroyed()) {
                        try { services.root.getContext().startActivity(new Intent(activity, activity.getClass())
                            .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP)); }
                        catch (ActivityNotFoundException | SecurityException ignored) { /* The session is already closed. */ }
                    }
                } break;
                // Android has no application global keyboard shortcut registration API.
                case "register_shortcut": reply(id, "unavailable"); break;
                case "unregister_shortcut": break;
                default: throw new IllegalArgumentException("Unknown system service command");
            }
        } catch (SecurityException error) {
            if ("open_auth_session".equals(command.optString("action"))) { clearAuth(); authReply(id, null, "denied"); }
            else if ("permission".equals(command.optString("action"))) permission(id, "denied");
            else reply(id, "denied");
        } catch (RuntimeException error) {
            if ("open_auth_session".equals(command.optString("action"))) { clearAuth(); authReply(id, null, "failed"); }
            else if ("permission".equals(command.optString("action"))) permission(id, "failed");
            else reply(id, "failed");
        }
    }
    private void clearAuth() {
        auth = 0; callback = null;
        if (authBridge != null) { authBridge.onReturn(null); authBridge = null; }
    }
    private void permission(int id, String error) {
        String authorization = "unknown";
        try {
            boolean granted = notifications != null && notifications.areNotificationsEnabled()
                && (Build.VERSION.SDK_INT < 33 || services.root.getContext().checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED);
            boolean asked = services.root.getContext().getSharedPreferences("jadix-system", Context.MODE_PRIVATE).getBoolean("notification-requested", false);
            authorization = granted ? "authorized" : Build.VERSION.SDK_INT >= 33 && !asked ? "not_determined" : "denied";
        } catch (RuntimeException queryError) {
            if (error == null) error = queryError instanceof SecurityException ? "denied" : "failed";
        }
        services.event("reply", id, NativeServices.object("type", "permission", "authorization", authorization, "error", error));
    }
    boolean open(String url) {
        Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE);
        if (NativePermissions.activity(services.root.getContext()) == null) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try { services.root.getContext().startActivity(intent); return true; }
        catch (ActivityNotFoundException error) { return false; }
    }
    private void channel(boolean sound) {
        NotificationChannel channel = new NotificationChannel(sound ? "jadix-alerts" : "jadix-quiet", sound ? "Notifications" : "Silent notifications", NotificationManager.IMPORTANCE_DEFAULT);
        if (!sound) { channel.setSound(null, null); channel.enableVibration(false); }
        channel.setShowBadge(true); notifications.createNotificationChannel(channel);
    }
    private Notification.Builder builder(boolean sound) {
        int icon = services.root.getContext().getApplicationInfo().icon;
        return new Notification.Builder(services.root.getContext(), sound ? "jadix-alerts" : "jadix-quiet")
            .setSmallIcon(icon == 0 ? android.R.drawable.ic_dialog_info : icon);
    }
    private void notify(int id, JSONObject notification) throws JSONException {
        if (notifications == null) { reply(id, "unavailable"); return; }
        if (!notifications.areNotificationsEnabled()) { reply(id, "denied"); return; }
        Context context = services.root.getContext();
        Intent launch = context.getPackageManager().getLaunchIntentForPackage(context.getPackageName());
        if (launch == null) { reply(id, "unavailable"); return; }
        launch.setAction(context.getPackageName() + ".JADIX_NOTIFICATION_CLICK." + tag + "." + id);
        launch.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        launch.putExtra("jadix.notification.root", tag).putExtra("jadix.notification.id", id)
            .putExtra("jadix.notification.data", notification.getString("data"));
        Intent deleted = new Intent(dismissAction).setPackage(context.getPackageName()).putExtra("jadix.notification.root", tag).putExtra("jadix.notification.id", id);
        int flags = PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE;
        boolean sound = notification.optBoolean("sound"); channel(sound);
        Notification.Builder value = builder(sound).setContentTitle(notification.getString("title")).setSubText(notification.getString("subtitle"))
            .setContentText(notification.getString("body")).setStyle(new Notification.BigTextStyle().bigText(notification.getString("body")))
            .setAutoCancel(true).setContentIntent(PendingIntent.getActivity(context, id, launch, flags))
            .setDeleteIntent(PendingIntent.getBroadcast(context, id, deleted, flags));
        notifications.notify(tag, id, value.build()); owned.add(id); reply(id, null);
    }
    boolean intent(Intent intent) {
        if (intent == null || services.closed) return false;
        Uri uri = intent.getData();
        if (auth != 0 && uri != null && callback.equalsIgnoreCase(uri.getScheme())) {
            int id = auth; clearAuth();
            authReply(id, NativeServices.object("status", "completed", "url", uri.toString()), null); return true;
        }
        String origin = intent.getStringExtra("jadix.notification.root");
        if (origin == null) return false;
        int id = intent.getIntExtra("jadix.notification.id", 0); String data = intent.getStringExtra("jadix.notification.data");
        if (id == 0 || origin.length() > 128 || data == null || data.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 4096) return false;
        if (tag.equals(origin) && owned.remove(id)) services.event("desktop", id, NativeServices.object("type", "click"));
        services.activation(NativeServices.object("identifier", origin + "." + id, "data", data));
        intent.removeExtra("jadix.notification.root"); intent.removeExtra("jadix.notification.data"); return true;
    }
    public void close() {
        clearAuth();
        if (notifications != null) { for (int id : owned) notifications.cancel(tag, id); notifications.cancel(tag, 0); } owned.clear();
        services.root.getContext().unregisterReceiver(dismiss);
    }
}
