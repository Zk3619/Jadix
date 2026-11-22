package dev.jadix.nativeui;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.media.AudioManager;
import android.os.Build;

/** Stops owned playback when an output disconnect would route it to the speaker. */
final class NativeAudioRoute implements AutoCloseable {
    private final Context context;
    private final BroadcastReceiver receiver;
    private boolean registered;
    NativeAudioRoute(Context context, Runnable disconnected) {
        this.context = context;
        receiver = new BroadcastReceiver() {
            @Override public void onReceive(Context ignored, Intent intent) {
                if (AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(intent.getAction())) disconnected.run();
            }
        };
    }
    void watch() {
        if (registered) return;
        IntentFilter filter = new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY);
        if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
        else context.registerReceiver(receiver, filter);
        registered = true;
    }
    @Override public void close() { if (registered) { registered = false; context.unregisterReceiver(receiver); } }
}
