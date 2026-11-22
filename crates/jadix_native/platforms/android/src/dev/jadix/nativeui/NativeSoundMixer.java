package dev.jadix.nativeui;

import android.media.*;
import java.io.*;
import java.nio.*;
import java.util.*;

/** One preallocated output and 32 reusable voices. Play never creates a player or decoder. */
final class NativeSoundMixer implements AutoCloseable {
    static final int MAX_BYTES = 16 * 1024 * 1024, RATE = 48000;
    static final class PCM {
        final short[] samples; final int rate, channels;
        PCM(short[] samples, int rate, int channels) { this.samples = samples; this.rate = rate; this.channels = channels; }
        double duration() { return (double) samples.length / (rate * channels); }
    }
    private static final class Voice { int id; PCM pcm; double position, step; float volume; long serial; }
    private final Voice[] voices = new Voice[32];
    private final short[] output = new short[512];
    private final NativeServices services;
    private final AudioTrack track;
    private final Thread worker;
    private boolean closed, suspended;
    private float gain = 1;
    private long serial;
    NativeSoundMixer(NativeServices services) {
        this.services = services;
        for (int i = 0; i < voices.length; i++) voices[i] = new Voice();
        AudioFormat format = new AudioFormat.Builder().setSampleRate(RATE).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build();
        int bytes = Math.max(4096, AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT));
        track = new AudioTrack.Builder().setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build())
            .setAudioFormat(format).setTransferMode(AudioTrack.MODE_STREAM).setBufferSizeInBytes(bytes).build();
        if (track.getState() != AudioTrack.STATE_INITIALIZED) { track.release(); throw new IllegalStateException("Audio output unavailable"); }
        worker = new Thread(this::mix, "jadix-sound-mixer"); worker.setDaemon(true); worker.start();
    }
    synchronized void play(int id, PCM pcm, float volume, float rate) {
        if (closed) throw new IllegalStateException("Sound mixer unavailable");
        Voice selected = voices[0];
        for (Voice voice : voices) { if (voice.pcm == null) { selected = voice; break; } if (voice.serial < selected.serial) selected = voice; }
        int replaced = selected.pcm == null ? 0 : selected.id;
        selected.id = id; selected.pcm = pcm; selected.position = 0; selected.step = (double) pcm.rate / RATE * rate; selected.volume = volume; selected.serial = ++serial;
        if (replaced != 0 && replaced != id) ended(replaced);
        notifyAll();
    }
    synchronized void stop(int id) { for (Voice voice : voices) if (voice.id == id) voice.pcm = null; }
    synchronized void stopAll() { for (Voice voice : voices) voice.pcm = null; }
    synchronized void suspend(boolean suspended, float gain) { this.suspended = suspended; this.gain = gain; notifyAll(); }
    private boolean active() { for (Voice voice : voices) if (voice.pcm != null) return true; return false; }
    private void ended(int id) {
        for (Voice voice : voices) if (voice.pcm != null && voice.id == id) return;
        services.event("audio", id, NativeServices.object("type", "ended"));
    }
    private void mix() {
        android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_AUDIO);
        try {
            while (true) {
                synchronized (this) {
                    while (!closed && (suspended || !active())) { track.pause(); track.flush(); wait(); }
                    if (closed) break;
                    Arrays.fill(output, (short) 0);
                    for (int frame = 0; frame < output.length / 2; frame++) {
                        double left = 0, right = 0;
                        for (Voice voice : voices) {
                            PCM pcm = voice.pcm; if (pcm == null) continue;
                            int position = (int) voice.position, frames = pcm.samples.length / pcm.channels;
                            if (position >= frames) { int id = voice.id; voice.pcm = null; ended(id); continue; }
                            int next = Math.min(position + 1, frames - 1); double fraction = voice.position - position;
                            for (int channel = 0; channel < 2; channel++) {
                                int sourceChannel = Math.min(channel, pcm.channels - 1);
                                double value = pcm.samples[position * pcm.channels + sourceChannel] * (1 - fraction) + pcm.samples[next * pcm.channels + sourceChannel] * fraction;
                                if (channel == 0) left += value * voice.volume * gain; else right += value * voice.volume * gain;
                            }
                            voice.position += voice.step;
                        }
                        output[frame * 2] = (short) Math.max(Short.MIN_VALUE, Math.min(Short.MAX_VALUE, left));
                        output[frame * 2 + 1] = (short) Math.max(Short.MIN_VALUE, Math.min(Short.MAX_VALUE, right));
                    }
                }
                track.play();
                if (track.write(output, 0, output.length, AudioTrack.WRITE_BLOCKING) < 0) throw new IOException("Audio output failed");
            }
        } catch (InterruptedException ignored) { Thread.currentThread().interrupt(); }
        catch (Exception error) {
            synchronized (this) { for (Voice voice : voices) if (voice.pcm != null) { services.event("audio", voice.id, NativeServices.object("type", "failed", "message", error.toString())); voice.pcm = null; } closed = true; }
        } finally { track.release(); }
    }
    public synchronized void close() { closed = true; for (Voice voice : voices) voice.pcm = null; notifyAll(); worker.interrupt(); }

    static PCM decode(File file) throws IOException {
        MediaExtractor extractor = new MediaExtractor(); MediaCodec codec = null;
        try {
            extractor.setDataSource(file.getAbsolutePath()); MediaFormat input = null;
            for (int i = 0; i < extractor.getTrackCount(); i++) {
                MediaFormat candidate = extractor.getTrackFormat(i);
                if (candidate.getString(MediaFormat.KEY_MIME).startsWith("audio/")) { input = candidate; extractor.selectTrack(i); break; }
            }
            if (input == null) throw new IOException("No audio track");
            if (input.containsKey(MediaFormat.KEY_DURATION) && input.getLong(MediaFormat.KEY_DURATION) > 5000000) throw new IOException("Sound exceeds five seconds");
            int initialRate = input.getInteger(MediaFormat.KEY_SAMPLE_RATE), initialChannels = input.getInteger(MediaFormat.KEY_CHANNEL_COUNT);
            if (initialChannels < 1 || initialChannels > 2 || initialRate < 8000 || initialRate > 192000) throw new IOException("Unsupported sound PCM format");
            // Extractors already decode uncompressed WAV/PCM; no audio/raw codec exists.
            if ("audio/raw".equals(input.getString(MediaFormat.KEY_MIME))) return raw(extractor, input, initialRate, initialChannels);
            input.setInteger(MediaFormat.KEY_PCM_ENCODING, AudioFormat.ENCODING_PCM_16BIT);
            codec = MediaCodec.createDecoderByType(input.getString(MediaFormat.KEY_MIME)); codec.configure(input, null, null, 0); codec.start();
            MediaCodec.BufferInfo info = new MediaCodec.BufferInfo(); ByteArrayOutputStream decoded = new ByteArrayOutputStream();
            boolean inputEnded = false; int rate = input.getInteger(MediaFormat.KEY_SAMPLE_RATE), channels = input.getInteger(MediaFormat.KEY_CHANNEL_COUNT), encoding = AudioFormat.ENCODING_PCM_16BIT;
            long deadline = android.os.SystemClock.elapsedRealtime() + 30000;
            while (true) {
                if (Thread.currentThread().isInterrupted() || android.os.SystemClock.elapsedRealtime() > deadline) throw new InterruptedIOException("Sound decoding interrupted");
                if (!inputEnded) {
                    int index = codec.dequeueInputBuffer(10000);
                    if (index >= 0) {
                        ByteBuffer buffer = codec.getInputBuffer(index); int size = extractor.readSampleData(buffer, 0);
                        if (size < 0) { codec.queueInputBuffer(index, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM); inputEnded = true; }
                        else { codec.queueInputBuffer(index, 0, size, extractor.getSampleTime(), 0); extractor.advance(); }
                    }
                }
                int index = codec.dequeueOutputBuffer(info, 10000);
                if (index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    MediaFormat output = codec.getOutputFormat(); rate = output.getInteger(MediaFormat.KEY_SAMPLE_RATE); channels = output.getInteger(MediaFormat.KEY_CHANNEL_COUNT);
                    encoding = output.containsKey(MediaFormat.KEY_PCM_ENCODING) ? output.getInteger(MediaFormat.KEY_PCM_ENCODING) : AudioFormat.ENCODING_PCM_16BIT;
                    if (channels < 1 || channels > 2 || rate < 8000 || rate > 192000 || encoding != AudioFormat.ENCODING_PCM_16BIT) throw new IOException("Unsupported sound PCM format");
                } else if (index >= 0) {
                    ByteBuffer buffer = codec.getOutputBuffer(index); buffer.position(info.offset); buffer.limit(info.offset + info.size);
                    if (decoded.size() + info.size > MAX_BYTES || (decoded.size() + info.size) / 2.0 / rate / channels > 5) throw new IOException("Sound decoded quota exceeded");
                    byte[] chunk = new byte[info.size]; buffer.get(chunk); decoded.write(chunk); codec.releaseOutputBuffer(index, false);
                    if ((info.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0) break;
                }
            }
            byte[] bytes = decoded.toByteArray();
            if (bytes.length == 0 || bytes.length % (2 * channels) != 0) throw new IOException("Invalid sound PCM");
            short[] samples = new short[bytes.length / 2]; ByteBuffer.wrap(bytes).order(ByteOrder.nativeOrder()).asShortBuffer().get(samples);
            return new PCM(samples, rate, channels);
        } finally { if (codec != null) { try { codec.stop(); } finally { codec.release(); } } extractor.release(); }
    }
    private static PCM raw(MediaExtractor extractor, MediaFormat format, int rate, int channels) throws IOException {
        int encoding = format.containsKey(MediaFormat.KEY_PCM_ENCODING) ? format.getInteger(MediaFormat.KEY_PCM_ENCODING) : AudioFormat.ENCODING_PCM_16BIT;
        int width = encoding == AudioFormat.ENCODING_PCM_8BIT ? 1 : encoding == AudioFormat.ENCODING_PCM_16BIT ? 2 : encoding == AudioFormat.ENCODING_PCM_FLOAT ? 4 : 0;
        if (width == 0) throw new IOException("Unsupported uncompressed sound encoding");
        ByteArrayOutputStream decoded = new ByteArrayOutputStream(); ByteBuffer buffer = ByteBuffer.allocate(1024 * 1024).order(ByteOrder.LITTLE_ENDIAN);
        while (true) {
            if (Thread.currentThread().isInterrupted()) throw new InterruptedIOException("Sound decoding interrupted");
            buffer.clear(); int count = extractor.readSampleData(buffer, 0); if (count < 0) break;
            if (count > buffer.capacity() || count % width != 0 || decoded.size() + (long)count / width * 2 > MAX_BYTES
                    || (decoded.size() / 2L + count / width) > 5L * rate * channels) throw new IOException("Sound decoded quota exceeded");
            buffer.position(0); buffer.limit(count);
            while (buffer.remaining() >= width) {
                short sample;
                if (width == 1) sample = (short)(((buffer.get() & 255) - 128) << 8);
                else if (width == 2) sample = buffer.getShort();
                else { float value = buffer.getFloat(); if (!Float.isFinite(value)) throw new IOException("Invalid PCM sample"); sample = (short)Math.max(-32768, Math.min(32767, value * 32768)); }
                decoded.write(sample & 255); decoded.write((sample >>> 8) & 255);
            }
            if (!extractor.advance()) break;
        }
        byte[] bytes = decoded.toByteArray();
        if (bytes.length == 0 || bytes.length % (2 * channels) != 0) throw new IOException("Invalid sound PCM");
        short[] samples = new short[bytes.length / 2]; ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asShortBuffer().get(samples);
        return new PCM(samples, rate, channels);
    }
}
