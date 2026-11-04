package dev.jadix.nativeui;

import org.json.JSONArray;
import org.json.JSONObject;

public final class NativeBatchHarness {
    private static int checks;
    private static void check(boolean result, String message) {
        checks++;
        if (!result) throw new AssertionError(message);
    }
    private static JSONObject descriptor(String kind, String text, int left) {
        return new JSONObject().put("kind", kind).put("text", text).put("frame", new JSONArray(new int[]{left, 0, 100, 32}));
    }
    private static JSONObject create(String identifier, String parent, int index, String kind) {
        return new JSONObject().put("op", "create").put("id", identifier).put("parent", parent == null ? JSONObject.NULL : parent)
            .put("index", index).put("descriptor", descriptor(kind, "initial", 0));
    }
    private static String batch(String root, long previous, JSONObject... operations) {
        return new JSONObject().put("protocol", 2).put("root", root).put("previous", Long.toString(previous))
            .put("revision", Long.toString(previous + 1)).put("operations", new JSONArray(operations)).toString();
    }
    private static void rejected(NativeBatch target, String payload) throws Exception {
        boolean failed = false;
        try { target.prepare(payload); } catch (IllegalArgumentException exception) { failed = true; }
        check(failed, "invalid candidate was accepted");
    }
    public static void main(String[] arguments) throws Exception {
        NativeBatch target = new NativeBatch();
        String root = "18446744073709551615";
        NativeBatch.Prepared first = target.prepare(batch(root, 0, create("left", null, 0, "box"), create("right", null, 1, "box"), create("editor", "left", 0, "input")));
        check(first.topology && first.order.length() == 3, "initial hierarchy");
        check("editor".equals(first.order.getJSONObject(1).getString("id")), "parent before descendants");
        check(root.equals(first.root), "decimal identity was rounded");
        target.commit(first);
        JSONObject update = new JSONObject().put("op", "update").put("id", "editor").put("descriptor", descriptor("input", "中文 😀", 0));
        NativeBatch.Prepared typing = target.prepare(batch(root, 1, update));
        check(!typing.topology && typing.updates.length() == 1, "typing dirtied unrelated native views");
        check("中文 😀".equals(typing.updates.getJSONObject(0).getString("text")), "Unicode payload");
        check("left".equals(typing.nodes.get("editor").getString("parent")), "update lost parent");
        NativeBatch.Prepared concurrent = target.prepare(batch(root, 1, update));
        target.commit(typing);
        boolean staleCommit = false;
        try { target.commit(concurrent); } catch (IllegalStateException exception) { staleCommit = true; }
        check(staleCommit, "stale acknowledgement");
        rejected(target, batch("foreign", 2, update));
        rejected(target, batch(root, 1, update));
        rejected(target, batch(root, 2, new JSONObject().put("op", "move").put("id", "left").put("parent", "editor").put("index", 0)));
        rejected(target, batch(root, 2, new JSONObject().put("op", "move").put("id", "editor").put("parent", "right").put("index", 99)));
        NativeBatch.Prepared moving = target.prepare(batch(root, 2, new JSONObject().put("op", "move").put("id", "editor").put("parent", "right").put("index", 0)));
        check(moving.topology && moving.order.getJSONObject(2).getString("id").equals("editor"), "move ordering");
        check(moving.nodes.get("editor").getString("text").equals("中文 😀"), "move replaced edited content");
        target.commit(moving);
        NativeBatch.Prepared geometry = target.prepare(batch(root, 3, new JSONObject().put("op", "update").put("id", "right").put("descriptor", descriptor("box", "parent", 20))));
        check(geometry.updates.length() == 2, "parent movement must rebase retained child geometry");
        target.commit(geometry);
        NativeBatch.Prepared removed = target.prepare(batch(root, 4, new JSONObject().put("op", "remove").put("id", "right")));
        check(removed.order.length() == 1 && !removed.nodes.containsKey("editor"), "recursive removal");
        target.commit(removed);
        rejected(target, batch(root, 5, update));
        rejected(target, new JSONObject(batch(root, 5)).put("revision", "18446744073709551616").toString());
        NativeBatch.Prepared focus = target.prepare(batch(root, 5, new JSONObject().put("op", "focus").put("id", JSONObject.NULL)));
        check(focus.focusChanged && focus.focus == null && focus.updates.length() == 0, "focus-only update");
        target.commit(focus);
        target.clear();
        NativeBatch.Prepared replacement = target.prepare(batch("replacement", 0, create("replacement-node", null, 0, "box")));
        check(replacement.nodes.size() == 1, "clear retained old root nodes");
        target.commit(replacement);
        System.out.println(new JSONObject().put("checks", checks).put("accepted", true).put("environment", "JVM with pinned JSON-java; Android runtime remains separate"));
    }
}
