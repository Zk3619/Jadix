package dev.jadix.nativeui;

import android.Manifest;
import android.content.Intent;
import android.os.Bundle;
import android.speech.RecognitionListener;
import android.speech.RecognizerIntent;
import android.speech.SpeechRecognizer;
import java.util.ArrayList;
import org.json.*;

/** Android recognition follows the shared start/result/end and stale-session contract. */
final class NativeSpeech implements AutoCloseable {
    private final NativeServices services;
    private SpeechRecognizer recognizer;
    private int active;
    private boolean started;
    NativeSpeech(NativeServices services) { this.services = services; }
    void command(JSONObject command) throws JSONException {
        int id = command.getInt("id"); String action = command.getString("action");
        if (!"start".equals(action)) {
            if (active != id) return;
            if ("stop".equals(action) && recognizer != null) recognizer.stopListening();
            else end(id, null);
            return;
        }
        close(); active = id; started = false;
        if (!SpeechRecognizer.isRecognitionAvailable(services.root.getContext())) { end(id, "unavailable"); return; }
        JSONObject request = command.getJSONObject("request");
        services.permission(Manifest.permission.RECORD_AUDIO, granted -> {
            if (active != id || services.closed) return;
            if (!granted) { end(id, "not-allowed"); return; }
            try {
                recognizer = SpeechRecognizer.createSpeechRecognizer(services.root.getContext());
                recognizer.setRecognitionListener(new RecognitionListener() {
                    private boolean current() { return active == id && !services.closed; }
                    private void start() { if (current() && !started) { started = true; services.event("speech", id, NativeServices.object("type", "start")); } }
                    private void result(Bundle results, boolean complete) {
                        if (!current()) return;
                        ArrayList<String> values = results == null ? null : results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION);
                        if (values != null && !values.isEmpty()) {
                            String text = values.get(0);
                            if (text.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > 65536) { end(id, "failed"); return; }
                            start(); services.event("speech", id, NativeServices.object("type", "result", "text", text, "is_final", complete));
                        }
                        if (complete) end(id, null);
                    }
                    public void onReadyForSpeech(Bundle params) { start(); }
                    public void onBeginningOfSpeech() { start(); }
                    public void onRmsChanged(float rms) {}
                    public void onBufferReceived(byte[] buffer) {}
                    public void onEndOfSpeech() {}
                    public void onError(int error) {
                        if (!current()) return;
                        String name;
                        switch (error) {
                            case SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS: name = "not-allowed"; break;
                            case SpeechRecognizer.ERROR_AUDIO: name = "audio-capture"; break;
                            case SpeechRecognizer.ERROR_NETWORK: case SpeechRecognizer.ERROR_NETWORK_TIMEOUT: name = "network"; break;
                            case SpeechRecognizer.ERROR_NO_MATCH: case SpeechRecognizer.ERROR_SPEECH_TIMEOUT: name = "no-speech"; break;
                            case 12: case 13: name = "language-not-supported"; break;
                            default: name = "failed";
                        }
                        end(id, name);
                    }
                    public void onResults(Bundle results) { result(results, true); }
                    public void onPartialResults(Bundle results) { if (request.optBoolean("interim", true)) result(results, false); }
                    public void onEvent(int type, Bundle params) {}
                });
                Intent intent = new Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH);
                intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM);
                intent.putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, request.optBoolean("interim", true));
                if (!request.isNull("lang")) intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE, request.optString("lang"));
                recognizer.startListening(intent);
            } catch (RuntimeException error) { end(id, "unavailable"); }
        });
    }
    private void end(int id, String error) {
        if (active != id) return;
        close(); services.event("speech", id, NativeServices.object("type", "end", "error", error));
    }
    public void close() {
        active = 0; started = false;
        SpeechRecognizer previous = recognizer; recognizer = null;
        if (previous != null) { previous.cancel(); previous.destroy(); }
    }
}
