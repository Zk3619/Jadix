package dev.jadix.nativeui;

import android.graphics.*;
import org.json.*;

/** Only maps shared resolved geometry to Android shaders. */
final class NativeGradient {
    static Shader shader(JSONObject gradient, float width, float height, float density) {
        if (width <= 0 || height <= 0) return null;
        try {
            JSONObject plan = (JSONObject)NativeRuntime.call(NativeRuntime.object("gradient")
                .put("gradient", gradient).put("size", new JSONArray().put(width / density).put(height / density)));
            JSONArray rgba = plan.getJSONArray("colors"), stops = plan.getJSONArray("positions");
            int[] colors = new int[rgba.length()]; float[] positions = new float[rgba.length()];
            for (int i = 0; i < colors.length; i++) {
                colors[i] = NativeAppearance.color(rgba.getJSONArray(i), 0);
                positions[i] = (float)stops.getDouble(i);
            }
            JSONArray start = plan.getJSONArray("start"), end = plan.getJSONArray("end");
            float x = (float)start.getDouble(0) * density, y = (float)start.getDouble(1) * density;
            switch (plan.getString("kind")) {
                case "conic": {
                    Shader shader = new SweepGradient(x, y, colors, positions);
                    Matrix matrix = new Matrix(); matrix.setRotate((float)Math.toDegrees(plan.getDouble("angle")) - 90, x, y);
                    shader.setLocalMatrix(matrix); return shader;
                }
                case "radial": {
                    JSONArray radii = plan.getJSONArray("radii");
                    float rx = (float)radii.getDouble(0) * density, ry = (float)radii.getDouble(1) * density;
                    if (rx <= 0 || ry <= 0) return new LinearGradient(0, 0, 1, 0, colors[colors.length - 1], colors[colors.length - 1], Shader.TileMode.CLAMP);
                    Shader shader = new RadialGradient(0, 0, 1, colors, positions, Shader.TileMode.CLAMP);
                    Matrix matrix = new Matrix(); matrix.setScale(rx, ry); matrix.postTranslate(x, y);
                    shader.setLocalMatrix(matrix); return shader;
                }
                default: return new LinearGradient(x, y, (float)end.getDouble(0) * density,
                    (float)end.getDouble(1) * density, colors, positions, Shader.TileMode.CLAMP);
            }
        } catch (JSONException error) { throw new IllegalArgumentException("Invalid gradient", error); }
    }
}
