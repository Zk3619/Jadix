package dev.jadix.nativeui;

import android.content.Context;
import android.graphics.SurfaceTexture;
import android.media.*;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.io.IOException;

/** A root-owned player. Resource redirects use the same policy as native images. */
final class NativeMedia extends NativeFrame implements AutoCloseable, MediaController.MediaPlayerControl {
    private final NativeView root;
    private final NativeHosted.Events events;
    private final TextureView texture;
    private final MediaController controls;
    private final AudioManager audio;
    private final NativeAudioRoute route;
    private MediaPlayer player;
    private Surface surface;
    private JSONObject specification = new JSONObject();
    private String source = "", seekRevision = "";
    private boolean prepared, closed, focused, resumeAfterFocus, focusSuspended, ducked, ended, userPaused, suspended;
    private int buffered;
    private long generation;
    private NativeMediaSource mediaSource;
    private AudioFocusRequest focus;
    private final AudioManager.OnAudioFocusChangeListener focusListener = this::audioFocusChanged;
    private void audioFocusChanged(int change) {
        if (closed || player == null || !prepared) return;
        try {
            if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
                boolean playing = player.isPlaying();
                focusSuspended = change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT;
                resumeAfterFocus = focusSuspended && (resumeAfterFocus || playing);
                if (!focusSuspended) userPaused = true;
                if (playing) player.pause();
                removeCallbacks(progress); focused = false; ducked = false;
                if (playing) events.emit("playing", false);
                if (!focusSuspended) abandonFocus();
            } else if (change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK) { ducked = true; volume(); }
            else if (change == AudioManager.AUDIOFOCUS_GAIN) {
                boolean resume = resumeAfterFocus;
                focused = true; focusSuspended = false; resumeAfterFocus = false; ducked = false; volume();
                if (resume && !specification.optBoolean("paused") && !userPaused && !ended) startPlayback();
            }
        } catch (RuntimeException error) { fail(error); }
    }
    private final Runnable progress = new Runnable() { public void run() {
        if (closed || !prepared || player == null) return;
        try {
            events.emit("time", player.getCurrentPosition() / 1000.0);
            events.emit("progress", new JSONObject().put("position", player.getCurrentPosition()/1000.0).put("duration", Math.max(0,player.getDuration())/1000.0));
        } catch (JSONException | IllegalStateException error) { fail(error); }
        if (!closed && prepared && player != null && player.isPlaying()) postDelayed(this, Math.max(50, Math.min(60000, specification.optInt("progress_interval_ms", 250))));
    }};
    NativeMedia(NativeView root, NativeHosted.Events events) {
        super(root.getContext()); this.root = root; this.events = events;
        audio = (AudioManager)getContext().getSystemService(Context.AUDIO_SERVICE);
        route = new NativeAudioRoute(getContext(), this::pause);
        texture = new TextureView(getContext()); addView(texture, new LayoutParams(-1,-1));
        controls = new MediaController(getContext()); controls.setMediaPlayer(this); controls.setAnchorView(this);
        setOnClickListener(v -> { if (specification.optBoolean("controls")) controls.show(); });
        texture.setSurfaceTextureListener(new TextureView.SurfaceTextureListener() {
            public void onSurfaceTextureAvailable(SurfaceTexture value,int width,int height) { if(closed)return; surface = new Surface(value); if(player!=null)player.setSurface(surface); fit(); }
            public void onSurfaceTextureSizeChanged(SurfaceTexture value,int width,int height) { fit(); }
            public boolean onSurfaceTextureDestroyed(SurfaceTexture value) { if(player!=null)player.setSurface(null); if(surface!=null)surface.release();surface=null;return true; }
            public void onSurfaceTextureUpdated(SurfaceTexture value) { NativeCapture.changed(texture); }
        });
    }
    void configure(JSONObject next) throws JSONException {
        if (closed) return;
        boolean playbackChanged = specification.optBoolean("paused") != next.optBoolean("paused");
        specification = next; String value = next.getString("source");
        if (playbackChanged || !value.equals(source)) userPaused = false;
        texture.setVisibility(next.optBoolean("audio") ? GONE : VISIBLE);
        if (!next.optBoolean("controls")) controls.hide();
        if (!value.equals(source)) { releasePlayer(); source = value; load(value); }
        else {
            if (playbackChanged && !next.optBoolean("paused") && ended && prepared && player != null) { player.seekTo(0); ended = false; }
            apply();
        }
    }
    private boolean current(MediaPlayer candidate, long token) { return !closed && token == generation && candidate == player; }
    private void load(String value) {
        long token = ++generation;
        events.emit("loadStart", true);
        try {
            mediaSource = new NativeMediaSource(root, value);
            player = new MediaPlayer();
            player.setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build());
            player.setDataSource(mediaSource);
            if (surface != null) player.setSurface(surface);
            player.setOnPreparedListener(ready -> {
                if (!current(ready, token)) return;
                prepared = true; ended = false;
                events.emit("duration", Math.max(0, ready.getDuration()) / 1000.0);
                try { events.emit("load", new JSONObject().put("duration", Math.max(0, ready.getDuration()) / 1000.0)); }
                catch (JSONException error) { throw new IllegalStateException(error); }
                apply(); fit();
            });
            player.setOnCompletionListener(ready -> { if (current(ready, token)) { ended = true; removeCallbacks(progress); events.emit("time", ready.getCurrentPosition() / 1000.0); events.emit("ended", true); events.emit("playing", false); abandonFocus(); } });
            player.setOnBufferingUpdateListener((ready, percent) -> {
                if (!current(ready, token)) return;
                buffered = Math.max(0, Math.min(100, percent));
                try { events.emit("progress", new JSONObject().put("buffered", Math.max(0, ready.getDuration()) / 1000.0 * buffered / 100)); }
                catch (JSONException error) { throw new IllegalStateException(error); }
            });
            player.setOnInfoListener((ready, what, extra) -> {
                if (current(ready, token) && (what == MediaPlayer.MEDIA_INFO_BUFFERING_START || what == MediaPlayer.MEDIA_INFO_BUFFERING_END)) events.emit("buffer", what == MediaPlayer.MEDIA_INFO_BUFFERING_START);
                return false;
            });
            player.setOnErrorListener((ready, what, extra) -> { if (current(ready, token)) fail(new IOException("Media error " + what + ":" + extra)); return true; });
            player.setOnVideoSizeChangedListener((ready, width, height) -> { if (current(ready, token)) fit(); });
            player.prepareAsync();
        } catch (IOException | RuntimeException error) { fail(error); }
    }
    private void volume() {
        float value = specification.optBoolean("muted") ? 0 : (float)specification.optDouble("volume", 1);
        if (ducked) value *= .2f;
        player.setVolume(value, value);
    }
    private void apply() {
        if (!prepared || closed || player == null) return;
        try {
            volume();
            player.setLooping(specification.optBoolean("looping"));
            String revision = String.valueOf(specification.opt("seek_revision"));
            if (!revision.equals(seekRevision)) {
                seekRevision = revision;
                if (!specification.isNull("seek")) seekTo((int)Math.min(Integer.MAX_VALUE, specification.optDouble("seek") * 1000));
            }
            if (specification.optBoolean("paused")) pausePlayback(); else if (!userPaused && !ended) startPlayback(); fit();
        } catch (RuntimeException error) { fail(error); }
    }
    private void fit() {
        if(player==null||!prepared||texture.getWidth()==0||texture.getHeight()==0)return;
        int width=player.getVideoWidth(),height=player.getVideoHeight();if(width<=0||height<=0)return;
        double sx=(double)texture.getWidth()/width,sy=(double)texture.getHeight()/height;
        String fit=specification.optString("fit","contain");double scale=fit.equals("cover")?Math.max(sx,sy):Math.min(sx,sy);
        android.graphics.Matrix transform=new android.graphics.Matrix();
        if(!fit.equals("fill"))transform.setScale((float)(width*scale/texture.getWidth()),(float)(height*scale/texture.getHeight()),texture.getWidth()/2f,texture.getHeight()/2f);
        texture.setTransform(transform);
    }
    private boolean requestAudioFocus() {
        if(focused)return true;
        if(focus==null)focus=new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN).setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build()).setOnAudioFocusChangeListener(focusListener).build();
        focused=audio != null && audio.requestAudioFocus(focus)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED;if(focused)route.watch();return focused;
    }
    private void abandonFocus(){route.close();focused=false;ducked=false;resumeAfterFocus=false;focusSuspended=false;if(focus!=null&&audio!=null)audio.abandonAudioFocusRequest(focus);}
    private void fail(Exception error){events.emit("error",error.getMessage()==null?"Media failed":error.getMessage());releasePlayer();}
    private void releasePlayer(){generation++;removeCallbacks(progress);if(mediaSource!=null)mediaSource.close();mediaSource=null;buffered=0;controls.hide();prepared=false;if(player!=null){player.release();player=null;}abandonFocus();seekRevision="";}
    @Override public void close(){if(closed)return;closed=true;route.close();releasePlayer();if(surface!=null)surface.release();surface=null;texture.setSurfaceTextureListener(null);}
    private void visibilityChanged() {
        if (player == null || !prepared || closed) return;
        if (!isShown() || getWindowVisibility() != VISIBLE) {
            suspended |= player.isPlaying(); pausePlayback();
        } else if (suspended) {
            suspended = false;
            if (!specification.optBoolean("paused") && !userPaused && !ended) startPlayback();
        } else if (!userPaused && !ended && !specification.optBoolean("paused")) startPlayback();
    }
    @Override protected void onWindowVisibilityChanged(int visibility) { super.onWindowVisibilityChanged(visibility); visibilityChanged(); }
    @Override protected void onVisibilityChanged(View changed,int visibility) { super.onVisibilityChanged(changed,visibility); visibilityChanged(); }
    public void start() {
        if (closed) return;
        userPaused = false; focusSuspended = false;
        try { if (ended && player != null && prepared) { player.seekTo(0); ended = false; } startPlayback(); }
        catch (RuntimeException error) { fail(error); }
    }
    private void startPlayback() {
        if(closed || focusSuspended || !prepared || player==null || !isShown() || getWindowVisibility()!=VISIBLE || !requestAudioFocus())return;
        boolean wasPlaying=player.isPlaying();
        float speed=(float)specification.optDouble("rate",1);
        if(player.getPlaybackParams().getSpeed()!=speed)player.setPlaybackParams(player.getPlaybackParams().setSpeed(speed));
        if(!player.isPlaying())player.start();
        if(!wasPlaying)events.emit("playing",true);
        removeCallbacks(progress); post(progress);
    }
    public void pause() { userPaused = true; pausePlayback(); }
    private void pausePlayback() { removeCallbacks(progress); if(prepared&&player!=null&&player.isPlaying()){player.pause();events.emit("playing",false);}abandonFocus(); }
    public int getDuration(){return prepared&&player!=null?Math.max(0,player.getDuration()):0;}
    public int getCurrentPosition(){return prepared&&player!=null?Math.max(0,player.getCurrentPosition()):0;}
    public void seekTo(int position){if(!closed&&prepared&&player!=null){ended=false;player.seekTo(Math.min(getDuration(),Math.max(0,position)));}}
    public boolean isPlaying(){return prepared&&player!=null&&player.isPlaying();}
    public int getBufferPercentage(){return buffered;}
    public boolean canPause(){return true;}public boolean canSeekBackward(){return true;}public boolean canSeekForward(){return true;}
    public int getAudioSessionId(){return player==null?0:player.getAudioSessionId();}
}
