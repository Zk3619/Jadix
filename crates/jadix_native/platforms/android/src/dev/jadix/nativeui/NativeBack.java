package dev.jadix.nativeui;

import android.app.Activity;
import android.app.Dialog;
import android.os.Build;

/** Keeps API 33 dispatcher types out of classes loaded on older Android versions. */
final class NativeBack {
    static Runnable install(Activity activity, Runnable action) {
        return Build.VERSION.SDK_INT >= 33 ? Modern.installActivity(activity, action) : () -> {};
    }
    static Runnable install(Dialog dialog, Runnable action) {
        return Build.VERSION.SDK_INT >= 33 ? Modern.installDialog(dialog, action) : () -> {};
    }
    private static final class Modern {
        static Runnable installActivity(Activity activity, Runnable action) { return install(activity.getOnBackInvokedDispatcher(), action); }
        static Runnable installDialog(Dialog dialog, Runnable action) { return install(dialog.getOnBackInvokedDispatcher(), action); }
        static Runnable install(android.window.OnBackInvokedDispatcher dispatcher, Runnable action) {
            android.window.OnBackInvokedCallback callback = action::run;
            // DEFAULT leaves the IME's dismissal ahead of application navigation.
            dispatcher.registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
            return () -> dispatcher.unregisterOnBackInvokedCallback(callback);
        }
    }
    private NativeBack() {}
}
