#pragma once
char *jadix_native_viewport_plan(const char *json);
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
char *jadix_native_environment(uint64_t root, const char *json);

typedef bool (*JadixNativeMeasure)(void *context, const char *json, double *size);
typedef bool (*JadixNativePublish)(void *context, const char *json);
typedef void (*JadixNativeClear)(void *context);
typedef bool (*JadixNativeFileRequest)(void *context, uint64_t request, const char *json);
typedef bool (*JadixNativeWindowRequest)(void *context, uint64_t request, const char *json);
typedef bool (*JadixNativeAlertRequest)(void *context, uint64_t request, const char *json);
typedef bool (*JadixNativeShareRequest)(void *context, uint64_t request, const char *json);
typedef bool (*JadixNativeContextMenuRequest)(void *context, uint64_t request, const char *json);

uint64_t jadix_native_create(void *context, JadixNativeMeasure measure, JadixNativePublish publish,
                            JadixNativeClear clear, float width, float height);
uint64_t jadix_native_create_with_file_requests(void *context, JadixNativeMeasure measure,
    JadixNativePublish publish, JadixNativeClear clear, JadixNativeFileRequest file_request,
    float width, float height);
bool jadix_native_file_result(uint64_t root, uint64_t request, const char *json);
uint64_t jadix_native_create_with_requests(void *context, JadixNativeMeasure measure,
    JadixNativePublish publish, JadixNativeClear clear, JadixNativeFileRequest file_request,
    JadixNativeWindowRequest window_request, float width, float height);
/* This constructor gives measure three output slots: width, height, and first baseline or NaN. */
uint64_t jadix_native_create_with_baseline(void *context, JadixNativeMeasure measure,
    JadixNativePublish publish, JadixNativeClear clear, JadixNativeFileRequest file_request,
    JadixNativeWindowRequest window_request, float width, float height);
/* Like the baseline constructor, with an additional system alert callback. */
uint64_t jadix_native_create_with_alerts(void *context, JadixNativeMeasure measure,
    JadixNativePublish publish, JadixNativeClear clear, JadixNativeFileRequest file_request,
    JadixNativeWindowRequest window_request, JadixNativeAlertRequest alert_request,
    float width, float height);
bool jadix_native_alert_result(uint64_t root, uint64_t request, const char *json);
uint64_t jadix_native_create_with_services(void *context, JadixNativeMeasure measure,
    JadixNativePublish publish, JadixNativeClear clear, JadixNativeFileRequest file_request,
    JadixNativeWindowRequest window_request, JadixNativeAlertRequest alert_request,
    JadixNativeShareRequest share_request, float width, float height);
uint64_t jadix_native_create_example_with_content(void *context, JadixNativeMeasure measure,
    JadixNativePublish publish, JadixNativeClear clear, JadixNativeFileRequest file_request,
    JadixNativeWindowRequest window_request, JadixNativeAlertRequest alert_request,
    JadixNativeShareRequest share_request, float width, float height, const char *content);
bool jadix_native_share_result(uint64_t root, uint64_t request, const char *json);
bool jadix_native_enable_context_menus(uint64_t root, JadixNativeContextMenuRequest callback);
bool jadix_native_context_menu_result(uint64_t root, uint64_t request, const char *json);
bool jadix_native_menu_command(uint64_t root, uint64_t revision, const char *command);
bool jadix_native_set_example_window_menu(uint64_t root, const char *locale);
bool jadix_native_show_example_context_menu(uint64_t root, const char *locale);
bool jadix_native_window_result(uint64_t root, uint64_t request, const char *json);
bool jadix_native_window_state(uint64_t root, const char *json);
bool jadix_native_window_event(uint64_t root, const char *json);
/* Application cleanup, called on the main thread by the application delegate. */
void jadix_native_collect_app_quit(void);
bool jadix_native_poll_app_quit(void);
/* A NULL address clears the current address. */
char *jadix_native_deep_link(uint64_t root, const char *json, bool configure);
char *jadix_native_set_app_address(uint64_t root, const char *address);
char *jadix_native_event(uint64_t root, const char *json);
char *jadix_native_resize(uint64_t root, float width, float height, uint64_t environment);
char *jadix_native_state(uint64_t root);
char *jadix_native_restore(uint64_t root, const char *json);
char *jadix_native_storage_result(uint64_t root, const char *error);
void jadix_native_dispose(uint64_t root);
void jadix_native_string_free(char *value);
char *jadix_native_rich_html(const char *html);
int8_t jadix_native_pan_decision(uint8_t axis, double x, double y, double active_x, double active_y, double fail_x, double fail_y);
char *jadix_native_rich_html_plan(const char *html);
char *jadix_native_rich_image_size(double width, double height, double natural_width, double natural_height);

typedef void (*JadixNativeSvgRaster)(void *context, const uint8_t *rgba, uint32_t width,
                                    uint32_t height, float natural_width, float natural_height);
/* fit: 0 contain, 1 cover/fill, 2 scale down, 3 natural size. */
void jadix_native_rasterize_svg(const uint8_t *bytes, size_t len, uint32_t target_width,
                              uint32_t target_height, uint8_t fit, void *context,
                              JadixNativeSvgRaster callback);

void jadix_native_web_poll(uint64_t root);
bool jadix_native_web_resource_allowed(uint64_t root, const char *url);
uint64_t jadix_native_web_resource_request(uint64_t root, const char *node, const char *url, const char *method);
void jadix_native_web_resource_cancel(uint64_t root, uint64_t request);
void jadix_native_display_tick(uint64_t root, double refresh_rate, double work_seconds, bool changed);
char *jadix_native_performance(uint64_t root);
bool jadix_native_web_ready(uint64_t root, const char *node, bool ready);
bool jadix_native_web_complete(uint64_t root, uint64_t request, bool success, const char *json);

bool jadix_native_tray_command(uint64_t root, uint64_t revision, const char *command);

#ifdef __OBJC__
#import <AppKit/AppKit.h>
NSView * _Nullable JadixMakeGlassView(BOOL clear, NSColor * _Nullable tint);
void JadixUpdateGlassView(NSView * _Nonnull view, BOOL clear, NSColor * _Nullable tint);

typedef BOOL (^JadixFieldEditFilter)(NSTextField * _Nonnull field,
                                     NSTextView * _Nonnull editor, NSRange range,
                                     NSString * _Nullable replacement);

@interface JadixTextField : NSTextField
@property (nonatomic, copy, nullable) JadixFieldEditFilter editFilter;
@property (nonatomic, copy, nullable) void (^didAcquireEditor)(NSTextField * _Nonnull field);
@end

@interface JadixSecureTextField : NSSecureTextField
@property (nonatomic, copy, nullable) JadixFieldEditFilter editFilter;
@property (nonatomic, copy, nullable) void (^didAcquireEditor)(NSTextField * _Nonnull field);
@end
#endif

bool jadix_native_audio_event(uint64_t root, uint32_t object, const char *json);
bool jadix_native_speech_event(uint64_t root, uint32_t session, const char *json);
bool jadix_native_fetch_event(uint64_t root, uint32_t request, const char *json);
uint64_t jadix_terminal_screen_new(uint16_t columns, uint16_t rows, size_t scrollback);
char *jadix_terminal_screen_feed(uint64_t screen, const uint8_t *bytes, size_t length);
bool jadix_terminal_screen_resize(uint64_t screen, uint16_t columns, uint16_t rows);
bool jadix_terminal_screen_configure(uint64_t screen, uint16_t columns, uint16_t rows, size_t scrollback);
char *jadix_terminal_screen_frame(uint64_t screen);
void jadix_terminal_screen_free(uint64_t screen);
int32_t JadixTerminalSpawn(const char *const *argv, uint16_t columns, uint16_t rows, int32_t *master);
bool JadixTerminalResize(int32_t master, uint16_t columns, uint16_t rows);
int32_t JadixTerminalWait(int32_t pid);
void JadixTerminalStop(int32_t pid);
bool jadix_native_desktop_service_event(uint64_t root, uint32_t object, const char *json);

/* Main-thread application service, independent of window root lifetimes. */
bool jadix_native_updater_configure(const char *json, const char *platform, bool can_install);
char *jadix_native_updater_commands(void);
bool jadix_native_updater_event(uint32_t generation, const char *json);
bool jadix_native_updater_chunk(uint32_t generation, const uint8_t *bytes, size_t length);
char *jadix_native_updater_unpack(const char *archive, const char *destination);
