#import "libtermsurf_webkit.h"

#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#import <WebKit/WebKit.h>
#import <WebKit/WKNavigationDelegatePrivate.h>
#import <WebKit/WKPreferencesPrivate.h>
#import <WebKit/WKUIDelegatePrivate.h>
#import <WebKit/WKWebViewPrivate.h>
#import <WebKit/WKWebsiteDataStorePrivate.h>
#import <WebKit/_WKHitTestResult.h>
#import <WebKit/_WKInspector.h>
#import <WebKit/_WKInspectorPrivateForTesting.h>
#import <WebKit/_WKWebsiteDataStoreConfiguration.h>

#include <atomic>
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cmath>
#include <cstdlib>
#include <vector>
#include <objc/runtime.h>

@interface CAContext : NSObject
+ (instancetype)remoteContextWithOptions:(NSDictionary *)options;
@property(nonatomic, readonly) uint32_t contextId;
@property(nonatomic, retain) CALayer *layer;
- (void)invalidate;
@end

@interface NSEvent (TermSurfPrivate)
- (NSEvent *)_eventRelativeToWindow:(NSWindow *)window;
@end

@interface NSApplication (TermSurfPrivate)
- (void)_setCurrentEvent:(NSEvent *)event;
@end

@interface TSHostWindow : NSWindow
@end

@implementation TSHostWindow
- (BOOL)canBecomeKeyWindow
{
    return NO;
}

- (BOOL)canBecomeMainWindow
{
    return NO;
}
@end

struct CallbackState {
    ts_initialized_cb on_initialized = nullptr;
    void *on_initialized_data = nullptr;
    ts_tab_ready_cb on_tab_ready = nullptr;
    void *on_tab_ready_data = nullptr;
    ts_ca_context_id_cb on_ca_context_id = nullptr;
    void *on_ca_context_id_data = nullptr;
    ts_url_changed_cb on_url_changed = nullptr;
    void *on_url_changed_data = nullptr;
    ts_loading_state_cb on_loading_state = nullptr;
    void *on_loading_state_data = nullptr;
    ts_title_changed_cb on_title_changed = nullptr;
    void *on_title_changed_data = nullptr;
    ts_cursor_changed_cb on_cursor_changed = nullptr;
    void *on_cursor_changed_data = nullptr;
    ts_target_url_changed_cb on_target_url_changed = nullptr;
    void *on_target_url_changed_data = nullptr;
    ts_javascript_dialog_request_cb on_javascript_dialog_request = nullptr;
    void *on_javascript_dialog_request_data = nullptr;
    ts_console_message_cb on_console_message = nullptr;
    void *on_console_message_data = nullptr;
    ts_http_auth_request_cb on_http_auth_request = nullptr;
    void *on_http_auth_request_data = nullptr;
    ts_renderer_crashed_cb on_renderer_crashed = nullptr;
    void *on_renderer_crashed_data = nullptr;
    ts_render_probe_cb on_render_probe = nullptr;
    void *on_render_probe_data = nullptr;
};

static CallbackState g_callbacks;
static std::atomic<int> g_next_tab_id{1};
static std::atomic<uint64_t> g_next_request_id{1};
static std::atomic<int> g_test_renderer_crash_delegate_count{0};
static NSString *const TermSurfCursorChangedNotification = @"TermSurfWebKitCursorChangedNotification";
static NSString *const TermSurfCursorTypeKey = @"cursorType";
static struct WebContents *g_dispatching_mouse_contents = nullptr;
static IMP g_original_pressed_mouse_buttons = nullptr;
static IMP g_original_button_number = nullptr;

static NSURL *profileURL(NSString *basePath, NSString *component, bool directory)
{
    NSURL *baseURL = [NSURL fileURLWithPath:basePath isDirectory:YES];
    return [baseURL URLByAppendingPathComponent:component isDirectory:directory];
}

static CGFloat hostWindowAlpha()
{
    NSString *value = NSProcessInfo.processInfo.environment[@"TERMSURF_SURFARI_HOST_WINDOW_ALPHA"];
    if (!value.length)
        return 0.0;
    double alpha = value.doubleValue;
    if (alpha < 0.0)
        alpha = 0.0;
    if (alpha > 1.0)
        alpha = 1.0;
    return (CGFloat)alpha;
}

static void createProfileDirectory(NSURL *url)
{
    [[NSFileManager defaultManager] createDirectoryAtURL:url withIntermediateDirectories:YES attributes:nil error:nil];
}

static WKWebsiteDataStore *createProfileDataStore(const char *path)
{
    if (!path || !*path)
        return [WKWebsiteDataStore defaultDataStore];

    NSString *basePath = [NSString stringWithUTF8String:path];
    if (!basePath.length)
        return [WKWebsiteDataStore defaultDataStore];

    NSURL *baseURL = [NSURL fileURLWithPath:basePath isDirectory:YES];
    createProfileDirectory(baseURL);

    NSURL *cacheURL = profileURL(basePath, @"Cache", true);
    NSURL *websiteDataURL = profileURL(basePath, @"WebsiteData", true);
    NSURL *cookiesURL = profileURL(basePath, @"Cookies", true);
    NSURL *cookiesFileURL = [cookiesURL URLByAppendingPathComponent:@"Cookies.binarycookies" isDirectory:NO];

    createProfileDirectory(cacheURL);
    createProfileDirectory(websiteDataURL);
    createProfileDirectory(cookiesURL);

    _WKWebsiteDataStoreConfiguration *configuration = [[_WKWebsiteDataStoreConfiguration alloc] init];
    configuration.networkCacheSpeculativeValidationEnabled = YES;
    configuration.networkCacheDirectory = profileURL(basePath, @"Cache/NetworkCache", true);
    configuration.generalStorageDirectory = profileURL(basePath, @"WebsiteData/GeneralStorage", true);
    configuration._webStorageDirectory = profileURL(basePath, @"WebsiteData/LocalStorage", true);
    configuration._indexedDBDatabaseDirectory = profileURL(basePath, @"WebsiteData/IndexedDB", true);
    configuration._cacheStorageDirectory = profileURL(basePath, @"WebsiteData/CacheStorage", true);
    configuration._serviceWorkerRegistrationDirectory = profileURL(basePath, @"WebsiteData/ServiceWorkers", true);
    configuration._cookieStorageFile = cookiesFileURL;

    createProfileDirectory(configuration.networkCacheDirectory);
    createProfileDirectory(configuration.generalStorageDirectory);
    createProfileDirectory(configuration._webStorageDirectory);
    createProfileDirectory(configuration._indexedDBDatabaseDirectory);
    createProfileDirectory(configuration._cacheStorageDirectory);
    createProfileDirectory(configuration._serviceWorkerRegistrationDirectory);

    return [[WKWebsiteDataStore alloc] _initWithConfiguration:configuration];
}

struct BrowserContext {
    WKWebsiteDataStore *data_store;
};

struct WebContents;

@interface TSNavigationDelegate : NSObject <WKNavigationDelegatePrivate>
@property(nonatomic) WebContents *owner;
@end

@interface TSUIDelegate : NSObject <WKUIDelegatePrivate>
@property(nonatomic) WebContents *owner;
@end

@interface TSConsoleMessageHandler : NSObject <WKScriptMessageHandler>
@property(nonatomic) WebContents *owner;
@end

@interface TSPendingJavaScriptDialog : NSObject
@property(nonatomic, copy) NSString *type;
@property(nonatomic, copy) void (^alertCompletion)(void);
@property(nonatomic, copy) void (^confirmCompletion)(BOOL);
@property(nonatomic, copy) void (^promptCompletion)(NSString *);
@end

@interface TSPendingHttpAuthRequest : NSObject
@property(nonatomic, copy) void (^completion)(NSURLSessionAuthChallengeDisposition, NSURLCredential *);
@end

struct WebContents {
    int tab_id;
    int inspected_tab_id;
    bool is_devtools;
    NSWindow *window;
    WKWebView *web_view;
    _WKInspector *inspector;
    TSNavigationDelegate *navigation_delegate;
    TSUIDelegate *ui_delegate;
    TSConsoleMessageHandler *console_message_handler;
    NSMutableDictionary<NSNumber *, TSPendingJavaScriptDialog *> *pending_javascript_dialogs;
    NSMutableDictionary<NSNumber *, TSPendingHttpAuthRequest *> *pending_http_auth_requests;
    NSString *last_target_url;
    id cursor_observer;
    int last_cursor_type;
    bool suppress_cursor_notifications;
    bool renderer_crash_reported;
    CAContext *remote_context;
    CALayer *snapshot_layer;
    int width;
    int height;
    bool gui_active;
    bool focused;
    bool dark;
    NSInteger mouse_event_number = 0;
    NSInteger mouse_click_count = 0;
    NSTimeInterval mouse_click_time = 0;
    NSPoint mouse_click_position = NSZeroPoint;
    int mouse_click_button = 0;
    int mouse_last_button = 0;
    NSUInteger mouse_buttons_down = 0;
};

static void refreshSnapshotLayer(WebContents *contents);

static bool useSnapshotLayer()
{
    NSString *mode = NSProcessInfo.processInfo.environment[@"TERMSURF_SURFARI_CACONTEXT_LAYER"];
    return [mode isEqualToString:@"snapshot"];
}

static CALayer *snapshotLayerForContents(WebContents *contents)
{
    if (!contents->snapshot_layer) {
        contents->snapshot_layer = [CALayer layer];
        contents->snapshot_layer.name = @"TermSurfSurfariSnapshotLayer";
        contents->snapshot_layer.contentsGravity = kCAGravityResize;
        contents->snapshot_layer.backgroundColor = NSColor.blackColor.CGColor;
    }
    contents->snapshot_layer.frame = CGRectMake(0, 0, MAX(contents->width, 64), MAX(contents->height, 64));
    contents->snapshot_layer.contentsScale = NSScreen.mainScreen.backingScaleFactor ?: 1.0;
    return contents->snapshot_layer;
}

static CALayer *remoteContextLayerForContents(WebContents *contents)
{
    NSString *mode = NSProcessInfo.processInfo.environment[@"TERMSURF_SURFARI_CACONTEXT_LAYER"];
    if ([mode isEqualToString:@"snapshot"])
        return snapshotLayerForContents(contents);
    if ([mode isEqualToString:@"diagnostic-color"]) {
        CALayer *root = [CALayer layer];
        root.frame = CGRectMake(0, 0, MAX(contents->width, 64), MAX(contents->height, 64));
        root.backgroundColor = NSColor.blackColor.CGColor;
        root.contentsScale = NSScreen.mainScreen.backingScaleFactor ?: 1.0;

        BOOL pdf = [contents->web_view.URL.pathExtension.lowercaseString isEqualToString:@"pdf"];
        if (pdf) {
            CALayer *green = [CALayer layer];
            green.frame = root.bounds;
            green.backgroundColor = [NSColor colorWithCalibratedRed:0.0 green:0.85 blue:0.25 alpha:1.0].CGColor;
            [root addSublayer:green];
        } else {
            CGFloat halfWidth = root.bounds.size.width / 2.0;
            CALayer *cyan = [CALayer layer];
            cyan.frame = CGRectMake(0, 0, halfWidth, root.bounds.size.height);
            cyan.backgroundColor = NSColor.cyanColor.CGColor;
            [root addSublayer:cyan];

            CALayer *yellow = [CALayer layer];
            yellow.frame = CGRectMake(halfWidth, 0, root.bounds.size.width - halfWidth, root.bounds.size.height);
            yellow.backgroundColor = NSColor.yellowColor.CGColor;
            [root addSublayer:yellow];
        }

        return root;
    }
    if ([mode isEqualToString:@"content-view"]) {
        NSView *content_view = contents->window.contentView;
        content_view.wantsLayer = YES;
        [content_view layoutSubtreeIfNeeded];
        return content_view.layer ?: contents->web_view.layer;
    }
    return contents->web_view.layer;
}

static std::vector<WebContents *> g_web_contents;

static void registerContents(WebContents *contents)
{
    g_web_contents.push_back(contents);
}

static void unregisterContents(WebContents *contents)
{
    g_web_contents.erase(std::remove(g_web_contents.begin(), g_web_contents.end(), contents), g_web_contents.end());
}

static WebContents *findContentsByTabId(int tab_id)
{
    for (WebContents *contents : g_web_contents) {
        if (contents && contents->tab_id == tab_id)
            return contents;
    }
    return nullptr;
}

typedef void (*ts_webkit_test_eval_cb)(const char *result, void *user_data);
typedef void (*ts_webkit_test_task_cb)(void *user_data);

static NSString *stringFromCString(const char *value)
{
    if (!value)
        return @"";
    return [NSString stringWithUTF8String:value] ?: @"";
}

static NSURL *urlFromCString(const char *value)
{
    NSString *string = stringFromCString(value);
    if ([string length] == 0)
        return nil;

    NSURL *url = [NSURL URLWithString:string];
    if (url && url.scheme)
        return url;

    return [NSURL fileURLWithPath:string];
}

static NSEventModifierFlags cocoaModifiers(int modifiers)
{
    NSEventModifierFlags flags = 0;
    if (modifiers & (1 << 0))
        flags |= NSEventModifierFlagShift;
    if (modifiers & (1 << 1))
        flags |= NSEventModifierFlagControl;
    if (modifiers & (1 << 2))
        flags |= NSEventModifierFlagOption;
    if (modifiers & (1 << 3))
        flags |= NSEventModifierFlagCommand;
    return flags;
}

static NSPoint eventLocationInWindow(WebContents *contents, int x, int y)
{
    NSPoint localPoint = NSMakePoint(x, y);
    return [contents->web_view convertPoint:localPoint toView:nil];
}

static CGPoint eventLocationInGlobalScreen(WebContents *contents, int x, int y)
{
    NSPoint windowPoint = eventLocationInWindow(contents, x, y);
    NSPoint screenPoint = [contents->window convertPointToScreen:windowPoint];
    CGFloat screenHeight = NSScreen.screens.firstObject.frame.size.height;
    return CGPointMake(screenPoint.x, screenHeight - screenPoint.y);
}

static NSEventType mouseEventType(int type, int button)
{
    if (button == 1)
        return type == 1 ? NSEventTypeRightMouseUp : NSEventTypeRightMouseDown;
    if (button == 2)
        return type == 1 ? NSEventTypeOtherMouseUp : NSEventTypeOtherMouseDown;
    return type == 1 ? NSEventTypeLeftMouseUp : NSEventTypeLeftMouseDown;
}

static NSUInteger mouseButtonMask(int button)
{
    if (button == 1)
        return 1 << 1;
    if (button == 2)
        return 1 << 2;
    return 1 << 0;
}

static NSInteger cocoaMouseButtonNumber(int button)
{
    if (button == 1)
        return 1;
    if (button == 2)
        return 2;
    return 0;
}

static NSUInteger swizzledPressedMouseButtons(id self, SEL selector)
{
    if (g_dispatching_mouse_contents)
        return g_dispatching_mouse_contents->mouse_buttons_down;
    if (g_original_pressed_mouse_buttons) {
        auto original = reinterpret_cast<NSUInteger (*)(id, SEL)>(g_original_pressed_mouse_buttons);
        return original(self, selector);
    }
    return 0;
}

static NSInteger swizzledButtonNumber(id self, SEL selector)
{
    if (g_dispatching_mouse_contents)
        return cocoaMouseButtonNumber(g_dispatching_mouse_contents->mouse_last_button);
    if (g_original_button_number) {
        auto original = reinterpret_cast<NSInteger (*)(id, SEL)>(g_original_button_number);
        return original(self, selector);
    }
    return 0;
}

static void installMouseEventSwizzles(WebContents *contents)
{
    g_dispatching_mouse_contents = contents;

    Method pressed_method = class_getClassMethod([NSEvent class], @selector(pressedMouseButtons));
    if (pressed_method) {
        IMP replacement = reinterpret_cast<IMP>(swizzledPressedMouseButtons);
        if (!g_original_pressed_mouse_buttons)
            g_original_pressed_mouse_buttons = method_setImplementation(pressed_method, replacement);
        else
            method_setImplementation(pressed_method, replacement);
    }

    Method button_method = class_getInstanceMethod([NSEvent class], @selector(buttonNumber));
    if (button_method) {
        IMP replacement = reinterpret_cast<IMP>(swizzledButtonNumber);
        if (!g_original_button_number)
            g_original_button_number = method_setImplementation(button_method, replacement);
        else
            method_setImplementation(button_method, replacement);
    }
}

static void restoreMouseEventSwizzles()
{
    Method pressed_method = class_getClassMethod([NSEvent class], @selector(pressedMouseButtons));
    if (pressed_method && g_original_pressed_mouse_buttons)
        method_setImplementation(pressed_method, g_original_pressed_mouse_buttons);

    Method button_method = class_getInstanceMethod([NSEvent class], @selector(buttonNumber));
    if (button_method && g_original_button_number)
        method_setImplementation(button_method, g_original_button_number);

    g_dispatching_mouse_contents = nullptr;
}

static void updateClickCount(WebContents *contents, int button, NSPoint position)
{
    NSTimeInterval now = [[NSDate date] timeIntervalSince1970];
    if (now - contents->mouse_click_time < 1.0
        && NSEqualPoints(contents->mouse_click_position, position)
        && contents->mouse_click_button == button) {
        contents->mouse_click_count++;
    } else {
        contents->mouse_click_count = 1;
    }
    contents->mouse_click_time = now;
    contents->mouse_click_position = position;
    contents->mouse_click_button = button;
}

static void deliverMouseEvent(WebContents *contents, NSEvent *event)
{
    NSView *target = [contents->web_view hitTest:event.locationInWindow] ?: contents->web_view;
    installMouseEventSwizzles(contents);
    switch (event.type) {
    case NSEventTypeLeftMouseDown:
        [NSApp _setCurrentEvent:event];
        [target mouseDown:event];
        [NSApp _setCurrentEvent:nil];
        break;
    case NSEventTypeLeftMouseUp:
        [NSApp _setCurrentEvent:event];
        [target mouseUp:event];
        [NSApp _setCurrentEvent:nil];
        break;
    case NSEventTypeRightMouseDown:
        [NSApp _setCurrentEvent:event];
        [target rightMouseDown:event];
        [NSApp _setCurrentEvent:nil];
        break;
    case NSEventTypeRightMouseUp:
        [NSApp _setCurrentEvent:event];
        [target rightMouseUp:event];
        [NSApp _setCurrentEvent:nil];
        break;
    case NSEventTypeOtherMouseDown:
        [NSApp _setCurrentEvent:event];
        [target otherMouseDown:event];
        [NSApp _setCurrentEvent:nil];
        break;
    case NSEventTypeOtherMouseUp:
        [NSApp _setCurrentEvent:event];
        [target otherMouseUp:event];
        [NSApp _setCurrentEvent:nil];
        break;
    case NSEventTypeMouseMoved:
        [NSApp _setCurrentEvent:event];
        [target mouseMoved:event];
        [NSApp _setCurrentEvent:nil];
        break;
    default:
        break;
    }
    restoreMouseEventSwizzles();
}

static void withCString(NSString *value, void (^block)(const char *))
{
    block(value ? [value UTF8String] : "");
}

static void fireLoading(WebContents *contents, NSString *url, int loading)
{
    if (!g_callbacks.on_loading_state)
        return;
    withCString(url, ^(const char *c_url) {
        g_callbacks.on_loading_state(contents, c_url, loading, g_callbacks.on_loading_state_data);
    });
}

static void fireUrl(WebContents *contents, NSString *url)
{
    if (!g_callbacks.on_url_changed)
        return;
    withCString(url, ^(const char *c_url) {
        g_callbacks.on_url_changed(contents, c_url, g_callbacks.on_url_changed_data);
    });
}

static void fireTitle(WebContents *contents, NSString *title)
{
    if (!g_callbacks.on_title_changed)
        return;
    withCString(title, ^(const char *c_title) {
        g_callbacks.on_title_changed(contents, c_title, g_callbacks.on_title_changed_data);
    });
}

static bool closeToColor(NSUInteger red, NSUInteger green, NSUInteger blue, NSUInteger target_red, NSUInteger target_green, NSUInteger target_blue)
{
    const NSInteger threshold = 40;
    NSInteger dr = labs((NSInteger)red - (NSInteger)target_red);
    NSInteger dg = labs((NSInteger)green - (NSInteger)target_green);
    NSInteger db = labs((NSInteger)blue - (NSInteger)target_blue);
    return dr + dg + db <= threshold;
}

static void fireRenderProbe(
    WebContents *contents,
    NSString *method,
    NSString *status,
    int width,
    int height,
    int magenta,
    int cyan,
    int yellow,
    int webkit_green,
    NSString *error)
{
    if (!g_callbacks.on_render_probe)
        return;

    withCString(method ?: @"unknown", ^(const char *c_method) {
        withCString(status ?: @"unknown", ^(const char *c_status) {
            withCString(error ?: @"", ^(const char *c_error) {
                g_callbacks.on_render_probe(
                    contents,
                    c_method,
                    c_status,
                    width,
                    height,
                    magenta,
                    cyan,
                    yellow,
                    webkit_green,
                    c_error,
                    g_callbacks.on_render_probe_data);
            });
        });
    });
}

static void classifySnapshotImage(WebContents *contents, NSString *method, NSImage *image, NSError *error)
{
    if (error || !image) {
        fireRenderProbe(contents, method, @"capture-failed", 0, 0, 0, 0, 0, 0, error.localizedDescription ?: @"missing-image");
        return;
    }

    CGImageRef cg_image = [image CGImageForProposedRect:nil context:nil hints:nil];
    if (!cg_image) {
        fireRenderProbe(contents, method, @"capture-failed", 0, 0, 0, 0, 0, 0, @"missing-cgimage");
        return;
    }

    NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithCGImage:cg_image];
    NSInteger width = bitmap.pixelsWide;
    NSInteger height = bitmap.pixelsHigh;
    int magenta = 0;
    int cyan = 0;
    int yellow = 0;
    int webkit_green = 0;

    for (NSInteger y = 0; y < height; y++) {
        for (NSInteger x = 0; x < width; x++) {
            NSColor *color = [[bitmap colorAtX:x y:y] colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
            if (!color)
                continue;
            NSUInteger red = (NSUInteger)lrint(color.redComponent * 255.0);
            NSUInteger green = (NSUInteger)lrint(color.greenComponent * 255.0);
            NSUInteger blue = (NSUInteger)lrint(color.blueComponent * 255.0);
            if (closeToColor(red, green, blue, 255, 0, 255))
                magenta++;
            if (closeToColor(red, green, blue, 0, 255, 255))
                cyan++;
            if (closeToColor(red, green, blue, 255, 255, 0))
                yellow++;
            if (closeToColor(red, green, blue, 0, 128, 0))
                webkit_green++;
        }
    }

    NSString *status = (magenta >= 5000 || cyan >= 5000 || yellow >= 5000 || webkit_green >= 5000) ? @"pass" : @"blank";
    fireRenderProbe(contents, method, status, (int)width, (int)height, magenta, cyan, yellow, webkit_green, @"");
}

static void refreshSnapshotLayer(WebContents *contents)
{
    if (!contents || !contents->web_view || !useSnapshotLayer())
        return;

    CALayer *snapshot_layer = snapshotLayerForContents(contents);
    WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
    configuration.rect = contents->web_view.bounds;
    [contents->web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
        if (error || !snapshotImage) {
            fprintf(stderr, "[libtermsurf_webkit] snapshot-layer-refresh failed: %s\n", error.localizedDescription.UTF8String ?: "missing-image");
            return;
        }
        CGImageRef cg_image = [snapshotImage CGImageForProposedRect:nil context:nil hints:nil];
        if (!cg_image) {
            fprintf(stderr, "[libtermsurf_webkit] snapshot-layer-refresh failed: missing-cgimage\n");
            return;
        }
        [CATransaction begin];
        [CATransaction setDisableActions:YES];
        snapshot_layer.contents = (__bridge id)cg_image;
        snapshot_layer.frame = CGRectMake(0, 0, MAX(contents->width, 64), MAX(contents->height, 64));
        [CATransaction commit];
    }];
}

static void captureRenderProbe(WebContents *contents)
{
    if (!contents || !contents->web_view)
        return;
    if (!g_callbacks.on_render_probe)
        return;

    WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
    configuration.rect = contents->web_view.bounds;
    [contents->web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
        classifySnapshotImage(contents, @"WKWebView.takeSnapshot", snapshotImage, error);
    }];
}

static void fireTargetUrl(WebContents *contents, NSString *url)
{
    if (!contents || !g_callbacks.on_target_url_changed)
        return;

    NSString *target_url = url ?: @"";
    if (!contents->last_target_url && [target_url length] == 0)
        return;

    if (contents->last_target_url && [contents->last_target_url isEqualToString:target_url])
        return;

    contents->last_target_url = [target_url copy];
    withCString(target_url, ^(const char *c_url) {
        g_callbacks.on_target_url_changed(contents, c_url, g_callbacks.on_target_url_changed_data);
    });
}

static int chromiumCursorTypeForWebCoreCursorType(NSInteger cursor_type)
{
    switch (cursor_type) {
    case 3:
        return 2;
    case 4:
        return 3;
    default:
        return 0;
    }
}

static void fireCursorChanged(WebContents *contents, int cursor_type)
{
    if (!contents || !g_callbacks.on_cursor_changed)
        return;
    if (contents->suppress_cursor_notifications)
        return;
    if (contents->last_cursor_type == cursor_type)
        return;

    contents->last_cursor_type = cursor_type;
    g_callbacks.on_cursor_changed(contents, cursor_type, g_callbacks.on_cursor_changed_data);
}

static NSString *consoleBridgeScriptSource(void)
{
    return @"(() => {"
            "if (window.__termsurfConsoleInstalled) return;"
            "window.__termsurfConsoleInstalled = true;"
            "const original = {};"
            "const levels = ['log', 'info', 'warn', 'error'];"
            "function serialize(value) {"
            "  if (typeof value === 'string') return value;"
            "  if (value === undefined) return 'undefined';"
            "  if (typeof value === 'number' || typeof value === 'boolean' || value === null) return String(value);"
            "  try {"
            "    const json = JSON.stringify(value);"
            "    return json === undefined ? String(value) : json;"
            "  } catch (error) {"
            "    try { return String(value); } catch (stringError) { return '[unserializable]'; }"
            "  }"
            "}"
            "function locationFromStack() {"
            "  const stack = String((new Error()).stack || '');"
            "  const lines = stack.split('\\n');"
            "  for (let i = 0; i < lines.length; i++) {"
            "    const line = lines[i].trim();"
            "    if (!line || line.indexOf('__termsurf') !== -1 || line.indexOf('termsurfConsoleWrapper') !== -1 || line.indexOf('locationFromStack') !== -1 || line.indexOf('reportConsole') !== -1) continue;"
            "    const match = line.match(/^(.*):(\\d+):(\\d+)$/);"
            "    if (match) return { source: match[1], lineNumber: Number(match[2]) || 0 };"
            "  }"
            "  return { source: String(location.href || document.URL || ''), lineNumber: 0 };"
            "}"
            "function reportConsole(level, args) {"
            "  try {"
            "    const locationInfo = locationFromStack();"
            "    window.webkit.messageHandlers.termsurfConsole.postMessage({"
            "      level,"
            "      message: Array.prototype.map.call(args, serialize).join(' '),"
            "      lineNumber: locationInfo.lineNumber,"
            "      source: locationInfo.source"
            "    });"
            "  } catch (error) { }"
            "}"
            "levels.forEach((level) => {"
            "  original[level] = console[level];"
            "  console[level] = function termsurfConsoleWrapper() {"
            "    reportConsole(level, arguments);"
            "    if (typeof original[level] === 'function') return original[level].apply(console, arguments);"
            "  };"
            "});"
            "})();";
}

static void fireConsoleMessage(WebContents *contents, NSDictionary *body)
{
    if (!contents || !g_callbacks.on_console_message)
        return;
    if (![body isKindOfClass:NSDictionary.class])
        return;

    NSString *level = body[@"level"];
    NSString *message = body[@"message"];
    NSString *source = body[@"source"];
    NSNumber *line_number = body[@"lineNumber"];
    if (![level isKindOfClass:NSString.class] || ![message isKindOfClass:NSString.class])
        return;
    if (![source isKindOfClass:NSString.class])
        source = @"";
    if (![line_number isKindOfClass:NSNumber.class])
        line_number = @0;

    withCString(level, ^(const char *c_level) {
        withCString(message, ^(const char *c_message) {
            withCString(source, ^(const char *c_source) {
                g_callbacks.on_console_message(
                    contents,
                    c_level,
                    c_message,
                    line_number.intValue,
                    c_source,
                    g_callbacks.on_console_message_data);
            });
        });
    });
}

static NSString *rendererCrashReason(NSInteger reason)
{
    switch (reason) {
    case 0:
        return @"memory";
    case 1:
        return @"cpu";
    case 2:
        return @"requested";
    case 3:
        return @"crash";
    case 4:
        return @"crash-limit";
    default:
        return @"unknown";
    }
}

static void fireRendererCrashed(WebContents *contents, NSString *reason)
{
    if (!contents || !g_callbacks.on_renderer_crashed)
        return;
    if (contents->renderer_crash_reported)
        return;

    contents->renderer_crash_reported = true;
    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    bool visible = contents->window.visible;
    withCString(reason ?: @"unknown", ^(const char *c_reason) {
        withCString(url, ^(const char *c_url) {
            g_callbacks.on_renderer_crashed(
                contents,
                c_reason,
                0,
                c_url,
                visible,
                g_callbacks.on_renderer_crashed_data);
        });
    });
}

static void installCursorObserver(WebContents *contents)
{
    contents->cursor_observer = [[NSNotificationCenter defaultCenter]
        addObserverForName:TermSurfCursorChangedNotification
                    object:contents->web_view
                     queue:nil
                usingBlock:^(NSNotification *notification) {
                    NSNumber *cursor_type = notification.userInfo[TermSurfCursorTypeKey];
                    if (![cursor_type isKindOfClass:NSNumber.class])
                        return;
                    fireCursorChanged(contents, chromiumCursorTypeForWebCoreCursorType(cursor_type.integerValue));
                }];
}

static void fireJavaScriptDialog(
    WebContents *contents,
    uint64_t request_id,
    NSString *dialog_type,
    NSString *origin_url,
    NSString *message,
    NSString *default_prompt_text)
{
    if (!g_callbacks.on_javascript_dialog_request)
        return;

    withCString(dialog_type, ^(const char *c_dialog_type) {
        withCString(origin_url, ^(const char *c_origin_url) {
            withCString(message, ^(const char *c_message) {
                withCString(default_prompt_text, ^(const char *c_default_prompt_text) {
                    g_callbacks.on_javascript_dialog_request(
                        contents,
                        request_id,
                        c_dialog_type,
                        c_origin_url,
                        c_message,
                        c_default_prompt_text,
                        g_callbacks.on_javascript_dialog_request_data);
                });
            });
        });
    });
}

static NSString *httpAuthScheme(NSURLAuthenticationChallenge *challenge)
{
    NSString *method = challenge.protectionSpace.authenticationMethod;
    if ([method isEqualToString:NSURLAuthenticationMethodHTTPBasic])
        return @"basic";
    return @"";
}

static NSString *httpAuthChallenger(WKWebView *webView, NSURLAuthenticationChallenge *challenge)
{
    NSURLProtectionSpace *space = challenge.protectionSpace;
    NSString *scheme = space.protocol ?: webView.URL.scheme ?: @"http";
    NSString *host = space.host ?: @"";
    NSInteger port = space.port;
    BOOL defaultPort = ([scheme isEqualToString:@"http"] && port == 80) || ([scheme isEqualToString:@"https"] && port == 443);
    if (port > 0 && !defaultPort)
        return [NSString stringWithFormat:@"%@://%@:%ld", scheme, host, (long)port];
    return [NSString stringWithFormat:@"%@://%@", scheme, host];
}

static bool isSupportedHttpAuthChallenge(NSURLAuthenticationChallenge *challenge)
{
    NSURLProtectionSpace *space = challenge.protectionSpace;
    return !space.isProxy && [space.authenticationMethod isEqualToString:NSURLAuthenticationMethodHTTPBasic];
}

static void fireHttpAuthRequest(
    WebContents *contents,
    uint64_t request_id,
    NSString *url,
    NSString *auth_scheme,
    NSString *challenger,
    NSString *realm,
    bool is_proxy,
    bool first_auth_attempt,
    bool is_primary_main_frame_navigation,
    bool is_navigation)
{
    if (!g_callbacks.on_http_auth_request)
        return;

    withCString(url, ^(const char *c_url) {
        withCString(auth_scheme, ^(const char *c_auth_scheme) {
            withCString(challenger, ^(const char *c_challenger) {
                withCString(realm, ^(const char *c_realm) {
                    g_callbacks.on_http_auth_request(
                        contents,
                        request_id,
                        c_url,
                        c_auth_scheme,
                        c_challenger,
                        c_realm,
                        is_proxy,
                        first_auth_attempt,
                        is_primary_main_frame_navigation,
                        is_navigation,
                        g_callbacks.on_http_auth_request_data);
                });
            });
        });
    });
}

static void exportContext(WebContents *contents)
{
    if (!contents || !contents->web_view)
        return;

    [contents->web_view layoutSubtreeIfNeeded];
    if (!contents->remote_context) {
        contents->remote_context = [CAContext remoteContextWithOptions:@{
            @"kCAContextCIFilterBehavior" : @"ignore",
        }];
        contents->remote_context.layer = remoteContextLayerForContents(contents);
    }
    refreshSnapshotLayer(contents);

    if (g_callbacks.on_ca_context_id) {
        g_callbacks.on_ca_context_id(
            contents,
            contents->remote_context.contextId,
            contents->width,
            contents->height,
            g_callbacks.on_ca_context_id_data);
    }
}

@implementation TSNavigationDelegate
- (void)webView:(WKWebView *)webView didStartProvisionalNavigation:(WKNavigation *)navigation
{
    (void)navigation;
    if (self.owner)
        self.owner->renderer_crash_reported = false;
    fireLoading(self.owner, webView.URL.absoluteString, 1);
}

- (void)_webView:(WKWebView *)webView webContentProcessDidTerminateWithReason:(_WKProcessTerminationReason)reason
{
    (void)webView;
    g_test_renderer_crash_delegate_count++;
    printf("CALLBACK renderer_crash_delegate reason=%s\n", rendererCrashReason((NSInteger)reason).UTF8String);
    fflush(stdout);
    fireRendererCrashed(self.owner, rendererCrashReason((NSInteger)reason));
}

- (void)webViewWebContentProcessDidTerminate:(WKWebView *)webView
{
    (void)webView;
    fireRendererCrashed(self.owner, @"unknown");
}

- (void)webView:(WKWebView *)webView didCommitNavigation:(WKNavigation *)navigation
{
    (void)navigation;
    fireUrl(self.owner, webView.URL.absoluteString);
}

- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation
{
    (void)navigation;
    fireUrl(self.owner, webView.URL.absoluteString);
    [webView evaluateJavaScript:@"document.title" completionHandler:^(id result, NSError *error) {
        if (error)
            NSLog(@"[libtermsurf_webkit] document.title evaluation failed: %@", error);
        NSString *title = [result isKindOfClass:NSString.class] ? result : webView.title;
        fireTitle(self.owner, title);
        fireLoading(self.owner, webView.URL.absoluteString, 0);
        exportContext(self.owner);
        refreshSnapshotLayer(self.owner);
        captureRenderProbe(self.owner);
    }];
}

- (void)webView:(WKWebView *)webView didFailNavigation:(WKNavigation *)navigation withError:(NSError *)error
{
    (void)navigation;
    NSLog(@"[libtermsurf_webkit] navigation failed: %@", error);
    fireLoading(self.owner, webView.URL.absoluteString, 0);
}

- (void)webView:(WKWebView *)webView didFailProvisionalNavigation:(WKNavigation *)navigation withError:(NSError *)error
{
    (void)navigation;
    NSLog(@"[libtermsurf_webkit] provisional navigation failed: %@", error);
    fireLoading(self.owner, webView.URL.absoluteString, 0);
}

- (void)webView:(WKWebView *)webView didReceiveAuthenticationChallenge:(NSURLAuthenticationChallenge *)challenge completionHandler:(void (^)(NSURLSessionAuthChallengeDisposition, NSURLCredential *))completionHandler
{
    WebContents *contents = self.owner;
    if (!contents || !g_callbacks.on_http_auth_request || !isSupportedHttpAuthChallenge(challenge)) {
        completionHandler(NSURLSessionAuthChallengeRejectProtectionSpace, nil);
        return;
    }

    uint64_t request_id = g_next_request_id.fetch_add(1);
    TSPendingHttpAuthRequest *pending = [[TSPendingHttpAuthRequest alloc] init];
    pending.completion = completionHandler;
    contents->pending_http_auth_requests[@(request_id)] = pending;

    NSURLProtectionSpace *space = challenge.protectionSpace;
    NSString *url = webView.URL.absoluteString ?: @"";
    fireHttpAuthRequest(
        contents,
        request_id,
        url,
        httpAuthScheme(challenge),
        httpAuthChallenger(webView, challenge),
        space.realm ?: @"",
        space.isProxy,
        challenge.previousFailureCount == 0,
        true,
        true);
}
@end

@implementation TSPendingJavaScriptDialog
@end

@implementation TSPendingHttpAuthRequest
@end

@implementation TSConsoleMessageHandler
- (void)userContentController:(WKUserContentController *)userContentController didReceiveScriptMessage:(WKScriptMessage *)message
{
    (void)userContentController;
    fireConsoleMessage(self.owner, [message.body isKindOfClass:NSDictionary.class] ? message.body : nil);
}
@end

@implementation TSUIDelegate
- (void)_webView:(WKWebView *)webView mouseDidMoveOverElement:(_WKHitTestResult *)hitTestResult withFlags:(NSEventModifierFlags)flags userInfo:(id<NSSecureCoding>)userInfo
{
    (void)webView;
    (void)flags;
    (void)userInfo;
    fireTargetUrl(self.owner, hitTestResult.absoluteLinkURL.absoluteString);
}

- (void)webView:(WKWebView *)webView runJavaScriptAlertPanelWithMessage:(NSString *)message initiatedByFrame:(WKFrameInfo *)frame completionHandler:(void (^)(void))completionHandler
{
    (void)webView;
    WebContents *contents = self.owner;
    if (!contents || !g_callbacks.on_javascript_dialog_request) {
        completionHandler();
        return;
    }

    uint64_t request_id = g_next_request_id.fetch_add(1);
    TSPendingJavaScriptDialog *pending = [[TSPendingJavaScriptDialog alloc] init];
    pending.type = @"alert";
    pending.alertCompletion = completionHandler;
    contents->pending_javascript_dialogs[@(request_id)] = pending;
    fireJavaScriptDialog(contents, request_id, @"alert", frame.request.URL.absoluteString, message, @"");
}

- (void)webView:(WKWebView *)webView runJavaScriptConfirmPanelWithMessage:(NSString *)message initiatedByFrame:(WKFrameInfo *)frame completionHandler:(void (^)(BOOL))completionHandler
{
    (void)webView;
    WebContents *contents = self.owner;
    if (!contents || !g_callbacks.on_javascript_dialog_request) {
        completionHandler(NO);
        return;
    }

    uint64_t request_id = g_next_request_id.fetch_add(1);
    TSPendingJavaScriptDialog *pending = [[TSPendingJavaScriptDialog alloc] init];
    pending.type = @"confirm";
    pending.confirmCompletion = completionHandler;
    contents->pending_javascript_dialogs[@(request_id)] = pending;
    fireJavaScriptDialog(contents, request_id, @"confirm", frame.request.URL.absoluteString, message, @"");
}

- (void)webView:(WKWebView *)webView runJavaScriptTextInputPanelWithPrompt:(NSString *)prompt defaultText:(NSString *)defaultText initiatedByFrame:(WKFrameInfo *)frame completionHandler:(void (^)(NSString *))completionHandler
{
    (void)webView;
    WebContents *contents = self.owner;
    if (!contents || !g_callbacks.on_javascript_dialog_request) {
        completionHandler(nil);
        return;
    }

    uint64_t request_id = g_next_request_id.fetch_add(1);
    TSPendingJavaScriptDialog *pending = [[TSPendingJavaScriptDialog alloc] init];
    pending.type = @"prompt";
    pending.promptCompletion = completionHandler;
    contents->pending_javascript_dialogs[@(request_id)] = pending;
    fireJavaScriptDialog(contents, request_id, @"prompt", frame.request.URL.absoluteString, prompt, defaultText ?: @"");
}
@end

int ts_content_main(int argc, const char *const *argv)
{
    (void)argc;
    (void)argv;

    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        [application setActivationPolicy:NSApplicationActivationPolicyAccessory];

        dispatch_async(dispatch_get_main_queue(), ^{
            if (g_callbacks.on_initialized)
                g_callbacks.on_initialized(g_callbacks.on_initialized_data);
        });

        [application run];
    }

    return 0;
}

void ts_set_on_initialized(ts_initialized_cb callback, void *user_data)
{
    g_callbacks.on_initialized = callback;
    g_callbacks.on_initialized_data = user_data;
}

void ts_post_task(ts_task_cb task, void *user_data)
{
    if (!task)
        return;
    dispatch_async(dispatch_get_main_queue(), ^{
        task(user_data);
    });
}

void ts_quit(void)
{
    dispatch_async(dispatch_get_main_queue(), ^{
        [NSApp terminate:nil];
    });
}

ts_browser_context_t ts_create_browser_context(const char *path)
{
    BrowserContext *context = new BrowserContext;
    context->data_store = createProfileDataStore(path);
    return context;
}

ts_browser_context_t ts_create_incognito_browser_context(void)
{
    BrowserContext *context = new BrowserContext;
    context->data_store = [WKWebsiteDataStore nonPersistentDataStore];
    return context;
}

void ts_destroy_browser_context(ts_browser_context_t ctx)
{
    delete static_cast<BrowserContext *>(ctx);
}

ts_web_contents_t ts_create_web_contents(ts_browser_context_t ctx, const char *url, int width, int height, bool dark)
{
    BrowserContext *context = static_cast<BrowserContext *>(ctx);
    if (!context)
        return nullptr;

    WebContents *contents = new WebContents;
    contents->tab_id = g_next_tab_id.fetch_add(1);
    contents->inspected_tab_id = 0;
    contents->is_devtools = false;
    contents->inspector = nil;
    contents->width = width;
    contents->height = height;
    contents->gui_active = true;
    contents->focused = false;
    contents->dark = dark;
    contents->last_cursor_type = -999;
    contents->suppress_cursor_notifications = false;
    contents->renderer_crash_reported = false;
    contents->pending_javascript_dialogs = [[NSMutableDictionary alloc] init];
    contents->pending_http_auth_requests = [[NSMutableDictionary alloc] init];

    NSRect frame = NSMakeRect(80, 80, MAX(width, 64), MAX(height, 64));
    contents->window = [[TSHostWindow alloc] initWithContentRect:frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    contents->window.releasedWhenClosed = NO;
    contents->window.title = @"libtermsurf_webkit";
    contents->window.acceptsMouseMovedEvents = YES;
    contents->window.ignoresMouseEvents = YES;
    contents->window.alphaValue = hostWindowAlpha();

    WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
    configuration.websiteDataStore = context->data_store;
    configuration.preferences._developerExtrasEnabled = YES;
    WKUserContentController *user_content_controller = [[WKUserContentController alloc] init];
    contents->console_message_handler = [[TSConsoleMessageHandler alloc] init];
    contents->console_message_handler.owner = contents;
    [user_content_controller addScriptMessageHandler:contents->console_message_handler name:@"termsurfConsole"];
    WKUserScript *console_script = [[WKUserScript alloc] initWithSource:consoleBridgeScriptSource()
                                                          injectionTime:WKUserScriptInjectionTimeAtDocumentStart
                                                       forMainFrameOnly:NO];
    [user_content_controller addUserScript:console_script];
    configuration.userContentController = user_content_controller;
    contents->web_view = [[WKWebView alloc] initWithFrame:contents->window.contentView.bounds configuration:configuration];
    contents->web_view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    contents->web_view.wantsLayer = YES;
    contents->web_view.appearance = [NSAppearance appearanceNamed:dark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua];

    contents->navigation_delegate = [[TSNavigationDelegate alloc] init];
    contents->navigation_delegate.owner = contents;
    contents->web_view.navigationDelegate = contents->navigation_delegate;
    contents->ui_delegate = [[TSUIDelegate alloc] init];
    contents->ui_delegate.owner = contents;
    contents->web_view.UIDelegate = contents->ui_delegate;
    installCursorObserver(contents);

    [contents->window.contentView addSubview:contents->web_view];
    [contents->window orderFront:nil];
    registerContents(contents);

    if (g_callbacks.on_tab_ready)
        g_callbacks.on_tab_ready(contents, contents->tab_id, g_callbacks.on_tab_ready_data);

    exportContext(contents);
    ts_load_url(contents, url);
    return contents;
}

ts_web_contents_t ts_create_devtools_web_contents(
    ts_browser_context_t ctx,
    int inspected_tab_id,
    int width,
    int height,
    bool dark)
{
    (void)ctx;
    WebContents *inspected = findContentsByTabId(inspected_tab_id);
    if (!inspected || !inspected->web_view) {
        fprintf(stderr, "[libtermsurf_webkit] devtools-unsupported inspected_tab_id=%d reason=missing-inspected-tab\n", inspected_tab_id);
        return nullptr;
    }

    _WKInspector *inspector = inspected->web_view._inspector;
    if (!inspector) {
        fprintf(stderr, "[libtermsurf_webkit] devtools-unsupported inspected_tab_id=%d reason=missing-inspector\n", inspected_tab_id);
        return nullptr;
    }

    [inspector show];
    WKWebView *inspector_web_view = [inspector inspectorWebView];
    if (!inspector_web_view) {
        fprintf(stderr, "[libtermsurf_webkit] devtools-unsupported inspected_tab_id=%d reason=missing-inspector-webview\n", inspected_tab_id);
        return nullptr;
    }

    WebContents *contents = new WebContents;
    contents->tab_id = g_next_tab_id.fetch_add(1);
    contents->inspected_tab_id = inspected_tab_id;
    contents->is_devtools = true;
    contents->inspector = inspector;
    contents->width = width;
    contents->height = height;
    contents->gui_active = true;
    contents->focused = false;
    contents->dark = dark;
    contents->last_cursor_type = -999;
    contents->suppress_cursor_notifications = false;
    contents->renderer_crash_reported = false;
    contents->pending_javascript_dialogs = [[NSMutableDictionary alloc] init];
    contents->pending_http_auth_requests = [[NSMutableDictionary alloc] init];

    NSRect frame = NSMakeRect(120, 120, MAX(width, 64), MAX(height, 64));
    contents->window = [[TSHostWindow alloc] initWithContentRect:frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    contents->window.releasedWhenClosed = NO;
    contents->window.title = @"libtermsurf_webkit_devtools";
    contents->window.acceptsMouseMovedEvents = YES;
    contents->window.ignoresMouseEvents = YES;
    contents->window.alphaValue = hostWindowAlpha();

    contents->web_view = inspector_web_view;
    [contents->web_view removeFromSuperview];
    contents->web_view.frame = contents->window.contentView.bounds;
    contents->web_view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    contents->web_view.wantsLayer = YES;
    contents->web_view.appearance = [NSAppearance appearanceNamed:dark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua];
    installCursorObserver(contents);

    [contents->window.contentView addSubview:contents->web_view];
    [contents->window orderFront:nil];
    registerContents(contents);

    if (g_callbacks.on_tab_ready)
        g_callbacks.on_tab_ready(contents, contents->tab_id, g_callbacks.on_tab_ready_data);

    exportContext(contents);
    return contents;
}

void ts_destroy_web_contents(ts_web_contents_t wc)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    unregisterContents(contents);
    [contents->remote_context invalidate];
    if (contents->cursor_observer)
        [[NSNotificationCenter defaultCenter] removeObserver:contents->cursor_observer];
    if (contents->is_devtools) {
        [contents->inspector close];
    } else {
        contents->web_view.navigationDelegate = nil;
        contents->web_view.UIDelegate = nil;
        [contents->web_view.configuration.userContentController removeScriptMessageHandlerForName:@"termsurfConsole"];
        contents->console_message_handler.owner = nullptr;
    }
    [contents->pending_javascript_dialogs removeAllObjects];
    [contents->pending_http_auth_requests removeAllObjects];
    [contents->web_view removeFromSuperview];
    [contents->window close];
    delete contents;
}

void ts_load_url(ts_web_contents_t wc, const char *url)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    NSURL *ns_url = urlFromCString(url);
    if (!contents || !ns_url)
        return;

    if (ns_url.isFileURL) {
        NSURL *directory = [ns_url URLByDeletingLastPathComponent];
        [contents->web_view loadFileURL:ns_url allowingReadAccessToURL:directory];
        return;
    }

    [contents->web_view loadRequest:[NSURLRequest requestWithURL:ns_url]];
}

void ts_set_view_size(
    ts_web_contents_t wc,
    int width,
    int height,
    double screen_x,
    double screen_y,
    double screen_width,
    double screen_height,
    double screen_scale)
{
    (void)screen_x;
    (void)screen_y;
    (void)screen_width;
    (void)screen_height;
    (void)screen_scale;

    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    contents->width = width;
    contents->height = height;
    NSRect frame = contents->window.frame;
    frame.size = NSMakeSize(MAX(width, 64), MAX(height, 64));
    [contents->window setFrame:frame display:YES animate:NO];
    contents->web_view.frame = contents->window.contentView.bounds;
    [contents->web_view layoutSubtreeIfNeeded];
    exportContext(contents);
    refreshSnapshotLayer(contents);
}

void ts_forward_mouse_event(ts_web_contents_t wc, int type, int button, int x, int y, int click_count, int modifiers)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    NSPoint location = eventLocationInWindow(contents, x, y);
    bool is_up = type == 1;
    if (!is_up) {
        updateClickCount(contents, button, location);
        contents->mouse_buttons_down |= mouseButtonMask(button);
    } else {
        contents->mouse_buttons_down &= ~mouseButtonMask(button);
    }
    contents->mouse_last_button = button;

    NSEvent *event = [NSEvent mouseEventWithType:mouseEventType(type, button)
        location:location
        modifierFlags:cocoaModifiers(modifiers)
        timestamp:[[NSDate date] timeIntervalSince1970]
        windowNumber:contents->window.windowNumber
        context:[NSGraphicsContext currentContext]
        eventNumber:++contents->mouse_event_number
        clickCount:MAX(click_count, (int)contents->mouse_click_count)
        pressure:type == 0 ? 1.0 : 0.0];
    deliverMouseEvent(contents, event);
}

void ts_forward_mouse_move(ts_web_contents_t wc, int x, int y, int modifiers)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    bool is_drag = contents->mouse_buttons_down & mouseButtonMask(0);
    NSEvent *event = [NSEvent mouseEventWithType:is_drag ? NSEventTypeLeftMouseDragged : NSEventTypeMouseMoved
        location:eventLocationInWindow(contents, x, y)
        modifierFlags:cocoaModifiers(modifiers)
        timestamp:[[NSDate date] timeIntervalSince1970]
        windowNumber:contents->window.windowNumber
        context:[NSGraphicsContext currentContext]
        eventNumber:++contents->mouse_event_number
        clickCount:is_drag ? contents->mouse_click_count : 0
        pressure:0.0];
    [NSApp _setCurrentEvent:event];
    contents->suppress_cursor_notifications = true;
    if (is_drag)
        [contents->web_view mouseDragged:event];
    else
        [contents->web_view _simulateMouseMove:event];
    contents->suppress_cursor_notifications = false;
    [NSApp _setCurrentEvent:nil];
}

void ts_forward_scroll_event(
    ts_web_contents_t wc,
    int x,
    int y,
    float delta_x,
    float delta_y,
    int phase,
    int momentum_phase,
    bool precise,
    int modifiers)
{
    (void)phase;
    (void)momentum_phase;
    (void)precise;

    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    CGEventRef cg_event = CGEventCreateScrollWheelEvent2(nullptr, kCGScrollEventUnitPixel, 2, delta_y, delta_x, 0);
    if (!cg_event)
        return;
    CGEventSetLocation(cg_event, eventLocationInGlobalScreen(contents, x, y));
    CGEventSetFlags(cg_event, (CGEventFlags)cocoaModifiers(modifiers));
    CGEventSetIntegerValueField(cg_event, kCGScrollWheelEventIsContinuous, precise ? 1 : 0);
    NSEvent *event = [NSEvent eventWithCGEvent:cg_event];
    CFRelease(cg_event);
    NSView *target = [contents->web_view hitTest:event.locationInWindow] ?: contents->web_view;
    [NSApp _setCurrentEvent:event];
    [target scrollWheel:event];
    [NSApp _setCurrentEvent:nil];
}

void ts_forward_key_event(ts_web_contents_t wc, int type, int keycode, const char *utf8, int modifiers)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    NSString *characters = stringFromCString(utf8);
    NSEventType eventType = type == 1 ? NSEventTypeKeyUp : NSEventTypeKeyDown;
    NSEvent *event = [NSEvent keyEventWithType:eventType
        location:NSMakePoint(0, 0)
        modifierFlags:cocoaModifiers(modifiers)
        timestamp:[[NSDate date] timeIntervalSince1970]
        windowNumber:contents->window.windowNumber
        context:nil
        characters:characters
        charactersIgnoringModifiers:characters
        isARepeat:type == 2
        keyCode:(unsigned short)keycode];

    if (eventType == NSEventTypeKeyUp)
    {
        [NSApp _setCurrentEvent:event];
        [contents->web_view keyUp:event];
        [NSApp _setCurrentEvent:nil];
    } else {
        [NSApp _setCurrentEvent:event];
        [contents->web_view keyDown:event];
        [NSApp _setCurrentEvent:nil];
    }
}

void ts_set_focus(ts_web_contents_t wc, bool focused)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    contents->focused = focused;
    if (!focused) {
        [contents->window makeFirstResponder:nil];
        [contents->window resignKeyWindow];
    }
}

void ts_set_gui_active(ts_web_contents_t wc, bool active, const char *reason)
{
    (void)reason;
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    contents->gui_active = active;
    if (!active) {
        [contents->window makeFirstResponder:nil];
        [contents->window resignKeyWindow];
    }
}

void ts_set_color_scheme(ts_web_contents_t wc, bool dark)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    contents->dark = dark;
    contents->web_view.appearance = [NSAppearance appearanceNamed:dark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua];
}

bool ts_reply_javascript_dialog(ts_web_contents_t wc, uint64_t request_id, bool accepted, const char *prompt_text)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return false;

    NSNumber *key = @(request_id);
    TSPendingJavaScriptDialog *pending = contents->pending_javascript_dialogs[key];
    if (!pending)
        return false;

    [contents->pending_javascript_dialogs removeObjectForKey:key];
    if ([pending.type isEqualToString:@"alert"]) {
        pending.alertCompletion();
        return true;
    }
    if ([pending.type isEqualToString:@"confirm"]) {
        pending.confirmCompletion(accepted);
        return true;
    }
    if ([pending.type isEqualToString:@"prompt"]) {
        pending.promptCompletion(accepted ? stringFromCString(prompt_text) : nil);
        return true;
    }
    return false;
}

bool ts_reply_http_auth(ts_web_contents_t wc, uint64_t request_id, bool accepted, const char *username, const char *password)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return false;

    NSNumber *key = @(request_id);
    TSPendingHttpAuthRequest *pending = contents->pending_http_auth_requests[key];
    if (!pending)
        return false;

    [contents->pending_http_auth_requests removeObjectForKey:key];
    if (accepted) {
        NSURLCredential *credential = [NSURLCredential credentialWithUser:stringFromCString(username)
                                                                  password:stringFromCString(password)
                                                               persistence:NSURLCredentialPersistenceForSession];
        pending.completion(NSURLSessionAuthChallengeUseCredential, credential);
    } else {
        pending.completion(NSURLSessionAuthChallengeCancelAuthenticationChallenge, nil);
    }
    return true;
}

void ts_set_on_tab_ready(ts_tab_ready_cb cb, void *user_data)
{
    g_callbacks.on_tab_ready = cb;
    g_callbacks.on_tab_ready_data = user_data;
}

void ts_set_on_ca_context_id(ts_ca_context_id_cb cb, void *user_data)
{
    g_callbacks.on_ca_context_id = cb;
    g_callbacks.on_ca_context_id_data = user_data;
}

void ts_set_on_url_changed(ts_url_changed_cb cb, void *user_data)
{
    g_callbacks.on_url_changed = cb;
    g_callbacks.on_url_changed_data = user_data;
}

void ts_set_on_loading_state(ts_loading_state_cb cb, void *user_data)
{
    g_callbacks.on_loading_state = cb;
    g_callbacks.on_loading_state_data = user_data;
}

void ts_set_on_title_changed(ts_title_changed_cb cb, void *user_data)
{
    g_callbacks.on_title_changed = cb;
    g_callbacks.on_title_changed_data = user_data;
}

void ts_set_on_cursor_changed(ts_cursor_changed_cb cb, void *user_data)
{
    g_callbacks.on_cursor_changed = cb;
    g_callbacks.on_cursor_changed_data = user_data;
}

void ts_set_on_target_url_changed(ts_target_url_changed_cb cb, void *user_data)
{
    g_callbacks.on_target_url_changed = cb;
    g_callbacks.on_target_url_changed_data = user_data;
}

void ts_set_on_javascript_dialog_request(ts_javascript_dialog_request_cb cb, void *user_data)
{
    g_callbacks.on_javascript_dialog_request = cb;
    g_callbacks.on_javascript_dialog_request_data = user_data;
}

void ts_set_on_console_message(ts_console_message_cb cb, void *user_data)
{
    g_callbacks.on_console_message = cb;
    g_callbacks.on_console_message_data = user_data;
}

void ts_set_on_http_auth_request(ts_http_auth_request_cb cb, void *user_data)
{
    g_callbacks.on_http_auth_request = cb;
    g_callbacks.on_http_auth_request_data = user_data;
}

void ts_set_on_renderer_crashed(ts_renderer_crashed_cb cb, void *user_data)
{
    g_callbacks.on_renderer_crashed = cb;
    g_callbacks.on_renderer_crashed_data = user_data;
}

void ts_set_on_render_probe(ts_render_probe_cb cb, void *user_data)
{
    g_callbacks.on_render_probe = cb;
    g_callbacks.on_render_probe_data = user_data;
}

void ts_webkit_test_capture_render_probe(ts_web_contents_t wc)
{
    captureRenderProbe(static_cast<WebContents *>(wc));
}

extern "C" void ts_webkit_test_evaluate_javascript(
    ts_web_contents_t wc,
    const char *script,
    ts_webkit_test_eval_cb callback,
    void *user_data)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents || !callback)
        return;

    NSString *source = stringFromCString(script);
    [contents->web_view evaluateJavaScript:source completionHandler:^(id result, NSError *error) {
        NSString *value = @"";
        if (error)
            value = [NSString stringWithFormat:@"ERROR:%@", error.localizedDescription];
        else if ([result isKindOfClass:NSString.class])
            value = result;
        else if (result)
            value = [result description];
        withCString(value, ^(const char *c_value) {
            callback(c_value, user_data);
        });
    }];
}

extern "C" void ts_webkit_test_post_delayed_task(double seconds, ts_webkit_test_task_cb callback, void *user_data)
{
    if (!callback)
        return;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(seconds * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        callback(user_data);
    });
}

extern "C" void ts_webkit_test_kill_web_content_process(ts_web_contents_t wc)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;
    [contents->web_view _killWebContentProcessAndResetState];
}

extern "C" int ts_webkit_test_renderer_crash_delegate_count(void)
{
    return g_test_renderer_crash_delegate_count.load();
}
