package dev.jadix.nativeui;

import android.view.*;
import android.widget.TextView;

/** The OS owns selection handles and editing actions; ancestors yield their gestures. */
final class NativeSelection implements ActionMode.Callback {
    private final TextView editor;
    private NativeSelection(TextView editor) { this.editor = editor; }
    static void install(TextView editor) {
        NativeSelection callback = new NativeSelection(editor);
        editor.setCustomSelectionActionModeCallback(callback);
        editor.setCustomInsertionActionModeCallback(callback);
    }
    @Override public boolean onCreateActionMode(ActionMode mode, Menu menu) {
        if (editor.getParent() != null) editor.getParent().requestDisallowInterceptTouchEvent(true);
        return true;
    }
    @Override public boolean onPrepareActionMode(ActionMode mode, Menu menu) { return false; }
    @Override public boolean onActionItemClicked(ActionMode mode, MenuItem item) { return false; }
    @Override public void onDestroyActionMode(ActionMode mode) {
        if (editor.getParent() != null) editor.getParent().requestDisallowInterceptTouchEvent(false);
    }
}
