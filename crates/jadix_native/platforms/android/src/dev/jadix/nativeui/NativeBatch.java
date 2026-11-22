package dev.jadix.nativeui;

import java.math.BigInteger;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

final class NativeBatch {
    private String root;
    private BigInteger revision = BigInteger.ZERO;
    private HashMap<String, JSONObject> nodes = new HashMap<>();
    private HashMap<String, ArrayList<String>> children = emptyChildren();

    static final class Prepared {
        String root;
        BigInteger previous;
        BigInteger revision;
        HashMap<String, JSONObject> nodes;
        HashMap<String, ArrayList<String>> children;
        JSONArray order;
        JSONArray updates;
        boolean topology;
        boolean labelsOnly;
        boolean focusChanged;
        String focus;

        JSONObject snapshot() throws JSONException {
            return new JSONObject().put("protocol", 1).put("root", root).put("revision", revision.toString()).put("nodes", order);
        }
    }

    private static HashMap<String, ArrayList<String>> emptyChildren() {
        HashMap<String, ArrayList<String>> result = new HashMap<>();
        result.put(null, new ArrayList<>());
        return result;
    }

    private static String parent(JSONObject node) throws JSONException {
        return node.isNull("parent") ? null : node.getString("parent");
    }

    private static BigInteger decimal(JSONObject value, String key) throws JSONException {
        String text = value.getString(key);
        if (!text.matches("0|[1-9][0-9]{0,19}")) throw new IllegalArgumentException("invalid native batch revision");
        BigInteger result = new BigInteger(text);
        if (result.bitLength() > 64) throw new IllegalArgumentException("native batch revision overflow");
        return result;
    }

    Prepared prepare(String json) throws JSONException {
        if (json.length() > 16 * 1024 * 1024) throw new IllegalArgumentException("native batch exceeds byte budget");
        JSONObject batch = new JSONObject(json);
        if (batch.getInt("protocol") != 2) throw new IllegalArgumentException("native batch protocol mismatch");
        Prepared result = new Prepared();
        result.root = batch.getString("root");
        result.previous = decimal(batch, "previous");
        result.revision = decimal(batch, "revision");
        if ((root != null && !root.equals(result.root)) || !revision.equals(result.previous)
                || !revision.add(BigInteger.ONE).equals(result.revision)) throw new IllegalArgumentException("stale or foreign native batch");
        JSONArray operations = batch.getJSONArray("operations");
        if (operations.length() > 65536) throw new IllegalArgumentException("native batch operation budget exceeded");
        if (prepareLabels(result, operations)) return result;
        result.focusChanged = false; result.focus = null;
        result.nodes = new HashMap<>(nodes);
        result.children = new HashMap<>();
        for (String identifier : children.keySet()) result.children.put(identifier, new ArrayList<>(children.get(identifier)));
        HashSet<String> changed = new HashSet<>();
        for (int index = 0; index < operations.length(); index++) {
            JSONObject operation = operations.getJSONObject(index);
            String action = operation.getString("op");
            String identifier = operation.isNull("id") ? null : operation.getString("id");
            if ("focus".equals(action)) {
                if (identifier != null && !result.nodes.containsKey(identifier)) throw new IllegalArgumentException("unknown native focus target");
                result.focusChanged = true; result.focus = identifier; continue;
            }
            if (identifier == null || identifier.isEmpty()) throw new IllegalArgumentException("missing native node identity");
            JSONObject previous = result.nodes.get(identifier);
            if ("create".equals(action)) {
                if (previous != null || nodes.containsKey(identifier)) throw new IllegalArgumentException("native identity reused");
                String parent = parent(operation);
                ArrayList<String> siblings = result.children.get(parent);
                int position = operation.getInt("index");
                if (siblings == null || position < 0 || position > siblings.size()) throw new IllegalArgumentException("invalid native insertion");
                JSONObject descriptor = new JSONObject(operation.getJSONObject("descriptor").toString());
                descriptor.put("id", identifier).put("parent", parent == null ? JSONObject.NULL : parent);
                result.nodes.put(identifier, descriptor); result.children.put(identifier, new ArrayList<>());
                siblings.add(position, identifier); changed.add(identifier); result.topology = true;
            } else {
                if (previous == null) throw new IllegalArgumentException("unknown native batch node");
                if ("update".equals(action)) {
                    JSONObject descriptor = new JSONObject(operation.getJSONObject("descriptor").toString());
                    if (!previous.getString("kind").equals(descriptor.getString("kind"))) throw new IllegalArgumentException("native node kind changed");
                    descriptor.put("id", identifier).put("parent", previous.get("parent"));
                    result.nodes.put(identifier, descriptor); changed.add(identifier);
                } else if ("move".equals(action)) {
                    String parent = parent(operation);
                    String ancestor = parent;
                    while (ancestor != null) {
                        if (ancestor.equals(identifier)) throw new IllegalArgumentException("native parent cycle");
                        JSONObject node = result.nodes.get(ancestor);
                        if (node == null) throw new IllegalArgumentException("unknown native parent");
                        ancestor = parent(node);
                    }
                    result.children.get(parent(previous)).remove(identifier);
                    ArrayList<String> siblings = result.children.get(parent);
                    int position = operation.getInt("index");
                    if (siblings == null || position < 0 || position > siblings.size()) throw new IllegalArgumentException("invalid native move");
                    siblings.add(position, identifier);
                    JSONObject descriptor = new JSONObject(previous.toString()).put("parent", parent == null ? JSONObject.NULL : parent);
                    result.nodes.put(identifier, descriptor); changed.add(identifier); result.topology = true;
                } else if ("remove".equals(action)) {
                    result.children.get(parent(previous)).remove(identifier);
                    ArrayList<String> pending = new ArrayList<>(); pending.add(identifier);
                    while (!pending.isEmpty()) {
                        String removed = pending.remove(pending.size() - 1);
                        pending.addAll(result.children.remove(removed)); result.nodes.remove(removed); changed.remove(removed);
                    }
                    result.topology = true;
                } else { throw new IllegalArgumentException("unknown native batch operation"); }
            }
        }
        for (String identifier : new HashSet<>(changed)) {
            JSONObject previous = nodes.get(identifier);
            JSONObject current = result.nodes.get(identifier);
            if (previous != null && current != null && !previous.getJSONArray("frame").toString().equals(current.getJSONArray("frame").toString())) {
                changed.addAll(result.children.get(identifier));
            }
        }
        result.order = new JSONArray(); result.updates = new JSONArray();
        ArrayList<String> pending = new ArrayList<>();
        ArrayList<String> roots = result.children.get(null);
        for (int index = roots.size() - 1; index >= 0; index--) pending.add(roots.get(index));
        HashSet<String> visited = new HashSet<>();
        while (!pending.isEmpty()) {
            String identifier = pending.remove(pending.size() - 1);
            if (!visited.add(identifier)) throw new IllegalArgumentException("duplicate native tree ownership");
            JSONObject node = result.nodes.get(identifier); result.order.put(node);
            if (changed.contains(identifier)) result.updates.put(node);
            ArrayList<String> descendants = result.children.get(identifier);
            for (int index = descendants.size() - 1; index >= 0; index--) pending.add(descendants.get(index));
        }
        if (visited.size() != result.nodes.size()) throw new IllegalArgumentException("unreachable native nodes");
        return result;
    }

    private boolean prepareLabels(Prepared result, JSONArray operations) throws JSONException {
        HashMap<String, JSONObject> updates = new HashMap<>();
        for (int index = 0; index < operations.length(); index++) {
            JSONObject operation = operations.getJSONObject(index);
            String action = operation.getString("op");
            String id = operation.isNull("id") ? null : operation.getString("id");
            if ("focus".equals(action)) {
                if (id != null && !nodes.containsKey(id)) throw new IllegalArgumentException("unknown native focus target");
                result.focusChanged = true; result.focus = id; continue;
            }
            if (!"update".equals(action)) return false;
            JSONObject previous = nodes.get(id);
            JSONObject descriptor = operation.getJSONObject("descriptor");
            if (previous == null || !("text".equals(previous.optString("kind")) || "button".equals(previous.optString("kind")))
                    || !children.get(id).isEmpty()) return false;
            // Only the label may differ. Layout, accessibility, styles and control
            // state still use the full publication path when they change.
            if (descriptor.length() != previous.length() - 2) return false;
            java.util.Iterator<String> keys = descriptor.keys();
            while (keys.hasNext()) {
                String key = keys.next();
                if ("id".equals(key) || "parent".equals(key) || !previous.has(key)) return false;
                if (!"text".equals(key) && !previous.get(key).toString().equals(descriptor.get(key).toString())) return false;
            }
            descriptor.getString("text");
            updates.put(id, new JSONObject(descriptor.toString()).put("id", id).put("parent", previous.get("parent")));
        }
        result.nodes = updates; result.children = children; result.updates = new JSONArray();
        for (JSONObject node : updates.values()) result.updates.put(node);
        result.labelsOnly = true;
        return true;
    }

    void commit(Prepared prepared) {
        if (!revision.equals(prepared.previous)) throw new IllegalStateException("native batch acknowledgement changed");
        root = prepared.root; revision = prepared.revision;
        if (prepared.labelsOnly) nodes.putAll(prepared.nodes);
        else { nodes = prepared.nodes; children = prepared.children; }
    }

    void clear() {
        root = null; revision = BigInteger.ZERO; nodes.clear(); children = emptyChildren();
    }
}
