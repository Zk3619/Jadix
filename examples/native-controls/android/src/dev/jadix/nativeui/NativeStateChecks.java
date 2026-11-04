package dev.jadix.nativeui;

import android.content.Context;
import android.content.ContextWrapper;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.UUID;

final class NativeStateChecks {
    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private static File path(Context context, NativeStateStore store) {
        return new File(context.getFilesDir(), "native-state-" + store.id + ".json");
    }

    static void run(Context target) throws Exception {
        File directory = new File(target.getFilesDir(), "state-quota-check-" + UUID.randomUUID());
        require(directory.mkdir(), "isolated state quota directory");
        Context context = new ContextWrapper(target) {
            @Override public File getFilesDir() { return directory; }
        };
        ArrayList<NativeStateStore> stores = new ArrayList<>();
        try {
            for (int index = 0; index < 16; index++) {
                NativeStateStore store = new NativeStateStore(context, null);
                store.write("slot-" + index);
                stores.add(store);
            }
            NativeStateStore extra = new NativeStateStore(context, null);
            boolean rejected = false;
            try { extra.write("overflow"); } catch (IOException expected) { rejected = true; }
            require(rejected && !path(context, extra).exists(), "state cache rejects the seventeenth identity");
            require("slot-0".equals(stores.get(0).read()), "slot limit preserves previous states");
            stores.get(0).delete();
            extra.write("reused slot");
            require("reused slot".equals(extra.read()), "deleted slot is reusable");
            extra.delete();
            for (NativeStateStore store : stores) store.delete();
            stores.clear();
            String large = new String(new char[NativeStateStore.MAX_BYTES]).replace('\0', 'x');
            for (int index = 0; index < 8; index++) {
                NativeStateStore store = new NativeStateStore(context, null);
                store.write(large);
                stores.add(store);
            }
            rejected = false;
            try { extra.write("x"); } catch (IOException expected) { rejected = true; }
            require(rejected && large.equals(stores.get(0).read()), "aggregate sixteen MiB limit preserves state");
            stores.get(0).write(large);
            require(large.equals(stores.get(0).read()), "same-size atomic replacement fits the quota");
            stores.get(0).write("small");
            extra.write("freed bytes");
            require("freed bytes".equals(extra.read()), "shrinking a state releases aggregate bytes");
            extra.delete();
            for (NativeStateStore store : stores) store.delete();
            stores.clear();
            NativeStateStore expired = new NativeStateStore(context, null);
            expired.write("expired");
            long old = System.currentTimeMillis() - 8L * 24 * 60 * 60 * 1000;
            require(path(context, expired).setLastModified(old), "age expired cache");
            File unrelated = new File(directory, "native-state-not-an-identity.json");
            try (FileOutputStream stream = new FileOutputStream(unrelated)) { stream.write(7); }
            require(unrelated.setLastModified(old), "age unrelated file");
            extra.write("cleanup trigger");
            require(!path(context, expired).exists() && unrelated.exists(), "expiry removes only owned stale state");
            NativeStateStore live = new NativeStateStore(context, null);
            NativeStateStore duplicate = new NativeStateStore(context, live.id);
            try {
                live.write("active");
                live.activate();
                live.activate();
                duplicate.activate();
                require(path(context, live).setLastModified(old), "age active state");
                extra.write("keep live");
                require("active".equals(live.read()), "active Activity state cannot expire");
                live.close();
                live.close();
                extra.write("keep duplicate lease");
                require("active".equals(live.read()), "another live handle protects the same identity");
                duplicate.close();
                extra.write("release expired lease");
                require(!path(context, live).exists(), "closing the final lease permits expiry");
            } finally { live.close(); duplicate.close(); }
            NativeStateStore orphan = new NativeStateStore(context, null);
            for (String suffix : new String[] {".new", ".bak"}) {
                File partial = new File(path(context, orphan).getPath() + suffix);
                try (FileOutputStream stream = new FileOutputStream(partial)) { stream.write(9); }
                require(partial.setLastModified(old), "age abandoned atomic artifact");
            }
            extra.write("reclaim abandoned atomic files");
            require(!new File(path(context, orphan).getPath() + ".new").exists()
                    && !new File(path(context, orphan).getPath() + ".bak").exists(), "expiry includes abandoned atomic siblings");
            NativeStateStore discarded = new NativeStateStore(context, null);
            discarded.write("discarded");
            for (String suffix : new String[] {".new", ".bak"}) {
                try (FileOutputStream stream = new FileOutputStream(new File(path(context, discarded).getPath() + suffix))) { stream.write(9); }
            }
            discarded.delete();
            for (String suffix : new String[] {"", ".new", ".bak"}) {
                require(!new File(path(context, discarded).getPath() + suffix).exists(), "explicit discard removes every atomic sibling");
            }
            NativeStateStore current = new NativeStateStore(context, null);
            current.write("previous");
            require(path(context, current).setLastModified(old), "age current identity");
            File collision = new File(path(context, current).getPath() + ".new");
            require(collision.mkdir(), "inject invalid atomic path");
            rejected = false;
            try { current.write("replacement"); } catch (IOException expected) { rejected = true; }
            require(collision.delete(), "remove invalid atomic path");
            require(rejected && "previous".equals(current.read()), "invalid atomic path preserves previous state");
            current.write("current replacement");
            require("current replacement".equals(current.read()), "writing the current identity never expires it");
        } finally {
            File[] files = directory.listFiles();
            if (files != null) for (File file : files) require(file.isFile() && file.delete(), "clean isolated quota artifact");
            require(directory.delete(), "clean isolated quota directory");
        }
    }
}
