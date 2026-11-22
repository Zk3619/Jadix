package dev.jadix.nativeui;

import android.content.Context;
import android.media.*;
import android.net.Uri;
import android.util.Base64;
import java.io.*;
import java.util.*;
import java.util.concurrent.*;
import org.json.*;

/** Shared Sound/AudioPlayer commands, bounded preload and focus-aware streaming players. */
final class NativeAudio implements AutoCloseable {
    private final NativeServices services;
    private final HashMap<Integer, Sound> sounds = new HashMap<>();
    private final HashMap<Integer, Player> players = new HashMap<>();
    private final ThreadPoolExecutor decoder = new ThreadPoolExecutor(1, 1, 30, TimeUnit.SECONDS,
        new ArrayBlockingQueue<>(64), task -> { Thread thread = new Thread(task, "jadix-sound-decoder"); thread.setDaemon(true); return thread; });
    private final AudioManager manager;
    private final NativeAudioRoute route;
    private final AudioFocusRequest focus;
    private boolean focused, transientLoss, closed;
    private float gain = 1;
    private NativeSoundMixer mixer;
    private int decodedBytes;
    private static final class Sound { NativeSoundMixer.PCM pcm; FutureTask<?> work; }
    NativeAudio(NativeServices services) {
        this.services = services; manager = (AudioManager) services.root.getContext().getSystemService(Context.AUDIO_SERVICE);
        focus = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(attributes()).setOnAudioFocusChangeListener(this::focusChanged, services.main).build();
        route = new NativeAudioRoute(services.root.getContext(), () -> {
            if (closed) return;
            transientLoss = false;
            if (mixer != null) mixer.stopAll();
            for (Player player : new ArrayList<>(players.values())) player.pause(true);
            if (manager != null) manager.abandonAudioFocusRequest(focus);
            focused = false;
        });
    }
    private static AudioAttributes attributes() { return new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build(); }
    private void event(int id, String type) { services.event("audio", id, NativeServices.object("type", type)); }
    private void failed(int id, Exception error) { services.event("audio", id, NativeServices.object("type", "failed", "message", error.toString())); }
    private boolean focus() { if (!focused) focused = manager != null && manager.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED; if (focused) route.watch(); return focused; }
    private void focusChanged(int change) {
        if (closed || services.closed) return;
        boolean resume = transientLoss && change == AudioManager.AUDIOFOCUS_GAIN;
        if (change == AudioManager.AUDIOFOCUS_GAIN) { focused = true; transientLoss = false; }
        gain = change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK ? .2f : 1;
        boolean paused = change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT;
        if (change == AudioManager.AUDIOFOCUS_LOSS) { focused = false; transientLoss = false; for (Player player : players.values()) player.wantsPlay = false; }
        if (change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) transientLoss = true;
        if (mixer != null) { if (change == AudioManager.AUDIOFOCUS_LOSS) mixer.stopAll(); mixer.suspend(paused, gain); }
        for (Player player : new ArrayList<>(players.values())) {
            try { player.volume(); if (paused) player.pause(false); else if (resume) player.start(); }
            catch (RuntimeException error) { player.fail(error); }
        }
    }
    void command(JSONObject command) throws JSONException {
        if (closed || services.closed) return;
        int id = command.getInt("id"); String action = command.getString("action");
        try {
            switch (action) {
                case "load": load(id, command.getJSONObject("source")); break;
                case "play": {
                    Sound sound = sounds.get(id);
                    if (sound == null || sound.pcm == null || mixer == null) throw new IllegalStateException("Sound is not loaded");
                    if (!focus()) throw new IllegalStateException("Audio focus denied");
                    mixer.suspend(transientLoss, gain); mixer.play(id, sound.pcm, (float) command.getDouble("volume"), (float) command.getDouble("rate")); break;
                }
                case "stop": if (mixer != null) mixer.stop(id); break;
                case "unload": unload(id); break;
                case "open": {
                    Player previous = players.remove(id); if (previous != null) previous.close();
                    Player player = new Player(id, command.getJSONObject("source"), command.getJSONObject("settings")); players.put(id, player); break;
                }
                case "close": { Player player = players.remove(id); if (player != null) player.close(); break; }
                default: {
                    Player player = players.get(id); if (player == null) return;
                    switch (action) {
                        case "preload": player.prepare(); break;
                        case "resume": player.wantsPlay = true; player.prepare(); player.start(); break;
                        case "pause": player.pause(true); break;
                        case "seek": player.seek = command.getDouble("position"); player.seek(); break;
                        case "set": player.settings = command.getJSONObject("settings"); player.apply(); break;
                        default: throw new IllegalArgumentException("Unknown audio command");
                    }
                }
            }
        } catch (Exception error) { Player player = players.remove(id); if (player != null) player.close(); failed(id, error); }
    }
    private InputStream open(JSONObject source) throws Exception {
        switch (source.getString("type")) {
            case "bytes": return new ByteArrayInputStream(Base64.decode(source.getString("base64"), Base64.DEFAULT));
            case "file": return services.root.resources.open(Uri.fromFile(new File(source.getString("path"))).toString());
            default: return services.root.resources.open(source.getString("url"));
        }
    }
    private File encoded(JSONObject source) throws Exception {
        File file = File.createTempFile("jadix-sound-", ".media", services.root.getContext().getCacheDir()); boolean complete = false;
        try {
            try (InputStream input = open(source); OutputStream output = new FileOutputStream(file)) {
                byte[] chunk = new byte[16384]; int total = 0, count;
                while ((count = input.read(chunk)) != -1) { if (Thread.currentThread().isInterrupted()) throw new InterruptedIOException(); total += count; if (total > 8 * 1024 * 1024) throw new IOException("Sound encoded quota exceeded"); output.write(chunk, 0, count); }
            }
            complete = true; return file;
        } finally { if (!complete) file.delete(); }
    }
    private void load(int id, JSONObject source) {
        unload(id); Sound sound = new Sound(); sounds.put(id, sound);
        sound.work = new FutureTask<Void>(() -> {
            NativeSoundMixer.PCM result = null; Exception failure = null; File file = null;
            try { file = encoded(source); result = NativeSoundMixer.decode(file); }
            catch (Exception error) { failure = error; }
            finally { if (file != null) file.delete(); }
            NativeSoundMixer.PCM pcm = result; Exception error = failure;
            services.main.post(() -> {
                if (services.closed || sounds.get(id) != sound) return;
                try {
                    if (error != null) throw error;
                    if (decodedBytes + pcm.samples.length * 2 > NativeSoundMixer.MAX_BYTES) throw new IOException("Sound decoded root quota exceeded");
                    if (mixer == null) mixer = new NativeSoundMixer(services);
                    sound.pcm = pcm; decodedBytes += pcm.samples.length * 2;
                    services.event("audio", id, NativeServices.object("type", "loaded", "duration", pcm.duration(), "decoded_bytes", pcm.samples.length * 2));
                } catch (Exception failed) { sounds.remove(id); failed(id, failed); }
            }); return null;
        });
        try { decoder.execute(sound.work); } catch (RejectedExecutionException error) { sounds.remove(id); failed(id, error); }
    }
    private void unload(int id) {
        Sound sound = sounds.remove(id); if (sound == null) return;
        if (mixer != null) mixer.stop(id);
        if (sound.work != null) sound.work.cancel(true);
        if (sound.pcm != null) decodedBytes -= sound.pcm.samples.length * 2;
    }
    private final class Player implements AutoCloseable {
        final int id; final JSONObject source;
        JSONObject settings;
        MediaPlayer player;
        NativeMediaSource stream;
        MediaDataSource bytes;
        boolean ready, wantsPlay, closed;
        double seek = -1;
        int buffered;
        final Runnable progress = new Runnable() { public void run() {
            if (!closed && ready) {
                try { double duration = Math.max(0, player.getDuration()) / 1000.0;
                    services.event("audio", id, NativeServices.object("type", "progress", "position", Math.max(0, player.getCurrentPosition()) / 1000.0, "duration", duration, "buffered", duration * buffered / 100));
                    if (wantsPlay) services.main.postDelayed(this, 250);
                } catch (RuntimeException error) { fail(error); }
            }
        } };
        Player(int id, JSONObject source, JSONObject settings) { this.id = id; this.source = source; this.settings = settings; }
        void fail(Exception error) { if (closed) return; close(); players.remove(id); failed(id, error); }
        void prepare() throws Exception {
            if (player != null || closed) return;
            player = new MediaPlayer(); player.setAudioAttributes(attributes());
            if ("bytes".equals(source.getString("type"))) {
                byte[] data = Base64.decode(source.getString("base64"), Base64.DEFAULT);
                if (data.length > 8 * 1024 * 1024) throw new IOException("Audio encoded quota exceeded");
                bytes = new MediaDataSource() {
                    private byte[] content = data;
                    public synchronized int readAt(long position, byte[] buffer, int offset, int size) throws IOException {
                        if (content == null) throw new IOException("Audio source closed");
                        if (position < 0 || offset < 0 || size < 0 || offset > buffer.length - size) throw new IOException("Invalid audio read");
                        if (size == 0) return 0;
                        if (position >= content.length) return -1;
                        int count = (int)Math.min(size, content.length - position); System.arraycopy(content, (int)position, buffer, offset, count); return count;
                    }
                    public synchronized long getSize() throws IOException { if (content == null) throw new IOException("Audio source closed"); return content.length; }
                    public synchronized void close() { content = null; }
                };
                player.setDataSource(bytes);
            }
            else { String url = "file".equals(source.getString("type")) ? Uri.fromFile(new File(source.getString("path"))).toString() : source.getString("url"); stream = new NativeMediaSource(services.root, url); player.setDataSource(stream); }
            player.setOnPreparedListener(value -> { if (closed) return; ready = true; try { apply(); seek(); services.event("audio", id, NativeServices.object("type", "load", "duration", Math.max(0, player.getDuration()) / 1000.0)); start(); } catch (Exception error) { fail(error); } });
            player.setOnBufferingUpdateListener((value, percent) -> { if (!closed) buffered = Math.max(0, Math.min(100, percent)); });
            player.setOnInfoListener((value, what, extra) -> { if (!closed && (what == MediaPlayer.MEDIA_INFO_BUFFERING_START || what == MediaPlayer.MEDIA_INFO_BUFFERING_END)) services.event("audio", id, NativeServices.object("type", "buffer", "buffering", what == MediaPlayer.MEDIA_INFO_BUFFERING_START)); return false; });
            player.setOnCompletionListener(value -> { if (closed) return; wantsPlay = false; event(id, "ended"); progress.run(); });
            player.setOnErrorListener((value, what, extra) -> { fail(new IOException("Audio decoder error " + what + "/" + extra)); return true; });
            player.prepareAsync();
        }
        void volume() { if (player != null) { float volume = settings.optBoolean("muted") ? 0 : (float) settings.optDouble("volume", 1) * gain; player.setVolume(volume, volume); } }
        void apply() { if (closed) return; volume(); if (ready) { player.setLooping(settings.optBoolean("looping")); if (player.isPlaying()) player.setPlaybackParams(player.getPlaybackParams().setSpeed((float) settings.optDouble("rate", 1))); } }
        void seek() { if (ready && seek >= 0) { player.seekTo((long) (Math.min(seek, Math.max(0, player.getDuration()) / 1000.0) * 1000), MediaPlayer.SEEK_CLOSEST); seek = -1; if (!wantsPlay) progress.run(); } }
        void start() {
            if (closed || !ready || !wantsPlay || transientLoss) return;
            try {
                if (!focus()) throw new IllegalStateException("Audio focus denied");
                if (player.getDuration() > 0 && player.getCurrentPosition() >= player.getDuration()) player.seekTo(0);
                player.setPlaybackParams(new PlaybackParams().setSpeed((float) settings.optDouble("rate", 1))); player.start(); event(id, "playing");
                services.main.removeCallbacks(progress); progress.run();
            } catch (RuntimeException error) { fail(error); }
        }
        void pause(boolean explicit) { if (explicit) wantsPlay = false; if (ready && player.isPlaying()) { player.pause(); event(id, "paused"); } services.main.removeCallbacks(progress); }
        public void close() { if (closed) return; closed = true; ready = false; services.main.removeCallbacks(progress); if (stream != null) { stream.close(); stream = null; } if (player != null) { player.release(); player = null; } if (bytes != null) { try { bytes.close(); } catch (IOException ignored) { } bytes = null; } }
    }
    public void close() {
        if (closed) return;
        closed = true;
        route.close();
        for (int id : new ArrayList<>(sounds.keySet())) unload(id);
        for (Player player : players.values()) player.close(); players.clear(); decoder.shutdownNow();
        if (mixer != null) { mixer.close(); mixer = null; }
        if (manager != null && focused) manager.abandonAudioFocusRequest(focus); focused = false;
    }
}
