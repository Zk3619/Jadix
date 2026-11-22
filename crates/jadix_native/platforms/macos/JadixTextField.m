#import "JadixNative.h"
#import <objc/message.h>
#import <objc/runtime.h>
#include <util.h>
#include <unistd.h>
#include <fcntl.h>
#include <signal.h>
#include <sys/ioctl.h>
#include <sys/wait.h>
#include <errno.h>

int32_t JadixTerminalSpawn(const char *const *argv, uint16_t columns, uint16_t rows, int32_t *master) {
    if (!argv || !argv[0] || !master) return -1;
    struct winsize size = { .ws_row = rows, .ws_col = columns };
    pid_t pid = forkpty(master, NULL, NULL, &size);
    if (pid == 0) { execvp(argv[0], (char *const *)argv); _exit(127); }
    if (pid > 0) { fcntl(*master, F_SETFD, FD_CLOEXEC); fcntl(*master, F_SETFL, O_NONBLOCK); }
    return pid;
}
bool JadixTerminalResize(int32_t master, uint16_t columns, uint16_t rows) {
    struct winsize size = { .ws_row = rows, .ws_col = columns };
    return ioctl(master, TIOCSWINSZ, &size) == 0;
}
int32_t JadixTerminalWait(int32_t pid) {
    // Keep the child identity reserved until Stop reaps it, so a late disposal
    // cannot signal a different process that reused its PID.
    siginfo_t info = {0}; int result;
    do { result = waitid(P_PID, pid, &info, WEXITED | WNOWAIT); } while (result < 0 && errno == EINTR);
    if (result < 0) return -1;
    return info.si_code == CLD_EXITED ? info.si_status : 128 + info.si_status;
}
void JadixTerminalStop(int32_t pid) {
    if (pid <= 0) return;
    int status = 0; pid_t result;
    do { result = waitpid(pid, &status, WNOHANG); } while (result < 0 && errno == EINTR);
    if (result != 0) return;
    kill(-pid, SIGHUP); kill(-pid, SIGKILL); kill(pid, SIGKILL);
    do { result = waitpid(pid, &status, 0); } while (result < 0 && errno == EINTR);
}

// AppKit sends the text-view delegate message to the owner of the field editor.
// Keep the superclass's decision when its implementation has the selector.
static BOOL AllowFieldEdit(NSTextField *field, Class superclass, SEL action,
                           NSTextView *editor, NSRange range, NSString *replacement,
                           JadixFieldEditFilter filter) {
    if ([superclass instancesRespondToSelector:action]) {
        struct objc_super receiver = {field, superclass};
        BOOL (*send)(struct objc_super *, SEL, NSTextView *, NSRange, NSString *) =
            (BOOL (*)(struct objc_super *, SEL, NSTextView *, NSRange, NSString *))objc_msgSendSuper;
        if (!send(&receiver, action, editor, range, replacement)) {
            return NO;
        }
    }
    return filter == nil || filter(field, editor, range, replacement);
}

@implementation JadixTextField
- (BOOL)becomeFirstResponder {
    BOOL accepted = [super becomeFirstResponder];
    if (accepted && self.didAcquireEditor) {
        self.didAcquireEditor(self);
    }
    return accepted;
}

- (void)mouseDown:(NSEvent *)event {
    BOOL wasEditing = self.currentEditor != nil;
    [super mouseDown:event];
    if (!wasEditing && self.currentEditor && self.didAcquireEditor) {
        self.didAcquireEditor(self);
    }
}

- (BOOL)textView:(NSTextView *)editor shouldChangeTextInRange:(NSRange)range
    replacementString:(NSString *)replacement {
    return AllowFieldEdit(self, NSTextField.class, _cmd, editor, range, replacement, self.editFilter);
}
@end

@implementation JadixSecureTextField
- (BOOL)becomeFirstResponder {
    BOOL accepted = [super becomeFirstResponder];
    if (accepted && self.didAcquireEditor) {
        self.didAcquireEditor(self);
    }
    return accepted;
}

- (void)mouseDown:(NSEvent *)event {
    BOOL wasEditing = self.currentEditor != nil;
    [super mouseDown:event];
    if (!wasEditing && self.currentEditor && self.didAcquireEditor) {
        self.didAcquireEditor(self);
    }
}

- (BOOL)textView:(NSTextView *)editor shouldChangeTextInRange:(NSRange)range
    replacementString:(NSString *)replacement {
    return AllowFieldEdit(self, NSSecureTextField.class, _cmd, editor, range, replacement, self.editFilter);
}
@end

// Compile against macOS 14 SDKs while using the public macOS 26 glass class when present.
void JadixUpdateGlassView(NSView *view, BOOL clear, NSColor *tint) {
    SEL style = NSSelectorFromString(@"setStyle:");
    if ([view respondsToSelector:style]) ((void (*)(id, SEL, NSInteger))objc_msgSend)(view, style, clear ? 1 : 0);
    SEL color = NSSelectorFromString(@"setTintColor:");
    if ([view respondsToSelector:color]) ((void (*)(id, SEL, id))objc_msgSend)(view, color, tint);
}
static NSView *JadixGlassHitTest(id view, SEL selector, NSPoint point) { return nil; }
NSView *JadixMakeGlassView(BOOL clear, NSColor *tint) {
    Class type = NSClassFromString(@"NSGlassEffectView");
    if (type == Nil || ![type isSubclassOfClass:NSView.class]) return nil;
    // A decoration cannot consume presses intended for the host control.
    Class passive = NSClassFromString(@"JadixPassiveGlassEffectView");
    if (passive == Nil) {
        passive = objc_allocateClassPair(type, "JadixPassiveGlassEffectView", 0);
        if (passive == Nil) return nil;
        Method hitTest = class_getInstanceMethod(NSView.class, @selector(hitTest:));
        class_addMethod(passive, @selector(hitTest:), (IMP)JadixGlassHitTest, method_getTypeEncoding(hitTest));
        objc_registerClassPair(passive);
    }
    NSView *view = [[passive alloc] initWithFrame:NSZeroRect];
    JadixUpdateGlassView(view, clear, tint);
    return view;
}
