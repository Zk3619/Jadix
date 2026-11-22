package dev.jadix.nativeui;

import android.graphics.*;
import android.graphics.drawable.*;
import android.os.SystemClock;

/** API 26/27 animation playback, with one independent clock per image binding. */
final class NativeAnimatedImage extends Drawable implements Animatable, Runnable {
    private final Movie movie;
    private boolean running;
    private long started, elapsed;
    private final Paint paint=new Paint(Paint.ANTI_ALIAS_FLAG|Paint.FILTER_BITMAP_FLAG);
    NativeAnimatedImage(Movie movie) { this.movie=movie; }
    @Override public int getIntrinsicWidth() { return movie.width(); }
    @Override public int getIntrinsicHeight() { return movie.height(); }
    @Override public void draw(Canvas canvas) {
        movie.setTime((int)((running?SystemClock.uptimeMillis()-started:elapsed)%Math.max(1,movie.duration())));
        int save=canvas.save(); Rect bounds=getBounds();
        canvas.translate(bounds.left,bounds.top); canvas.scale((float)bounds.width()/Math.max(1,movie.width()),(float)bounds.height()/Math.max(1,movie.height()));
        movie.draw(canvas,0,0,paint); canvas.restoreToCount(save);
    }
    public void run() { if(running&&isVisible()) { invalidateSelf(); scheduleSelf(this,SystemClock.uptimeMillis()+17); } }
    public void start() { if(!running) { running=true; started=SystemClock.uptimeMillis()-elapsed; run(); } }
    public void stop() { if(running)elapsed=SystemClock.uptimeMillis()-started; running=false; unscheduleSelf(this); }
    public boolean isRunning() { return running; }
    @Override public boolean setVisible(boolean visible,boolean restart) { if(!visible)stop(); if(restart)elapsed=0; return super.setVisible(visible,restart); }
    @Override public void setAlpha(int alpha) { paint.setAlpha(alpha); invalidateSelf(); }
    @Override public void setColorFilter(ColorFilter filter) { paint.setColorFilter(filter); invalidateSelf(); }
    @Override public int getOpacity() { return PixelFormat.TRANSLUCENT; }
}
