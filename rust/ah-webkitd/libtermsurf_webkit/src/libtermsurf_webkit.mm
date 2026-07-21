#import "libtermsurf_webkit.h"

#import <Cocoa/Cocoa.h>
#import <CommonCrypto/CommonDigest.h>
#import <PDFKit/PDFKit.h>
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
#include <cstring>
#include <vector>
#include <objc/runtime.h>
#include <unistd.h>

@interface NSEvent (TermSurfPrivate)
- (NSEvent *)_eventRelativeToWindow:(NSWindow *)window;
@end

@interface NSApplication (TermSurfPrivate)
- (void)_setCurrentEvent:(NSEvent *)event;
@end

@interface TSHostWindow : NSWindow
@end

static bool g_pdf_copy_bridge_allows_key_main = false;

static NSString *pdfResponderProbeModeRaw()
{
    if (NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_RESPONDER_PROBE"].length == 0)
        return nil;
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_RESPONDER_MODE"];
    return mode.length ? mode : @"baseline";
}

static bool pdfResponderProbeModeIs(NSString *mode)
{
    return [pdfResponderProbeModeRaw() isEqualToString:mode];
}

@implementation TSHostWindow
- (BOOL)canBecomeKeyWindow
{
    if (g_pdf_copy_bridge_allows_key_main)
        return YES;
    if (pdfResponderProbeModeIs(@"key-window") || pdfResponderProbeModeIs(@"key-main-window"))
        return YES;
    return NO;
}

- (BOOL)canBecomeMainWindow
{
    if (g_pdf_copy_bridge_allows_key_main)
        return YES;
    if (pdfResponderProbeModeIs(@"main-window") || pdfResponderProbeModeIs(@"key-main-window"))
        return YES;
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
    ts_navigation_state_cb on_navigation_state = nullptr;
    void *on_navigation_state_data = nullptr;
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
static std::atomic<bool> g_native_ui_probe_started{false};
static std::atomic<bool> g_synthetic_print_probe_started{false};

static NSURL *profileURL(NSString *basePath, NSString *component, bool directory)
{
    NSURL *baseURL = [NSURL fileURLWithPath:basePath isDirectory:YES];
    return [baseURL URLByAppendingPathComponent:component isDirectory:directory];
}

static NSString *safariApplicationNameForUserAgent()
{
    NSArray<NSString *> *infoPlistPaths = @[
        @"/Applications/Safari.app/Contents/Info.plist",
        @"/System/Volumes/Preboot/Cryptexes/App/System/Applications/Safari.app/Contents/Info.plist",
    ];

    for (NSString *path in infoPlistPaths) {
        NSDictionary *info = [NSDictionary dictionaryWithContentsOfFile:path];
        NSString *version = [info objectForKey:@"CFBundleShortVersionString"];
        if (![version isKindOfClass:NSString.class] || !version.length)
            continue;

        return [NSString stringWithFormat:@"Version/%@ Safari/605.1.15", version];
    }

    fprintf(stderr, "[libtermsurf_webkit] could not read Safari version; leaving applicationNameForUserAgent unset\n");
    return nil;
}

static CGFloat hostWindowAlpha()
{
    NSString *value = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_HOST_WINDOW_ALPHA"];
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
static bool currentUrlLooksPdf(WebContents *contents);

@interface TSNavigationDelegate : NSObject <WKNavigationDelegatePrivate>
@property(nonatomic) WebContents *owner;
@end

@interface TSNavigationStateObserver : NSObject
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
    TSNavigationStateObserver *navigation_state_observer;
    TSUIDelegate *ui_delegate;
    TSConsoleMessageHandler *console_message_handler;
    NSMutableDictionary<NSNumber *, TSPendingJavaScriptDialog *> *pending_javascript_dialogs;
    NSMutableDictionary<NSNumber *, TSPendingHttpAuthRequest *> *pending_http_auth_requests;
    NSString *last_target_url;
    id cursor_observer;
    int last_cursor_type;
    uint64_t cursor_probe_generation;
    bool suppress_cursor_notifications;
    bool renderer_crash_reported;
    bool renderer_crashed;
    bool has_committed_document;
    uint32_t live_context_id;
    bool presentation_visible;
    NSString *last_render_probe_pass_url;
    NSString *pdf_load_watchdog_url;
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
    NSPoint pdf_selected_text_drag_start = NSZeroPoint;
    bool pdf_selected_text_drag_exceeded_threshold = false;
    NSString *pdf_selected_text_cache;
    NSString *pdf_selected_text_cache_phase;
    NSString *pdf_selected_text_cache_url;
    NSTimeInterval pdf_selected_text_cache_time = 0;
    uint64_t pdf_selected_text_generation = 0;
    uint64_t pdf_selected_text_cache_generation = 0;
    uint64_t pdf_selected_text_cache_epoch = 0;
    uint64_t pdf_selected_text_cache_capture_epoch = 0;
    bool pdf_selected_text_cache_consumed = false;
    NSString *pdf_selected_text_copy_start_pasteboard;
    bool pdf_selected_text_stabilization_active = false;
    NSTimeInterval pdf_selected_text_stabilization_deadline = 0;
    uint64_t pdf_selected_text_stabilization_epoch = 0;
    NSString *pdf_selection_state_transition_url;
    NSString *pdf_selection_state_transition_pending_url;
    bool pdf_selection_state_transition_consumed = false;
    bool pdf_selection_state_transition_pending = false;
    bool pdf_find_session_active = false;
    NSMutableString *pdf_find_query;
    NSString *pdf_editable_document_url;
    bool pdf_editable_document_known = false;
    bool pdf_editable_document_has_widgets = false;
    NSString *pdf_editable_document_reason;
};

static void tracePdfViewGeometry(WebContents *contents, NSString *label, int x, int y, NSPoint windowPoint);
static NSString *routeTraceStringSample(NSString *value);
static NSString *describeScrollViews(NSView *view);
static NSView *findDescendantViewWithClassName(NSView *view, NSString *className);
static NSSize hostWindowPointSizeForContents(WebContents *contents);

static bool pdfCopyTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_COPY_TRACE"].length > 0;
}

static bool pdfCopyInProcessProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_COPY_INPROCESS"].length > 0;
}

static bool pdfCopyDirectEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_COPY_DIRECT"].length > 0;
}

static bool pdfCopyBridgeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_COPY_BRIDGE"].length > 0;
}

static bool webkitCursorTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_CURSOR_TRACE"].length > 0;
}

static bool webkitScrollTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SCROLL_TRACE"].length > 0
        || NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SCROLL_TRACE_FILE"].length > 0;
}

static NSString *webkitScrollDispatchMode()
{
    NSString *value = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SCROLL_DISPATCH_MODE"];
    return value.length ? value : @"window-send-event";
}

static void appendWebKitScrollTrace(NSString *line)
{
    if (!webkitScrollTraceEnabled())
        return;

    NSString *entry = [[@"webkit-scroll " stringByAppendingString:line ?: @""] stringByAppendingString:@"\n"];
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SCROLL_TRACE_FILE"];
    if (!path.length) {
        fprintf(stderr, "%s", entry.UTF8String ?: "");
        return;
    }

    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void traceWebKitCursor(WebContents *contents, NSString *phase, NSInteger raw_cursor_type, int mapped_cursor_type)
{
    if (!webkitCursorTraceEnabled())
        return;

    fprintf(stderr,
        "[libtermsurf_webkit] webkit-cursor phase=%s tab=%d raw=%ld mapped=%d suppress=%d last=%d url=%s\n",
        phase.UTF8String ?: "",
        contents ? contents->tab_id : 0,
        (long)raw_cursor_type,
        mapped_cursor_type,
        contents && contents->suppress_cursor_notifications ? 1 : 0,
        contents ? contents->last_cursor_type : 0,
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString.UTF8String : "");
}

static bool cursorNotificationBelongsToContents(WebContents *contents, id object)
{
    if (!contents || !contents->web_view || !object)
        return false;
    if (object == contents->web_view)
        return true;
    if ([object isKindOfClass:NSView.class])
        return [(NSView *)object isDescendantOf:contents->web_view];
    return false;
}

static void traceWebKitCursorNotification(WebContents *contents, NSString *phase, id object, NSInteger raw_cursor_type, int mapped_cursor_type)
{
    if (!webkitCursorTraceEnabled())
        return;

    NSString *object_class = object ? NSStringFromClass([object class]) : @"nil";
    fprintf(stderr,
        "[libtermsurf_webkit] webkit-cursor phase=%s tab=%d raw=%ld mapped=%d suppress=%d last=%d object=%s belongs=%d url=%s\n",
        phase.UTF8String ?: "",
        contents ? contents->tab_id : 0,
        (long)raw_cursor_type,
        mapped_cursor_type,
        contents && contents->suppress_cursor_notifications ? 1 : 0,
        contents ? contents->last_cursor_type : 0,
        object_class.UTF8String ?: "",
        cursorNotificationBelongsToContents(contents, object) ? 1 : 0,
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString.UTF8String : "");
}

static NSString *pdfCopyBridgeMode()
{
    if (!pdfCopyBridgeEnabled())
        return nil;
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_COPY_BRIDGE_MODE"];
    return mode.length ? mode : @"baseline";
}

static NSString *pdfMouseDispatchProbeMode()
{
    if (NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_MOUSE_DISPATCH_PROBE"].length == 0)
        return nil;
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_MOUSE_DISPATCH_MODE"];
    return mode.length ? mode : @"current";
}

static NSString *pdfSelectionEdgeProbeMode()
{
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_EDGE_MODE"];
    if (mode.length == 0)
        return nil;
    if (NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_EDGE_PROBE"].length == 0)
        return nil;
    return mode;
}

static CGFloat pdfSelectionEdgeDeltaX()
{
    NSString *value = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_EDGE_DELTA_X"];
    if (value.length == 0)
        return 0;
    return value.doubleValue;
}

static bool pdfViewGeometryTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VIEW_GEOMETRY_TRACE"].length > 0;
}

static bool pdfSelectionSurfaceTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_SURFACE_TRACE"].length > 0;
}

static bool pdfSelectedTextRouteTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_ROUTE_TRACE"].length > 0;
}

static bool pdfSelectedTextCacheCopyEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_CACHE_COPY"].length > 0;
}

static bool pdfSelectedTextCacheCopyTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_CACHE_COPY_TRACE"].length > 0;
}

static bool pdfSelectedTextStabilizedCaptureEnabled()
{
    return pdfSelectedTextCacheCopyEnabled() && NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_STABILIZED_CAPTURE"].length > 0;
}

static bool pdfSelectionGeometryRemediationEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_GEOMETRY_REMEDIATION"].length > 0;
}

static NSString *pdfSelectionGeometryRemediationMode()
{
    if (!pdfSelectionGeometryRemediationEnabled())
        return nil;
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_GEOMETRY_REMEDIATION_MODE"];
    return mode.length ? mode : @"geometry-only";
}

static bool pdfSelectionGeometryRemediationModeIs(NSString *mode, NSString *expected)
{
    return [mode isEqualToString:expected] || ([mode isEqualToString:@"combined"] && ([expected isEqualToString:@"geometry-only"] || [expected isEqualToString:@"focus-only"]));
}

static bool pdfSelectionStateTransitionEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_STATE_TRANSITION"].length > 0;
}

static bool pdfPointerPrimeProductionDisabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_POINTER_PRIME_DISABLE"].length > 0;
}

static bool pdfHudSaveTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_HUD_SAVE_TRACE"].length > 0;
}

static bool pdfActionProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_ACTION_PROBE"].length > 0;
}

static bool pdfStateOracleProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_STATE_ORACLE_PROBE"].length > 0;
}

static bool pdfVisualOracleProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VISUAL_ORACLE_PROBE"].length > 0;
}

static bool pdfDirectCommandProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_DIRECT_COMMAND_PROBE"].length > 0;
}

static bool pdfPrintOperationProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_OPERATION_PROBE"].length > 0;
}

static bool pdfPrintDialogProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_DIALOG_PROBE"].length > 0;
}

static bool pdfPrintModalProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_MODAL_PROBE"].length > 0;
}

static bool printPresentationProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_PROBE"].length > 0;
}

static bool nativeUiProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_PROBE"].length > 0;
}

static bool syntheticPrintProbeEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_PROBE"].length > 0;
}

static bool pdfProductionZoomTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRODUCTION_ZOOM_TRACE_FILE"].length > 0;
}

static bool pdfFormOracleTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_FORM_ORACLE_TRACE_FILE"].length > 0;
}

static bool pdfViewHierarchyTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VIEW_HIERARCHY_TRACE_FILE"].length > 0;
}

static bool pdfPointerPrimeProductionEnabled()
{
    return !pdfPointerPrimeProductionDisabled() && !pdfSelectionStateTransitionEnabled();
}

static NSString *pdfSelectionStateTransitionMode()
{
    if (!pdfSelectionStateTransitionEnabled())
        return nil;
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_STATE_TRANSITION_MODE"];
    return mode.length ? mode : @"focus-prime";
}

static bool pdfSelectionStateTransitionModeIs(NSString *mode, NSString *expected)
{
    if ([mode isEqualToString:expected])
        return true;
    if ([mode isEqualToString:@"focus-pointer"] && ([expected isEqualToString:@"focus-prime"] || [expected isEqualToString:@"pointer-prime"]))
        return true;
    if ([mode isEqualToString:@"focus-pointer-clamp"] && ([expected isEqualToString:@"focus-prime"] || [expected isEqualToString:@"pointer-prime"] || [expected isEqualToString:@"clamp"]))
        return true;
    return false;
}

static NSTimeInterval pdfSelectedTextCacheMaxAge()
{
    NSString *value = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_CACHE_MAX_AGE"];
    if (!value.length)
        return 2.0;
    double seconds = value.doubleValue;
    if (seconds < 0.1)
        seconds = 0.1;
    if (seconds > 30.0)
        seconds = 30.0;
    return seconds;
}

static NSString *pdfResponderProbeMode()
{
    return pdfResponderProbeModeRaw();
}

static NSString *describeObject(id object)
{
    if (!object)
        return @"nil";
    return [NSString stringWithFormat:@"%@:%p", NSStringFromClass([object class]), object];
}

static NSString *describeView(NSView *view)
{
    if (!view)
        return @"nil";
    return [NSString stringWithFormat:@"%@:%p frame=%@ bounds=%@ hidden=%d alpha=%.3f",
                     NSStringFromClass([view class]),
                     view,
                     NSStringFromRect(view.frame),
                     NSStringFromRect(view.bounds),
                     view.hidden ? 1 : 0,
                     view.alphaValue];
}

static NSString *responderChain(NSResponder *responder)
{
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    NSResponder *current = responder;
    for (int i = 0; current && i < 12; i++) {
        [items addObject:describeObject(current)];
        current = current.nextResponder;
    }
    if (current)
        [items addObject:@"..."];
    return [items componentsJoinedByString:@">"];
}

static NSString *clipboardSample()
{
    NSString *value = [NSPasteboard.generalPasteboard stringForType:NSPasteboardTypeString] ?: @"";
    NSString *sample = value.length > 120 ? [value substringToIndex:120] : value;
    sample = [[sample stringByReplacingOccurrencesOfString:@"\n" withString:@" "] stringByReplacingOccurrencesOfString:@"\t" withString:@" "];
    return [NSString stringWithFormat:@"len=%lu change=%ld sample=%@", (unsigned long)value.length, (long)NSPasteboard.generalPasteboard.changeCount, sample];
}

static NSString *copyTargetValidation(id target, SEL action)
{
    if (!target)
        return @"target=nil";

    NSMutableArray<NSString *> *parts = [NSMutableArray array];
    [parts addObject:describeObject(target)];
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:@"Copy" action:action keyEquivalent:@""];
    item.target = target;

    if ([target respondsToSelector:@selector(validateUserInterfaceItem:)]) {
        BOOL result = NO;
        @try {
            result = [target validateUserInterfaceItem:item];
            [parts addObject:[NSString stringWithFormat:@"validateUserInterfaceItem=%d", result ? 1 : 0]];
        } @catch (NSException *exception) {
            [parts addObject:[NSString stringWithFormat:@"validateUserInterfaceItem=exception:%@", exception.name]];
        }
    } else {
        [parts addObject:@"validateUserInterfaceItem=unavailable"];
    }

    if ([target respondsToSelector:@selector(validateMenuItem:)]) {
        BOOL result = NO;
        @try {
            result = [target validateMenuItem:item];
            [parts addObject:[NSString stringWithFormat:@"validateMenuItem=%d", result ? 1 : 0]];
        } @catch (NSException *exception) {
            [parts addObject:[NSString stringWithFormat:@"validateMenuItem=exception:%@", exception.name]];
        }
    } else {
        [parts addObject:@"validateMenuItem=unavailable"];
    }

    return [parts componentsJoinedByString:@","];
}

static void appendPdfCopyTrace(NSString *line)
{
    if (!pdfCopyTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_COPY_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-copy-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static NSString *sha256ForData(NSData *data)
{
    if (!data)
        return @"";
    unsigned char digest[CC_SHA256_DIGEST_LENGTH];
    CC_SHA256(data.bytes, (CC_LONG)data.length, digest);
    NSMutableString *result = [NSMutableString stringWithCapacity:CC_SHA256_DIGEST_LENGTH * 2];
    for (int i = 0; i < CC_SHA256_DIGEST_LENGTH; i++)
        [result appendFormat:@"%02x", digest[i]];
    return result;
}

static void appendPdfHudSaveTrace(NSString *line)
{
    if (!pdfHudSaveTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_HUD_SAVE_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-hud-save-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfHudSave(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!pdfHudSaveTraceEnabled())
        return;
    appendPdfHudSaveTrace([NSString stringWithFormat:@"webkit-pdf-hud-save tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static bool pdfKeyboardTraceEnabled()
{
    return NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_KEYBOARD_TRACE"].length > 0
        || NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_KEYBOARD_TRACE_FILE"].length > 0;
}

static void appendPdfKeyboardTrace(NSString *line)
{
    if (!pdfKeyboardTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_KEYBOARD_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-keyboard-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfKeyboard(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!pdfKeyboardTraceEnabled())
        return;
    appendPdfKeyboardTrace([NSString stringWithFormat:@"webkit-pdf-keyboard tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPdfActionTrace(NSString *line)
{
    if (!pdfActionProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_ACTION_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-action-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfAction(WebContents *contents, NSString *phase, NSString *action, NSString *detail)
{
    if (!pdfActionProbeEnabled())
        return;
    appendPdfActionTrace([NSString stringWithFormat:@"webkit-pdf-action tab=%d phase=%@ action=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        action ?: @"none",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPdfStateOracleTrace(NSString *line)
{
    if (!pdfStateOracleProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_STATE_ORACLE_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-state-oracle-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfStateOracle(WebContents *contents, NSString *phase, NSString *action, NSString *detail)
{
    if (!pdfStateOracleProbeEnabled())
        return;
    appendPdfStateOracleTrace([NSString stringWithFormat:@"webkit-pdf-state-oracle tab=%d phase=%@ action=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        action ?: @"none",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void tracePdfPasswordOracle(WebContents *contents, NSString *phase)
{
    if (!pdfStateOracleProbeEnabled() || !contents || !currentUrlLooksPdf(contents))
        return;

    NSString *script = @"(() => {"
        "const text = ((document.body && document.body.innerText) || '').toLowerCase();"
        "const active = document.activeElement;"
        "const activeType = active && active.getAttribute ? (active.getAttribute('type') || '') : '';"
        "const activeTag = active && active.tagName ? active.tagName : '';"
        "const activeValueLength = active && typeof active.value === 'string' ? active.value.length : 0;"
        "const activeSelectionStart = active && typeof active.selectionStart === 'number' ? active.selectionStart : null;"
        "const activeSelectionEnd = active && typeof active.selectionEnd === 'number' ? active.selectionEnd : null;"
        "const activeSelectionLength = activeSelectionStart !== null && activeSelectionEnd !== null ? activeSelectionEnd - activeSelectionStart : null;"
        "const passwordInput = document.querySelector('input[type=\"password\"]');"
        "if (!window.__termsurfPdfPasswordEvents) window.__termsurfPdfPasswordEvents = [];"
        "const recordEvent = (event) => {"
        "window.__termsurfPdfPasswordEvents.push({"
        "type: event.type,"
        "key: typeof event.key === 'string' ? event.key : '',"
        "code: typeof event.code === 'string' ? event.code : '',"
        "keyIdentifier: typeof event.keyIdentifier === 'string' ? event.keyIdentifier : '',"
        "keyCode: typeof event.keyCode === 'number' ? event.keyCode : null,"
        "which: typeof event.which === 'number' ? event.which : null,"
        "charCode: typeof event.charCode === 'number' ? event.charCode : null,"
        "inputType: typeof event.inputType === 'string' ? event.inputType : '',"
        "defaultPrevented: !!event.defaultPrevented,"
        "activeTag: document.activeElement && document.activeElement.tagName ? document.activeElement.tagName : '',"
        "targetTag: event.target && event.target.tagName ? event.target.tagName : '',"
        "valueLength: event.target && typeof event.target.value === 'string' ? event.target.value.length : null"
        "});"
        "if (window.__termsurfPdfPasswordEvents.length > 40) window.__termsurfPdfPasswordEvents.shift();"
        "};"
        "if (passwordInput && !passwordInput.__termsurfPdfPasswordProbe) {"
        "passwordInput.__termsurfPdfPasswordProbe = true;"
        "['keydown','keyup','beforeinput','input'].forEach((type) => passwordInput.addEventListener(type, recordEvent, true));"
        "}"
        "const passwordValueLength = passwordInput && typeof passwordInput.value === 'string' ? passwordInput.value.length : 0;"
        "const passwordSelectionStart = passwordInput && typeof passwordInput.selectionStart === 'number' ? passwordInput.selectionStart : null;"
        "const passwordSelectionEnd = passwordInput && typeof passwordInput.selectionEnd === 'number' ? passwordInput.selectionEnd : null;"
        "const passwordSelectionLength = passwordSelectionStart !== null && passwordSelectionEnd !== null ? passwordSelectionEnd - passwordSelectionStart : null;"
        "return JSON.stringify({"
        "passwordForms: document.querySelectorAll('.password-form').length,"
        "passwordInputs: document.querySelectorAll('input[type=\"password\"]').length,"
        "hasPasswordText: text.includes('password'),"
        "hasInvalidText: text.includes('invalid') || text.includes('incorrect'),"
        "activeTag,"
        "activeType,"
        "activeValueLength,"
        "activeSelectionStart,"
        "activeSelectionEnd,"
        "activeSelectionLength,"
        "passwordValueLength,"
        "passwordSelectionStart,"
        "passwordSelectionEnd,"
        "passwordSelectionLength,"
        "recentEvents: window.__termsurfPdfPasswordEvents.slice(-8),"
        "title: document.title || ''"
        "});"
        "})()";
    NSString *phaseCopy = [phase copy] ?: @"unknown";
    int tab = contents->tab_id;
    WKWebView *webView = contents->web_view;
    [webView evaluateJavaScript:script completionHandler:^(id result, NSError *error) {
        NSString *url = webView.URL.absoluteString ?: @"";
        NSString *detail = error
            ? [NSString stringWithFormat:@"status=error error=%@", error.localizedDescription ?: @""]
            : [NSString stringWithFormat:@"status=ok state=%@", [result isKindOfClass:NSString.class] ? result : @""];
        appendPdfStateOracleTrace([NSString stringWithFormat:@"webkit-pdf-password-oracle tab=%d phase=%@ action=pdf-password url=%@ %@",
            tab,
            phaseCopy,
            url,
            detail]);
    }];
}

static void appendPdfFormOracleTrace(NSString *line)
{
    if (!pdfFormOracleTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_FORM_ORACLE_TRACE_FILE"];
    if (!path.length)
        return;
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void appendPdfViewHierarchyTrace(NSString *line)
{
    if (!pdfViewHierarchyTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VIEW_HIERARCHY_TRACE_FILE"];
    if (!path.length)
        return;
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static NSString *pdfFormOracleToken(NSString *value)
{
    if (!value.length)
        return @"";
    NSMutableString *result = [value mutableCopy];
    [result replaceOccurrencesOfString:@" " withString:@"_" options:0 range:NSMakeRange(0, result.length)];
    [result replaceOccurrencesOfString:@"\n" withString:@"\\n" options:0 range:NSMakeRange(0, result.length)];
    [result replaceOccurrencesOfString:@"\r" withString:@"\\r" options:0 range:NSMakeRange(0, result.length)];
    [result replaceOccurrencesOfString:@"\t" withString:@"\\t" options:0 range:NSMakeRange(0, result.length)];
    return result;
}

static bool classNameContains(Class klass, NSString *needle)
{
    for (Class current = klass; current; current = class_getSuperclass(current)) {
        if ([NSStringFromClass(current).lowercaseString containsString:needle.lowercaseString])
            return true;
    }
    return false;
}

static id pdfAnnotationValueForKey(PDFAnnotation *annotation, NSString *key)
{
    if (!annotation || !key.length)
        return nil;
    @try {
        return [annotation valueForKey:key];
    } @catch (NSException *) {
        return nil;
    }
}

static PDFDocument *safePDFDocumentFromObject(id object)
{
    if (!object || ![object respondsToSelector:@selector(document)])
        return nil;
    NSMethodSignature *signature = [object methodSignatureForSelector:@selector(document)];
    if (!signature || signature.methodReturnType[0] != '@')
        return nil;
    @try {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        id document = [object performSelector:@selector(document)];
#pragma clang diagnostic pop
        if ([document isKindOfClass:PDFDocument.class])
            return document;
    } @catch (NSException *) {
        return nil;
    }
    return nil;
}

static PDFDocument *pdfDocumentFromViewTree(NSView *view)
{
    if (!view)
        return nil;
    if (PDFDocument *document = safePDFDocumentFromObject(view))
        return document;
    for (NSView *subview in view.subviews) {
        PDFDocument *document = pdfDocumentFromViewTree(subview);
        if (document)
            return document;
    }
    return nil;
}

static void tracePdfViewHierarchyView(WebContents *contents, NSString *phase, NSView *view, NSUInteger depth, NSInteger parentIndex, NSView *hitTarget, NSResponder *firstResponder, NSMutableSet<NSValue *> *seen, NSUInteger *nextIndex)
{
    if (!view || depth > 12 || *nextIndex > 80)
        return;
    NSValue *key = [NSValue valueWithPointer:(__bridge const void *)(view)];
    if ([seen containsObject:key])
        return;
    [seen addObject:key];
    NSUInteger index = (*nextIndex)++;

    NSString *className = NSStringFromClass(view.class) ?: @"unknown";
    NSString *parentClass = view.superview ? NSStringFromClass(view.superview.class) : @"none";
    NSString *layerClass = view.layer ? NSStringFromClass(view.layer.class) : @"none";
    PDFDocument *document = safePDFDocumentFromObject(view);
    bool isPDFClass = classNameContains(view.class, @"pdf");
    bool isPDFHost = classNameContains(view.class, @"pdfhost") || classNameContains(view.class, @"wkpdf");
    bool isPDFKitView = [view isKindOfClass:PDFView.class] || classNameContains(view.class, @"pdfview");
    bool isScrollView = [view isKindOfClass:NSScrollView.class] || classNameContains(view.class, @"scroll");
    bool isScrollDocumentView = [view.superview isKindOfClass:NSClipView.class] && ((NSClipView *)view.superview).documentView == view;
    bool isHitTarget = view == hitTarget || [hitTarget isDescendantOf:view];
    bool isFirstResponder = view == firstResponder;

    appendPdfViewHierarchyTrace([NSString stringWithFormat:@"webkit-pdf-view-hierarchy tab=%d phase=%@ pdf=%d index=%lu parent_index=%ld depth=%lu class=%@ parent_class=%@ superview_class=%@ frame_w=%.1f frame_h=%.1f bounds_w=%.1f bounds_h=%.1f hidden=%d alpha=%.3f wants_layer=%d layer_class=%@ is_web_view_root=%d is_hit_target=%d is_first_responder=%d is_scroll_document_view=%d responds_document=%d responds_current_page=%d responds_page_count=%d accepts_first_responder=%d responds_mouse_down=%d responds_key_down=%d responds_set_needs_display=%d returns_pdf_document=%d pdf_document_source=%@ page_count=%lu is_pdf_class=%d is_pdfkit_view=%d is_private_pdf_host=%d is_scroll_view=%d marker=issue-26062612000853-exp14",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && currentUrlLooksPdf(contents) ? 1 : 0,
        (unsigned long)index,
        (long)parentIndex,
        (unsigned long)depth,
        className,
        parentClass,
        parentClass,
        view.frame.size.width,
        view.frame.size.height,
        view.bounds.size.width,
        view.bounds.size.height,
        view.hidden ? 1 : 0,
        view.alphaValue,
        view.wantsLayer ? 1 : 0,
        layerClass,
        contents && view == contents->web_view ? 1 : 0,
        isHitTarget ? 1 : 0,
        isFirstResponder ? 1 : 0,
        isScrollDocumentView ? 1 : 0,
        [view respondsToSelector:@selector(document)] ? 1 : 0,
        [view respondsToSelector:NSSelectorFromString(@"currentPage")] ? 1 : 0,
        [view respondsToSelector:NSSelectorFromString(@"pageCount")] ? 1 : 0,
        view.acceptsFirstResponder ? 1 : 0,
        [view respondsToSelector:@selector(mouseDown:)] ? 1 : 0,
        [view respondsToSelector:@selector(keyDown:)] ? 1 : 0,
        [view respondsToSelector:@selector(setNeedsDisplay:)] ? 1 : 0,
        document ? 1 : 0,
        document ? @"view-tree" : @"none",
        document ? (unsigned long)document.pageCount : 0,
        isPDFClass ? 1 : 0,
        isPDFKitView ? 1 : 0,
        isPDFHost ? 1 : 0,
        isScrollView ? 1 : 0]);

    NSArray<NSView *> *subviews = view.subviews;
    for (NSView *subview in subviews)
        tracePdfViewHierarchyView(contents, phase, subview, depth + 1, (NSInteger)index, hitTarget, firstResponder, seen, nextIndex);
}

static void tracePdfViewHierarchy(WebContents *contents, NSString *phase)
{
    if (!pdfViewHierarchyTraceEnabled() || !contents || !contents->web_view || !currentUrlLooksPdf(contents))
        return;

    WKWebView *webView = contents->web_view;
    NSPoint center = NSMakePoint(NSMidX(webView.bounds), NSMidY(webView.bounds));
    NSView *hitTarget = [webView hitTest:center];
    NSResponder *firstResponder = contents->window.firstResponder;
    NSMutableSet<NSValue *> *seen = [NSMutableSet set];
    NSUInteger nextIndex = 0;
    tracePdfViewHierarchyView(contents, phase, webView, 0, -1, hitTarget, firstResponder, seen, &nextIndex);

    PDFDocument *document = pdfDocumentFromViewTree(webView);
    NSString *source = document ? @"view-tree" : @"none";
    if (!document && webView.URL) {
        PDFDocument *urlDocument = [[PDFDocument alloc] initWithURL:webView.URL];
        if (urlDocument) {
            document = urlDocument;
            source = @"url";
        }
    }
    appendPdfViewHierarchyTrace([NSString stringWithFormat:@"webkit-pdf-view-hierarchy-summary tab=%d phase=%@ pdf=1 visited_count=%lu pdf_document_source=%@ returns_pdf_document=%d page_count=%lu marker=issue-26062612000853-exp14",
        contents->tab_id,
        phase ?: @"unknown",
        (unsigned long)seen.count,
        source,
        document ? 1 : 0,
        document ? (unsigned long)document.pageCount : 0]);
}

static void tracePdfFormPdfKitOracle(WebContents *contents, NSString *phase)
{
    if (!pdfFormOracleTraceEnabled() || !contents || !currentUrlLooksPdf(contents))
        return;

    NSString *targetField = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_FORM_TARGET_FIELD"] ?: @"issue834_text";
    NSString *expectedValue = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_FORM_EXPECTED_VALUE"] ?: @"";
    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    NSString *source = @"view-tree";
    PDFDocument *document = pdfDocumentFromViewTree(contents->web_view);
    if (!document && contents->web_view.URL) {
        source = @"url";
        document = [[PDFDocument alloc] initWithURL:contents->web_view.URL];
    }
    if (!document) {
        appendPdfFormOracleTrace([NSString stringWithFormat:@"webkit-pdf-form-oracle tab=%d phase=%@ action=pdfkit url=%@ status=error reason=document-unavailable",
            contents->tab_id,
            phase ?: @"unknown",
            url]);
        return;
    }

    NSUInteger widgetCount = 0;
    NSUInteger targetPage = NSNotFound;
    NSString *targetType = @"";
    NSString *targetWidget = @"";
    NSString *targetValue = @"";
    bool targetFound = false;
    bool targetReadOnly = false;

    for (NSUInteger pageIndex = 0; pageIndex < document.pageCount; pageIndex++) {
        PDFPage *page = [document pageAtIndex:pageIndex];
        for (PDFAnnotation *annotation in page.annotations) {
            NSString *type = annotation.type ?: @"";
            NSString *widgetFieldType = @"";
            id widgetValue = pdfAnnotationValueForKey(annotation, @"widgetFieldType");
            if ([widgetValue isKindOfClass:NSString.class])
                widgetFieldType = widgetValue;
            bool isWidget = [type.lowercaseString containsString:@"widget"] || widgetFieldType.length > 0;
            if (!isWidget)
                continue;
            widgetCount++;

            NSString *fieldName = @"";
            id fieldValue = pdfAnnotationValueForKey(annotation, @"fieldName");
            if ([fieldValue isKindOfClass:NSString.class])
                fieldName = fieldValue;
            bool matchesTarget = [fieldName isEqualToString:targetField] || (!targetFound && !targetField.length);
            if (!matchesTarget)
                continue;

            id stringValue = pdfAnnotationValueForKey(annotation, @"widgetStringValue");
            if ([stringValue isKindOfClass:NSString.class])
                targetValue = stringValue;
            targetFound = true;
            targetPage = pageIndex;
            targetType = type;
            targetWidget = widgetFieldType;
            targetReadOnly = [annotation respondsToSelector:@selector(isReadOnly)] && annotation.isReadOnly;
        }
    }

    appendPdfFormOracleTrace([NSString stringWithFormat:@"webkit-pdf-form-oracle tab=%d phase=%@ action=pdfkit url=%@ status=ok source=%@ page=%@ widget_count=%lu target_found=%d field=%@ type=%@ widget=%@ value_length=%lu expected_length=%lu value_equals_expected=%d read_only=%d",
        contents->tab_id,
        phase ?: @"unknown",
        url,
        source,
        targetPage == NSNotFound ? @"none" : [NSString stringWithFormat:@"%lu", (unsigned long)targetPage],
        (unsigned long)widgetCount,
        targetFound ? 1 : 0,
        pdfFormOracleToken(targetField),
        pdfFormOracleToken(targetType),
        pdfFormOracleToken(targetWidget),
        (unsigned long)targetValue.length,
        (unsigned long)expectedValue.length,
        expectedValue.length > 0 && [targetValue isEqualToString:expectedValue] ? 1 : 0,
        targetReadOnly ? 1 : 0]);
}

static void tracePdfFormOracle(WebContents *contents, NSString *phase)
{
    if (!pdfFormOracleTraceEnabled() || !contents || !currentUrlLooksPdf(contents))
        return;

    tracePdfViewHierarchy(contents, phase);
    tracePdfFormPdfKitOracle(contents, phase);

    NSString *targetField = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_FORM_TARGET_FIELD"] ?: @"issue834_text";
    NSString *expectedValue = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_FORM_EXPECTED_VALUE"] ?: @"";
    NSString *escapedTarget = [targetField stringByReplacingOccurrencesOfString:@"\\" withString:@"\\\\"];
    escapedTarget = [escapedTarget stringByReplacingOccurrencesOfString:@"'" withString:@"\\'"];
    NSString *escapedExpected = [expectedValue stringByReplacingOccurrencesOfString:@"\\" withString:@"\\\\"];
    escapedExpected = [escapedExpected stringByReplacingOccurrencesOfString:@"'" withString:@"\\'"];
    NSString *script = [NSString stringWithFormat:
        @"(() => {"
        "const targetName = '%@';"
        "const expected = '%@';"
        "const all = Array.from(document.querySelectorAll('input, textarea, select, [contenteditable=\"true\"]'));"
        "const annotation = Array.from(document.querySelectorAll('[x-apple-pdf-annotation=\"true\"]'));"
        "const named = all.find((el) => (el.getAttribute('name') || el.getAttribute('id') || el.getAttribute('aria-label') || '') === targetName)"
        " || all.find((el) => (el.getAttribute('name') || el.getAttribute('id') || el.getAttribute('aria-label') || '').includes(targetName))"
        " || all.find((el) => el.getAttribute('x-apple-pdf-annotation') === 'true')"
        " || all[0] || null;"
        "const active = document.activeElement;"
        "const valueOf = (el) => !el ? '' : (typeof el.value === 'string' ? el.value : (typeof el.innerText === 'string' ? el.innerText : ''));"
        "const nameOf = (el) => !el ? '' : (el.getAttribute('name') || el.getAttribute('id') || el.getAttribute('aria-label') || '');"
        "const targetValue = valueOf(named);"
        "return JSON.stringify({"
        "fieldCount: all.length,"
        "annotationCount: annotation.length,"
        "targetField: targetName,"
        "targetFound: !!named,"
        "targetName: nameOf(named),"
        "targetTag: named && named.tagName ? named.tagName : '',"
        "targetType: named && named.getAttribute ? (named.getAttribute('type') || '') : '',"
        "targetPdfAnnotation: !!(named && named.getAttribute && named.getAttribute('x-apple-pdf-annotation') === 'true'),"
        "targetValueLength: targetValue.length,"
        "targetValueEqualsExpected: expected.length > 0 && targetValue === expected,"
        "activeTag: active && active.tagName ? active.tagName : '',"
        "activeType: active && active.getAttribute ? (active.getAttribute('type') || '') : '',"
        "activeName: nameOf(active),"
        "activePdfAnnotation: !!(active && active.getAttribute && active.getAttribute('x-apple-pdf-annotation') === 'true'),"
        "activeValueLength: valueOf(active).length,"
        "activeEqualsTarget: !!(named && active === named),"
        "title: document.title || ''"
        "});"
        "})()",
        escapedTarget,
        escapedExpected];
    NSString *phaseCopy = [phase copy] ?: @"unknown";
    int tab = contents->tab_id;
    WKWebView *webView = contents->web_view;
    [webView evaluateJavaScript:script completionHandler:^(id result, NSError *error) {
        NSString *url = webView.URL.absoluteString ?: @"";
        NSString *detail = error
            ? [NSString stringWithFormat:@"status=error error=%@", error.localizedDescription ?: @""]
            : [NSString stringWithFormat:@"status=ok state=%@", [result isKindOfClass:NSString.class] ? result : @""];
        appendPdfFormOracleTrace([NSString stringWithFormat:@"webkit-pdf-form-oracle tab=%d phase=%@ url=%@ %@",
            tab,
            phaseCopy,
            url,
            detail]);
    }];
}

static void dispatchPdfPasswordReturnKeyUp(WebContents *contents)
{
    if (!contents || !contents->web_view || !currentUrlLooksPdf(contents))
        return;

    NSString *script = @"(() => {"
        "const active = document.activeElement;"
        "const form = document.querySelector('.password-form[x-apple-pdf-annotation=\"true\"]');"
        "const isPassword = !!active && active.tagName === 'INPUT' && active.getAttribute('type') === 'password' && active.getAttribute('x-apple-pdf-annotation') === 'true' && !!form;"
        "if (!isPassword) return JSON.stringify({dispatched:false, reason:'not-active-pdf-password-field'});"
        "let event;"
        "if (document.createEvent) {"
        "event = document.createEvent('KeyboardEvent');"
        "if (event.initKeyboardEvent) {"
        "event.initKeyboardEvent('keyup', true, true, window, 'Enter', 0, false, false, false, false, false);"
        "} else {"
        "event.initEvent('keyup', true, true);"
        "}"
        "} else {"
        "event = new KeyboardEvent('keyup', { key: 'Enter', code: 'Enter', bubbles: true, cancelable: true });"
        "}"
        "try { Object.defineProperty(event, 'key', { get: () => 'Enter' }); } catch (_) {}"
        "try { Object.defineProperty(event, 'code', { get: () => 'Enter' }); } catch (_) {}"
        "try { Object.defineProperty(event, 'keyIdentifier', { get: () => 'Enter' }); } catch (_) {}"
        "try { Object.defineProperty(event, 'keyCode', { get: () => 13 }); } catch (_) {}"
        "try { Object.defineProperty(event, 'which', { get: () => 13 }); } catch (_) {}"
        "const result = active.dispatchEvent(event);"
        "return JSON.stringify({dispatched:true, dispatchResult:!!result, defaultPrevented:!!event.defaultPrevented, key:event.key || '', keyIdentifier:event.keyIdentifier || '', keyCode:event.keyCode || 0});"
        "})()";

    WebContents *captured = contents;
    WKWebView *webView = contents->web_view;
    int tab = contents->tab_id;
    [webView evaluateJavaScript:script completionHandler:^(id result, NSError *error) {
        NSString *url = webView.URL.absoluteString ?: @"";
        NSString *detail = error
            ? [NSString stringWithFormat:@"status=error error=%@", error.localizedDescription ?: @""]
            : [NSString stringWithFormat:@"status=ok result=%@", [result isKindOfClass:NSString.class] ? result : @""];
        appendPdfStateOracleTrace([NSString stringWithFormat:@"webkit-pdf-password-return-keyup tab=%d url=%@ %@",
            tab,
            url,
            detail]);
        tracePdfPasswordOracle(captured, @"return-keyup-dispatch");
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            tracePdfPasswordOracle(captured, @"return-keyup-dispatch-delayed");
        });
    }];
}

static void appendPdfVisualOracleTrace(NSString *line)
{
    if (!pdfVisualOracleProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VISUAL_ORACLE_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-visual-oracle-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfVisualOracle(WebContents *contents, NSString *phase, NSString *action, NSString *detail)
{
    if (!pdfVisualOracleProbeEnabled())
        return;
    appendPdfVisualOracleTrace([NSString stringWithFormat:@"webkit-pdf-visual-oracle tab=%d phase=%@ action=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        action ?: @"none",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPdfDirectCommandTrace(NSString *line)
{
    if (!pdfDirectCommandProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_DIRECT_COMMAND_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-direct-command-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfDirectCommand(WebContents *contents, NSString *phase, NSString *action, NSString *detail)
{
    if (!pdfDirectCommandProbeEnabled())
        return;
    appendPdfDirectCommandTrace([NSString stringWithFormat:@"webkit-pdf-direct-command tab=%d phase=%@ action=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        action ?: @"none",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPdfPrintOperationTrace(NSString *line)
{
    if (!pdfPrintOperationProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_OPERATION_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-print-operation-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfPrintOperation(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!pdfPrintOperationProbeEnabled())
        return;
    appendPdfPrintOperationTrace([NSString stringWithFormat:@"webkit-pdf-print-operation tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPdfPrintDialogTrace(NSString *line)
{
    if (!pdfPrintDialogProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_DIALOG_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-print-dialog-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfPrintDialog(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!pdfPrintDialogProbeEnabled())
        return;
    appendPdfPrintDialogTrace([NSString stringWithFormat:@"webkit-pdf-print-dialog tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPdfPrintModalTrace(NSString *line)
{
    if (!pdfPrintModalProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_MODAL_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-print-modal-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfPrintModal(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!pdfPrintModalProbeEnabled())
        return;
    appendPdfPrintModalTrace([NSString stringWithFormat:@"webkit-pdf-print-modal tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static void appendPrintPresentationTrace(NSString *line)
{
    if (!printPresentationProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-print-presentation-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static NSString *printPresentationWindowsSummary()
{
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (NSWindow *window in NSApp.windows) {
        [items addObject:[NSString stringWithFormat:@"%@:%p title={%@} visible=%d key=%d main=%d alpha=%.3f level=%ld style=%llu collection=%llu",
                                   NSStringFromClass(window.class),
                                   window,
                                   window.title ?: @"",
                                   window.visible ? 1 : 0,
                                   window.keyWindow ? 1 : 0,
                                   window.mainWindow ? 1 : 0,
                                   window.alphaValue,
                                   (long)window.level,
                                   (unsigned long long)window.styleMask,
                                   (unsigned long long)window.collectionBehavior]];
    }
    return [items componentsJoinedByString:@"|"];
}

static NSString *printPresentationStateSummary(WebContents *contents, NSString *attempts)
{
    NSWindow *window = contents ? contents->window : nil;
    NSResponder *firstResponder = window.firstResponder;
    NSBundle *bundle = NSBundle.mainBundle;
    return [NSString stringWithFormat:@"pid=%d bundle_id=%@ activation_policy=%ld app_active=%d key_window=%@ main_window=%@ host_window=%@ host_visible=%d host_alpha=%.3f host_level=%ld host_style=%llu host_collection=%llu host_can_key=%d host_can_main=%d first_responder=%@ responder_chain=%@ attempts=%@ windows={%@}",
                     getpid(),
                     bundle.bundleIdentifier ?: @"",
                     (long)NSApp.activationPolicy,
                     NSApp.active ? 1 : 0,
                     describeObject(NSApp.keyWindow),
                     describeObject(NSApp.mainWindow),
                     describeObject(window),
                     window && window.visible ? 1 : 0,
                     window ? window.alphaValue : 0.0,
                     window ? (long)window.level : 0,
                     window ? (unsigned long long)window.styleMask : 0,
                     window ? (unsigned long long)window.collectionBehavior : 0,
                     window && [window canBecomeKeyWindow] ? 1 : 0,
                     window && [window canBecomeMainWindow] ? 1 : 0,
                     describeObject(firstResponder),
                     responderChain(firstResponder),
                     attempts ?: @"none",
                     printPresentationWindowsSummary()];
}

static void tracePrintPresentation(WebContents *contents, NSString *phase, NSString *attempts)
{
    if (!printPresentationProbeEnabled())
        return;
    appendPrintPresentationTrace([NSString stringWithFormat:@"webkit-print-presentation tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        printPresentationStateSummary(contents, attempts)]);
}

static void appendNativeUiProbeTrace(NSString *line)
{
    if (!nativeUiProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_TRACE_FILE"];
    if (!path.length)
        return;
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void traceNativeUiProbe(WebContents *contents, NSString *phase, NSString *attempts, NSString *detail)
{
    if (!nativeUiProbeEnabled())
        return;
    NSString *row = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_ROW"];
    appendNativeUiProbeTrace([NSString stringWithFormat:@"webkit-native-ui-probe row=%@ tab=%d phase=%@ ppid=%d url=%@ %@ %@",
        row.length ? row : @"unknown",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        getppid(),
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        printPresentationStateSummary(contents, attempts),
        detail ?: @""]);
}

static bool publishNativeUiTrackedPid(NSString *trackedPath)
{
    if (!trackedPath.length)
        return false;
    NSString *line = [NSString stringWithFormat:@"webkit %d\n", getpid()];
    NSData *data = [line dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = trackedPath.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:trackedPath])
        return [data writeToFile:trackedPath atomically:YES];
    NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:trackedPath];
    if (!handle)
        return false;
    [handle seekToEndOfFile];
    [handle writeData:data];
    [handle closeFile];
    return true;
}

static bool waitForNativeUiWatcherReady(NSString *readyPath)
{
    if (!readyPath.length)
        return false;
    for (int i = 0; i < 100; i++) {
        if ([NSFileManager.defaultManager fileExistsAtPath:readyPath])
            return true;
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
    }
    return false;
}

static void performNativeUiProbe(WebContents *contents)
{
    if (!nativeUiProbeEnabled() || !contents)
        return;

    NSString *tracePath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_TRACE_FILE"];
    NSString *trackedPath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_TRACKED_PIDS_FILE"];
    NSString *readyPath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_WATCHER_READY_FILE"];
    if (!tracePath.length)
        return;
    if (!trackedPath.length || !readyPath.length) {
        traceNativeUiProbe(contents, @"refuse", @"none", [NSString stringWithFormat:@"reason=missing-path trace_path=%@ tracked_path=%@ ready_path=%@",
                                   tracePath ?: @"",
                                   trackedPath ?: @"",
                                   readyPath ?: @""]);
        return;
    }
    if (!publishNativeUiTrackedPid(trackedPath)) {
        traceNativeUiProbe(contents, @"refuse", @"none", [NSString stringWithFormat:@"reason=tracked-pid-write-failed tracked_path=%@", trackedPath]);
        return;
    }
    if (!waitForNativeUiWatcherReady(readyPath)) {
        traceNativeUiProbe(contents, @"refuse", @"none", [NSString stringWithFormat:@"reason=watcher-not-ready tracked_path=%@ ready_path=%@", trackedPath, readyPath]);
        return;
    }

    traceNativeUiProbe(contents, @"before-activation", @"none", @"");
    NSMutableArray<NSString *> *attempts = [NSMutableArray array];
    NSString *activationMode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_ACTIVATION_MODE"];
    if ([activationMode isEqualToString:@"regular"]) {
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        [attempts addObject:@"setActivationPolicyRegular"];
    }
    NSString *windowMode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_WINDOW_MODE"];
    if (contents->window) {
        if ([windowMode containsString:@"make-key"]) {
            [contents->window makeKeyAndOrderFront:nil];
            [attempts addObject:@"makeKeyAndOrderFront"];
        }
        if ([windowMode containsString:@"order-front"]) {
            [contents->window orderFrontRegardless];
            [attempts addObject:@"orderFrontRegardless"];
        }
    }
    if ([activationMode isEqualToString:@"regular"]) {
        [NSApp activateIgnoringOtherApps:YES];
        [attempts addObject:@"activateIgnoringOtherApps"];
    }
    if (attempts.count)
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.2]];
    NSString *attemptSummary = attempts.count ? [attempts componentsJoinedByString:@","] : @"none";
    traceNativeUiProbe(contents, @"after-activation", attemptSummary, @"");

    NSAlert *alert = [[NSAlert alloc] init];
    alert.messageText = @"TermSurf Issue 26062212000834 Activation Probe";
    alert.informativeText = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_NATIVE_UI_ROW"] ?: @"webkit-engine";
    [alert addButtonWithTitle:@"OK"];
    alert.window.title = @"TermSurf Issue 26062212000834 Activation Probe";
    traceNativeUiProbe(contents, @"before-alert", attemptSummary, @"");
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(30 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        if (alert.window.visible) {
            traceNativeUiProbe(contents, @"alert-timeout", attemptSummary, @"reason=failsafe-abort-modal");
            [NSApp abortModal];
        }
    });
    NSModalResponse response = [alert runModal];
    traceNativeUiProbe(contents, @"after-alert", attemptSummary, [NSString stringWithFormat:@"response=%ld", (long)response]);
}

static void scheduleNativeUiProbe(WebContents *contents)
{
    if (!nativeUiProbeEnabled())
        return;
    bool expected = false;
    if (!g_native_ui_probe_started.compare_exchange_strong(expected, true))
        return;
    dispatch_async(dispatch_get_main_queue(), ^{
        performNativeUiProbe(contents);
    });
}

@interface TSSyntheticPrintView : NSView
@end

@implementation TSSyntheticPrintView
- (BOOL)isFlipped
{
    return YES;
}

- (void)drawRect:(NSRect)dirtyRect
{
    (void)dirtyRect;
    [[NSColor colorWithCalibratedRed:0.08 green:0.32 blue:0.34 alpha:1.0] setFill];
    NSRectFill(self.bounds);
    NSDictionary *attrs = @{
        NSFontAttributeName: [NSFont boldSystemFontOfSize:24.0],
        NSForegroundColorAttributeName: NSColor.whiteColor,
    };
    [@"TermSurf Issue 26062212000834 Synthetic Print Probe" drawAtPoint:NSMakePoint(32, 48) withAttributes:attrs];
}
@end

@interface TSSyntheticPrintProbeDelegate : NSObject
@property(nonatomic) WebContents *contents;
@property(nonatomic, copy) NSString *attempts;
@property(nonatomic, copy) NSString *identity;
- (void)syntheticPrintOperationDidRun:(NSPrintOperation *)operation success:(BOOL)success contextInfo:(void *)contextInfo;
@end

static NSMutableSet<TSSyntheticPrintProbeDelegate *> *syntheticPrintProbeDelegates()
{
    static NSMutableSet<TSSyntheticPrintProbeDelegate *> *delegates = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        delegates = [NSMutableSet set];
    });
    return delegates;
}

static void appendSyntheticPrintTrace(NSString *line)
{
    if (!syntheticPrintProbeEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_TRACE_FILE"];
    if (!path.length)
        return;
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void traceSyntheticPrint(WebContents *contents, NSString *phase, NSString *attempts, NSString *detail)
{
    if (!syntheticPrintProbeEnabled())
        return;
    NSString *row = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_ROW"];
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_RUN_MODE"];
    appendSyntheticPrintTrace([NSString stringWithFormat:@"webkit-synthetic-print row=%@ mode=%@ tab=%d phase=%@ ppid=%d url=%@ %@ %@",
        row.length ? row : @"unknown",
        mode.length ? mode : @"unknown",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        getppid(),
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        printPresentationStateSummary(contents, attempts),
        detail ?: @""]);
}

@implementation TSSyntheticPrintProbeDelegate
- (void)syntheticPrintOperationDidRun:(NSPrintOperation *)operation success:(BOOL)success contextInfo:(void *)contextInfo
{
    (void)contextInfo;
    traceSyntheticPrint(self.contents, @"completion-callback", self.attempts ?: @"none", [NSString stringWithFormat:@"operation=%@ success=%d", describeObject(operation), success ? 1 : 0]);
    [syntheticPrintProbeDelegates() removeObject:self];
}
@end

static bool publishSyntheticPrintTrackedPid(NSString *trackedPath)
{
    if (!trackedPath.length)
        return false;
    NSString *line = [NSString stringWithFormat:@"webkit %d\n", getpid()];
    NSData *data = [line dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = trackedPath.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:trackedPath])
        return [data writeToFile:trackedPath atomically:YES];
    NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:trackedPath];
    if (!handle)
        return false;
    [handle seekToEndOfFile];
    [handle writeData:data];
    [handle closeFile];
    return true;
}

static bool waitForSyntheticPrintWatcherReady(NSString *readyPath)
{
    if (!readyPath.length)
        return false;
    for (int i = 0; i < 100; i++) {
        if ([NSFileManager.defaultManager fileExistsAtPath:readyPath])
            return true;
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
    }
    return false;
}

static NSString *applySyntheticPrintPresentationAttempts(WebContents *contents)
{
    NSMutableArray<NSString *> *attempts = [NSMutableArray array];
    NSString *activationMode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_ACTIVATION_MODE"];
    if ([activationMode isEqualToString:@"regular"]) {
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        [attempts addObject:@"setActivationPolicyRegular"];
    }
    NSString *windowMode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_WINDOW_MODE"];
    if (contents && contents->window) {
        if ([windowMode containsString:@"make-key"]) {
            [contents->window makeKeyAndOrderFront:nil];
            [attempts addObject:@"makeKeyAndOrderFront"];
        }
        if ([windowMode containsString:@"order-front"]) {
            [contents->window orderFrontRegardless];
            [attempts addObject:@"orderFrontRegardless"];
        }
    }
    if ([activationMode isEqualToString:@"regular"]) {
        [NSApp activateIgnoringOtherApps:YES];
        [attempts addObject:@"activateIgnoringOtherApps"];
    }
    if (attempts.count)
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.2]];
    return attempts.count ? [attempts componentsJoinedByString:@","] : @"none";
}

static void performSyntheticPrintProbe(WebContents *contents)
{
    if (!syntheticPrintProbeEnabled() || !contents)
        return;

    NSString *tracePath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_TRACE_FILE"];
    NSString *trackedPath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_TRACKED_PIDS_FILE"];
    NSString *readyPath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_WATCHER_READY_FILE"];
    if (!tracePath.length)
        return;
    if (!trackedPath.length || !readyPath.length) {
        traceSyntheticPrint(contents, @"refuse", @"none", [NSString stringWithFormat:@"reason=missing-path trace_path=%@ tracked_path=%@ ready_path=%@ operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false",
                                   tracePath ?: @"",
                                   trackedPath ?: @"",
                                   readyPath ?: @""]);
        return;
    }
    if (!publishSyntheticPrintTrackedPid(trackedPath)) {
        traceSyntheticPrint(contents, @"refuse", @"none", [NSString stringWithFormat:@"reason=tracked-pid-write-failed tracked_path=%@ operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false", trackedPath]);
        return;
    }
    if (!waitForSyntheticPrintWatcherReady(readyPath)) {
        traceSyntheticPrint(contents, @"refuse", @"none", [NSString stringWithFormat:@"reason=watcher-not-ready tracked_path=%@ ready_path=%@ operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false", trackedPath, readyPath]);
        return;
    }

    traceSyntheticPrint(contents, @"before-activation", @"none", @"operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false");
    NSString *attemptSummary = applySyntheticPrintPresentationAttempts(contents);
    traceSyntheticPrint(contents, @"after-activation", attemptSummary, @"operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false");

    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_SYNTHETIC_PRINT_RUN_MODE"];
    if (!mode.length)
        mode = @"runOperation";

    TSSyntheticPrintView *view = [[TSSyntheticPrintView alloc] initWithFrame:NSMakeRect(0, 0, 612, 792)];
    NSPrintInfo *printInfo = [NSPrintInfo.sharedPrintInfo copy];
    NSPrintOperation *operation = [NSPrintOperation printOperationWithView:view printInfo:printInfo];
    operation.showsPrintPanel = YES;
    operation.showsProgressPanel = NO;
    traceSyntheticPrint(contents, @"operation-created", attemptSummary, [NSString stringWithFormat:@"operation=%@ view=%@ operation_created=true run_invoked=false modal_run_invoked=false completion_callback=false showsPrintPanel=%d showsProgressPanel=%d",
                                    describeObject(operation),
                                    describeObject(view),
                                    operation.showsPrintPanel ? 1 : 0,
                                    operation.showsProgressPanel ? 1 : 0]);

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(30 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        traceSyntheticPrint(contents, @"operation-timeout", attemptSummary, @"reason=failsafe-abort-modal");
        [NSApp abortModal];
    });

    if ([mode isEqualToString:@"runOperationModalForWindow"]) {
        if (!contents->window) {
            traceSyntheticPrint(contents, @"refuse", attemptSummary, @"reason=missing-window operation_created=true run_invoked=false modal_run_invoked=false completion_callback=false");
            return;
        }
        TSSyntheticPrintProbeDelegate *delegate = [[TSSyntheticPrintProbeDelegate alloc] init];
        delegate.contents = contents;
        delegate.attempts = attemptSummary;
        delegate.identity = [NSString stringWithFormat:@"synthetic-print-%@-tab-%d", NSUUID.UUID.UUIDString, contents->tab_id];
        [syntheticPrintProbeDelegates() addObject:delegate];
        traceSyntheticPrint(contents, @"before-operation", attemptSummary, [NSString stringWithFormat:@"operation=%@ run_path=runOperationModalForWindow operation_created=true run_invoked=false modal_run_invoked=true completion_callback=false delegate=%@",
                                      describeObject(operation),
                                      delegate.identity]);
        [operation runOperationModalForWindow:contents->window delegate:delegate didRunSelector:@selector(syntheticPrintOperationDidRun:success:contextInfo:) contextInfo:nullptr];
        traceSyntheticPrint(contents, @"after-operation-invocation", attemptSummary, [NSString stringWithFormat:@"operation=%@ run_path=runOperationModalForWindow operation_created=true run_invoked=false modal_run_invoked=true completion_callback=pending delegate=%@",
                                      describeObject(operation),
                                      delegate.identity]);
        return;
    }

    traceSyntheticPrint(contents, @"before-operation", attemptSummary, [NSString stringWithFormat:@"operation=%@ run_path=runOperation operation_created=true run_invoked=true modal_run_invoked=false completion_callback=false",
                                  describeObject(operation)]);
    BOOL success = [operation runOperation];
    traceSyntheticPrint(contents, @"after-operation", attemptSummary, [NSString stringWithFormat:@"operation=%@ run_path=runOperation operation_created=true run_invoked=true modal_run_invoked=false completion_callback=false success=%d",
                                  describeObject(operation),
                                  success ? 1 : 0]);
}

static void scheduleSyntheticPrintProbe(WebContents *contents)
{
    if (!syntheticPrintProbeEnabled())
        return;
    bool expected = false;
    if (!g_synthetic_print_probe_started.compare_exchange_strong(expected, true))
        return;
    dispatch_async(dispatch_get_main_queue(), ^{
        performSyntheticPrintProbe(contents);
    });
}

@interface TSPdfPrintModalProbeDelegate : NSObject
@property(nonatomic) WebContents *contents;
@property(nonatomic, copy) NSString *identity;
- (void)printOperationDidRun:(NSPrintOperation *)operation success:(BOOL)success contextInfo:(void *)contextInfo;
@end

static NSMutableSet<TSPdfPrintModalProbeDelegate *> *pdfPrintModalProbeDelegates()
{
    static NSMutableSet<TSPdfPrintModalProbeDelegate *> *delegates = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        delegates = [NSMutableSet set];
    });
    return delegates;
}

@implementation TSPdfPrintModalProbeDelegate
- (void)printOperationDidRun:(NSPrintOperation *)operation success:(BOOL)success contextInfo:(void *)contextInfo
{
    WebContents *contents = self.contents;
    NSString *result = success ? @"submitted-or-success" : @"canceled";
    tracePdfPrintModal(contents, @"completion", [NSString stringWithFormat:@"delegate=%@ operation=%@ operation_class=%@ completion_callback=true completion_success=%d completion_result=%@ context=%p delegate_release=true",
                                                   self.identity ?: @"",
                                                   describeObject(operation),
                                                   operation ? NSStringFromClass(operation.class) : @"",
                                                   success ? 1 : 0,
                                                   result,
                                                   contextInfo]);
    tracePrintPresentation(contents, @"completion", @"delegate-callback");
    [pdfPrintModalProbeDelegates() removeObject:self];
}
@end

static void appendPdfProductionZoomTrace(NSString *line)
{
    if (!pdfProductionZoomTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRODUCTION_ZOOM_TRACE_FILE"];
    if (!path.length)
        return;
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfProductionZoom(WebContents *contents, NSString *phase, NSString *action, NSString *detail)
{
    if (!pdfProductionZoomTraceEnabled())
        return;
    appendPdfProductionZoomTrace([NSString stringWithFormat:@"webkit-pdf-production-zoom tab=%d phase=%@ action=%@ url=%@ %@",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        action ?: @"none",
        contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @"",
        detail ?: @""]);
}

static NSString *pdfSaveDownloadDirectoryPath()
{
    NSString *override = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_DOWNLOAD_DIR"];
    if (override.length)
        return override;

    NSURL *downloads = [NSFileManager.defaultManager URLsForDirectory:NSDownloadsDirectory inDomains:NSUserDomainMask].firstObject;
    return downloads.path ?: [NSHomeDirectory() stringByAppendingPathComponent:@"Downloads"];
}

static NSString *safePdfSaveFilename(NSString *suggestedFilename)
{
    NSString *candidate = suggestedFilename.length ? suggestedFilename.lastPathComponent : @"";
    NSMutableCharacterSet *unsafe = [NSMutableCharacterSet controlCharacterSet];
    [unsafe formUnionWithCharacterSet:[NSCharacterSet characterSetWithCharactersInString:@"/:\\"]];
    NSArray<NSString *> *parts = [candidate componentsSeparatedByCharactersInSet:unsafe];
    candidate = [parts componentsJoinedByString:@"_"];
    candidate = [candidate stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    while ([candidate containsString:@".."])
        candidate = [candidate stringByReplacingOccurrencesOfString:@".." withString:@"_"];
    if (!candidate.length || [candidate isEqualToString:@"."] || [candidate isEqualToString:@".."])
        candidate = @"download.pdf";
    if (!candidate.pathExtension.length)
        candidate = [candidate stringByAppendingPathExtension:@"pdf"];
    return candidate;
}

static bool pathIsSymlink(NSString *path)
{
    NSDictionary<NSFileAttributeKey, id> *attributes = [NSFileManager.defaultManager attributesOfItemAtPath:path error:nil];
    return [attributes[NSFileType] isEqualToString:NSFileTypeSymbolicLink];
}

static bool pathIsInsideDirectory(NSString *path, NSString *directory)
{
    NSString *standardPath = path.stringByStandardizingPath.stringByResolvingSymlinksInPath;
    NSString *standardDirectory = directory.stringByStandardizingPath.stringByResolvingSymlinksInPath;
    if ([standardPath isEqualToString:standardDirectory])
        return true;
    NSString *prefix = [standardDirectory stringByAppendingString:@"/"];
    return [standardPath hasPrefix:prefix];
}

static NSString *uniquePdfSavePath(NSString *directory, NSString *filename, NSError **error)
{
    NSFileManager *fm = NSFileManager.defaultManager;
    [fm createDirectoryAtPath:directory withIntermediateDirectories:YES attributes:nil error:error];
    if (error && *error)
        return nil;
    if (pathIsSymlink(directory)) {
        if (error)
            *error = [NSError errorWithDomain:@"TermSurfWebKitPDFSave" code:6 userInfo:@{ NSLocalizedDescriptionKey: @"refusing symlink download directory" }];
        return nil;
    }

    NSString *base = filename.stringByDeletingPathExtension;
    NSString *extension = filename.pathExtension;
    if (!base.length)
        base = @"download";
    if (!extension.length)
        extension = @"pdf";

    for (NSUInteger index = 0; index < 1000; index++) {
        NSString *candidateName = index == 0
            ? [base stringByAppendingPathExtension:extension]
            : [[NSString stringWithFormat:@"%@-%lu", base, (unsigned long)index] stringByAppendingPathExtension:extension];
        NSString *candidate = [directory stringByAppendingPathComponent:candidateName];
        if (!pathIsInsideDirectory(candidate, directory)) {
            if (error)
                *error = [NSError errorWithDomain:@"TermSurfWebKitPDFSave" code:1 userInfo:@{ NSLocalizedDescriptionKey: @"candidate outside download directory" }];
            return nil;
        }
        if (pathIsSymlink(candidate)) {
            if (error)
                *error = [NSError errorWithDomain:@"TermSurfWebKitPDFSave" code:2 userInfo:@{ NSLocalizedDescriptionKey: @"refusing symlink destination" }];
            return nil;
        }
        if (![fm fileExistsAtPath:candidate])
            return candidate;
    }

    if (error)
        *error = [NSError errorWithDomain:@"TermSurfWebKitPDFSave" code:3 userInfo:@{ NSLocalizedDescriptionKey: @"no available unique filename" }];
    return nil;
}

static NSString *savePdfDataToDownloads(NSData *data, NSString *suggestedFilename, NSError **error)
{
    if (!data.length) {
        if (error)
            *error = [NSError errorWithDomain:@"TermSurfWebKitPDFSave" code:4 userInfo:@{ NSLocalizedDescriptionKey: @"empty PDF data" }];
        return nil;
    }

    NSString *directory = pdfSaveDownloadDirectoryPath();
    NSString *filename = safePdfSaveFilename(suggestedFilename);
    NSString *destination = uniquePdfSavePath(directory, filename, error);
    if (!destination)
        return nil;

    NSURL *destinationURL = [NSURL fileURLWithPath:destination isDirectory:NO];
    if (![data writeToURL:destinationURL options:NSDataWritingAtomic error:error])
        return nil;

    NSString *standardDestination = destination.stringByStandardizingPath;
    if (!pathIsInsideDirectory(standardDestination, directory)) {
        [NSFileManager.defaultManager removeItemAtPath:standardDestination error:nil];
        if (error)
            *error = [NSError errorWithDomain:@"TermSurfWebKitPDFSave" code:5 userInfo:@{ NSLocalizedDescriptionKey: @"written path escaped download directory" }];
        return nil;
    }
    return standardDestination;
}

static void appendPdfViewGeometryTrace(NSString *line)
{
    if (!pdfViewGeometryTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VIEW_GEOMETRY_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-view-geometry-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void tracePdfFind(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!contents)
        return;
    NSString *line = [NSString stringWithFormat:@"webkit-pdf-find tab=%d phase=%@ url=%@ query={%@} %@",
        contents->tab_id,
        phase ?: @"unknown",
        contents->web_view.URL.absoluteString ?: @"",
        routeTraceStringSample(contents->pdf_find_query ?: @""),
        detail ?: @""];
    appendPdfViewGeometryTrace(line);
    fprintf(stderr, "%s\n", line.UTF8String);
}

static void appendPdfSelectionSurfaceTrace(NSString *line)
{
    if (!pdfSelectionSurfaceTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTION_SURFACE_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-selection-surface-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void appendPdfSelectedTextRouteTrace(NSString *line)
{
    if (!pdfSelectedTextRouteTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_ROUTE_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-selected-text-route-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static void appendPdfSelectedTextCacheCopyTrace(NSString *line)
{
    if (!pdfSelectedTextCacheCopyTraceEnabled())
        return;
    NSString *path = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_SELECTED_TEXT_CACHE_COPY_TRACE_FILE"];
    if (!path.length)
        path = [NSTemporaryDirectory() stringByAppendingPathComponent:@"termsurf-webkit-pdf-selected-text-cache-copy-trace.log"];
    NSString *entry = [line stringByAppendingString:@"\n"];
    NSData *data = [entry dataUsingEncoding:NSUTF8StringEncoding];
    NSFileManager *fm = NSFileManager.defaultManager;
    NSString *parent = path.stringByDeletingLastPathComponent;
    if (parent.length)
        [fm createDirectoryAtPath:parent withIntermediateDirectories:YES attributes:nil error:nil];
    if (![fm fileExistsAtPath:path])
        [data writeToFile:path atomically:YES];
    else {
        NSFileHandle *handle = [NSFileHandle fileHandleForWritingAtPath:path];
        [handle seekToEndOfFile];
        [handle writeData:data];
        [handle closeFile];
    }
}

static NSString *surfaceSafeString(NSString *value)
{
    if (!value)
        return @"nil";
    NSString *sample = value.length > 180 ? [value substringToIndex:180] : value;
    sample = [[sample stringByReplacingOccurrencesOfString:@"\n" withString:@" "] stringByReplacingOccurrencesOfString:@"\t" withString:@" "];
    return [NSString stringWithFormat:@"NSString(len=%lu sample=%@)", (unsigned long)value.length, sample];
}

static NSString *surfaceValueSummary(id value)
{
    if (!value)
        return @"nil";
    if ([value isKindOfClass:NSString.class])
        return surfaceSafeString((NSString *)value);
    if ([value isKindOfClass:NSAttributedString.class])
        return [NSString stringWithFormat:@"NSAttributedString(%@)", surfaceSafeString([(NSAttributedString *)value string])];
    if ([value isKindOfClass:NSValue.class])
        return [NSString stringWithFormat:@"%@:%@", NSStringFromClass([value class]), value];
    if ([value isKindOfClass:NSArray.class])
        return [NSString stringWithFormat:@"NSArray(count=%lu)", (unsigned long)[(NSArray *)value count]];
    if ([value isKindOfClass:NSDictionary.class])
        return [NSString stringWithFormat:@"NSDictionary(count=%lu)", (unsigned long)[(NSDictionary *)value count]];
    NSString *description = [NSString stringWithFormat:@"%@", value];
    return [NSString stringWithFormat:@"%@(%@)", NSStringFromClass([value class]), surfaceSafeString(description)];
}

static bool selectionSurfaceTermMatches(NSString *name)
{
    NSString *lower = name.lowercaseString;
    return [lower containsString:@"select"] || [lower containsString:@"selection"] || [lower containsString:@"copy"] || [lower containsString:@"pdf"] || [lower containsString:@"text"] || [lower containsString:@"string"];
}

static NSString *superclassChainForObject(id object)
{
    if (!object)
        return @"nil";
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    Class current = [object class];
    for (int i = 0; current && i < 16; i++) {
        [items addObject:NSStringFromClass(current)];
        current = class_getSuperclass(current);
    }
    if (current)
        [items addObject:@"..."];
    return [items componentsJoinedByString:@">"];
}

static NSString *methodSurfaceSummary(Class cls)
{
    if (!cls)
        return @"";
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (Class current = cls; current && items.count < 80; current = class_getSuperclass(current)) {
        unsigned int count = 0;
        Method *methods = class_copyMethodList(current, &count);
        for (unsigned int i = 0; methods && i < count && items.count < 80; i++) {
            SEL selector = method_getName(methods[i]);
            NSString *name = NSStringFromSelector(selector);
            if (!selectionSurfaceTermMatches(name))
                continue;
            const char *types = method_getTypeEncoding(methods[i]);
            [items addObject:[NSString stringWithFormat:@"%@#%@:%s", NSStringFromClass(current), name, types ?: ""]];
        }
        if (methods)
            free(methods);
    }
    return [items componentsJoinedByString:@","];
}

static NSString *propertySurfaceSummary(Class cls)
{
    if (!cls)
        return @"";
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (Class current = cls; current && items.count < 80; current = class_getSuperclass(current)) {
        unsigned int count = 0;
        objc_property_t *properties = class_copyPropertyList(current, &count);
        for (unsigned int i = 0; properties && i < count && items.count < 80; i++) {
            const char *rawName = property_getName(properties[i]);
            NSString *name = rawName ? [NSString stringWithUTF8String:rawName] : @"";
            if (!selectionSurfaceTermMatches(name))
                continue;
            const char *attributes = property_getAttributes(properties[i]);
            [items addObject:[NSString stringWithFormat:@"%@#%@:%s", NSStringFromClass(current), name, attributes ?: ""]];
        }
        if (properties)
            free(properties);
    }
    return [items componentsJoinedByString:@","];
}

static NSString *ivarSurfaceSummary(Class cls)
{
    if (!cls)
        return @"";
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (Class current = cls; current && items.count < 80; current = class_getSuperclass(current)) {
        unsigned int count = 0;
        Ivar *ivars = class_copyIvarList(current, &count);
        for (unsigned int i = 0; ivars && i < count && items.count < 80; i++) {
            const char *rawName = ivar_getName(ivars[i]);
            NSString *name = rawName ? [NSString stringWithUTF8String:rawName] : @"";
            if (!selectionSurfaceTermMatches(name))
                continue;
            const char *type = ivar_getTypeEncoding(ivars[i]);
            [items addObject:[NSString stringWithFormat:@"%@#%@:%s", NSStringFromClass(current), name, type ?: ""]];
        }
        if (ivars)
            free(ivars);
    }
    return [items componentsJoinedByString:@","];
}

static NSString *selectorPresenceSummary(id object)
{
    if (!object)
        return @"";
    NSArray<NSString *> *names = @[
        @"copy:",
        @"selectAll:",
        @"selectedText",
        @"selectedString",
        @"string",
        @"attributedString",
        @"accessibilitySelectedText",
        @"accessibilityValue",
        @"accessibilitySelectedTextRange",
        @"accessibilityVisibleCharacterRange",
    ];
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (NSString *name in names) {
        SEL selector = NSSelectorFromString(name);
        [items addObject:[NSString stringWithFormat:@"%@=%d", name, [object respondsToSelector:selector] ? 1 : 0]];
    }
    return [items componentsJoinedByString:@","];
}

static NSString *safeSelectorValueSummary(id object)
{
    if (!object)
        return @"";
    NSArray<NSString *> *names = @[
        @"accessibilityValue",
        @"accessibilitySelectedText",
        @"string",
    ];
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (NSString *name in names) {
        SEL selector = NSSelectorFromString(name);
        if (![object respondsToSelector:selector]) {
            [items addObject:[NSString stringWithFormat:@"%@=unavailable", name]];
            continue;
        }
        @try {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
            id value = [object performSelector:selector];
#pragma clang diagnostic pop
            [items addObject:[NSString stringWithFormat:@"%@=%@", name, surfaceValueSummary(value)]];
        } @catch (NSException *exception) {
            [items addObject:[NSString stringWithFormat:@"%@=exception:%@", name, exception.name]];
        }
    }
    return [items componentsJoinedByString:@","];
}

static NSString *accessibilitySurfaceSummary(id object)
{
    if (!object || ![object respondsToSelector:@selector(accessibilityAttributeValue:)])
        return @"unavailable";
    NSArray<NSString *> *attributes = @[
        NSAccessibilityValueAttribute,
        NSAccessibilitySelectedTextAttribute,
        NSAccessibilitySelectedTextRangeAttribute,
        NSAccessibilityVisibleCharacterRangeAttribute,
    ];
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    for (NSString *attribute in attributes) {
        @try {
            id value = [object accessibilityAttributeValue:attribute];
            [items addObject:[NSString stringWithFormat:@"%@=%@", attribute, surfaceValueSummary(value)]];
        } @catch (NSException *exception) {
            [items addObject:[NSString stringWithFormat:@"%@=exception:%@", attribute, exception.name]];
        }
    }
    return [items componentsJoinedByString:@","];
}

static bool objectLooksPdfSelectionRelated(id object)
{
    if (!object)
        return false;
    Class cls = [object class];
    for (Class current = cls; current; current = class_getSuperclass(current)) {
        NSString *name = NSStringFromClass(current);
        if (selectionSurfaceTermMatches(name))
            return true;
    }
    return false;
}

static void addSurfaceObject(NSMutableArray<id> *objects, NSMutableSet<NSValue *> *seen, id object)
{
    if (!object)
        return;
    NSValue *key = [NSValue valueWithPointer:(__bridge const void *)(object)];
    if ([seen containsObject:key])
        return;
    [seen addObject:key];
    [objects addObject:object];
}

static void collectPdfSurfaceDescendants(NSView *view, NSMutableArray<id> *objects, NSMutableSet<NSValue *> *seen, NSUInteger depth)
{
    if (!view || depth > 8)
        return;
    if (objectLooksPdfSelectionRelated(view))
        addSurfaceObject(objects, seen, view);
    for (NSView *subview in view.subviews)
        collectPdfSurfaceDescendants(subview, objects, seen, depth + 1);
}

static NSArray<id> *collectPdfSurfaceObjects(WebContents *contents)
{
    NSMutableArray<id> *objects = [NSMutableArray array];
    NSMutableSet<NSValue *> *seen = [NSMutableSet set];
    if (!contents || !contents->web_view)
        return objects;

    addSurfaceObject(objects, seen, contents->web_view);
    NSPoint hitPoint = NSMakePoint(0, contents->web_view.bounds.size.height);
    addSurfaceObject(objects, seen, [contents->web_view hitTest:hitPoint]);
    collectPdfSurfaceDescendants(contents->web_view, objects, seen, 0);

    NSResponder *responder = contents->window.firstResponder;
    for (int i = 0; responder && i < 12; i++) {
        addSurfaceObject(objects, seen, responder);
        responder = responder.nextResponder;
    }

    SEL copySelector = @selector(copy:);
    addSurfaceObject(objects, seen, [NSApp targetForAction:copySelector to:nil from:nil]);
    addSurfaceObject(objects, seen, [NSApp targetForAction:copySelector to:nil from:contents->web_view]);
    return objects;
}

static NSString *surfaceFrameSummary(id object)
{
    if (![object isKindOfClass:NSView.class])
        return @"non-view";
    NSView *view = (NSView *)object;
    return [NSString stringWithFormat:@"frame=%@ bounds=%@ hidden=%d alpha=%.3f",
                     NSStringFromRect(view.frame),
                     NSStringFromRect(view.bounds),
                     view.hidden ? 1 : 0,
                     view.alphaValue];
}

static void tracePdfSelectionSurface(WebContents *contents, NSString *phase)
{
    if (!pdfSelectionSurfaceTraceEnabled() || !contents || !contents->web_view)
        return;

    NSArray<id> *objects = collectPdfSurfaceObjects(contents);
    NSView *hitTarget = [contents->web_view hitTest:NSMakePoint(0, contents->web_view.bounds.size.height)] ?: contents->web_view;
    appendPdfSelectionSurfaceTrace([NSString stringWithFormat:
        @"webkit-pdf-selection-surface-summary tab=%d phase=%@ url=%@ focused=%d gui_active=%d object_count=%lu window=%@ key_window=%d main_window=%d app_key_window=%@ app_main_window=%@ web_view=%@ hit_target=%@ first_responder=%@ responder_chain=%@ clipboard={%@}",
        contents->tab_id,
        phase ?: @"unknown",
        contents->web_view.URL.absoluteString ?: @"",
        contents->focused ? 1 : 0,
        contents->gui_active ? 1 : 0,
        (unsigned long)objects.count,
        describeObject(contents->window),
        contents->window.isKeyWindow ? 1 : 0,
        contents->window.isMainWindow ? 1 : 0,
        describeObject(NSApp.keyWindow),
        describeObject(NSApp.mainWindow),
        describeObject(contents->web_view),
        describeObject(hitTarget),
        describeObject(contents->window.firstResponder),
        responderChain(contents->window.firstResponder),
        clipboardSample()]);

    NSUInteger index = 0;
    for (id object in objects) {
        appendPdfSelectionSurfaceTrace([NSString stringWithFormat:
            @"webkit-pdf-selection-surface-object tab=%d phase=%@ index=%lu object=%@ superclass_chain=%@ %@ selectors={%@} safe_values={%@} accessibility={%@} methods={%@} properties={%@} ivars={%@}",
            contents->tab_id,
            phase ?: @"unknown",
            (unsigned long)index++,
            describeObject(object),
            superclassChainForObject(object),
            surfaceFrameSummary(object),
            selectorPresenceSummary(object),
            safeSelectorValueSummary(object),
            accessibilitySurfaceSummary(object),
            methodSurfaceSummary([object class]),
            propertySurfaceSummary([object class]),
            ivarSurfaceSummary([object class])]);
    }
}

static NSString *routeTraceStringSample(NSString *value)
{
    if (!value)
        return @"nil";
    NSString *sample = value.length > 180 ? [value substringToIndex:180] : value;
    sample = [[sample stringByReplacingOccurrencesOfString:@"\n" withString:@" "] stringByReplacingOccurrencesOfString:@"\t" withString:@" "];
    return [NSString stringWithFormat:@"len=%lu sample=%@", (unsigned long)value.length, sample];
}

static NSString *routeTraceRectSummary(NSRect rect)
{
    return [NSString stringWithFormat:@"x=%.2f y=%.2f w=%.2f h=%.2f empty=%d",
                     rect.origin.x,
                     rect.origin.y,
                     rect.size.width,
                     rect.size.height,
                     NSIsEmptyRect(rect) ? 1 : 0];
}

static bool currentUrlLooksPdf(WebContents *contents)
{
    if (!contents || !contents->web_view.URL)
        return false;
    NSURL *url = contents->web_view.URL;
    if ([url.pathExtension.lowercaseString isEqualToString:@"pdf"])
        return true;
    NSString *absolute = url.absoluteString.lowercaseString;
    return [absolute containsString:@".pdf?"] || [absolute containsString:@".pdf#"];
}

static CGFloat hostBackingScaleForContents(WebContents *contents)
{
    NSScreen *screen = contents && contents->window ? contents->window.screen : nil;
    CGFloat scale = screen.backingScaleFactor ?: NSScreen.mainScreen.backingScaleFactor ?: 1.0;
    return scale > 0 ? scale : 1.0;
}

static NSSize hostWindowPointSizeForContents(WebContents *contents)
{
    if (!contents)
        return NSMakeSize(64, 64);
    CGFloat width = MAX(contents->width, 64);
    CGFloat height = MAX(contents->height, 64);
    CGFloat scale = hostBackingScaleForContents(contents);
    width = MAX(width / scale, 32);
    height = MAX(height / scale, 32);
    return NSMakeSize(width, height);
}

static NSString *pdfViewportFixMode()
{
    NSString *mode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_VIEWPORT_FIX_PROBE"];
    return mode.length ? mode : @"point-scale";
}

static void tracePdfViewportFix(WebContents *contents, NSString *phase, NSString *detail)
{
    if (!pdfViewGeometryTraceEnabled())
        return;
    appendPdfViewGeometryTrace([NSString stringWithFormat:
        @"webkit-pdf-viewport-fix tab=%d phase=%@ url=%@ %@",
        contents ? contents->tab_id : -1,
        phase ?: @"unknown",
        contents && contents->web_view ? contents->web_view.URL.absoluteString ?: @"" : @"",
        detail ?: @""]);
}

static void applyPdfViewportFix(WebContents *contents, NSString *phase)
{
    NSString *mode = pdfViewportFixMode();
    if (!mode.length || !contents || !contents->web_view)
        return;
    if (![mode isEqualToString:@"point-scale"]) {
        tracePdfViewportFix(contents, phase, [NSString stringWithFormat:@"mode=%@ result=skip reason=unsupported-mode", mode]);
        return;
    }

    NSView *hud = findDescendantViewWithClassName(contents->web_view, @"WKPDFHUDView");
    bool pdfLike = currentUrlLooksPdf(contents) || hud != nil;
    if (!pdfLike) {
        tracePdfViewportFix(contents, phase, @"mode=point-scale result=skip reason=non-pdf");
        return;
    }

    NSScreen *screen = contents->window.screen ?: NSScreen.mainScreen;
    CGFloat scale = screen.backingScaleFactor > 0 ? screen.backingScaleFactor : 1.0;
    CGFloat magnification = scale > 0 ? 1.0 / scale : 1.0;
    if (![contents->web_view respondsToSelector:@selector(setMagnification:centeredAtPoint:)]) {
        tracePdfViewportFix(contents, phase, @"mode=point-scale result=skip reason=missing-setMagnification");
        return;
    }

    @try {
        if ([contents->web_view respondsToSelector:@selector(setAllowsMagnification:)])
            [contents->web_view setAllowsMagnification:YES];
        [contents->web_view setMagnification:magnification centeredAtPoint:NSMakePoint(0, contents->web_view.bounds.size.height)];
        [contents->web_view layoutSubtreeIfNeeded];
        tracePdfViewportFix(contents, phase, [NSString stringWithFormat:@"mode=point-scale result=applied backing_scale=%.3f magnification=%.6f bounds=%@ hud=%@",
            scale,
            contents->web_view.magnification,
            NSStringFromRect(contents->web_view.bounds),
            describeObject(hud)]);
    } @catch (NSException *exception) {
        tracePdfViewportFix(contents, phase, [NSString stringWithFormat:@"mode=point-scale result=exception name=%@", exception.name]);
    }
}

static bool currentPdfAllowsCopying(WebContents *contents, NSString **reason)
{
    if (!contents || !contents->web_view.URL) {
        if (reason)
            *reason = @"missing-url";
        return true;
    }
    if (!currentUrlLooksPdf(contents)) {
        if (reason)
            *reason = @"not-pdf";
        return true;
    }

    NSURL *url = contents->web_view.URL;
    PDFDocument *document = [[PDFDocument alloc] initWithURL:url];
    if (!document) {
        if (reason)
            *reason = @"pdf-document-unavailable";
        return false;
    }
    if (!document.allowsCopying) {
        if (reason)
            *reason = @"pdfkit-allows-copying-false";
        return false;
    }
    if (reason) {
        *reason = [NSString stringWithFormat:@"pdfkit-allows-copying-true encrypted=%d locked=%d permissions=%ld",
                            document.isEncrypted ? 1 : 0,
                            document.isLocked ? 1 : 0,
                            (long)document.permissionsStatus];
    }
    return true;
}

static bool pdfDocumentHasEditableWidgets(PDFDocument *document, NSString **reason)
{
    if (!document) {
        if (reason)
            *reason = @"pdf-document-unavailable";
        return false;
    }
    if (document.isEncrypted && document.isLocked) {
        if (reason)
            *reason = @"encrypted-locked";
        return true;
    }

    for (NSUInteger pageIndex = 0; pageIndex < document.pageCount; pageIndex++) {
        PDFPage *page = [document pageAtIndex:pageIndex];
        for (PDFAnnotation *annotation in page.annotations) {
            NSString *type = annotation.type ?: @"";
            NSString *widgetFieldType = nil;
            if ([annotation respondsToSelector:@selector(widgetFieldType)])
                widgetFieldType = [annotation valueForKey:@"widgetFieldType"];
            NSString *fieldName = nil;
            if ([annotation respondsToSelector:@selector(fieldName)])
                fieldName = [annotation valueForKey:@"fieldName"];
            if ([type.lowercaseString containsString:@"widget"] || widgetFieldType.length > 0) {
                if (reason) {
                    *reason = [NSString stringWithFormat:@"widget-annotation page=%lu type=%@ widget_field_type=%@ field_name=%@",
                                        (unsigned long)pageIndex,
                                        type.length ? type : @"none",
                                        widgetFieldType.length ? widgetFieldType : @"none",
                                        fieldName.length ? fieldName : @"none"];
                }
                return true;
            }
        }
    }

    if (reason)
        *reason = [NSString stringWithFormat:@"no-widget-annotations pages=%lu", (unsigned long)document.pageCount];
    return false;
}

static void updatePdfEditableDocumentCache(WebContents *contents, NSString *phase)
{
    if (!contents)
        return;

    NSString *urlString = contents->web_view.URL.absoluteString ?: @"";
    contents->pdf_editable_document_url = [urlString copy];
    contents->pdf_editable_document_known = true;
    contents->pdf_editable_document_has_widgets = false;
    contents->pdf_editable_document_reason = currentUrlLooksPdf(contents) ? @"not-classified" : @"not-pdf";

    if (!currentUrlLooksPdf(contents) || !contents->web_view.URL)
        return;

    NSString *reason = nil;
    PDFDocument *document = [[PDFDocument alloc] initWithURL:contents->web_view.URL];
    bool hasWidgets = pdfDocumentHasEditableWidgets(document, &reason);
    contents->pdf_editable_document_has_widgets = hasWidgets;
    contents->pdf_editable_document_reason = [reason copy] ?: @"unknown";
    tracePdfKeyboard(contents, @"document", [NSString stringWithFormat:@"phase=%@ url=%@ editable_widgets=%d reason=%@",
        phase ?: @"unknown",
        urlString,
        hasWidgets ? 1 : 0,
        reason ?: @"unknown"]);
}

static bool currentPdfHasEditableDocumentWidgets(WebContents *contents)
{
    if (!contents || !contents->web_view.URL || !currentUrlLooksPdf(contents))
        return false;
    NSString *urlString = contents->web_view.URL.absoluteString ?: @"";
    if (!contents->pdf_editable_document_known || ![contents->pdf_editable_document_url isEqualToString:urlString])
        updatePdfEditableDocumentCache(contents, @"keyboard");
    return contents->pdf_editable_document_has_widgets;
}

static void clearPdfSelectedTextCache(WebContents *contents, NSString *reason)
{
    if (!contents)
        return;
    contents->pdf_selected_text_cache_epoch++;
    if (pdfSelectedTextCacheCopyTraceEnabled()) {
        appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
            @"webkit-pdf-selected-text-cache tab=%d action=clear reason=%@ had_cache=%d generation=%llu cache_generation=%llu epoch=%llu consumed=%d text={%@}",
            contents->tab_id,
            reason ?: @"unknown",
            contents->pdf_selected_text_cache.length > 0 ? 1 : 0,
            (unsigned long long)contents->pdf_selected_text_generation,
            (unsigned long long)contents->pdf_selected_text_cache_generation,
            (unsigned long long)contents->pdf_selected_text_cache_epoch,
            contents->pdf_selected_text_cache_consumed ? 1 : 0,
            routeTraceStringSample(contents->pdf_selected_text_cache)]);
    }
    contents->pdf_selected_text_cache = nil;
    contents->pdf_selected_text_cache_phase = nil;
    contents->pdf_selected_text_cache_url = nil;
    contents->pdf_selected_text_cache_time = 0;
    contents->pdf_selected_text_cache_generation = 0;
    contents->pdf_selected_text_cache_capture_epoch = 0;
    contents->pdf_selected_text_cache_consumed = false;
    contents->pdf_selected_text_copy_start_pasteboard = nil;
    contents->pdf_selected_text_stabilization_active = false;
    contents->pdf_selected_text_stabilization_deadline = 0;
    contents->pdf_selected_text_stabilization_epoch = contents->pdf_selected_text_cache_epoch;
}

static void requestPdfSelectedTextCacheCaptureSample(WebContents *contents, NSString *phase, uint64_t generation, uint64_t epoch, NSString *url, NSTimeInterval delaySeconds, bool stabilized)
{
    if (!pdfSelectedTextCacheCopyEnabled() || !contents || !contents->web_view)
        return;

    appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-selected-text-cache tab=%d action=%@ phase=%@ delay=%.3f url=%@ generation=%llu epoch=%llu pdf_url=%d",
        contents->tab_id,
        stabilized ? @"stabilized-sample-request" : @"capture-request",
        phase ?: @"unknown",
        delaySeconds,
        url,
        (unsigned long long)generation,
        (unsigned long long)epoch,
        currentUrlLooksPdf(contents) ? 1 : 0]);

    if (!currentUrlLooksPdf(contents)) {
        clearPdfSelectedTextCache(contents, @"capture-not-pdf");
        return;
    }
    NSString *copyPermissionReason = nil;
    if (!currentPdfAllowsCopying(contents, &copyPermissionReason)) {
        appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
            @"webkit-pdf-selected-text-cache tab=%d action=capture-skip reason=copy-restricted detail=%@",
            contents->tab_id,
            copyPermissionReason ?: @"unknown"]);
        clearPdfSelectedTextCache(contents, @"capture-copy-restricted");
        return;
    }

    SEL getSelectedText = NSSelectorFromString(@"getSelectedText:");
    if (![contents->web_view respondsToSelector:getSelectedText]) {
        clearPdfSelectedTextCache(contents, @"capture-route-unavailable");
        return;
    }

    @try {
        typedef void (*GetSelectedTextFn)(id, SEL, void (^)(NSString *));
        GetSelectedTextFn fn = (GetSelectedTextFn)[contents->web_view methodForSelector:getSelectedText];
        WebContents *capturedContents = contents;
        NSString *phaseCopy = [phase copy] ?: @"unknown";
        NSString *urlCopy = [url copy];
        NSTimeInterval delayCopy = delaySeconds;
        fn(contents->web_view, getSelectedText, ^(NSString *text) {
            if (!capturedContents)
                return;
            NSString *value = [text isKindOfClass:NSString.class] ? text : @"";
            NSString *reason = nil;
            if (!currentUrlLooksPdf(capturedContents))
                reason = @"callback-not-pdf";
            else if (generation != capturedContents->pdf_selected_text_generation)
                reason = @"generation-mismatch";
            else if (epoch != capturedContents->pdf_selected_text_cache_epoch)
                reason = @"epoch-mismatch";
            else if (![capturedContents->web_view.URL.absoluteString ?: @"" isEqualToString:urlCopy])
                reason = @"url-changed";
            else if (value.length == 0)
                reason = @"empty";
            else if ([value isEqualToString:urlCopy])
                reason = @"looks-like-url";
            else if (value.length <= 8)
                reason = @"too-short";

            if (reason) {
                appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
                    @"webkit-pdf-selected-text-cache tab=%d action=%@ reason=%@ phase=%@ delay=%.3f generation=%llu epoch=%llu text={%@}",
                    capturedContents->tab_id,
                    stabilized ? @"stabilized-sample-reject" : @"capture-reject",
                    reason,
                    phaseCopy,
                    delayCopy,
                    (unsigned long long)generation,
                    (unsigned long long)epoch,
                    routeTraceStringSample(value)]);
                return;
            }

            NSString *previous = capturedContents->pdf_selected_text_cache ?: @"";
            bool replace = !previous.length || value.length >= previous.length;
            if (replace) {
                capturedContents->pdf_selected_text_cache = [value copy];
                capturedContents->pdf_selected_text_cache_phase = phaseCopy;
                capturedContents->pdf_selected_text_cache_url = urlCopy;
                capturedContents->pdf_selected_text_cache_time = [[NSDate date] timeIntervalSince1970];
                capturedContents->pdf_selected_text_cache_generation = generation;
                capturedContents->pdf_selected_text_cache_capture_epoch = epoch;
                capturedContents->pdf_selected_text_cache_consumed = false;
            }
            appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
                @"webkit-pdf-selected-text-cache tab=%d action=%@ phase=%@ delay=%.3f generation=%llu epoch=%llu replace=%d previous={%@} text={%@}",
                capturedContents->tab_id,
                stabilized ? @"stabilized-sample-accept" : @"capture-accept",
                phaseCopy,
                delayCopy,
                (unsigned long long)generation,
                (unsigned long long)epoch,
                replace ? 1 : 0,
                routeTraceStringSample(previous),
                routeTraceStringSample(value)]);
            if (stabilized && replace) {
                appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
                    @"webkit-pdf-selected-text-cache tab=%d action=capture-accept phase=%@ generation=%llu text={%@}",
                    capturedContents->tab_id,
                    phaseCopy,
                    (unsigned long long)generation,
                    routeTraceStringSample(value)]);
            }
        });
    } @catch (NSException *exception) {
        appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-cache tab=%d action=capture-exception phase=%@ exception=%@", contents->tab_id, phase ?: @"unknown", exception.name]);
        clearPdfSelectedTextCache(contents, @"capture-exception");
    }
}

static void requestPdfSelectedTextCacheCapture(WebContents *contents, NSString *phase)
{
    if (!pdfSelectedTextCacheCopyEnabled() || !contents || !contents->web_view)
        return;

    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    uint64_t generation = contents->pdf_selected_text_generation;
    uint64_t epoch = contents->pdf_selected_text_cache_epoch;
    if (!pdfSelectedTextStabilizedCaptureEnabled()) {
        requestPdfSelectedTextCacheCaptureSample(contents, phase, generation, epoch, url, 0, false);
        return;
    }

    contents->pdf_selected_text_stabilization_active = true;
    contents->pdf_selected_text_stabilization_epoch = epoch;
    contents->pdf_selected_text_stabilization_deadline = [[NSDate date] timeIntervalSince1970] + 0.75;
    appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-selected-text-cache tab=%d action=stabilized-capture-start phase=%@ generation=%llu epoch=%llu url=%@ deadline=%.3f",
        contents->tab_id,
        phase ?: @"unknown",
        (unsigned long long)generation,
        (unsigned long long)epoch,
        url,
        contents->pdf_selected_text_stabilization_deadline]);

    NSArray<NSNumber *> *delays = @[ @0.0, @0.15, @0.35, @0.70 ];
    for (NSNumber *delayNumber in delays) {
        NSTimeInterval delay = delayNumber.doubleValue;
        WebContents *capturedContents = contents;
        NSString *phaseCopy = [phase copy] ?: @"unknown";
        NSString *urlCopy = [url copy];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            requestPdfSelectedTextCacheCaptureSample(capturedContents, phaseCopy, generation, epoch, urlCopy, delay, true);
            if (delay >= 0.70 && capturedContents && capturedContents->pdf_selected_text_cache_epoch == epoch) {
                capturedContents->pdf_selected_text_stabilization_active = false;
                appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
                    @"webkit-pdf-selected-text-cache tab=%d action=stabilized-capture-complete phase=%@ generation=%llu epoch=%llu cached={%@}",
                    capturedContents->tab_id,
                    phaseCopy,
                    (unsigned long long)generation,
                    (unsigned long long)epoch,
                    routeTraceStringSample(capturedContents->pdf_selected_text_cache)]);
            }
        });
    }
}

static bool shouldReplaceClipboardWithPdfSelectedTextCache(WebContents *contents, NSString *normalText, NSString **reason)
{
    if (!contents) {
        if (reason)
            *reason = @"missing-contents";
        return false;
    }
    NSString *cached = contents->pdf_selected_text_cache;
    if (!pdfSelectedTextCacheCopyEnabled()) {
        if (reason)
            *reason = @"flag-disabled";
        return false;
    }
    if (!cached.length) {
        if (reason)
            *reason = @"empty-cache";
        return false;
    }
    if (contents->pdf_selected_text_cache_consumed) {
        if (reason)
            *reason = @"cache-consumed";
        return false;
    }
    if (!currentUrlLooksPdf(contents)) {
        if (reason)
            *reason = @"not-pdf";
        return false;
    }
    NSString *copyPermissionReason = nil;
    if (!currentPdfAllowsCopying(contents, &copyPermissionReason)) {
        if (reason)
            *reason = [NSString stringWithFormat:@"copy-restricted:%@", copyPermissionReason ?: @"unknown"];
        return false;
    }
    NSString *currentUrl = contents->web_view.URL.absoluteString ?: @"";
    if (![(contents->pdf_selected_text_cache_url ?: @"") isEqualToString:currentUrl]) {
        if (reason)
            *reason = @"url-mismatch";
        return false;
    }
    if (contents->pdf_selected_text_cache_generation != contents->pdf_selected_text_generation) {
        if (reason)
            *reason = @"generation-mismatch";
        return false;
    }
    if (contents->pdf_selected_text_cache_capture_epoch != contents->pdf_selected_text_cache_epoch) {
        if (reason)
            *reason = @"epoch-mismatch";
        return false;
    }
    NSTimeInterval age = [[NSDate date] timeIntervalSince1970] - contents->pdf_selected_text_cache_time;
    NSTimeInterval maxAge = pdfSelectedTextCacheMaxAge();
    if (age < 0 || age > maxAge) {
        if (reason)
            *reason = [NSString stringWithFormat:@"stale-age:%.3f>%.3f", age, maxAge];
        return false;
    }
    NSString *normal = normalText ?: @"";
    if ([normal isEqualToString:cached] || [normal containsString:cached]) {
        if (reason)
            *reason = @"normal-already-complete";
        return false;
    }
    if (normal.length == 0 || ([cached hasPrefix:normal] && cached.length > normal.length)) {
        if (reason)
            *reason = @"replace-prefix";
        return true;
    }
    if (contents->pdf_selected_text_copy_start_pasteboard && [normal isEqualToString:contents->pdf_selected_text_copy_start_pasteboard]) {
        if (reason)
            *reason = @"replace-unchanged";
        return true;
    }
    if (reason)
        *reason = @"normal-not-prefix";
    return false;
}

static void applyPdfSelectedTextCacheCopyIfNeeded(WebContents *contents, NSString *phase)
{
    if (!pdfSelectedTextCacheCopyEnabled() || !contents)
        return;
    NSString *normalText = [NSPasteboard.generalPasteboard stringForType:NSPasteboardTypeString] ?: @"";
    NSString *reason = nil;
    bool replace = shouldReplaceClipboardWithPdfSelectedTextCache(contents, normalText, &reason);
    appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-selected-text-cache tab=%d action=copy-decision phase=%@ replace=%d reason=%@ normal={%@} cached={%@} generation=%llu cache_generation=%llu",
        contents->tab_id,
        phase ?: @"unknown",
        replace ? 1 : 0,
        reason ?: @"unknown",
        routeTraceStringSample(normalText),
        routeTraceStringSample(contents->pdf_selected_text_cache),
        (unsigned long long)contents->pdf_selected_text_generation,
        (unsigned long long)contents->pdf_selected_text_cache_generation]);
    if (replace) {
        [NSPasteboard.generalPasteboard clearContents];
        [NSPasteboard.generalPasteboard setString:contents->pdf_selected_text_cache forType:NSPasteboardTypeString];
        appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
            @"webkit-pdf-selected-text-cache tab=%d action=copy-replace phase=%@ final={%@}",
            contents->tab_id,
            phase ?: @"unknown",
            clipboardSample()]);
    }
    contents->pdf_selected_text_cache_consumed = true;
    clearPdfSelectedTextCache(contents, @"copy-attempt-complete");
}

static void schedulePdfSelectedTextCacheCopyDecision(WebContents *contents, NSString *phase)
{
    if (!pdfSelectedTextCacheCopyEnabled() || !contents)
        return;
    WebContents *capturedContents = contents;
    NSString *phaseCopy = [phase copy] ?: @"unknown";
    NSTimeInterval now = [[NSDate date] timeIntervalSince1970];
    NSTimeInterval delay = 0.5;
    NSString *waitReason = @"default";
    if (pdfSelectedTextStabilizedCaptureEnabled() && contents->pdf_selected_text_stabilization_active) {
        NSTimeInterval untilDeadline = contents->pdf_selected_text_stabilization_deadline - now;
        if (untilDeadline > 0) {
            delay = MIN(0.5, untilDeadline);
            waitReason = delay >= untilDeadline ? @"stabilization-complete" : @"stabilization-timeout";
        } else {
            delay = 0;
            waitReason = @"stabilization-deadline-passed";
        }
    }
    appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-selected-text-cache tab=%d action=copy-schedule phase=%@ delay=%.3f wait_reason=%@ stabilization_active=%d deadline=%.3f epoch=%llu",
        contents->tab_id,
        phaseCopy,
        delay,
        waitReason,
        contents->pdf_selected_text_stabilization_active ? 1 : 0,
        contents->pdf_selected_text_stabilization_deadline,
        (unsigned long long)contents->pdf_selected_text_cache_epoch]);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(delay * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        applyPdfSelectedTextCacheCopyIfNeeded(capturedContents, phaseCopy);
    });
}

static void tracePdfSelectedTextRoutes(WebContents *contents, NSString *phase)
{
    if (!pdfSelectedTextRouteTraceEnabled() || !contents || !contents->web_view)
        return;

    WKWebView *webView = contents->web_view;
    appendPdfSelectedTextRouteTrace([NSString stringWithFormat:
        @"webkit-pdf-selected-text-route-summary tab=%d phase=%@ url=%@ web_view=%@ clipboard={%@}",
        contents->tab_id,
        phase ?: @"unknown",
        webView.URL.absoluteString ?: @"",
        describeObject(webView),
        clipboardSample()]);

    SEL getSelectedText = NSSelectorFromString(@"getSelectedText:");
    if ([webView respondsToSelector:getSelectedText]) {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-start tab=%d phase=%@ route=getSelectedText responds=1", contents->tab_id, phase ?: @"unknown"]);
        @try {
            typedef void (*GetSelectedTextFn)(id, SEL, void (^)(NSString *));
            GetSelectedTextFn fn = (GetSelectedTextFn)[webView methodForSelector:getSelectedText];
            int tab = contents->tab_id;
            NSString *phaseCopy = [phase copy] ?: @"unknown";
            fn(webView, getSelectedText, ^(NSString *text) {
                appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=getSelectedText text={%@}", tab, phaseCopy, routeTraceStringSample(text)]);
            });
        } @catch (NSException *exception) {
            appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=getSelectedText exception=%@", contents->tab_id, phase ?: @"unknown", exception.name]);
        }
    } else {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-start tab=%d phase=%@ route=getSelectedText responds=0", contents->tab_id, phase ?: @"unknown"]);
    }

    SEL selectedRangeWithCompletionHandler = NSSelectorFromString(@"selectedRangeWithCompletionHandler:");
    if ([webView respondsToSelector:selectedRangeWithCompletionHandler]) {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-start tab=%d phase=%@ route=selectedRangeWithCompletionHandler responds=1", contents->tab_id, phase ?: @"unknown"]);
        @try {
            typedef void (*SelectedRangeFn)(id, SEL, void (^)(NSRange));
            SelectedRangeFn fn = (SelectedRangeFn)[webView methodForSelector:selectedRangeWithCompletionHandler];
            int tab = contents->tab_id;
            NSString *phaseCopy = [phase copy] ?: @"unknown";
            fn(webView, selectedRangeWithCompletionHandler, ^(NSRange range) {
                appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=selectedRangeWithCompletionHandler location=%lu length=%lu", tab, phaseCopy, (unsigned long)range.location, (unsigned long)range.length]);
            });
        } @catch (NSException *exception) {
            appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=selectedRangeWithCompletionHandler exception=%@", contents->tab_id, phase ?: @"unknown", exception.name]);
        }
    } else {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-start tab=%d phase=%@ route=selectedRangeWithCompletionHandler responds=0", contents->tab_id, phase ?: @"unknown"]);
    }

    SEL unionRectInVisibleSelectedRange = NSSelectorFromString(@"unionRectInVisibleSelectedRange");
    if ([webView respondsToSelector:unionRectInVisibleSelectedRange]) {
        @try {
            typedef NSRect (*UnionRectFn)(id, SEL);
            UnionRectFn fn = (UnionRectFn)[webView methodForSelector:unionRectInVisibleSelectedRange];
            NSRect rect = fn(webView, unionRectInVisibleSelectedRange);
            appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=unionRectInVisibleSelectedRange rect={%@}", contents->tab_id, phase ?: @"unknown", routeTraceRectSummary(rect)]);
        } @catch (NSException *exception) {
            appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=unionRectInVisibleSelectedRange exception=%@", contents->tab_id, phase ?: @"unknown", exception.name]);
        }
    } else {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-start tab=%d phase=%@ route=unionRectInVisibleSelectedRange responds=0", contents->tab_id, phase ?: @"unknown"]);
    }

    SEL writeSelectionToPasteboard = NSSelectorFromString(@"writeSelectionToPasteboard:types:");
    if ([webView respondsToSelector:writeSelectionToPasteboard]) {
        @try {
            NSString *pasteboardName = [NSString stringWithFormat:@"termsurf-exp63-%d-%@-%@", contents->tab_id, phase ?: @"unknown", NSUUID.UUID.UUIDString];
            NSPasteboard *pasteboard = [NSPasteboard pasteboardWithName:pasteboardName];
            [pasteboard clearContents];
            typedef BOOL (*WriteSelectionFn)(id, SEL, NSPasteboard *, NSArray *);
            WriteSelectionFn fn = (WriteSelectionFn)[webView methodForSelector:writeSelectionToPasteboard];
            NSArray *types = @[ NSPasteboardTypeString ];
            BOOL ok = fn(webView, writeSelectionToPasteboard, pasteboard, types);
            NSString *text = [pasteboard stringForType:NSPasteboardTypeString];
            appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=writeSelectionToPasteboard ok=%d pasteboard=%@ text={%@}", contents->tab_id, phase ?: @"unknown", ok ? 1 : 0, pasteboardName, routeTraceStringSample(text)]);
            [pasteboard clearContents];
        } @catch (NSException *exception) {
            appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-result tab=%d phase=%@ route=writeSelectionToPasteboard exception=%@", contents->tab_id, phase ?: @"unknown", exception.name]);
        }
    } else {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-route-start tab=%d phase=%@ route=writeSelectionToPasteboard responds=0", contents->tab_id, phase ?: @"unknown"]);
    }
}

static void tracePdfNavigationDiagnostics(WebContents *contents, NSString *phase, NSString *url)
{
    if (!contents || !contents->web_view)
        return;
    if (pdfSelectedTextRouteTraceEnabled()) {
        appendPdfSelectedTextRouteTrace([NSString stringWithFormat:
            @"webkit-pdf-selection-navigation tab=%d phase=%@ pid=%d url=%@ web_view=%@ window=%@ first_responder=%@ responder_chain=%@ ca_context=%u pdf_url=%d",
            contents->tab_id,
            phase ?: @"unknown",
            getpid(),
            url ?: contents->web_view.URL.absoluteString ?: @"",
            describeObject(contents->web_view),
            describeObject(contents->window),
            describeObject(contents->window.firstResponder),
            responderChain(contents->window.firstResponder),
            contents->live_context_id,
            currentUrlLooksPdf(contents) ? 1 : 0]);
    }
    tracePdfSelectionSurface(contents, phase);
    tracePdfSelectedTextRoutes(contents, phase);
    tracePdfViewGeometry(contents, phase, 0, 0, NSMakePoint(0, 0));
}

static NSString *describeViewTree(NSView *view, NSUInteger depth)
{
    if (!view || depth > 5)
        return @"";

    NSMutableArray<NSString *> *items = [NSMutableArray array];
    NSString *layerBacked = view.wantsLayer ? @"layered" : @"not-layered";
    NSString *hidden = view.hidden ? @"hidden" : @"visible";
    [items addObject:[NSString stringWithFormat:@"%@:%p frame=%@ bounds=%@ %@ alpha=%.3f %@",
                               NSStringFromClass([view class]),
                               view,
                               NSStringFromRect(view.frame),
                               NSStringFromRect(view.bounds),
                               hidden,
                               view.alphaValue,
                               layerBacked]];
    for (NSView *subview in view.subviews) {
        NSString *child = describeViewTree(subview, depth + 1);
        if (child.length)
            [items addObject:[NSString stringWithFormat:@"[%@]", child]];
    }
    return [items componentsJoinedByString:@" "];
}

static NSView *findDescendantViewWithClassName(NSView *view, NSString *className)
{
    if (!view || !className.length)
        return nil;
    if ([NSStringFromClass([view class]) isEqualToString:className])
        return view;
    for (NSView *subview in view.subviews) {
        NSView *found = findDescendantViewWithClassName(subview, className);
        if (found)
            return found;
    }
    return nil;
}

static bool performPdfHudSavePrivateHook(WebContents *contents)
{
    if (!contents || !contents->web_view)
        return false;
    if (!currentUrlLooksPdf(contents)) {
        tracePdfHudSave(contents, @"private-hook-skip", @"reason=non-pdf-url");
        return false;
    }

    NSView *hud = findDescendantViewWithClassName(contents->web_view, @"WKPDFHUDView");
    if (!hud) {
        tracePdfHudSave(contents, @"private-hook-skip", @"reason=missing-hud");
        return false;
    }

    SEL selector = NSSelectorFromString(@"_performActionForControl:");
    if (![hud respondsToSelector:selector]) {
        tracePdfHudSave(contents, @"private-hook-skip", [NSString stringWithFormat:@"reason=selector-unavailable hud_class=%@", NSStringFromClass(hud.class)]);
        return false;
    }

    tracePdfHudSave(contents, @"private-hook-invoke", [NSString stringWithFormat:@"hud_class=%@ control=arrow.down.circle", NSStringFromClass(hud.class)]);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
    [hud performSelector:selector withObject:@"arrow.down.circle"];
#pragma clang diagnostic pop
    return true;
}

static WebContents *findContentsByTabId(int tab_id);
static void capturePdfVisualOracleSnapshot(WebContents *contents, NSString *action, NSString *phase, void (^completion)(void));

static NSArray<NSString *> *allowedPdfActionControls(NSString *action)
{
    if ([action isEqualToString:@"page-next"])
        return @[ @"chevron.down", @"arrow.down" ];
    if ([action isEqualToString:@"page-previous"])
        return @[ @"chevron.up", @"arrow.up" ];
    if ([action isEqualToString:@"zoom-in"])
        return @[ @"plus.magnifyingglass", @"plus" ];
    if ([action isEqualToString:@"zoom-out"])
        return @[ @"minus.magnifyingglass", @"minus" ];
    if ([action isEqualToString:@"fit-width"])
        return @[ @"rectangle.expand.horizontal", @"arrow.left.and.right" ];
    if ([action isEqualToString:@"fit-page"])
        return @[ @"rectangle.expand.vertical", @"arrow.up.left.and.arrow.down.right" ];
    if ([action isEqualToString:@"rotate"])
        return @[ @"rotate.right", @"arrow.clockwise" ];
    return @[];
}

static bool selectorNameAllowedForPdfStateOracle(NSString *name)
{
    static NSSet<NSString *> *allowed = nil;
    static NSSet<NSString *> *deniedTerms = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        allowed = [NSSet setWithArray:@[
            @"accessibilityValue",
            @"accessibilityLabel",
            @"accessibilityRole",
            @"accessibilityEnabled",
            @"accessibilitySelectedText",
            @"accessibilitySelectedTextRange",
            @"accessibilityVisibleCharacterRange",
            @"bounds",
            @"frame",
            @"visibleRect",
            @"documentView",
            @"contentView",
            @"documentVisibleRect",
            @"magnification",
            @"pageCount",
            @"currentPage",
            @"currentPageIndex",
            @"displayMode",
            @"displayBox",
            @"scaleFactor",
            @"minScaleFactor",
            @"maxScaleFactor",
            @"autoScales",
            @"rotation",
            @"document",
            @"isHidden",
            @"isFlipped",
            @"wantsLayer",
        ]];
        deniedTerms = [NSSet setWithArray:@[
            @"set",
            @"add",
            @"remove",
            @"open",
            @"save",
            @"print",
            @"download",
            @"perform",
            @"navigate",
            @"reload",
            @"delete",
            @"write",
            @"show",
            @"present",
            @"run",
            @"menu",
        ]];
    });
    if (![allowed containsObject:name])
        return false;
    NSString *lower = name.lowercaseString;
    for (NSString *term in deniedTerms) {
        if ([lower containsString:term])
            return false;
    }
    return true;
}

static NSString *pdfStateOracleSelectorValue(id object, NSString *name)
{
    if (!object || !selectorNameAllowedForPdfStateOracle(name))
        return @"blocked";
    SEL selector = NSSelectorFromString(name);
    if (![object respondsToSelector:selector])
        return @"unavailable";
    NSMethodSignature *signature = [object methodSignatureForSelector:selector];
    if (!signature)
        return @"missing-signature";
    if (signature.numberOfArguments != 2)
        return @"blocked-arguments";
    const char *returnType = signature.methodReturnType;
    if (!returnType || returnType[0] == 'v')
        return @"blocked-void";

    @try {
        NSInvocation *invocation = [NSInvocation invocationWithMethodSignature:signature];
        invocation.target = object;
        invocation.selector = selector;
        [invocation invoke];

        if (strcmp(returnType, @encode(id)) == 0 || returnType[0] == '@') {
            __unsafe_unretained id value = nil;
            [invocation getReturnValue:&value];
            return surfaceValueSummary(value);
        }
        if (strcmp(returnType, @encode(BOOL)) == 0 || strcmp(returnType, "B") == 0 || returnType[0] == 'c') {
            BOOL value = NO;
            [invocation getReturnValue:&value];
            return value ? @"true" : @"false";
        }
        if (strcmp(returnType, @encode(NSRect)) == 0 || strcmp(returnType, @encode(CGRect)) == 0) {
            NSRect value = NSZeroRect;
            [invocation getReturnValue:&value];
            return NSStringFromRect(value);
        }
        if (strcmp(returnType, @encode(NSSize)) == 0 || strcmp(returnType, @encode(CGSize)) == 0) {
            NSSize value = NSZeroSize;
            [invocation getReturnValue:&value];
            return NSStringFromSize(value);
        }
        if (strcmp(returnType, @encode(NSPoint)) == 0 || strcmp(returnType, @encode(CGPoint)) == 0) {
            NSPoint value = NSZeroPoint;
            [invocation getReturnValue:&value];
            return NSStringFromPoint(value);
        }
        if (returnType[0] == 'f') {
            float value = 0;
            [invocation getReturnValue:&value];
            return [NSString stringWithFormat:@"%.6f", value];
        }
        if (returnType[0] == 'd') {
            double value = 0;
            [invocation getReturnValue:&value];
            return [NSString stringWithFormat:@"%.6f", value];
        }
        if (strchr("islqISLQ", returnType[0])) {
            long long value = 0;
            [invocation getReturnValue:&value];
            return [NSString stringWithFormat:@"%lld", value];
        }
        return [NSString stringWithFormat:@"blocked-type:%s", returnType];
    } @catch (NSException *exception) {
        return [NSString stringWithFormat:@"exception:%@", exception.name];
    }
}

static bool objectLooksPdfStateRelated(id object)
{
    if (!object)
        return false;
    for (Class current = [object class]; current; current = class_getSuperclass(current)) {
        NSString *lower = NSStringFromClass(current).lowercaseString;
        if ([lower containsString:@"pdf"] || [lower containsString:@"scroll"] || [lower containsString:@"clip"] || [lower containsString:@"web"])
            return true;
    }
    return false;
}

static void collectPdfStateOracleObjectsFromView(NSView *view, NSMutableArray<id> *objects, NSMutableSet<NSValue *> *seen, NSUInteger depth)
{
    if (!view || depth > 8)
        return;
    NSValue *key = [NSValue valueWithPointer:(__bridge const void *)(view)];
    if (![seen containsObject:key] && objectLooksPdfStateRelated(view)) {
        [seen addObject:key];
        [objects addObject:view];
    }
    for (NSView *subview in view.subviews)
        collectPdfStateOracleObjectsFromView(subview, objects, seen, depth + 1);
}

static NSString *pdfStateOracleSurfaceSummary(WebContents *contents, NSView *hud)
{
    if (!contents || !contents->web_view)
        return @"missing-webview";

    NSMutableArray<id> *objects = [NSMutableArray array];
    NSMutableSet<NSValue *> *seen = [NSMutableSet set];
    void (^addObject)(id) = ^(id object) {
        if (!object)
            return;
        NSValue *key = [NSValue valueWithPointer:(__bridge const void *)(object)];
        if ([seen containsObject:key])
            return;
        [seen addObject:key];
        [objects addObject:object];
    };
    addObject(contents->web_view);
    addObject(hud);
    addObject(contents->window.firstResponder);
    collectPdfStateOracleObjectsFromView(contents->web_view, objects, seen, 0);

    NSArray<NSString *> *selectors = @[
        @"accessibilityValue",
        @"accessibilityLabel",
        @"accessibilityRole",
        @"accessibilityEnabled",
        @"accessibilitySelectedText",
        @"accessibilitySelectedTextRange",
        @"accessibilityVisibleCharacterRange",
        @"bounds",
        @"frame",
        @"visibleRect",
        @"documentView",
        @"contentView",
        @"documentVisibleRect",
        @"magnification",
        @"pageCount",
        @"currentPage",
        @"currentPageIndex",
        @"displayMode",
        @"displayBox",
        @"scaleFactor",
        @"minScaleFactor",
        @"maxScaleFactor",
        @"autoScales",
        @"rotation",
        @"document",
        @"isHidden",
        @"isFlipped",
        @"wantsLayer",
    ];

    NSMutableArray<NSString *> *items = [NSMutableArray array];
    NSUInteger index = 0;
    for (id object in objects) {
        if (index >= 24)
            break;
        NSMutableArray<NSString *> *values = [NSMutableArray array];
        for (NSString *selectorName in selectors) {
            NSString *value = pdfStateOracleSelectorValue(object, selectorName);
            if (![value isEqualToString:@"unavailable"])
                [values addObject:[NSString stringWithFormat:@"%@=%@", selectorName, value]];
        }
        [items addObject:[NSString stringWithFormat:@"object%lu=%@ superclass=%@ frame=%@ values={%@} methods={%@} properties={%@} ivars={%@}",
            (unsigned long)index++,
            describeObject(object),
            superclassChainForObject(object),
            [object isKindOfClass:NSView.class] ? surfaceFrameSummary(object) : @"non-view",
            [values componentsJoinedByString:@","],
            methodSurfaceSummary([object class]),
            propertySurfaceSummary([object class]),
            ivarSurfaceSummary([object class])]];
    }

    return [items componentsJoinedByString:@" || "];
}

static NSString *pdfActionStateSummary(WebContents *contents, NSView *hud)
{
    if (!contents || !contents->web_view)
        return @"missing-webview";
    WKWebView *webView = contents->web_view;
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    [items addObject:[NSString stringWithFormat:@"web_view=%@ frame=%@ bounds=%@",
        describeObject(webView),
        NSStringFromRect(webView.frame),
        NSStringFromRect(webView.bounds)]];
    if ([webView respondsToSelector:@selector(magnification)])
        [items addObject:[NSString stringWithFormat:@"magnification=%.6f", webView.magnification]];
    [items addObject:[NSString stringWithFormat:@"hud=%@ hud_responds=%d hud_accessibility={%@}",
        describeObject(hud),
        hud && [hud respondsToSelector:NSSelectorFromString(@"_performActionForControl:")] ? 1 : 0,
        hud ? accessibilitySurfaceSummary(hud) : @"missing"]];
    NSString *scrollViews = describeScrollViews(webView);
    [items addObject:[NSString stringWithFormat:@"scroll_views={%@}", scrollViews.length ? scrollViews : @"none"]];
    [items addObject:[NSString stringWithFormat:@"view_tree={%@}", describeViewTree(webView, 0)]];
    if (pdfStateOracleProbeEnabled())
        [items addObject:[NSString stringWithFormat:@"state_oracle={%@}", pdfStateOracleSurfaceSummary(contents, hud)]];
    return [items componentsJoinedByString:@" "];
}

static NSString *pdfDirectCommandSelectorForAction(NSString *action)
{
    if ([action isEqualToString:@"zoom-in"])
        return @"performPDFZoomIn";
    if ([action isEqualToString:@"zoom-out"])
        return @"performPDFZoomOut";
    return nil;
}

static NSString *pdfDirectCommandIdentitySummary(WebContents *contents, NSView *hud, NSString *selectorName)
{
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    SEL selector = NSSelectorFromString(selectorName ?: @"");
    [items addObject:[NSString stringWithFormat:@"hud=%@", describeObject(hud)]];
    [items addObject:[NSString stringWithFormat:@"hud_class=%@", hud ? NSStringFromClass(hud.class) : @"missing"]];
    [items addObject:[NSString stringWithFormat:@"selector=%@", selectorName ?: @"none"]];
    [items addObject:[NSString stringWithFormat:@"responds=%d", hud && [hud respondsToSelector:selector] ? 1 : 0]];
    [items addObject:[NSString stringWithFormat:@"web_view=%@", describeObject(contents ? contents->web_view : nil)]];
    [items addObject:[NSString stringWithFormat:@"tab=%d", contents ? contents->tab_id : 0]];
    [items addObject:[NSString stringWithFormat:@"url=%@", contents && contents->web_view.URL.absoluteString ? contents->web_view.URL.absoluteString : @""]];
    @try {
        id pluginIdentifier = hud ? [hud valueForKey:@"pluginIdentifier"] : nil;
        id frameIdentifier = hud ? [hud valueForKey:@"frameIdentifier"] : nil;
        id hudWebView = hud ? [hud valueForKey:@"webView"] : nil;
        [items addObject:[NSString stringWithFormat:@"pluginIdentifier=%@", pluginIdentifier ?: @"nil"]];
        [items addObject:[NSString stringWithFormat:@"frameIdentifier=%@", frameIdentifier ?: @"nil"]];
        [items addObject:[NSString stringWithFormat:@"hud_web_view=%@", describeObject(hudWebView)]];
        [items addObject:[NSString stringWithFormat:@"hud_web_view_matches=%d", contents && hudWebView == contents->web_view ? 1 : 0]];
    } @catch (NSException *exception) {
        [items addObject:[NSString stringWithFormat:@"identity_exception=%@", exception.name]];
    }
    return [items componentsJoinedByString:@" "];
}

static NSString *pdfVisualMetricsForSnapshot(NSImage *image, NSError *error);

static bool pdfHudWebViewMatches(WebContents *contents, NSView *hud, bool *known)
{
    if (known)
        *known = false;
    if (!contents || !contents->web_view || !hud)
        return false;
    @try {
        id hudWebView = [hud valueForKey:@"webView"];
        if (known)
            *known = hudWebView != nil;
        return hudWebView == nil || hudWebView == contents->web_view;
    } @catch (NSException *) {
        return true;
    }
}

static void capturePdfProductionZoomSnapshot(WebContents *contents, NSString *action, NSString *phase, void (^completion)(void))
{
    if (!pdfProductionZoomTraceEnabled() || !contents || !contents->web_view) {
        if (completion)
            completion();
        return;
    }

    [contents->web_view layoutSubtreeIfNeeded];
    int tab_id = contents->tab_id;
    WKWebView *web_view = contents->web_view;
    NSString *action_copy = [action copy] ?: @"none";
    NSString *phase_copy = [phase copy] ?: @"unknown";
    WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
    configuration.rect = web_view.bounds;
    [web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
        WebContents *current = findContentsByTabId(tab_id);
        if (!current || current->web_view != web_view) {
            tracePdfProductionZoom(contents, phase_copy, action_copy, @"status=stale-webview");
            if (completion)
                completion();
            return;
        }
        NSString *metrics = pdfVisualMetricsForSnapshot(snapshotImage, error);
        tracePdfProductionZoom(current, phase_copy, action_copy, [NSString stringWithFormat:@"webview_bounds=%@ %@", NSStringFromRect(web_view.bounds), metrics]);
        if (completion)
            completion();
    }];
}

static bool invokePdfZoomSelector(WebContents *contents, NSString *action, NSView *hud, NSString *traceSource, int keycode, int modifiers)
{
    NSString *selectorName = pdfDirectCommandSelectorForAction(action);
    if (!selectorName.length) {
        NSString *detail = [NSString stringWithFormat:@"source=%@ result=refused reason=missing-webkit-command-path keycode=%d modifiers=%d allowed_selectors=performPDFZoomIn,performPDFZoomOut",
                                      traceSource ?: @"unknown",
                                      keycode,
                                      modifiers];
        tracePdfProductionZoom(contents, @"refuse", action, detail);
        tracePdfDirectCommand(contents, @"refuse", action, @"reason=missing-webkit-command-path allowed_selectors=performPDFZoomIn,performPDFZoomOut");
        return false;
    }

    SEL selector = NSSelectorFromString(selectorName);
    NSString *identity = pdfDirectCommandIdentitySummary(contents, hud, selectorName);
    NSString *baseDetail = [NSString stringWithFormat:@"source=%@ keycode=%d modifiers=%d %@",
                                      traceSource ?: @"unknown",
                                      keycode,
                                      modifiers,
                                      identity];
    tracePdfProductionZoom(contents, @"state-before", action, baseDetail);
    tracePdfDirectCommand(contents, @"state-before", action, identity);

    if (!contents || !contents->web_view || !currentUrlLooksPdf(contents)) {
        tracePdfProductionZoom(contents, @"skip", action, [NSString stringWithFormat:@"result=refused reason=non-pdf-url %@", baseDetail]);
        return false;
    }

    if (!hud) {
        tracePdfProductionZoom(contents, @"skip", action, [NSString stringWithFormat:@"result=missing-hud %@", baseDetail]);
        tracePdfDirectCommand(contents, @"skip", action, [NSString stringWithFormat:@"reason=missing-hud %@", identity]);
        return false;
    }

    bool matchKnown = false;
    bool match = pdfHudWebViewMatches(contents, hud, &matchKnown);
    if (matchKnown && !match) {
        tracePdfProductionZoom(contents, @"skip", action, [NSString stringWithFormat:@"result=refused reason=hud-webview-mismatch %@", baseDetail]);
        tracePdfDirectCommand(contents, @"skip", action, [NSString stringWithFormat:@"reason=hud-webview-mismatch %@", identity]);
        return false;
    }

    if (![hud respondsToSelector:selector]) {
        tracePdfProductionZoom(contents, @"skip", action, [NSString stringWithFormat:@"result=selector-unavailable %@", baseDetail]);
        tracePdfDirectCommand(contents, @"skip", action, [NSString stringWithFormat:@"reason=selector-unavailable %@", identity]);
        return false;
    }

    int tabId = contents->tab_id;
    WKWebView *webView = contents->web_view;
    NSString *actionCopy = [action copy];
    NSString *selectorCopy = [selectorName copy];
    void (^invoke)(void) = ^{
        WebContents *current = findContentsByTabId(tabId);
        if (!current || current->web_view != webView)
            return;
        NSString *invokeIdentity = pdfDirectCommandIdentitySummary(current, hud, selectorCopy);
        NSString *invokeDetail = [NSString stringWithFormat:@"source=%@ keycode=%d modifiers=%d result=invoked %@",
                                            traceSource ?: @"unknown",
                                            keycode,
                                            modifiers,
                                            invokeIdentity];
        tracePdfProductionZoom(current, @"invoke", actionCopy, invokeDetail);
        tracePdfDirectCommand(current, @"direct-command-invoke", actionCopy, invokeIdentity);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        [hud performSelector:selector];
#pragma clang diagnostic pop
    };

    if (pdfProductionZoomTraceEnabled()) {
        capturePdfProductionZoomSnapshot(contents, action, @"snapshot-before", ^{
            invoke();
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.85 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                WebContents *after = findContentsByTabId(tabId);
                if (!after || after->web_view != webView)
                    return;
                tracePdfProductionZoom(after, @"state-after", actionCopy, [NSString stringWithFormat:@"source=%@ keycode=%d modifiers=%d result=invoked %@",
                                                                        traceSource ?: @"unknown",
                                                                        keycode,
                                                                        modifiers,
                                                                        pdfDirectCommandIdentitySummary(after, hud, selectorCopy)]);
                tracePdfDirectCommand(after, @"state-after", actionCopy, pdfDirectCommandIdentitySummary(after, hud, selectorCopy));
                capturePdfProductionZoomSnapshot(after, actionCopy, @"snapshot-after", ^{
                });
            });
        });
        return true;
    }

    invoke();
    tracePdfProductionZoom(contents, @"state-after", action, [NSString stringWithFormat:@"source=%@ keycode=%d modifiers=%d result=invoked %@",
                                                          traceSource ?: @"unknown",
                                                          keycode,
                                                          modifiers,
                                                          pdfDirectCommandIdentitySummary(contents, hud, selectorName)]);
    tracePdfDirectCommand(contents, @"state-after", action, pdfDirectCommandIdentitySummary(contents, hud, selectorName));
    return true;
}

static bool performPdfDirectCommandDiagnostic(WebContents *contents, NSString *action, NSView *hud)
{
    if (!pdfDirectCommandSelectorForAction(action).length) {
        tracePdfDirectCommand(contents, @"refuse", action, @"reason=missing-webkit-command-path allowed_selectors=performPDFZoomIn,performPDFZoomOut");
        tracePdfVisualOracle(contents, @"direct-command-refuse", action, @"reason=missing-webkit-command-path");
        return false;
    }

    NSString *selectorName = pdfDirectCommandSelectorForAction(action);
    if (!hud || ![hud respondsToSelector:NSSelectorFromString(selectorName)]) {
        tracePdfDirectCommand(contents, @"skip", action, [NSString stringWithFormat:@"reason=selector-unavailable %@", pdfDirectCommandIdentitySummary(contents, hud, selectorName)]);
        tracePdfVisualOracle(contents, @"direct-command-skip", action, [NSString stringWithFormat:@"reason=selector-unavailable selector=%@", selectorName]);
        return false;
    }

    if (pdfVisualOracleProbeEnabled()) {
        int tabId = contents->tab_id;
        WKWebView *webView = contents->web_view;
        NSString *actionCopy = [action copy];
        NSString *selectorCopy = [selectorName copy];
        SEL selector = NSSelectorFromString(selectorName);
        NSString *identity = pdfDirectCommandIdentitySummary(contents, hud, selectorName);
        tracePdfDirectCommand(contents, @"state-before", action, identity);
        tracePdfVisualOracle(contents, @"precondition", action, [NSString stringWithFormat:@"direct_selector=%@ %@", selectorName, identity]);
        capturePdfVisualOracleSnapshot(contents, action, @"snapshot-before", ^{
            WebContents *current = findContentsByTabId(tabId);
            if (!current || current->web_view != webView)
                return;
            NSString *invokeIdentity = pdfDirectCommandIdentitySummary(current, hud, selectorCopy);
            tracePdfDirectCommand(current, @"direct-command-invoke", actionCopy, invokeIdentity);
            tracePdfVisualOracle(current, @"direct-command-invoke", actionCopy, invokeIdentity);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
            [hud performSelector:selector];
#pragma clang diagnostic pop
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.85 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                WebContents *after = findContentsByTabId(tabId);
                if (!after || after->web_view != webView)
                    return;
                tracePdfDirectCommand(after, @"state-after", actionCopy, pdfDirectCommandIdentitySummary(after, hud, selectorCopy));
                tracePdfAction(after, @"state-after", actionCopy, pdfActionStateSummary(after, hud));
                tracePdfStateOracle(after, @"state-after", actionCopy, pdfStateOracleSurfaceSummary(after, hud));
                capturePdfVisualOracleSnapshot(after, actionCopy, @"snapshot-after", ^{
                });
            });
        });
        return true;
    }

    return invokePdfZoomSelector(contents, action, hud, @"diagnostic-direct-command", 0, 0);
}

static bool performPdfPrintOperationDiagnostic(WebContents *contents, int keycode, int modifiers)
{
    if (!pdfPrintOperationProbeEnabled())
        return false;
    if (!contents || !contents->web_view)
        return false;

    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    NSString *baseDetail = [NSString stringWithFormat:@"keycode=%d modifiers=%d web_view=%@ tab=%d url=%@ run_invoked=false modal_run_invoked=false equivalent_run_invoked=false",
                                      keycode,
                                      modifiers,
                                      describeObject(contents->web_view),
                                      contents->tab_id,
                                      url];
    if (!currentUrlLooksPdf(contents)) {
        tracePdfPrintOperation(contents, @"refuse", [NSString stringWithFormat:@"reason=non-pdf-url %@", baseDetail]);
        return false;
    }

    SEL selector = NSSelectorFromString(@"_printOperationWithPrintInfo:");
    if (![contents->web_view respondsToSelector:selector]) {
        tracePdfPrintOperation(contents, @"skip", [NSString stringWithFormat:@"reason=selector-unavailable selector=_printOperationWithPrintInfo: responds=0 %@", baseDetail]);
        return true;
    }

    NSPrintInfo *printInfo = [NSPrintInfo.sharedPrintInfo copy];
    id operation = nil;
    @try {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        operation = [contents->web_view performSelector:selector withObject:printInfo];
#pragma clang diagnostic pop
    } @catch (NSException *exception) {
        tracePdfPrintOperation(contents, @"exception", [NSString stringWithFormat:@"selector=_printOperationWithPrintInfo: responds=1 exception=%@ reason=%@ %@", exception.name, exception.reason ?: @"", baseDetail]);
        return true;
    }

    if (!operation) {
        tracePdfPrintOperation(contents, @"created", [NSString stringWithFormat:@"selector=_printOperationWithPrintInfo: responds=1 operation_created=false %@", baseDetail]);
        return true;
    }

    if ([operation respondsToSelector:@selector(setShowsPrintPanel:)])
        [operation setShowsPrintPanel:NO];
    if ([operation respondsToSelector:@selector(setShowsProgressPanel:)])
        [operation setShowsProgressPanel:NO];

    NSMutableArray<NSString *> *items = [NSMutableArray array];
    [items addObject:@"selector=_printOperationWithPrintInfo:"];
    [items addObject:@"responds=1"];
    [items addObject:@"operation_created=true"];
    [items addObject:[NSString stringWithFormat:@"operation=%@", describeObject(operation)]];
    [items addObject:[NSString stringWithFormat:@"operation_class=%@", NSStringFromClass([operation class])]];
    [items addObject:[NSString stringWithFormat:@"showsPrintPanel=%d", [operation respondsToSelector:@selector(showsPrintPanel)] ? (int)[operation showsPrintPanel] : -1]];
    [items addObject:[NSString stringWithFormat:@"showsProgressPanel=%d", [operation respondsToSelector:@selector(showsProgressPanel)] ? (int)[operation showsProgressPanel] : -1]];
    [items addObject:@"run_invoked=false"];
    [items addObject:@"modal_run_invoked=false"];
    [items addObject:@"equivalent_run_invoked=false"];
    if ([operation respondsToSelector:@selector(jobTitle)])
        [items addObject:[NSString stringWithFormat:@"jobTitle=%@", [operation jobTitle] ?: @""]];
    if ([operation respondsToSelector:@selector(printInfo)]) {
        NSPrintInfo *info = [operation printInfo];
        [items addObject:[NSString stringWithFormat:@"paperSize=%@", NSStringFromSize(info.paperSize)]];
        [items addObject:[NSString stringWithFormat:@"orientation=%ld", (long)info.orientation]];
        [items addObject:[NSString stringWithFormat:@"scalingFactor=%.6f", info.scalingFactor]];
    }
    if ([operation respondsToSelector:@selector(view)])
        [items addObject:[NSString stringWithFormat:@"printView=%@", describeObject([operation view])]];
    tracePdfPrintOperation(contents, @"created", [NSString stringWithFormat:@"%@ %@", [items componentsJoinedByString:@" "], baseDetail]);
    return true;
}

static bool performPdfPrintDialogDiagnostic(WebContents *contents, int keycode, int modifiers)
{
    if (!pdfPrintDialogProbeEnabled())
        return false;
    if (!contents || !contents->web_view)
        return false;

    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    NSString *baseDetail = [NSString stringWithFormat:@"keycode=%d modifiers=%d web_view=%@ window=%@ tab=%d url=%@",
                                      keycode,
                                      modifiers,
                                      describeObject(contents->web_view),
                                      describeObject(contents->window),
                                      contents->tab_id,
                                      url];
    if (!currentUrlLooksPdf(contents)) {
        tracePdfPrintDialog(contents, @"refuse", [NSString stringWithFormat:@"reason=non-pdf-url run_invoked=false modal_run_invoked=false completion_result=not-started %@", baseDetail]);
        return false;
    }
    if (!contents->focused) {
        tracePdfPrintDialog(contents, @"refuse", [NSString stringWithFormat:@"reason=tab-not-focused run_invoked=false modal_run_invoked=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    SEL selector = NSSelectorFromString(@"_printOperationWithPrintInfo:");
    if (![contents->web_view respondsToSelector:selector]) {
        tracePdfPrintDialog(contents, @"skip", [NSString stringWithFormat:@"reason=selector-unavailable selector=_printOperationWithPrintInfo: responds=0 operation_created=false run_invoked=false modal_run_invoked=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    NSPrintInfo *printInfo = [NSPrintInfo.sharedPrintInfo copy];
    id operation = nil;
    @try {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        operation = [contents->web_view performSelector:selector withObject:printInfo];
#pragma clang diagnostic pop
    } @catch (NSException *exception) {
        tracePdfPrintDialog(contents, @"exception", [NSString stringWithFormat:@"selector=_printOperationWithPrintInfo: responds=1 exception=%@ reason=%@ operation_created=false run_invoked=false modal_run_invoked=false completion_result=not-started %@", exception.name, exception.reason ?: @"", baseDetail]);
        return true;
    }

    if (!operation) {
        tracePdfPrintDialog(contents, @"created", [NSString stringWithFormat:@"selector=_printOperationWithPrintInfo: responds=1 operation_created=false run_invoked=false modal_run_invoked=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    if ([operation respondsToSelector:@selector(setShowsPrintPanel:)])
        [operation setShowsPrintPanel:YES];
    if ([operation respondsToSelector:@selector(setShowsProgressPanel:)])
        [operation setShowsProgressPanel:NO];

    NSMutableArray<NSString *> *items = [NSMutableArray array];
    [items addObject:@"selector=_printOperationWithPrintInfo:"];
    [items addObject:@"responds=1"];
    [items addObject:@"operation_created=true"];
    [items addObject:[NSString stringWithFormat:@"operation=%@", describeObject(operation)]];
    [items addObject:[NSString stringWithFormat:@"operation_class=%@", NSStringFromClass([operation class])]];
    [items addObject:[NSString stringWithFormat:@"showsPrintPanel=%d", [operation respondsToSelector:@selector(showsPrintPanel)] ? (int)[operation showsPrintPanel] : -1]];
    [items addObject:[NSString stringWithFormat:@"showsProgressPanel=%d", [operation respondsToSelector:@selector(showsProgressPanel)] ? (int)[operation showsProgressPanel] : -1]];
    [items addObject:@"run_path=runOperation"];
    [items addObject:@"run_invoked=true"];
    [items addObject:@"modal_run_invoked=false"];
    if ([operation respondsToSelector:@selector(view)])
        [items addObject:[NSString stringWithFormat:@"printView=%@", describeObject([operation view])]];
    if ([operation respondsToSelector:@selector(printInfo)]) {
        NSPrintInfo *info = [operation printInfo];
        [items addObject:[NSString stringWithFormat:@"paperSize=%@", NSStringFromSize(info.paperSize)]];
        [items addObject:[NSString stringWithFormat:@"orientation=%ld", (long)info.orientation]];
        [items addObject:[NSString stringWithFormat:@"scalingFactor=%.6f", info.scalingFactor]];
    }

    [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    if (contents->window) {
        [contents->window makeKeyAndOrderFront:nil];
        [contents->window orderFrontRegardless];
    }
    [NSApp activateIgnoringOtherApps:YES];
    [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.5]];
    [items addObject:[NSString stringWithFormat:@"app_active=%d", NSApp.active ? 1 : 0]];
    [items addObject:[NSString stringWithFormat:@"window_visible=%d", contents->window && contents->window.visible ? 1 : 0]];
    [items addObject:[NSString stringWithFormat:@"window_key=%d", contents->window && contents->window.keyWindow ? 1 : 0]];

    tracePdfPrintDialog(contents, @"before-run", [NSString stringWithFormat:@"%@ completion_callback=false completion_result=not-started %@", [items componentsJoinedByString:@" "], baseDetail]);

    BOOL success = NO;
    @try {
        success = [operation runOperation];
    } @catch (NSException *exception) {
        tracePdfPrintDialog(contents, @"exception", [NSString stringWithFormat:@"%@ exception=%@ reason=%@ completion_callback=false completion_result=exception %@", [items componentsJoinedByString:@" "], exception.name, exception.reason ?: @"", baseDetail]);
        return true;
    }

    NSString *result = success ? @"submitted-or-success" : @"canceled";
    tracePdfPrintDialog(contents, @"after-run", [NSString stringWithFormat:@"%@ completion_callback=true completion_success=%d completion_result=%@ %@", [items componentsJoinedByString:@" "], success ? 1 : 0, result, baseDetail]);
    return true;
}

static bool performPdfPrintModalDiagnostic(WebContents *contents, int keycode, int modifiers)
{
    if (!pdfPrintModalProbeEnabled())
        return false;
    if (!contents || !contents->web_view)
        return false;

    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    NSString *baseDetail = [NSString stringWithFormat:@"keycode=%d modifiers=%d web_view=%@ window=%@ tab=%d url=%@",
                                      keycode,
                                      modifiers,
                                      describeObject(contents->web_view),
                                      describeObject(contents->window),
                                      contents->tab_id,
                                      url];

    NSString *readyPath = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PDF_PRINT_MODAL_WATCHER_READY_FILE"];
    BOOL watcherReady = readyPath.length && [NSFileManager.defaultManager fileExistsAtPath:readyPath];
    if (!watcherReady) {
        tracePdfPrintModal(contents, @"refuse", [NSString stringWithFormat:@"reason=watcher-not-ready watcher_ready=0 ready_file=%@ operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", readyPath ?: @"", baseDetail]);
        return true;
    }
    if (!currentUrlLooksPdf(contents)) {
        tracePdfPrintModal(contents, @"refuse", [NSString stringWithFormat:@"reason=non-pdf-url watcher_ready=1 operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", baseDetail]);
        return false;
    }
    if (!contents->focused) {
        tracePdfPrintModal(contents, @"refuse", [NSString stringWithFormat:@"reason=tab-not-focused watcher_ready=1 operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", baseDetail]);
        return true;
    }
    if (!contents->window) {
        tracePdfPrintModal(contents, @"refuse", [NSString stringWithFormat:@"reason=missing-window watcher_ready=1 operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    SEL selector = NSSelectorFromString(@"_printOperationWithPrintInfo:");
    if (![contents->web_view respondsToSelector:selector]) {
        tracePdfPrintModal(contents, @"skip", [NSString stringWithFormat:@"reason=selector-unavailable selector=_printOperationWithPrintInfo: responds=0 watcher_ready=1 operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    NSPrintInfo *printInfo = [NSPrintInfo.sharedPrintInfo copy];
    id operation = nil;
    @try {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        operation = [contents->web_view performSelector:selector withObject:printInfo];
#pragma clang diagnostic pop
    } @catch (NSException *exception) {
        tracePdfPrintModal(contents, @"exception", [NSString stringWithFormat:@"selector=_printOperationWithPrintInfo: responds=1 exception=%@ reason=%@ watcher_ready=1 operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", exception.name, exception.reason ?: @"", baseDetail]);
        return true;
    }

    if (!operation) {
        tracePdfPrintModal(contents, @"created", [NSString stringWithFormat:@"selector=_printOperationWithPrintInfo: responds=1 watcher_ready=1 operation_created=false run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    SEL modalSelector = @selector(runOperationModalForWindow:delegate:didRunSelector:contextInfo:);
    if (![operation respondsToSelector:modalSelector]) {
        tracePdfPrintModal(contents, @"skip", [NSString stringWithFormat:@"reason=modal-selector-unavailable selector=_printOperationWithPrintInfo: responds=1 watcher_ready=1 operation_created=true run_invoked=false modal_run_invoked=false completion_callback=false completion_result=not-started %@", baseDetail]);
        return true;
    }

    if ([operation respondsToSelector:@selector(setShowsPrintPanel:)])
        [operation setShowsPrintPanel:YES];
    if ([operation respondsToSelector:@selector(setShowsProgressPanel:)])
        [operation setShowsProgressPanel:NO];

    tracePrintPresentation(contents, @"before-activation", @"none");
    NSMutableArray<NSString *> *presentationAttempts = [NSMutableArray array];
    NSString *activationMode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_ACTIVATION_MODE"];
    if ([activationMode isEqualToString:@"regular"]) {
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        [presentationAttempts addObject:@"setActivationPolicyRegular"];
    }
    if (contents->window) {
        NSString *windowMode = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_PRINT_PRESENTATION_WINDOW_MODE"];
        if ([windowMode containsString:@"make-key"]) {
            [contents->window makeKeyAndOrderFront:nil];
            [presentationAttempts addObject:@"makeKeyAndOrderFront"];
        }
        if ([windowMode containsString:@"order-front"]) {
            [contents->window orderFrontRegardless];
            [presentationAttempts addObject:@"orderFrontRegardless"];
        }
    }
    if ([activationMode isEqualToString:@"regular"]) {
        [NSApp activateIgnoringOtherApps:YES];
        [presentationAttempts addObject:@"activateIgnoringOtherApps"];
    }
    if (presentationAttempts.count)
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.2]];
    NSString *presentationAttemptSummary = presentationAttempts.count ? [presentationAttempts componentsJoinedByString:@","] : @"none";
    tracePrintPresentation(contents, @"after-activation", presentationAttemptSummary);

    TSPdfPrintModalProbeDelegate *delegate = [[TSPdfPrintModalProbeDelegate alloc] init];
    delegate.contents = contents;
    delegate.identity = [NSString stringWithFormat:@"modal-delegate-%@-tab-%d", NSUUID.UUID.UUIDString, contents->tab_id];
    [pdfPrintModalProbeDelegates() addObject:delegate];

    NSMutableArray<NSString *> *items = [NSMutableArray array];
    [items addObject:@"selector=_printOperationWithPrintInfo:"];
    [items addObject:@"responds=1"];
    [items addObject:@"watcher_ready=1"];
    [items addObject:[NSString stringWithFormat:@"ready_file=%@", readyPath ?: @""]];
    [items addObject:@"operation_created=true"];
    [items addObject:[NSString stringWithFormat:@"operation=%@", describeObject(operation)]];
    [items addObject:[NSString stringWithFormat:@"operation_class=%@", NSStringFromClass([operation class])]];
    [items addObject:[NSString stringWithFormat:@"showsPrintPanel=%d", [operation respondsToSelector:@selector(showsPrintPanel)] ? (int)[operation showsPrintPanel] : -1]];
    [items addObject:[NSString stringWithFormat:@"showsProgressPanel=%d", [operation respondsToSelector:@selector(showsProgressPanel)] ? (int)[operation showsProgressPanel] : -1]];
    [items addObject:@"run_path=runOperationModalForWindow"];
    [items addObject:@"run_invoked=false"];
    [items addObject:@"modal_run_invoked=true"];
    [items addObject:[NSString stringWithFormat:@"delegate=%@", delegate.identity]];
    [items addObject:[NSString stringWithFormat:@"delegate_retained=%d", [pdfPrintModalProbeDelegates() containsObject:delegate] ? 1 : 0]];
    [items addObject:[NSString stringWithFormat:@"presentation_attempts=%@", presentationAttemptSummary]];
    [items addObject:[NSString stringWithFormat:@"window=%@", describeObject(contents->window)]];
    [items addObject:[NSString stringWithFormat:@"app_active=%d", NSApp.active ? 1 : 0]];
    [items addObject:[NSString stringWithFormat:@"window_visible=%d", contents->window.visible ? 1 : 0]];
    [items addObject:[NSString stringWithFormat:@"window_key=%d", contents->window.keyWindow ? 1 : 0]];
    if ([operation respondsToSelector:@selector(view)])
        [items addObject:[NSString stringWithFormat:@"printView=%@", describeObject([operation view])]];
    if ([operation respondsToSelector:@selector(printInfo)]) {
        NSPrintInfo *info = [operation printInfo];
        [items addObject:[NSString stringWithFormat:@"paperSize=%@", NSStringFromSize(info.paperSize)]];
        [items addObject:[NSString stringWithFormat:@"orientation=%ld", (long)info.orientation]];
        [items addObject:[NSString stringWithFormat:@"scalingFactor=%.6f", info.scalingFactor]];
    }

    tracePdfPrintModal(contents, @"before-modal-run", [NSString stringWithFormat:@"%@ completion_callback=false completion_result=not-started %@", [items componentsJoinedByString:@" "], baseDetail]);
    tracePrintPresentation(contents, @"before-modal-run", presentationAttemptSummary);
    @try {
        [operation runOperationModalForWindow:contents->window
                                     delegate:delegate
                               didRunSelector:@selector(printOperationDidRun:success:contextInfo:)
                                  contextInfo:contents];
    } @catch (NSException *exception) {
        [pdfPrintModalProbeDelegates() removeObject:delegate];
        tracePdfPrintModal(contents, @"exception", [NSString stringWithFormat:@"%@ exception=%@ reason=%@ delegate_release=true completion_callback=false completion_result=exception %@", [items componentsJoinedByString:@" "], exception.name, exception.reason ?: @"", baseDetail]);
        return true;
    }

    tracePdfPrintModal(contents, @"after-modal-invoke", [NSString stringWithFormat:@"%@ completion_callback=false completion_result=pending %@", [items componentsJoinedByString:@" "], baseDetail]);
    tracePrintPresentation(contents, @"after-modal-invoke", presentationAttemptSummary);
    return true;
}

static bool performPdfActionDiagnostic(WebContents *contents, NSString *action)
{
    if (!pdfActionProbeEnabled() || !contents || !contents->web_view)
        return false;
    if (!currentUrlLooksPdf(contents)) {
        tracePdfAction(contents, @"skip", action, @"reason=non-pdf-url");
        return false;
    }

    NSView *hud = findDescendantViewWithClassName(contents->web_view, @"WKPDFHUDView");
    tracePdfAction(contents, @"state-before", action, pdfActionStateSummary(contents, hud));
    tracePdfStateOracle(contents, @"state-before", action, pdfStateOracleSurfaceSummary(contents, hud));
    if (!hud) {
        tracePdfAction(contents, @"skip", action, @"reason=missing-hud");
        tracePdfStateOracle(contents, @"skip", action, @"reason=missing-hud");
        return false;
    }

    if (pdfDirectCommandProbeEnabled())
        return performPdfDirectCommandDiagnostic(contents, action, hud);

    NSArray<NSString *> *controls = allowedPdfActionControls(action);
    if (!controls.count) {
        tracePdfAction(contents, @"reject", action, @"reason=action-not-allowlisted");
        return false;
    }

    SEL selector = NSSelectorFromString(@"_performActionForControl:");
    if (![hud respondsToSelector:selector]) {
        tracePdfAction(contents, @"skip", action, [NSString stringWithFormat:@"reason=selector-unavailable hud_class=%@", NSStringFromClass(hud.class)]);
        tracePdfStateOracle(contents, @"skip", action, [NSString stringWithFormat:@"reason=selector-unavailable hud_class=%@", NSStringFromClass(hud.class)]);
        return false;
    }

    if (pdfVisualOracleProbeEnabled()) {
        NSArray<NSString *> *controlsCopy = [controls copy];
        int tabId = contents->tab_id;
        WKWebView *webView = contents->web_view;
        tracePdfVisualOracle(contents, @"precondition", action, [NSString stringWithFormat:@"controls=%@ webview_bounds=%@",
            [controlsCopy componentsJoinedByString:@","],
            NSStringFromRect(contents->web_view.bounds)]);
        capturePdfVisualOracleSnapshot(contents, action, @"snapshot-before", ^{
            WebContents *current = findContentsByTabId(tabId);
            if (!current || current->web_view != webView)
                return;
            bool attempted = false;
            for (NSString *control in controlsCopy) {
                attempted = true;
                tracePdfAction(current, @"private-hook-invoke", action, [NSString stringWithFormat:@"hud_class=%@ control=%@", NSStringFromClass(hud.class), control]);
                tracePdfVisualOracle(current, @"private-hook-invoke", action, [NSString stringWithFormat:@"hud_class=%@ control=%@", NSStringFromClass(hud.class), control]);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
                [hud performSelector:selector withObject:control];
#pragma clang diagnostic pop
            }
            tracePdfAction(current, attempted ? @"private-hook-result" : @"private-hook-noop", action, [NSString stringWithFormat:@"attempted=%d controls=%@", attempted ? 1 : 0, [controlsCopy componentsJoinedByString:@","]]);
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.65 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                WebContents *after = findContentsByTabId(tabId);
                if (!after || after->web_view != webView)
                    return;
                tracePdfAction(after, @"state-after", action, pdfActionStateSummary(after, hud));
                tracePdfStateOracle(after, @"state-after", action, pdfStateOracleSurfaceSummary(after, hud));
                capturePdfVisualOracleSnapshot(after, action, @"snapshot-after", ^{
                });
            });
        });
        return true;
    }

    bool attempted = false;
    for (NSString *control in controls) {
        attempted = true;
        tracePdfAction(contents, @"private-hook-invoke", action, [NSString stringWithFormat:@"hud_class=%@ control=%@", NSStringFromClass(hud.class), control]);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
        [hud performSelector:selector withObject:control];
#pragma clang diagnostic pop
    }
    tracePdfAction(contents, attempted ? @"private-hook-result" : @"private-hook-noop", action, [NSString stringWithFormat:@"attempted=%d controls=%@", attempted ? 1 : 0, [controls componentsJoinedByString:@","]]);
    tracePdfAction(contents, @"state-after", action, pdfActionStateSummary(contents, hud));
    tracePdfStateOracle(contents, @"state-after", action, pdfStateOracleSurfaceSummary(contents, hud));
    return attempted;
}

static NSString *pdfDiagnosticActionForKeyEvent(int keycode, int modifiers)
{
    if ((modifiers & 8) == 0)
        return nil;
    if (keycode == 39 || keycode == 124 || keycode == 30)
        return @"page-next";
    if (keycode == 37 || keycode == 123 || keycode == 33)
        return @"page-previous";
    if (keycode == 187 || keycode == 24)
        return @"zoom-in";
    if (keycode == 55 || keycode == 189 || keycode == 27)
        return @"zoom-out";
    if (keycode == 57 || keycode == 25)
        return @"fit-width";
    if (keycode == 56 || keycode == 48 || keycode == 29)
        return @"fit-page";
    if (keycode == 82 || keycode == 15)
        return @"rotate";
    return nil;
}

static NSString *pdfProductionZoomActionForKeyEvent(int keycode, int modifiers)
{
    if ((modifiers & 8) == 0)
        return nil;
    if (keycode == 24 || keycode == 69 || keycode == 187)
        return @"zoom-in";
    if (keycode == 27 || keycode == 78 || keycode == 189)
        return @"zoom-out";
    return nil;
}

static bool performPdfProductionZoomForKeyEvent(WebContents *contents, int keycode, int modifiers)
{
    NSString *action = pdfProductionZoomActionForKeyEvent(keycode, modifiers);
    if (!action.length)
        return false;
    if (!contents || !contents->web_view || !currentUrlLooksPdf(contents)) {
        tracePdfProductionZoom(contents, @"skip", action, [NSString stringWithFormat:@"source=production-key result=refused reason=non-pdf-url keycode=%d modifiers=%d", keycode, modifiers]);
        return false;
    }

    NSView *hud = findDescendantViewWithClassName(contents->web_view, @"WKPDFHUDView");
    return invokePdfZoomSelector(contents, action, hud, @"production-key", keycode, modifiers);
}

static NSString *describeScrollViews(NSView *view)
{
    if (!view)
        return @"";
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    if ([view isKindOfClass:NSScrollView.class]) {
        NSScrollView *scroll = (NSScrollView *)view;
        NSClipView *clip = scroll.contentView;
        [items addObject:[NSString stringWithFormat:@"%@:%p frame=%@ bounds=%@ document=%@ document_frame=%@ document_bounds=%@ clip_bounds=%@",
                                   NSStringFromClass([scroll class]),
                                   scroll,
                                   NSStringFromRect(scroll.frame),
                                   NSStringFromRect(scroll.bounds),
                                   describeObject(scroll.documentView),
                                   NSStringFromRect(scroll.documentView.frame),
                                   NSStringFromRect(scroll.documentView.bounds),
                                   NSStringFromRect(clip.bounds)]];
    }
    for (NSView *subview in view.subviews) {
        NSString *child = describeScrollViews(subview);
        if (child.length)
            [items addObject:child];
    }
    return [items componentsJoinedByString:@" | "];
}

static NSString *describePointInViewChain(NSView *view, NSPoint windowPoint)
{
    NSMutableArray<NSString *> *items = [NSMutableArray array];
    NSView *current = view;
    for (int i = 0; current && i < 12; i++) {
        NSPoint local = [current convertPoint:windowPoint fromView:nil];
        [items addObject:[NSString stringWithFormat:@"%@:%p point=%@ frame=%@ bounds=%@",
                                   NSStringFromClass([current class]),
                                   current,
                                   NSStringFromPoint(local),
                                   NSStringFromRect(current.frame),
                                   NSStringFromRect(current.bounds)]];
        current = current.superview;
    }
    return [items componentsJoinedByString:@">"];
}

static void tracePdfViewGeometry(WebContents *contents, NSString *label, int x, int y, NSPoint windowPoint)
{
    if (!pdfViewGeometryTraceEnabled() || !contents || !contents->web_view)
        return;

    NSView *hit = [contents->web_view hitTest:windowPoint] ?: contents->web_view;
    NSPoint webPoint = [contents->web_view convertPoint:windowPoint fromView:nil];
    NSWindow *window = contents->window;
    NSScreen *screen = window.screen ?: NSScreen.mainScreen;
    SEL copySelector = @selector(copy:);
    id targetFromNil = [NSApp targetForAction:copySelector to:nil from:nil];
    id targetFromWebView = [NSApp targetForAction:copySelector to:nil from:contents->web_view];
    NSResponder *firstResponder = window.firstResponder;
    appendPdfViewGeometryTrace([NSString stringWithFormat:
        @"webkit-pdf-view-geometry-state tab=%d label=%@ url=%@ input=%d,%d window_point=%@ web_point=%@ hit=%@ window=%@ window_frame=%@ key_window=%d main_window=%d app_key_window=%@ app_main_window=%@ backing_scale=%.3f web_view=%@ web_frame=%@ web_bounds=%@ first_responder=%@ responder_chain=%@ target_nil=%@ target_webview=%@ clipboard={%@}",
        contents->tab_id,
        label,
        contents->web_view.URL.absoluteString ?: @"",
        x,
        y,
        NSStringFromPoint(windowPoint),
        NSStringFromPoint(webPoint),
        describeObject(hit),
        describeObject(window),
        NSStringFromRect(window.frame),
        window.isKeyWindow ? 1 : 0,
        window.isMainWindow ? 1 : 0,
        describeObject(NSApp.keyWindow),
        describeObject(NSApp.mainWindow),
        screen.backingScaleFactor ?: 1.0,
        describeObject(contents->web_view),
        NSStringFromRect(contents->web_view.frame),
        NSStringFromRect(contents->web_view.bounds),
        describeObject(firstResponder),
        responderChain(firstResponder),
        describeObject(targetFromNil),
        describeObject(targetFromWebView),
        clipboardSample()]);
    appendPdfViewGeometryTrace([NSString stringWithFormat:@"webkit-pdf-view-geometry-hit-chain tab=%d label=%@ chain=%@", contents->tab_id, label, describePointInViewChain(hit, windowPoint)]);
    appendPdfViewGeometryTrace([NSString stringWithFormat:@"webkit-pdf-view-geometry-tree tab=%d label=%@ tree=%@", contents->tab_id, label, describeViewTree(contents->web_view, 0)]);
    appendPdfViewGeometryTrace([NSString stringWithFormat:@"webkit-pdf-view-geometry-scroll tab=%d label=%@ scroll=%@", contents->tab_id, label, describeScrollViews(contents->web_view)]);
}

static void applyPdfResponderProbe(WebContents *contents, NSString *phase)
{
    NSString *mode = pdfResponderProbeMode();
    if (!mode.length || [mode isEqualToString:@"baseline"] || !contents || !contents->web_view)
        return;

    NSWindow *window = contents->window;
    BOOL beforeKey = window.isKeyWindow;
    BOOL beforeMain = window.isMainWindow;
    id beforeTargetNil = [NSApp targetForAction:@selector(copy:) to:nil from:nil];
    id beforeTargetWebView = [NSApp targetForAction:@selector(copy:) to:nil from:contents->web_view];

    if ([mode isEqualToString:@"activate-app"]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
        [NSApp activateIgnoringOtherApps:YES];
#pragma clang diagnostic pop
    } else if ([mode isEqualToString:@"key-window"]) {
        [window makeKeyWindow];
    } else if ([mode isEqualToString:@"main-window"]) {
        [window makeMainWindow];
    } else if ([mode isEqualToString:@"key-main-window"]) {
        [window makeKeyAndOrderFront:nil];
        [window makeMainWindow];
    } else if ([mode isEqualToString:@"explicit-first-responder"]) {
        [window makeFirstResponder:contents->web_view];
    }

    id afterTargetNil = [NSApp targetForAction:@selector(copy:) to:nil from:nil];
    id afterTargetWebView = [NSApp targetForAction:@selector(copy:) to:nil from:contents->web_view];
    appendPdfViewGeometryTrace([NSString stringWithFormat:
        @"webkit-pdf-responder-probe tab=%d phase=%@ mode=%@ before_key=%d before_main=%d after_key=%d after_main=%d app_key_window=%@ app_main_window=%@ before_target_nil=%@ before_target_webview=%@ after_target_nil=%@ after_target_webview=%@ first_responder=%@ responder_chain=%@",
        contents->tab_id,
        phase ?: @"unknown",
        mode,
        beforeKey ? 1 : 0,
        beforeMain ? 1 : 0,
        window.isKeyWindow ? 1 : 0,
        window.isMainWindow ? 1 : 0,
        describeObject(NSApp.keyWindow),
        describeObject(NSApp.mainWindow),
        describeObject(beforeTargetNil),
        describeObject(beforeTargetWebView),
        describeObject(afterTargetNil),
        describeObject(afterTargetWebView),
        describeObject(window.firstResponder),
        responderChain(window.firstResponder)]);
}

static void traceJavaScriptSelection(WebContents *contents, NSString *label)
{
    if (!pdfCopyTraceEnabled() || !contents || !contents->web_view)
        return;
    NSString *script = @"(() => { const s = window.getSelection ? String(window.getSelection()) : ''; return JSON.stringify({ length: s.length, sample: s.slice(0, 120), activeElement: document.activeElement ? document.activeElement.tagName : '', hasFocus: document.hasFocus ? document.hasFocus() : false }); })()";
    int tab_id = contents->tab_id;
    [contents->web_view evaluateJavaScript:script completionHandler:^(id result, NSError *error) {
        NSString *resultString = result ? [NSString stringWithFormat:@"%@", result] : @"";
        NSString *errorString = error ? error.localizedDescription : @"";
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-js tab=%d label=%@ result=%@ error=%@", tab_id, label, resultString, errorString]);
    }];
}

static void traceCopyState(WebContents *contents, NSString *label)
{
    if (!pdfCopyTraceEnabled() || !contents)
        return;
    SEL copySelector = @selector(copy:);
    id targetFromNil = [NSApp targetForAction:copySelector to:nil from:nil];
    id targetFromWebView = [NSApp targetForAction:copySelector to:nil from:contents->web_view];
    NSView *hitTarget = [contents->web_view hitTest:NSMakePoint(0, contents->web_view.bounds.size.height)] ?: contents->web_view;
    id targetFromHit = [NSApp targetForAction:copySelector to:nil from:hitTarget];
    NSResponder *firstResponder = contents->window.firstResponder;
    appendPdfCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-copy-state tab=%d label=%@ url=%@ focused=%d gui_active=%d window=%@ key_window=%d main_window=%d app_key_window=%@ web_view=%@ web_frame=%@ first_responder=%@ responder_chain=%@ hit_target=%@ target_nil=%@ target_webview=%@ target_hit=%@ validate_nil={%@} validate_webview={%@} validate_hit={%@} clipboard={%@}",
        contents->tab_id,
        label,
        contents->web_view.URL.absoluteString ?: @"",
        contents->focused ? 1 : 0,
        contents->gui_active ? 1 : 0,
        describeObject(contents->window),
        contents->window.isKeyWindow ? 1 : 0,
        contents->window.isMainWindow ? 1 : 0,
        describeObject(NSApp.keyWindow),
        describeObject(contents->web_view),
        NSStringFromRect(contents->web_view.frame),
        describeObject(firstResponder),
        responderChain(firstResponder),
        describeObject(hitTarget),
        describeObject(targetFromNil),
        describeObject(targetFromWebView),
        describeObject(targetFromHit),
        copyTargetValidation(targetFromNil, copySelector),
        copyTargetValidation(targetFromWebView, copySelector),
        copyTargetValidation(targetFromHit, copySelector),
        clipboardSample()]);
    traceJavaScriptSelection(contents, label);
}

struct PdfCopyBridgeState {
    bool active = false;
    bool original_allows_key_main = false;
    NSWindow *original_key_window = nil;
    NSWindow *original_main_window = nil;
    NSResponder *original_first_responder = nil;
};

static void tracePdfCopyBridgeState(WebContents *contents, NSString *phase, NSString *mode, PdfCopyBridgeState *state)
{
    if (!pdfCopyTraceEnabled() || !contents)
        return;
    appendPdfCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-copy-bridge-state tab=%d phase=%@ mode=%@ active=%d original_allow_key_main=%d current_allow_key_main=%d original_key_window=%@ original_main_window=%@ original_first_responder=%@ current_key_window=%@ current_main_window=%@ host_key=%d host_main=%d first_responder=%@ responder_chain=%@ clipboard={%@}",
        contents->tab_id,
        phase ?: @"unknown",
        mode ?: @"none",
        state && state->active ? 1 : 0,
        state ? (state->original_allows_key_main ? 1 : 0) : 0,
        g_pdf_copy_bridge_allows_key_main ? 1 : 0,
        state ? describeObject(state->original_key_window) : @"nil",
        state ? describeObject(state->original_main_window) : @"nil",
        state ? describeObject(state->original_first_responder) : @"nil",
        describeObject(NSApp.keyWindow),
        describeObject(NSApp.mainWindow),
        contents->window.isKeyWindow ? 1 : 0,
        contents->window.isMainWindow ? 1 : 0,
        describeObject(contents->window.firstResponder),
        responderChain(contents->window.firstResponder),
        clipboardSample()]);
}

static PdfCopyBridgeState applyPdfCopyBridge(WebContents *contents, NSString *mode)
{
    PdfCopyBridgeState state;
    if (!pdfCopyBridgeEnabled() || !contents || !contents->web_view || !mode.length || [mode isEqualToString:@"baseline"])
        return state;

    state.active = true;
    state.original_allows_key_main = g_pdf_copy_bridge_allows_key_main;
    state.original_key_window = NSApp.keyWindow;
    state.original_main_window = NSApp.mainWindow;
    state.original_first_responder = contents->window.firstResponder;

    tracePdfCopyBridgeState(contents, @"before-setup", mode, &state);

    g_pdf_copy_bridge_allows_key_main = true;
    [contents->window makeKeyAndOrderFront:nil];
    [contents->window makeMainWindow];
    [contents->window makeFirstResponder:contents->web_view];

    tracePdfCopyBridgeState(contents, @"after-setup", mode, &state);
    traceCopyState(contents, @"copy-bridge-after-setup");
    return state;
}

static void restorePdfCopyBridge(WebContents *contents, NSString *mode, PdfCopyBridgeState *state)
{
    if (!state || !state->active || !contents)
        return;

    if (state->original_first_responder)
        [contents->window makeFirstResponder:state->original_first_responder];
    else
        [contents->window makeFirstResponder:nil];

    g_pdf_copy_bridge_allows_key_main = state->original_allows_key_main;

    if (state->original_main_window)
        [state->original_main_window makeMainWindow];
    if (state->original_key_window)
        [state->original_key_window makeKeyWindow];

    tracePdfCopyBridgeState(contents, @"after-restore", mode, state);
    traceCopyState(contents, @"copy-bridge-after-restore");
}

static void runPdfCopyBridgePreKeyRoutes(WebContents *contents, NSString *mode, NSEvent *event)
{
    if (!pdfCopyBridgeEnabled() || !contents || !mode.length)
        return;

    SEL copySelector = @selector(copy:);
    if ([mode isEqualToString:@"send-action-first"]) {
        traceCopyState(contents, @"copy-bridge-before-send-action-first");
        BOOL ok_nil = [NSApp sendAction:copySelector to:nil from:nil];
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-bridge-route tab=%d mode=%@ route=sendActionNil ok=%d clipboard={%@}", contents->tab_id, mode, ok_nil ? 1 : 0, clipboardSample()]);
        BOOL ok_webview = [NSApp sendAction:copySelector to:contents->web_view from:nil];
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-bridge-route tab=%d mode=%@ route=sendActionWebView ok=%d clipboard={%@}", contents->tab_id, mode, ok_webview ? 1 : 0, clipboardSample()]);
        traceCopyState(contents, @"copy-bridge-after-send-action-first");
    } else if ([mode isEqualToString:@"window-perform-key-equivalent"]) {
        traceCopyState(contents, @"copy-bridge-before-key-equivalent");
        BOOL ok_window = [contents->window performKeyEquivalent:event];
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-bridge-route tab=%d mode=%@ route=windowPerformKeyEquivalent ok=%d clipboard={%@}", contents->tab_id, mode, ok_window ? 1 : 0, clipboardSample()]);
        BOOL ok_webview = [contents->web_view performKeyEquivalent:event];
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-bridge-route tab=%d mode=%@ route=webViewPerformKeyEquivalent ok=%d clipboard={%@}", contents->tab_id, mode, ok_webview ? 1 : 0, clipboardSample()]);
        traceCopyState(contents, @"copy-bridge-after-key-equivalent");
    }
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

static bool isRegisteredContents(WebContents *contents)
{
    return std::find(g_web_contents.begin(), g_web_contents.end(), contents) != g_web_contents.end();
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

static unsigned short macKeyCodeForWindowsKeyCode(int keycode)
{
    switch (keycode) {
    case 0x08: return 0x33; // Backspace
    case 0x09: return 0x30; // Tab
    case 0x0D: return 0x24; // Enter
    case 0x1B: return 0x35; // Escape
    case 0x20: return 0x31; // Space
    case 0x21: return 0x74; // Page Up
    case 0x22: return 0x79; // Page Down
    case 0x23: return 0x77; // End
    case 0x24: return 0x73; // Home
    case 0x25: return 0x7B; // Left
    case 0x26: return 0x7E; // Up
    case 0x27: return 0x7C; // Right
    case 0x28: return 0x7D; // Down
    case 0x2D: return 0x72; // Insert
    case 0x2E: return 0x75; // Delete
    case 0x30: return 0x1D;
    case 0x31: return 0x12;
    case 0x32: return 0x13;
    case 0x33: return 0x14;
    case 0x34: return 0x15;
    case 0x35: return 0x17;
    case 0x36: return 0x16;
    case 0x37: return 0x1A;
    case 0x38: return 0x1C;
    case 0x39: return 0x19;
    case 0x41: return 0x00;
    case 0x42: return 0x0B;
    case 0x43: return 0x08;
    case 0x44: return 0x02;
    case 0x45: return 0x0E;
    case 0x46: return 0x03;
    case 0x47: return 0x05;
    case 0x48: return 0x04;
    case 0x49: return 0x22;
    case 0x4A: return 0x26;
    case 0x4B: return 0x28;
    case 0x4C: return 0x25;
    case 0x4D: return 0x2E;
    case 0x4E: return 0x2D;
    case 0x4F: return 0x1F;
    case 0x50: return 0x23;
    case 0x51: return 0x0C;
    case 0x52: return 0x0F;
    case 0x53: return 0x01;
    case 0x54: return 0x11;
    case 0x55: return 0x20;
    case 0x56: return 0x09;
    case 0x57: return 0x0D;
    case 0x58: return 0x07;
    case 0x59: return 0x10;
    case 0x5A: return 0x06;
    case 0x70: return 0x7A;
    case 0x71: return 0x78;
    case 0x72: return 0x63;
    case 0x73: return 0x76;
    case 0x74: return 0x60;
    case 0x75: return 0x61;
    case 0x76: return 0x62;
    case 0x77: return 0x64;
    case 0x78: return 0x65;
    case 0x79: return 0x6D;
    case 0x7A: return 0x67;
    case 0x7B: return 0x6F;
    case 0xBA: return 0x29; // Semicolon
    case 0xBB: return 0x18; // Equal
    case 0xBC: return 0x2B; // Comma
    case 0xBD: return 0x1B; // Minus
    case 0xBE: return 0x2F; // Period
    case 0xBF: return 0x2C; // Slash
    case 0xC0: return 0x32; // Backquote
    case 0xDB: return 0x21; // Left bracket
    case 0xDC: return 0x2A; // Backslash
    case 0xDD: return 0x1E; // Right bracket
    case 0xDE: return 0x27; // Quote
    default:
        return (unsigned short)keycode;
    }
}

static bool isPrintableTextInput(NSString *characters)
{
    if (!characters.length)
        return false;
    NSCharacterSet *controlSet = NSCharacterSet.controlCharacterSet;
    for (NSUInteger i = 0; i < characters.length; i++) {
        unichar ch = [characters characterAtIndex:i];
        if ([controlSet characterIsMember:ch])
            return false;
    }
    return true;
}

static NSPoint eventLocationInWindow(WebContents *contents, int x, int y)
{
    // Smoke and input use CSS/document top-left coordinates. AppKit views are
    // bottom-left unless flipped; convert so hit-testing matches elementFromPoint.
    CGFloat localY = y;
    if (contents->web_view && !contents->web_view.isFlipped)
        localY = contents->web_view.bounds.size.height - y;
    NSPoint localPoint = NSMakePoint(x, localY);
    return [contents->web_view convertPoint:localPoint toView:nil];
}

static CGPoint eventLocationInGlobalScreen(WebContents *contents, int x, int y)
{
    NSPoint windowPoint = eventLocationInWindow(contents, x, y);
    NSPoint screenPoint = [contents->window convertPointToScreen:windowPoint];
    CGFloat screenHeight = NSScreen.screens.firstObject.frame.size.height;
    return CGPointMake(screenPoint.x, screenHeight - screenPoint.y);
}

static NSPoint adjustedPdfSelectionLocation(WebContents *contents, int x, int y, bool dragging)
{
    NSPoint location = eventLocationInWindow(contents, x, y);
    NSString *mode = pdfSelectionEdgeProbeMode();
    if (!dragging || ![mode isEqualToString:@"delta"])
        return location;

    location.x += pdfSelectionEdgeDeltaX();
    return location;
}

static void tracePdfSelectionGeometryRemediation(WebContents *contents, NSString *phase, NSString *mode, NSString *reason, NSPoint originalLocal, NSPoint adjustedLocal, NSPoint originalWindow, NSPoint adjustedWindow, bool appliedGeometry, bool appliedFocus)
{
    if (!pdfSelectionGeometryRemediationEnabled() || !pdfViewGeometryTraceEnabled() || !contents || !contents->web_view)
        return;

    appendPdfViewGeometryTrace([NSString stringWithFormat:
        @"webkit-pdf-selection-remediation tab=%d phase=%@ gate=1 mode=%@ reason=%@ url=%@ pdf_url=%d original_local=%@ adjusted_local=%@ original_window=%@ adjusted_window=%@ target_y=%.2f edge_avoidance=%d focus_action=%d geometry_action=%d first_responder=%@ responder_chain=%@",
        contents->tab_id,
        phase ?: @"unknown",
        mode ?: @"none",
        reason ?: @"unknown",
        contents->web_view.URL.absoluteString ?: @"",
        currentUrlLooksPdf(contents) ? 1 : 0,
        NSStringFromPoint(originalLocal),
        NSStringFromPoint(adjustedLocal),
        NSStringFromPoint(originalWindow),
        NSStringFromPoint(adjustedWindow),
        adjustedLocal.y,
        appliedGeometry ? 1 : 0,
        appliedFocus ? 1 : 0,
        appliedGeometry ? 1 : 0,
        describeObject(contents->window.firstResponder),
        responderChain(contents->window.firstResponder)]);
}

static void tracePdfSelectionStateTransition(WebContents *contents, NSString *phase, NSString *mode, NSString *reason, NSPoint userLocal, NSPoint primeLocal, bool focusAction, bool pointerAction, bool clampAction)
{
    if ((!pdfSelectionStateTransitionEnabled() && !pdfPointerPrimeProductionEnabled()) || !pdfViewGeometryTraceEnabled() || !contents || !contents->web_view)
        return;

    NSString *source = pdfSelectionStateTransitionEnabled() ? @"experiment" : @"production";
    appendPdfViewGeometryTrace([NSString stringWithFormat:
        @"webkit-pdf-selection-state-transition tab=%d phase=%@ source=%@ gate=1 mode=%@ reason=%@ url=%@ generation=%llu consumed=%d pending=%d pdf_url=%d user_local=%@ prime_local=%@ focus_action=%d pointer_action=%d clamp_action=%d coordinate_changed=0 first_responder=%@ responder_chain=%@",
        contents->tab_id,
        phase ?: @"unknown",
        source,
        mode ?: @"none",
        reason ?: @"unknown",
        contents->web_view.URL.absoluteString ?: @"",
        (unsigned long long)contents->pdf_selected_text_generation,
        contents->pdf_selection_state_transition_consumed ? 1 : 0,
        contents->pdf_selection_state_transition_pending ? 1 : 0,
        currentUrlLooksPdf(contents) ? 1 : 0,
        NSStringFromPoint(userLocal),
        NSStringFromPoint(primeLocal),
        focusAction ? 1 : 0,
        pointerAction ? 1 : 0,
        clampAction ? 1 : 0,
        describeObject(contents->window.firstResponder),
        responderChain(contents->window.firstResponder)]);
}

static void resetPdfSelectionStateTransition(WebContents *contents, NSString *reason)
{
    if (!contents || !contents->web_view)
        return;
    contents->pdf_selection_state_transition_url = [contents->web_view.URL.absoluteString copy] ?: @"";
    contents->pdf_selection_state_transition_pending_url = nil;
    contents->pdf_selection_state_transition_consumed = false;
    contents->pdf_selection_state_transition_pending = false;
    NSString *mode = pdfSelectionStateTransitionEnabled() ? pdfSelectionStateTransitionMode() : @"pointer-prime";
    tracePdfSelectionStateTransition(contents, @"reset", mode, reason ?: @"reset", NSZeroPoint, NSZeroPoint, false, false, false);
}

static void applyPdfSelectionStateTransition(WebContents *contents, int x, int y, NSString *phase)
{
    if ((!pdfSelectionStateTransitionEnabled() && !pdfPointerPrimeProductionEnabled()) || !contents || !contents->web_view)
        return;

    NSString *mode = pdfSelectionStateTransitionEnabled() ? pdfSelectionStateTransitionMode() : @"pointer-prime";
    NSPoint userLocal = NSMakePoint(x, y);
    NSSize bounds = contents->web_view.bounds.size;
    NSPoint primeLocal = NSMakePoint(MAX(1.0, bounds.width * 0.05), MAX(1.0, bounds.height * 0.10));
    if (!currentUrlLooksPdf(contents)) {
        tracePdfSelectionStateTransition(contents, phase, mode, @"non-pdf-skip", userLocal, primeLocal, false, false, false);
        return;
    }
    if (currentPdfHasEditableDocumentWidgets(contents)) {
        tracePdfSelectionStateTransition(contents, phase, mode, @"editable-pdf-document-skip", userLocal, primeLocal, false, false, false);
        return;
    }

    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    if (contents->pdf_selection_state_transition_consumed && [contents->pdf_selection_state_transition_url isEqualToString:url]) {
        tracePdfSelectionStateTransition(contents, phase, mode, @"consumed-skip", userLocal, primeLocal, false, false, false);
        return;
    }
    if (contents->pdf_selection_state_transition_pending && [contents->pdf_selection_state_transition_pending_url isEqualToString:url]) {
        tracePdfSelectionStateTransition(contents, phase, mode, @"pending-skip", userLocal, primeLocal, false, false, false);
        return;
    }

    bool production = pdfPointerPrimeProductionEnabled();
    bool focusAction = !production && pdfSelectionStateTransitionModeIs(mode, @"focus-prime");
    bool pointerAction = production || pdfSelectionStateTransitionModeIs(mode, @"pointer-prime");
    if (focusAction)
        [contents->window makeFirstResponder:contents->web_view];

    if (pointerAction) {
        NSPoint primeWindow = [contents->web_view convertPoint:primeLocal toView:nil];
        NSEvent *event = [NSEvent mouseEventWithType:NSEventTypeMouseMoved
            location:primeWindow
            modifierFlags:0
            timestamp:[[NSDate date] timeIntervalSince1970]
            windowNumber:contents->window.windowNumber
            context:[NSGraphicsContext currentContext]
            eventNumber:++contents->mouse_event_number
            clickCount:0
            pressure:0.0];
        [NSApp _setCurrentEvent:event];
        contents->suppress_cursor_notifications = true;
        [contents->web_view _simulateMouseMove:event];
        contents->suppress_cursor_notifications = false;
        [NSApp _setCurrentEvent:nil];
    }

    contents->pdf_selection_state_transition_url = [url copy];
    contents->pdf_selection_state_transition_pending_url = [url copy];
    contents->pdf_selection_state_transition_pending = pointerAction;
    contents->pdf_selection_state_transition_consumed = false;
    tracePdfSelectionStateTransition(contents, phase, mode, @"applied", userLocal, primeLocal, focusAction, pointerAction, pdfSelectionStateTransitionModeIs(mode, @"clamp"));
}

static void markPdfSelectionStateTransitionConsumed(WebContents *contents, NSString *phase)
{
    if ((!pdfSelectionStateTransitionEnabled() && !pdfPointerPrimeProductionEnabled()) || !contents || !contents->web_view)
        return;
    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    if (!contents->pdf_selection_state_transition_pending || ![contents->pdf_selection_state_transition_pending_url isEqualToString:url])
        return;

    contents->pdf_selection_state_transition_url = [url copy];
    contents->pdf_selection_state_transition_pending_url = nil;
    contents->pdf_selection_state_transition_pending = false;
    contents->pdf_selection_state_transition_consumed = true;
    NSString *mode = pdfSelectionStateTransitionEnabled() ? pdfSelectionStateTransitionMode() : @"pointer-prime";
    tracePdfSelectionStateTransition(contents, phase, mode, @"consumed", NSZeroPoint, NSZeroPoint, false, false, false);
}

static void clearPendingPdfSelectionStateTransition(WebContents *contents, NSString *phase)
{
    if ((!pdfSelectionStateTransitionEnabled() && !pdfPointerPrimeProductionEnabled()) || !contents || !contents->web_view || !contents->pdf_selection_state_transition_pending)
        return;

    contents->pdf_selection_state_transition_pending_url = nil;
    contents->pdf_selection_state_transition_pending = false;
    NSString *mode = pdfSelectionStateTransitionEnabled() ? pdfSelectionStateTransitionMode() : @"pointer-prime";
    tracePdfSelectionStateTransition(contents, phase, mode, @"pending-clear-click", NSZeroPoint, NSZeroPoint, false, false, false);
}

static NSPoint remediatedPdfSelectionLocation(WebContents *contents, int x, int y, NSString *phase, bool allowGeometry, bool *applied)
{
    if (applied)
        *applied = false;

    NSPoint originalLocal = NSMakePoint(x, y);
    NSPoint originalWindow = eventLocationInWindow(contents, x, y);
    if (!contents || !contents->web_view || (!pdfSelectionGeometryRemediationEnabled() && !pdfSelectionStateTransitionEnabled())) {
        return originalWindow;
    }

    NSString *mode = pdfSelectionGeometryRemediationMode();
    NSString *transitionMode = pdfSelectionStateTransitionMode();
    if (!currentUrlLooksPdf(contents)) {
        tracePdfSelectionGeometryRemediation(contents, phase, mode, @"not-pdf", originalLocal, originalLocal, originalWindow, originalWindow, false, false);
        if (pdfSelectionStateTransitionEnabled()) {
            tracePdfSelectionStateTransition(contents, phase, transitionMode, @"non-pdf-coordinate-skip", originalLocal, originalLocal, false, false, false);
        }
        return originalWindow;
    }

    bool transitionClamp = pdfSelectionStateTransitionEnabled() && pdfSelectionStateTransitionModeIs(transitionMode, @"clamp");
    bool geometryRemediation = pdfSelectionGeometryRemediationEnabled() && pdfSelectionGeometryRemediationModeIs(mode, @"geometry-only");
    if (!allowGeometry || (!geometryRemediation && !transitionClamp)) {
        tracePdfSelectionGeometryRemediation(contents, phase, mode, @"geometry-disabled", originalLocal, originalLocal, originalWindow, originalWindow, false, false);
        if (pdfSelectionStateTransitionEnabled()) {
            tracePdfSelectionStateTransition(contents, phase, transitionMode, @"coordinate-preserve", originalLocal, originalLocal, false, false, false);
        }
        return originalWindow;
    }

    NSSize bounds = contents->web_view.bounds.size;
    CGFloat targetY = bounds.height > 1 ? bounds.height * 0.10 : originalLocal.y;
    CGFloat minX = bounds.width > 1 ? bounds.width * 0.01 : originalLocal.x;
    CGFloat maxX = bounds.width > 1 ? bounds.width * 0.99 : originalLocal.x;
    NSPoint adjustedLocal = NSMakePoint(MIN(MAX(originalLocal.x, minX), maxX), targetY);
    NSPoint adjustedWindow = [contents->web_view convertPoint:adjustedLocal toView:nil];
    if (applied)
        *applied = true;
    tracePdfSelectionGeometryRemediation(contents, phase, mode, @"applied", originalLocal, adjustedLocal, originalWindow, adjustedWindow, true, false);
    if (pdfSelectionStateTransitionEnabled()) {
        tracePdfSelectionStateTransition(contents, phase, transitionMode, @"coordinate-clamp", originalLocal, adjustedLocal, false, false, true);
    }
    return adjustedWindow;
}

static void applyPdfSelectionFocusRemediation(WebContents *contents, NSString *phase, bool *applied)
{
    if (applied)
        *applied = false;
    if (!contents || !contents->web_view || !pdfSelectionGeometryRemediationEnabled())
        return;

    NSString *mode = pdfSelectionGeometryRemediationMode();
    NSPoint zero = NSZeroPoint;
    if (!currentUrlLooksPdf(contents)) {
        tracePdfSelectionGeometryRemediation(contents, phase, mode, @"focus-not-pdf", zero, zero, zero, zero, false, false);
        return;
    }
    if (!pdfSelectionGeometryRemediationModeIs(mode, @"focus-only")) {
        tracePdfSelectionGeometryRemediation(contents, phase, mode, @"focus-disabled", zero, zero, zero, zero, false, false);
        return;
    }

    [contents->window makeFirstResponder:contents->web_view];
    if (applied)
        *applied = true;
    tracePdfSelectionGeometryRemediation(contents, phase, mode, @"focus-applied", zero, zero, zero, zero, false, true);
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

static void invokeMouseEventOnTarget(WebContents *contents, NSEvent *event, NSView *target)
{
    (void)contents;
    if (!target)
        return;
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
    case NSEventTypeLeftMouseDragged:
        [NSApp _setCurrentEvent:event];
        [target mouseDragged:event];
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
}

static NSView *mouseDispatchTarget(WebContents *contents, NSEvent *event, NSString *mode, NSView *hit)
{
    if (!contents || !contents->web_view)
        return nil;
    if ([mode isEqualToString:@"webview-direct"])
        return contents->web_view;
    if ([mode isEqualToString:@"flipped-view-direct"])
        return findDescendantViewWithClassName(contents->web_view, @"WKFlippedView");
    if ([mode isEqualToString:@"pdf-hud-direct"])
        return findDescendantViewWithClassName(contents->web_view, @"WKPDFHUDView");
    (void)event;
    return hit ?: contents->web_view;
}

static void appendMouseDispatchTrace(WebContents *contents, NSEvent *event, NSString *phase, NSString *mode, NSView *hit, NSView *target, bool delivered)
{
    if (!pdfCopyTraceEnabled())
        return;
    NSWindow *window = contents ? contents->window : nil;
    appendPdfCopyTrace([NSString stringWithFormat:
        @"webkit-pdf-mouse-dispatch tab=%d phase=%@ mode=%@ type=%ld button=%ld event_number=%ld click_count=%ld modifiers=%lu location=%@ hit=%@ target=%@ target_exists=%d delivered=%d window=%@ key=%d main=%d visible=%d window_number=%ld current_event=%@ swizzle_active=%d",
        contents ? contents->tab_id : 0,
        phase ?: @"unknown",
        mode ?: @"normal",
        (long)event.type,
        (long)event.buttonNumber,
        (long)event.eventNumber,
        (long)event.clickCount,
        (unsigned long)event.modifierFlags,
        NSStringFromPoint(event.locationInWindow),
        describeView(hit),
        describeView(target),
        target ? 1 : 0,
        delivered ? 1 : 0,
        describeObject(window),
        window.isKeyWindow ? 1 : 0,
        window.isMainWindow ? 1 : 0,
        window.isVisible ? 1 : 0,
        (long)window.windowNumber,
        describeObject(NSApp.currentEvent),
        g_dispatching_mouse_contents ? 1 : 0]);
}

static void deliverMouseEvent(WebContents *contents, NSEvent *event, NSString *phase)
{
    NSString *mode = pdfMouseDispatchProbeMode();
    NSView *hit = [contents->web_view hitTest:event.locationInWindow] ?: contents->web_view;
    NSString *effectiveMode = mode ?: @"current";
    NSView *target = mouseDispatchTarget(contents, event, effectiveMode, hit);

    installMouseEventSwizzles(contents);
    if ([effectiveMode isEqualToString:@"window-send-event"]) {
        [NSApp _setCurrentEvent:event];
        [contents->window sendEvent:event];
        [NSApp _setCurrentEvent:nil];
        appendMouseDispatchTrace(contents, event, phase, effectiveMode, hit, target, true);
    } else {
        invokeMouseEventOnTarget(contents, event, target);
        appendMouseDispatchTrace(contents, event, phase, effectiveMode, hit, target, target != nil);
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

static void fireNavigationState(WebContents *contents)
{
    if (!contents || !g_callbacks.on_navigation_state)
        return;
    bool can_go_back = !contents->is_devtools
        && !contents->renderer_crashed
        && contents->web_view
        && contents->web_view.canGoBack;
    bool can_go_forward = !contents->is_devtools
        && !contents->renderer_crashed
        && contents->web_view
        && contents->web_view.canGoForward;
    bool can_refresh = !contents->is_devtools
        && contents->has_committed_document;
    g_callbacks.on_navigation_state(
        contents,
        can_go_back,
        can_go_forward,
        can_refresh,
        g_callbacks.on_navigation_state_data);
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

@implementation TSNavigationStateObserver
- (void)observeValueForKeyPath:(NSString *)keyPath
                      ofObject:(id)object
                        change:(NSDictionary<NSKeyValueChangeKey, id> *)change
                       context:(void *)context
{
    (void)change;
    (void)context;
    if (!self.owner || object != self.owner->web_view
        || !([keyPath isEqualToString:@"canGoBack"]
            || [keyPath isEqualToString:@"canGoForward"]
            || [keyPath isEqualToString:@"URL"])) {
        [super observeValueForKeyPath:keyPath ofObject:object change:change context:context];
        return;
    }
    fireNavigationState(self.owner);
}
@end

static bool closeToColor(NSUInteger red, NSUInteger green, NSUInteger blue, NSUInteger target_red, NSUInteger target_green, NSUInteger target_blue)
{
    const NSInteger threshold = 40;
    NSInteger dr = labs((NSInteger)red - (NSInteger)target_red);
    NSInteger dg = labs((NSInteger)green - (NSInteger)target_green);
    NSInteger db = labs((NSInteger)blue - (NSInteger)target_blue);
    return dr + dg + db <= threshold;
}

static bool hasWebKitRenderProofColor(int magenta, int cyan, int yellow, int webkit_green)
{
    constexpr int minimumMarkerPixels = 900;
    return magenta >= minimumMarkerPixels
        || cyan >= minimumMarkerPixels
        || yellow >= minimumMarkerPixels
        || webkit_green >= minimumMarkerPixels;
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

    NSString *status = hasWebKitRenderProofColor(magenta, cyan, yellow, webkit_green) ? @"pass" : @"blank";
    if ([status isEqualToString:@"pass"])
        contents->last_render_probe_pass_url = [contents->web_view.URL.absoluteString copy] ?: @"";
    fireRenderProbe(contents, method, status, (int)width, (int)height, magenta, cyan, yellow, webkit_green, @"");
}

static bool snapshotHasRenderProof(NSImage *image, NSError *error)
{
    if (error || !image)
        return false;

    CGImageRef cg_image = [image CGImageForProposedRect:nil context:nil hints:nil];
    if (!cg_image)
        return false;

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

    return hasWebKitRenderProofColor(magenta, cyan, yellow, webkit_green);
}

static bool closeToVisualColor(NSColor *color, CGFloat red, CGFloat green, CGFloat blue)
{
    if (!color)
        return false;
    NSColor *srgb = [color colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    if (!srgb)
        return false;
    NSInteger r = (NSInteger)lrint(srgb.redComponent * 255.0);
    NSInteger g = (NSInteger)lrint(srgb.greenComponent * 255.0);
    NSInteger b = (NSInteger)lrint(srgb.blueComponent * 255.0);
    NSInteger targetR = (NSInteger)lrint(red * 255.0);
    NSInteger targetG = (NSInteger)lrint(green * 255.0);
    NSInteger targetB = (NSInteger)lrint(blue * 255.0);
    return labs(r - targetR) + labs(g - targetG) + labs(b - targetB) <= 70;
}

static NSString *pdfVisualMetricsForSnapshot(NSImage *image, NSError *error)
{
    if (error || !image)
        return [NSString stringWithFormat:@"status=capture-failed error=%@", error.localizedDescription ?: @"missing-image"];

    CGImageRef cg_image = [image CGImageForProposedRect:nil context:nil hints:nil];
    if (!cg_image)
        return @"status=capture-failed error=missing-cgimage";

    NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithCGImage:cg_image];
    NSInteger width = bitmap.pixelsWide;
    NSInteger height = bitmap.pixelsHigh;
    NSInteger green = 0;
    NSInteger magenta = 0;
    NSInteger cyan = 0;
    NSInteger red = 0;
    NSInteger black = 0;
    NSInteger white = 0;
    NSInteger nonwhite = 0;
    NSInteger sampled = 0;
    NSInteger minNonwhiteX = width;
    NSInteger minNonwhiteY = height;
    NSInteger maxNonwhiteX = -1;
    NSInteger maxNonwhiteY = -1;
    NSInteger stride = 4;

    for (NSInteger y = 0; y < height; y += stride) {
        for (NSInteger x = 0; x < width; x += stride) {
            NSColor *color = [bitmap colorAtX:x y:y];
            sampled++;
            if (closeToVisualColor(color, 0.0, 0.502, 0.0))
                green++;
            if (closeToVisualColor(color, 1.0, 0.0, 1.0))
                magenta++;
            if (closeToVisualColor(color, 0.0, 1.0, 1.0))
                cyan++;
            if (closeToVisualColor(color, 1.0, 0.0, 0.0))
                red++;
            if (closeToVisualColor(color, 0.0, 0.0, 0.0))
                black++;
            if (closeToVisualColor(color, 1.0, 1.0, 1.0))
                white++;
            if (!closeToVisualColor(color, 1.0, 1.0, 1.0)) {
                nonwhite++;
                minNonwhiteX = MIN(minNonwhiteX, x);
                minNonwhiteY = MIN(minNonwhiteY, y);
                maxNonwhiteX = MAX(maxNonwhiteX, x);
                maxNonwhiteY = MAX(maxNonwhiteY, y);
            }
        }
    }

    NSString *dominant = @"none";
    NSInteger dominantCount = 0;
    NSDictionary<NSString *, NSNumber *> *counts = @{
        @"green": @(green),
        @"magenta": @(magenta),
        @"cyan": @(cyan),
        @"red": @(red),
        @"black": @(black),
        @"white": @(white),
    };
    for (NSString *name in counts) {
        NSInteger count = counts[name].integerValue;
        if (count > dominantCount) {
            dominant = name;
            dominantCount = count;
        }
    }

    NSString *nonwhiteBounds = nonwhite ? [NSString stringWithFormat:@"x:%ld,y:%ld,w:%ld,h:%ld",
                                                      (long)minNonwhiteX,
                                                      (long)minNonwhiteY,
                                                      (long)(maxNonwhiteX - minNonwhiteX + 1),
                                                      (long)(maxNonwhiteY - minNonwhiteY + 1)]
                                       : @"none";
    return [NSString stringWithFormat:@"status=pass width=%ld height=%ld stride=%ld sampled=%ld dominant=%@ dominant_count=%ld green=%ld magenta=%ld cyan=%ld red=%ld black=%ld white=%ld nonwhite=%ld nonwhite_bounds=%@",
                     (long)width,
                     (long)height,
                     (long)stride,
                     (long)sampled,
                     dominant,
                     (long)dominantCount,
                     (long)green,
                     (long)magenta,
                     (long)cyan,
                     (long)red,
                     (long)black,
                     (long)white,
                     (long)nonwhite,
                     nonwhiteBounds];
}

static void capturePdfVisualOracleSnapshot(WebContents *contents, NSString *action, NSString *phase, void (^completion)(void))
{
    if (!pdfVisualOracleProbeEnabled() || !contents || !contents->web_view) {
        if (completion)
            completion();
        return;
    }

    [contents->web_view layoutSubtreeIfNeeded];
    int tab_id = contents->tab_id;
    WKWebView *web_view = contents->web_view;
    NSString *action_copy = [action copy] ?: @"none";
    NSString *phase_copy = [phase copy] ?: @"unknown";
    WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
    configuration.rect = web_view.bounds;
    [web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
        WebContents *current = findContentsByTabId(tab_id);
        if (!current || current->web_view != web_view) {
            tracePdfVisualOracle(contents, phase_copy, action_copy, @"status=stale-webview");
            if (completion)
                completion();
            return;
        }
        NSString *metrics = pdfVisualMetricsForSnapshot(snapshotImage, error);
        tracePdfVisualOracle(current, phase_copy, action_copy, [NSString stringWithFormat:@"webview_bounds=%@ %@", NSStringFromRect(web_view.bounds), metrics]);
        if (completion)
            completion();
    }];
}

static void captureRenderProbeAttempt(WebContents *contents, int attempt)
{
    if (!contents || !contents->web_view)
        return;
    if (!g_callbacks.on_render_probe)
        return;

    [contents->web_view layoutSubtreeIfNeeded];
    WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
    configuration.rect = contents->web_view.bounds;
    int tab_id = contents->tab_id;
    WKWebView *web_view = contents->web_view;
    [web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
        WebContents *current = findContentsByTabId(tab_id);
        if (!current || current->web_view != web_view)
            return;
        if (!snapshotHasRenderProof(snapshotImage, error) && attempt < 120) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                WebContents *retry = findContentsByTabId(tab_id);
                if (retry && retry->web_view == web_view)
                    captureRenderProbeAttempt(retry, attempt + 1);
            });
            return;
        }
        classifySnapshotImage(current, @"WKWebView.takeSnapshot", snapshotImage, error);
    }];
}

static void captureRenderProbe(WebContents *contents)
{
    captureRenderProbeAttempt(contents, 0);
}

static void capturePdfSafeFailureProbe(WebContents *contents, NSString *expectedUrl)
{
    if (!contents || !contents->web_view || !g_callbacks.on_render_probe || !expectedUrl.length)
        return;

    int tab_id = contents->tab_id;
    NSString *urlCopy = [expectedUrl copy];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.0 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        WebContents *current = findContentsByTabId(tab_id);
        if (!current || !current->web_view)
            return;
        NSString *currentUrl = current->web_view.URL.absoluteString ?: @"";
        if (![currentUrl isEqualToString:urlCopy])
            return;
        if ([current->last_render_probe_pass_url isEqualToString:urlCopy])
            return;

        [current->web_view layoutSubtreeIfNeeded];
        WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
        configuration.rect = current->web_view.bounds;
        WKWebView *web_view = current->web_view;
        [web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
            WebContents *snapshotContents = findContentsByTabId(tab_id);
            if (!snapshotContents || snapshotContents->web_view != web_view)
                return;
            NSString *snapshotUrl = snapshotContents->web_view.URL.absoluteString ?: @"";
            if (![snapshotUrl isEqualToString:urlCopy])
                return;
            if ([snapshotContents->last_render_probe_pass_url isEqualToString:urlCopy])
                return;
            classifySnapshotImage(snapshotContents, @"WKWebView.pdfSafeFailureSnapshot", snapshotImage, error);
        }];
    });
}

static void schedulePdfLoadWatchdogProbe(WebContents *contents, NSString *expectedUrl)
{
    if (!contents || !contents->web_view || !g_callbacks.on_render_probe || !expectedUrl.length)
        return;
    if (!currentUrlLooksPdf(contents) && ![expectedUrl.lowercaseString containsString:@".pdf"])
        return;

    int tab_id = contents->tab_id;
    NSString *urlCopy = [expectedUrl copy];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3.0 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        WebContents *current = findContentsByTabId(tab_id);
        if (!current || !current->web_view)
            return;
        if (![current->pdf_load_watchdog_url isEqualToString:urlCopy])
            return;
        if ([current->last_render_probe_pass_url isEqualToString:urlCopy])
            return;

        [current->web_view layoutSubtreeIfNeeded];
        WKSnapshotConfiguration *configuration = [[WKSnapshotConfiguration alloc] init];
        configuration.rect = current->web_view.bounds;
        WKWebView *web_view = current->web_view;
        [web_view takeSnapshotWithConfiguration:configuration completionHandler:^(NSImage *snapshotImage, NSError *error) {
            WebContents *snapshotContents = findContentsByTabId(tab_id);
            if (!snapshotContents || snapshotContents->web_view != web_view)
                return;
            if (![snapshotContents->pdf_load_watchdog_url isEqualToString:urlCopy])
                return;
            if ([snapshotContents->last_render_probe_pass_url isEqualToString:urlCopy])
                return;
            classifySnapshotImage(snapshotContents, @"WKWebView.pdfLoadWatchdogSnapshot", snapshotImage, error);
        }];
    });
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
static void updateTargetUrlFromDocumentPoint(WebContents *contents, int x, int y)
{
    if (!contents || !contents->web_view || !g_callbacks.on_target_url_changed)
        return;
    // Real hit-test stack (not smallest-rect scan): elementsFromPoint + ancestor href.
    NSString *script = [NSString stringWithFormat:
        @"(() => {"
         "const x = %d, y = %d;"
         "const list = document.elementsFromPoint ? document.elementsFromPoint(x, y) : [];"
         "const top = list.length ? list[0] : document.elementFromPoint(x, y);"
         "if (!top) return '';"
         "let node = top;"
         "while (node) {"
         "  if (node instanceof HTMLAnchorElement && node.href) return node.href;"
         "  if (node.closest) {"
         "    const a = node.closest('a[href]');"
         "    if (a && a.href) return a.href;"
         "  }"
         "  node = node.parentElement;"
         "}"
         "return '';"
         "})()",
        x, y];
    [contents->web_view evaluateJavaScript:script completionHandler:^(id result, NSError *error) {
        if (!isRegisteredContents(contents) || error)
            return;
        NSString *href = [result isKindOfClass:NSString.class] ? (NSString *)result : @"";
        fireTargetUrl(contents, href);
    }];
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
    if (contents->suppress_cursor_notifications) {
        traceWebKitCursor(contents, @"suppressed", -1, cursor_type);
        return;
    }
    if (contents->last_cursor_type == cursor_type)
        return;

    contents->last_cursor_type = cursor_type;
    traceWebKitCursor(contents, @"fire", -1, cursor_type);
    g_callbacks.on_cursor_changed(contents, cursor_type, g_callbacks.on_cursor_changed_data);
}

static void updateCursorFromDocumentPoint(WebContents *contents, int x, int y)
{
    if (!contents || !contents->web_view || !g_callbacks.on_cursor_changed)
        return;

    uint64_t generation = ++contents->cursor_probe_generation;
    // Real hit-test stack: elementsFromPoint (top-first) + ancestor walk.
    // Do not invent hits via querySelectorAll/smallest-rect geometry.
    NSString *script = [NSString stringWithFormat:
        @"(() => {"
         "const x = %d, y = %d;"
         "const list = document.elementsFromPoint ? document.elementsFromPoint(x, y) : [];"
         "const top = list.length ? list[0] : document.elementFromPoint(x, y);"
         "if (!top) return JSON.stringify({t:0, tag:'none', n:0, via:'empty'});"
         "let node = top;"
         "while (node) {"
         "  if (node instanceof HTMLAnchorElement && node.href) return JSON.stringify({t:2, tag:node.tagName||'', id:node.id||'', via:'a'});"
         "  const tag = node.tagName ? node.tagName.toLowerCase() : '';"
         "  if (tag === 'input' || tag === 'textarea' || node.isContentEditable) return JSON.stringify({t:3, tag:tag, id:node.id||'', via:'input'});"
         "  const cursor = getComputedStyle(node).cursor;"
         "  if (cursor === 'pointer') return JSON.stringify({t:2, tag:tag, cursor:cursor, id:node.id||'', via:'pointer'});"
         "  if (cursor === 'text' || cursor === 'vertical-text') return JSON.stringify({t:3, tag:tag, cursor:cursor, id:node.id||'', via:'text'});"
         "  node = node.parentElement;"
         "}"
         "return JSON.stringify({t:0, tag:top.tagName||'', id:top.id||'', cursor:getComputedStyle(top).cursor, n:list.length, via:'top'});"
         "})()",
        x,
        y];

    [contents->web_view evaluateJavaScript:script completionHandler:^(id result, NSError *error) {
        if (!isRegisteredContents(contents))
            return;
        if (contents->cursor_probe_generation != generation)
            return;
        if (error)
            return;
        int cursor_type = 0;
        if ([result isKindOfClass:NSString.class]) {
            NSString *s = (NSString *)result;
            NSRange r = [s rangeOfString:@"\"t\":"];
            if (r.location != NSNotFound)
                cursor_type = [[s substringFromIndex:r.location + 4] intValue];
            fprintf(stderr, "[libtermsurf_webkit] webkit-cursor dom-json %s\n", s.UTF8String ?: "");
        } else if ([result respondsToSelector:@selector(intValue)]) {
            cursor_type = [result intValue];
        } else {
            return;
        }
        traceWebKitCursor(contents, @"dom", -1, cursor_type);
        fireCursorChanged(contents, cursor_type);
    }];
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
    if (!contents)
        return;
    if (contents->renderer_crash_reported)
        return;

    contents->renderer_crash_reported = true;
    contents->renderer_crashed = true;
    fireNavigationState(contents);
    if (!g_callbacks.on_renderer_crashed)
        return;
    NSString *url = contents->web_view.URL.absoluteString ?: @"";
    bool can_reload = contents->has_committed_document;
    withCString(reason ?: @"unknown", ^(const char *c_reason) {
        withCString(url, ^(const char *c_url) {
            g_callbacks.on_renderer_crashed(
                contents,
                c_reason,
                0,
                c_url,
                can_reload,
                g_callbacks.on_renderer_crashed_data);
        });
    });
}

static void installCursorObserver(WebContents *contents)
{
    contents->cursor_observer = [[NSNotificationCenter defaultCenter]
        addObserverForName:TermSurfCursorChangedNotification
                    object:nil
                     queue:nil
                usingBlock:^(NSNotification *notification) {
                    NSNumber *cursor_type = notification.userInfo[TermSurfCursorTypeKey];
                    if (![cursor_type isKindOfClass:NSNumber.class])
                        return;
                    int mapped_cursor_type = chromiumCursorTypeForWebCoreCursorType(cursor_type.integerValue);
                    traceWebKitCursorNotification(contents, @"observe", notification.object, cursor_type.integerValue, mapped_cursor_type);
                    if (!cursorNotificationBelongsToContents(contents, notification.object))
                        return;
                    fireCursorChanged(contents, mapped_cursor_type);
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

static bool isAllowedLocalHttpsServerTrustChallenge(NSURLAuthenticationChallenge *challenge)
{
    NSURLProtectionSpace *space = challenge.protectionSpace;
    if (space.isProxy)
        return false;
    if (![space.authenticationMethod isEqualToString:NSURLAuthenticationMethodServerTrust])
        return false;
    if (![space.protocol isEqualToString:@"https"])
        return false;
    if (!space.serverTrust)
        return false;

    NSDictionary<NSString *, NSString *> *environment = NSProcessInfo.processInfo.environment;
    NSString *allowedHost = environment[@"ASTROHACKER_WEBKIT_TEST_ALLOW_LOCAL_HTTPS_CERT_HOST"];
    if (![allowedHost isEqualToString:space.host])
        return false;

    NSString *allowedPort = environment[@"ASTROHACKER_WEBKIT_TEST_ALLOW_LOCAL_HTTPS_CERT_PORT"];
    if (allowedPort.length > 0 && allowedPort.integerValue != space.port)
        return false;

    return true;
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
    if (!contents->live_context_id)
        contents->live_context_id = [contents->web_view _enableTermSurfExternalPresentation];

    if (!contents->live_context_id) {
        fprintf(stderr, "[libtermsurf_webkit] live-context export failed tab_id=%d context_id=0\n", contents->tab_id);
        return;
    }

    fprintf(stderr,
        "[libtermsurf_webkit] live-context tab_id=%d context_id=%u width=%d height=%d\n",
        contents->tab_id,
        contents->live_context_id,
        contents->width,
        contents->height);

    if (g_callbacks.on_ca_context_id) {
        g_callbacks.on_ca_context_id(
            contents,
            contents->live_context_id,
            contents->width,
            contents->height,
            g_callbacks.on_ca_context_id_data);
    }
}

@implementation TSNavigationDelegate
- (void)webView:(WKWebView *)webView didStartProvisionalNavigation:(WKNavigation *)navigation
{
    (void)navigation;
    if (self.owner) {
        self.owner->renderer_crash_reported = false;
        clearPdfSelectedTextCache(self.owner, @"navigation-start");
        tracePdfNavigationDiagnostics(self.owner, @"navigation-start", webView.URL.absoluteString);
    }
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
    self.owner->renderer_crashed = false;
    self.owner->has_committed_document = true;
    clearPdfSelectedTextCache(self.owner, @"navigation-commit");
    tracePdfNavigationDiagnostics(self.owner, @"navigation-commit", webView.URL.absoluteString);
    fireUrl(self.owner, webView.URL.absoluteString);
    fireNavigationState(self.owner);
}

- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation
{
    (void)navigation;
    clearPdfSelectedTextCache(self.owner, @"navigation-finish");
    tracePdfNavigationDiagnostics(self.owner, @"navigation-finish", webView.URL.absoluteString);
    if (currentUrlLooksPdf(self.owner))
        resetPdfSelectionStateTransition(self.owner, @"navigation-finish");
    updatePdfEditableDocumentCache(self.owner, @"navigation-finish");
    fireUrl(self.owner, webView.URL.absoluteString);
    [webView evaluateJavaScript:@"document.title" completionHandler:^(id result, NSError *error) {
        if (error)
            NSLog(@"[libtermsurf_webkit] document.title evaluation failed: %@", error);
        NSString *title = [result isKindOfClass:NSString.class] ? result : webView.title;
        fireTitle(self.owner, title);
        fireLoading(self.owner, webView.URL.absoluteString, 0);
        applyPdfViewportFix(self.owner, @"navigation-finish");
        exportContext(self.owner);
        captureRenderProbe(self.owner);
        if (currentUrlLooksPdf(self.owner))
            capturePdfSafeFailureProbe(self.owner, webView.URL.absoluteString);
        tracePdfPasswordOracle(self.owner, @"navigation-finish");
        tracePdfFormOracle(self.owner, @"navigation-finish");
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
    if (isAllowedLocalHttpsServerTrustChallenge(challenge)) {
        NSURLCredential *credential = [NSURLCredential credentialForTrust:challenge.protectionSpace.serverTrust];
        completionHandler(NSURLSessionAuthChallengeUseCredential, credential);
        return;
    }

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

- (void)_webView:(WKWebView *)webView saveDataToFile:(NSData *)data suggestedFilename:(NSString *)suggestedFilename mimeType:(NSString *)mimeType originatingURL:(NSURL *)url
{
    (void)webView;
    WebContents *contents = self.owner;
    NSError *error = nil;
    NSString *savedPath = savePdfDataToDownloads(data, suggestedFilename, &error);
    tracePdfHudSave(contents, @"delegate-save-data", [NSString stringWithFormat:@"suggested_filename=%@ mime_type=%@ originating_url=%@ byte_count=%lu sha256=%@ saved_path=%@ error=%@",
        suggestedFilename ?: @"",
        mimeType ?: @"",
        url.absoluteString ?: @"",
        (unsigned long)data.length,
        sha256ForData(data),
        savedPath ?: @"",
        error.localizedDescription ?: @""]);
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

@interface TSApplicationDelegate : NSObject <NSApplicationDelegate>
@end

@implementation TSApplicationDelegate
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender
{
    (void)sender;
    return NO;
}
@end

int ts_content_main(int argc, const char *const *argv)
{
    (void)argc;
    (void)argv;

    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        static TSApplicationDelegate *delegate = nil;
        delegate = [[TSApplicationDelegate alloc] init];
        application.delegate = delegate;
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
    contents->cursor_probe_generation = 0;
    contents->suppress_cursor_notifications = false;
    contents->renderer_crash_reported = false;
    contents->renderer_crashed = false;
    contents->has_committed_document = false;
    contents->live_context_id = 0;
    contents->presentation_visible = false;
    contents->pending_javascript_dialogs = [[NSMutableDictionary alloc] init];
    contents->pending_http_auth_requests = [[NSMutableDictionary alloc] init];

    NSSize pointSize = hostWindowPointSizeForContents(contents);
    NSRect frame = NSMakeRect(80, 80, pointSize.width, pointSize.height);
    contents->window = [[TSHostWindow alloc] initWithContentRect:frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    contents->window.releasedWhenClosed = NO;
    contents->window.title = @"libtermsurf_webkit";
    contents->window.acceptsMouseMovedEvents = YES;
    contents->window.ignoresMouseEvents = YES;
    // Space co-location with host (issue 26072110403572 Exp 2).
    // Clear exclusive fullscreen roles before setting Auxiliary (AppKit).
    {
      NSWindowCollectionBehavior b = contents->window.collectionBehavior;
      b &= ~NSWindowCollectionBehaviorFullScreenPrimary;
      b &= ~NSWindowCollectionBehaviorFullScreenNone;
      b |= NSWindowCollectionBehaviorCanJoinAllSpaces;
      b |= NSWindowCollectionBehaviorFullScreenAuxiliary;
      contents->window.collectionBehavior = b;
    }
    contents->window.alphaValue = hostWindowAlpha();

    WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
    configuration.websiteDataStore = context->data_store;
    configuration.preferences._developerExtrasEnabled = YES;
    configuration.applicationNameForUserAgent = safariApplicationNameForUserAgent();
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
    contents->navigation_state_observer = [[TSNavigationStateObserver alloc] init];
    contents->navigation_state_observer.owner = contents;
    for (NSString *keyPath in @[@"canGoBack", @"canGoForward", @"URL"])
        [contents->web_view addObserver:contents->navigation_state_observer
                            forKeyPath:keyPath
                               options:NSKeyValueObservingOptionNew
                               context:nullptr];
    contents->ui_delegate = [[TSUIDelegate alloc] init];
    contents->ui_delegate.owner = contents;
    contents->web_view.UIDelegate = contents->ui_delegate;
    installCursorObserver(contents);

    [contents->window.contentView addSubview:contents->web_view];
    [contents->window orderFront:nil];
    registerContents(contents);
    scheduleNativeUiProbe(contents);
    scheduleSyntheticPrintProbe(contents);

    if (g_callbacks.on_tab_ready)
        g_callbacks.on_tab_ready(contents, contents->tab_id, g_callbacks.on_tab_ready_data);
    fireNavigationState(contents);

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
    contents->cursor_probe_generation = 0;
    contents->suppress_cursor_notifications = false;
    contents->renderer_crash_reported = false;
    contents->renderer_crashed = false;
    contents->has_committed_document = false;
    contents->live_context_id = 0;
    contents->presentation_visible = false;
    contents->pending_javascript_dialogs = [[NSMutableDictionary alloc] init];
    contents->pending_http_auth_requests = [[NSMutableDictionary alloc] init];

    NSSize pointSize = hostWindowPointSizeForContents(contents);
    NSRect frame = NSMakeRect(120, 120, pointSize.width, pointSize.height);
    contents->window = [[TSHostWindow alloc] initWithContentRect:frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
    contents->window.releasedWhenClosed = NO;
    contents->window.title = @"libtermsurf_webkit_devtools";
    contents->window.acceptsMouseMovedEvents = YES;
    contents->window.ignoresMouseEvents = YES;
    // Space co-location with host (issue 26072110403572 Exp 2).
    // Clear exclusive fullscreen roles before setting Auxiliary (AppKit).
    {
      NSWindowCollectionBehavior b = contents->window.collectionBehavior;
      b &= ~NSWindowCollectionBehaviorFullScreenPrimary;
      b &= ~NSWindowCollectionBehaviorFullScreenNone;
      b |= NSWindowCollectionBehaviorCanJoinAllSpaces;
      b |= NSWindowCollectionBehaviorFullScreenAuxiliary;
      contents->window.collectionBehavior = b;
    }
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
    fireNavigationState(contents);

    exportContext(contents);
    return contents;
}

void ts_destroy_web_contents(ts_web_contents_t wc)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    unregisterContents(contents);
    [contents->web_view _setTermSurfExternalPresentationVisible:NO];
    if (contents->cursor_observer)
        [[NSNotificationCenter defaultCenter] removeObserver:contents->cursor_observer];
    if (contents->is_devtools) {
        [contents->inspector close];
    } else {
        for (NSString *keyPath in @[@"canGoBack", @"canGoForward", @"URL"])
            [contents->web_view removeObserver:contents->navigation_state_observer forKeyPath:keyPath];
        contents->navigation_state_observer.owner = nullptr;
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

    tracePdfNavigationDiagnostics(contents, @"load-url-before", ns_url.absoluteString);
    contents->pdf_load_watchdog_url = [ns_url.absoluteString copy];
    schedulePdfLoadWatchdogProbe(contents, ns_url.absoluteString);

    if (ns_url.isFileURL) {
        NSURL *directory = [ns_url URLByDeletingLastPathComponent];
        [contents->web_view loadFileURL:ns_url allowingReadAccessToURL:directory];
        return;
    }

    [contents->web_view loadRequest:[NSURLRequest requestWithURL:ns_url]];
}

bool ts_navigation_action(ts_web_contents_t wc, const char *action)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents || !isRegisteredContents(contents) || !contents->web_view
        || contents->is_devtools || !action)
        return false;
    if (strcmp(action, "back") == 0) {
        if (contents->renderer_crashed || !contents->web_view.canGoBack)
            return false;
        [contents->web_view goBack];
    } else if (strcmp(action, "forward") == 0) {
        if (contents->renderer_crashed || !contents->web_view.canGoForward)
            return false;
        [contents->web_view goForward];
    } else if (strcmp(action, "refresh") == 0) {
        if (!contents->has_committed_document)
            return false;
        [contents->web_view reload];
    } else {
        return false;
    }
    return true;
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
    frame.size = hostWindowPointSizeForContents(contents);
    [contents->window setFrame:frame display:YES animate:NO];
    contents->web_view.frame = contents->window.contentView.bounds;
    [contents->web_view layoutSubtreeIfNeeded];
    tracePdfViewGeometry(contents, @"resize", 0, 0, NSMakePoint(0, 0));
    applyPdfViewportFix(contents, @"resize");
    exportContext(contents);
}

void ts_forward_mouse_event(ts_web_contents_t wc, int type, int button, int x, int y, int click_count, int modifiers)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    bool is_up = type == 1;
    bool was_dragging = (contents->mouse_buttons_down & mouseButtonMask(button)) != 0;
    NSPoint original_location = eventLocationInWindow(contents, x, y);
    NSPoint location = adjustedPdfSelectionLocation(contents, x, y, is_up && was_dragging);
    bool geometryRemediationApplied = false;
    if (pdfSelectionGeometryRemediationEnabled() || pdfSelectionStateTransitionEnabled()) {
        location = remediatedPdfSelectionLocation(contents, x, y, type == 1 ? @"mouse-up" : @"mouse-down", button == 0, &geometryRemediationApplied);
    }
    if (!is_up) {
        if (button == 0)
            applyPdfSelectionStateTransition(contents, x, y, @"before-selection");
        bool focusRemediationApplied = false;
        applyPdfSelectionFocusRemediation(contents, @"before-gesture", &focusRemediationApplied);
        contents->pdf_selected_text_generation++;
        contents->pdf_selected_text_drag_start = original_location;
        contents->pdf_selected_text_drag_exceeded_threshold = false;
        clearPdfSelectedTextCache(contents, @"mouse-down");
        applyPdfResponderProbe(contents, @"before-gesture");
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
    if (pdfCopyTraceEnabled()) {
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-mouse tab=%d type=%d button=%d x=%d y=%d click_count=%d modifiers=%d location=%@ original_location=%@ edge_mode=%@ edge_delta=%.2f remediation_geometry=%d", contents->tab_id, type, button, x, y, click_count, modifiers, NSStringFromPoint(location), NSStringFromPoint(original_location), pdfSelectionEdgeProbeMode() ?: @"none", pdfSelectionEdgeDeltaX(), geometryRemediationApplied ? 1 : 0]);
    }
    tracePdfViewGeometry(contents, type == 1 ? @"mouse-up" : @"mouse-down", x, y, original_location);
    if (is_up && was_dragging && [pdfSelectionEdgeProbeMode() isEqualToString:@"extra-drag"]) {
        NSPoint extra_location = original_location;
        extra_location.x += pdfSelectionEdgeDeltaX();
        NSEvent *extra_event = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDragged
            location:extra_location
            modifierFlags:cocoaModifiers(modifiers)
            timestamp:[[NSDate date] timeIntervalSince1970]
            windowNumber:contents->window.windowNumber
            context:[NSGraphicsContext currentContext]
            eventNumber:++contents->mouse_event_number
            clickCount:contents->mouse_click_count
            pressure:0.0];
        [NSApp _setCurrentEvent:extra_event];
        [contents->web_view mouseDragged:extra_event];
        [NSApp _setCurrentEvent:nil];
        if (pdfCopyTraceEnabled()) {
            appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-selection-edge tab=%d mode=extra-drag x=%d y=%d original_location=%@ adjusted_location=%@ delta=%.2f", contents->tab_id, x, y, NSStringFromPoint(original_location), NSStringFromPoint(extra_location), pdfSelectionEdgeDeltaX()]);
        }
    }
    deliverMouseEvent(contents, event, type == 1 ? @"mouse-up" : @"mouse-down");
    if (type == 1) {
        if (button == 0 && was_dragging && !contents->pdf_selected_text_drag_exceeded_threshold)
            clearPendingPdfSelectionStateTransition(contents, @"mouse-up-click");
        traceCopyState(contents, @"after-mouse-up");
        tracePdfSelectionSurface(contents, @"after-mouse-up");
        tracePdfSelectedTextRoutes(contents, @"after-mouse-up");
        tracePdfFormOracle(contents, @"after-mouse-up");
        tracePdfViewHierarchy(contents, @"post-click");
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            tracePdfFormOracle(contents, @"after-mouse-up-delayed");
        });
        if (was_dragging && contents->pdf_selected_text_drag_exceeded_threshold)
            requestPdfSelectedTextCacheCapture(contents, @"after-mouse-up");
        else if (was_dragging && pdfSelectedTextCacheCopyTraceEnabled())
            appendPdfSelectedTextCacheCopyTrace([NSString stringWithFormat:@"webkit-pdf-selected-text-cache tab=%d action=capture-skip reason=drag-threshold generation=%llu", contents->tab_id, (unsigned long long)contents->pdf_selected_text_generation]);
    }
}

void ts_forward_mouse_move(ts_web_contents_t wc, int x, int y, int modifiers)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    bool is_drag = contents->mouse_buttons_down & mouseButtonMask(0);
    NSPoint original_location = eventLocationInWindow(contents, x, y);
    if (is_drag) {
        CGFloat dx = original_location.x - contents->pdf_selected_text_drag_start.x;
        CGFloat dy = original_location.y - contents->pdf_selected_text_drag_start.y;
        if ((dx * dx) + (dy * dy) >= 64.0) {
            contents->pdf_selected_text_drag_exceeded_threshold = true;
            markPdfSelectionStateTransitionConsumed(contents, @"selection-threshold");
        }
    }
    NSPoint location = adjustedPdfSelectionLocation(contents, x, y, is_drag);
    bool geometryRemediationApplied = false;
    if (pdfSelectionGeometryRemediationEnabled() || pdfSelectionStateTransitionEnabled()) {
        location = remediatedPdfSelectionLocation(contents, x, y, is_drag ? @"mouse-drag" : @"mouse-move", is_drag, &geometryRemediationApplied);
    }
    NSEvent *event = [NSEvent mouseEventWithType:is_drag ? NSEventTypeLeftMouseDragged : NSEventTypeMouseMoved
        location:location
        modifierFlags:cocoaModifiers(modifiers)
        timestamp:[[NSDate date] timeIntervalSince1970]
        windowNumber:contents->window.windowNumber
        context:[NSGraphicsContext currentContext]
        eventNumber:++contents->mouse_event_number
        clickCount:is_drag ? contents->mouse_click_count : 0
        pressure:0.0];
    if (pdfMouseDispatchProbeMode() && is_drag) {
        contents->suppress_cursor_notifications = true;
        deliverMouseEvent(contents, event, is_drag ? @"mouse-drag" : @"mouse-move");
        contents->suppress_cursor_notifications = false;
    } else {
        if (is_drag) {
            [NSApp _setCurrentEvent:event];
            contents->suppress_cursor_notifications = true;
            if ([pdfSelectionEdgeProbeMode() isEqualToString:@"target"]) {
                NSView *target = [contents->web_view hitTest:event.locationInWindow] ?: contents->web_view;
                [target mouseDragged:event];
            } else {
                [contents->web_view mouseDragged:event];
            }
            contents->suppress_cursor_notifications = false;
            [NSApp _setCurrentEvent:nil];
        } else {
            deliverMouseEvent(contents, event, @"mouse-move");
            updateCursorFromDocumentPoint(contents, x, y);
            updateTargetUrlFromDocumentPoint(contents, x, y);
        }
    }
    if (is_drag && pdfCopyTraceEnabled()) {
        appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-copy-drag tab=%d x=%d y=%d modifiers=%d location=%@ original_location=%@ edge_mode=%@ edge_delta=%.2f remediation_geometry=%d", contents->tab_id, x, y, modifiers, NSStringFromPoint(event.locationInWindow), NSStringFromPoint(original_location), pdfSelectionEdgeProbeMode() ?: @"none", pdfSelectionEdgeDeltaX(), geometryRemediationApplied ? 1 : 0]);
    }
    if (is_drag)
        tracePdfViewGeometry(contents, @"mouse-drag", x, y, original_location);
}

static int64_t coreGraphicsScrollPhaseForTermSurfPhase(int phase)
{
    if (phase & 1)
        return kCGScrollPhaseBegan;
    if (phase & 4)
        return kCGScrollPhaseChanged;
    if (phase & 8)
        return kCGScrollPhaseEnded;
    if (phase & 16)
        return kCGScrollPhaseCancelled;
    if (phase & 32)
        return kCGScrollPhaseMayBegin;
    return 0;
}

static int64_t coreGraphicsMomentumPhaseForTermSurfPhase(int momentum_phase)
{
    if (momentum_phase & 1)
        return kCGMomentumScrollPhaseBegin;
    if (momentum_phase & 4)
        return kCGMomentumScrollPhaseContinue;
    if (momentum_phase & 8)
        return kCGMomentumScrollPhaseEnd;
    return kCGMomentumScrollPhaseNone;
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
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    CGEventRef cg_event = CGEventCreateScrollWheelEvent2(nullptr, kCGScrollEventUnitPixel, 2, delta_y, delta_x, 0);
    if (!cg_event)
        return;
    CGEventSetLocation(cg_event, eventLocationInGlobalScreen(contents, x, y));
    CGEventSetFlags(cg_event, (CGEventFlags)cocoaModifiers(modifiers));
    CGEventSetIntegerValueField(cg_event, kCGScrollWheelEventIsContinuous, precise ? 1 : 0);
    if (momentum_phase != 0) {
        CGEventSetIntegerValueField(cg_event, kCGScrollWheelEventMomentumPhase, coreGraphicsMomentumPhaseForTermSurfPhase(momentum_phase));
    } else {
        CGEventSetIntegerValueField(cg_event, kCGScrollWheelEventScrollPhase, coreGraphicsScrollPhaseForTermSurfPhase(phase));
    }
    NSEvent *event = [NSEvent eventWithCGEvent:cg_event];
    NSEvent *window_event = [event respondsToSelector:@selector(_eventRelativeToWindow:)] ? [event _eventRelativeToWindow:contents->window] : event;
    if (!window_event)
        window_event = event;
    int64_t cg_continuous = CGEventGetIntegerValueField(cg_event, kCGScrollWheelEventIsContinuous);
    int64_t cg_scroll_phase = CGEventGetIntegerValueField(cg_event, kCGScrollWheelEventScrollPhase);
    int64_t cg_momentum_phase = CGEventGetIntegerValueField(cg_event, kCGScrollWheelEventMomentumPhase);
    CFRelease(cg_event);
    NSView *hit_target = [contents->web_view hitTest:window_event.locationInWindow] ?: contents->web_view;
    NSString *dispatch_mode = webkitScrollDispatchMode();
    NSView *dispatch_target = [dispatch_mode isEqualToString:@"target"] ? hit_target : contents->web_view;
    appendWebKitScrollTrace([NSString stringWithFormat:@"event=forward-scroll tab=%d url=%@ x=%d y=%d input_delta_x=%.3f input_delta_y=%.3f input_phase=%d input_momentum_phase=%d input_precise=%d modifiers=%d cg_scroll_phase=%lld cg_momentum_phase=%lld ns_delta_x=%.3f ns_delta_y=%.3f ns_phase=%lu ns_momentum_phase=%lu ns_precise=%d cg_continuous=%lld dispatch_mode=%@ target=%@ hit_target=%@ delivered=before",
        contents->tab_id,
        contents->web_view.URL.absoluteString ?: @"",
        x,
        y,
        delta_x,
        delta_y,
        phase,
        momentum_phase,
        precise ? 1 : 0,
        modifiers,
        (long long)cg_scroll_phase,
        (long long)cg_momentum_phase,
        event.scrollingDeltaX,
        event.scrollingDeltaY,
        (unsigned long)event.phase,
        (unsigned long)event.momentumPhase,
        event.hasPreciseScrollingDeltas ? 1 : 0,
        (long long)cg_continuous,
        dispatch_mode,
        dispatch_target ? NSStringFromClass(dispatch_target.class) : @"nil",
        hit_target ? NSStringFromClass(hit_target.class) : @"nil"]);
    [NSApp _setCurrentEvent:window_event];
    if ([dispatch_mode isEqualToString:@"window-send-event"])
        [contents->window sendEvent:window_event];
    else
        [dispatch_target scrollWheel:window_event];
    [NSApp _setCurrentEvent:nil];
    appendWebKitScrollTrace([NSString stringWithFormat:@"event=forward-scroll tab=%d url=%@ x=%d y=%d input_delta_x=%.3f input_delta_y=%.3f input_phase=%d input_momentum_phase=%d input_precise=%d modifiers=%d cg_scroll_phase=%lld cg_momentum_phase=%lld ns_delta_x=%.3f ns_delta_y=%.3f ns_phase=%lu ns_momentum_phase=%lu ns_precise=%d cg_continuous=%lld dispatch_mode=%@ target=%@ hit_target=%@ delivered=after",
        contents->tab_id,
        contents->web_view.URL.absoluteString ?: @"",
        x,
        y,
        delta_x,
        delta_y,
        phase,
        momentum_phase,
        precise ? 1 : 0,
        modifiers,
        (long long)cg_scroll_phase,
        (long long)cg_momentum_phase,
        event.scrollingDeltaX,
        event.scrollingDeltaY,
        (unsigned long)event.phase,
        (unsigned long)event.momentumPhase,
        event.hasPreciseScrollingDeltas ? 1 : 0,
        (long long)cg_continuous,
        dispatch_mode,
        dispatch_target ? NSStringFromClass(dispatch_target.class) : @"nil",
        hit_target ? NSStringFromClass(hit_target.class) : @"nil"]);
}

static void submitPdfFindQuery(WebContents *contents)
{
    if (!contents || !contents->web_view)
        return;
    NSString *query = [contents->pdf_find_query copy] ?: @"";
    if (!query.length) {
        tracePdfFind(contents, @"submit-skip", @"reason=empty-query");
        return;
    }
    if (![contents->web_view respondsToSelector:@selector(findString:withConfiguration:completionHandler:)]) {
        tracePdfFind(contents, @"submit-skip", @"reason=api-unavailable");
        return;
    }

    WKFindConfiguration *configuration = [[WKFindConfiguration alloc] init];
    configuration.wraps = YES;
    configuration.caseSensitive = NO;
    configuration.backwards = NO;
    tracePdfFind(contents, @"submit", [NSString stringWithFormat:@"length=%lu", (unsigned long)query.length]);
    WebContents *captured = contents;
    [contents->web_view findString:query withConfiguration:configuration completionHandler:^(WKFindResult *result) {
        tracePdfFind(captured, @"result", [NSString stringWithFormat:@"match_found=%d", result.matchFound ? 1 : 0]);
    }];
}

static bool responderLooksEditable(NSResponder *responder)
{
    if (!responder)
        return false;
    if ([responder isKindOfClass:NSTextView.class])
        return true;
    if ([responder isKindOfClass:NSTextField.class])
        return true;
    if ([responder isKindOfClass:NSComboBox.class])
        return true;
    for (Class current = [responder class]; current; current = class_getSuperclass(current)) {
        NSString *lower = NSStringFromClass(current).lowercaseString;
        if ([lower containsString:@"text"] || [lower containsString:@"field"] || [lower containsString:@"editor"])
            return true;
    }
    return false;
}

static bool performPdfKeyboardPageScrollForKeyEvent(WebContents *contents, int type, int keycode, int modifiers)
{
    if (type != 0)
        return false;
    if (modifiers != 0)
        return false;
    if (keycode != 33 && keycode != 34)
        return false;
    if (!contents || !contents->web_view || !currentUrlLooksPdf(contents)) {
        tracePdfKeyboard(contents, @"skip", [NSString stringWithFormat:@"reason=non-pdf-or-missing keycode=%d modifiers=%d", keycode, modifiers]);
        return false;
    }
    if (!contents->focused) {
        tracePdfKeyboard(contents, @"skip", [NSString stringWithFormat:@"reason=not-focused keycode=%d modifiers=%d", keycode, modifiers]);
        return false;
    }
    if (contents->pdf_find_session_active) {
        tracePdfKeyboard(contents, @"skip", [NSString stringWithFormat:@"reason=find-session-active keycode=%d modifiers=%d", keycode, modifiers]);
        return false;
    }
    if (currentPdfHasEditableDocumentWidgets(contents)) {
        tracePdfKeyboard(contents, @"skip", [NSString stringWithFormat:@"reason=editable-pdf-document keycode=%d modifiers=%d cached_url=%@ cached_reason=%@",
            keycode,
            modifiers,
            contents->pdf_editable_document_url ?: @"",
            contents->pdf_editable_document_reason ?: @"unknown"]);
        return false;
    }
    NSResponder *firstResponder = contents->window.firstResponder;
    if (responderLooksEditable(firstResponder)) {
        tracePdfKeyboard(contents, @"skip", [NSString stringWithFormat:@"reason=editable-first-responder keycode=%d modifiers=%d first_responder=%@ responder_chain=%@",
            keycode,
            modifiers,
            describeObject(firstResponder),
            responderChain(firstResponder)]);
        return false;
    }

    int x = MAX(1, (int)round(NSWidth(contents->web_view.bounds) / 2.0));
    int y = MAX(1, (int)round(NSHeight(contents->web_view.bounds) / 2.0));
    float deltaY = keycode == 34 ? -960.0f : 960.0f;
    CGEventRef cg_event = CGEventCreateScrollWheelEvent2(nullptr, kCGScrollEventUnitPixel, 2, deltaY, 0, 0);
    if (!cg_event) {
        tracePdfKeyboard(contents, @"skip", [NSString stringWithFormat:@"reason=missing-cg-event keycode=%d", keycode]);
        return false;
    }
    CGEventSetLocation(cg_event, eventLocationInGlobalScreen(contents, x, y));
    NSEvent *event = [NSEvent eventWithCGEvent:cg_event];
    CFRelease(cg_event);
    NSView *target = [contents->web_view hitTest:event.locationInWindow] ?: contents->web_view;
    tracePdfKeyboard(contents, @"invoke", [NSString stringWithFormat:@"keycode=%d action=%@ delta_y=%.1f point=%d,%d target=%@ first_responder=%@ responder_chain=%@",
        keycode,
        keycode == 34 ? @"page-down" : @"page-up",
        deltaY,
        x,
        y,
        describeObject(target),
        describeObject(firstResponder),
        responderChain(firstResponder)]);
    [NSApp _setCurrentEvent:event];
    [target scrollWheel:event];
    [NSApp _setCurrentEvent:nil];
    return true;
}

void ts_forward_key_event(ts_web_contents_t wc, int type, int keycode, const char *utf8, int modifiers)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    NSString *characters = stringFromCString(utf8);
    NSEventType eventType = type == 1 ? NSEventTypeKeyUp : NSEventTypeKeyDown;
    bool is_pdf_url = currentUrlLooksPdf(contents);
    if (type == 0 && is_pdf_url && pdfActionProbeEnabled()) {
        NSString *diagnosticAction = pdfDiagnosticActionForKeyEvent(keycode, modifiers);
        if (diagnosticAction.length) {
            bool attempted = performPdfActionDiagnostic(contents, diagnosticAction);
            tracePdfAction(contents, attempted ? @"diagnostic-key-result" : @"diagnostic-key-failed", diagnosticAction, [NSString stringWithFormat:@"keycode=%d modifiers=%d attempted=%d", keycode, modifiers, attempted ? 1 : 0]);
            return;
        }
    }
    if (type == 0 && is_pdf_url && keycode == 80 && (modifiers & 8) != 0 && performPdfPrintModalDiagnostic(contents, keycode, modifiers))
        return;
    if (type == 0 && is_pdf_url && keycode == 80 && (modifiers & 8) != 0 && performPdfPrintDialogDiagnostic(contents, keycode, modifiers))
        return;
    if (type == 0 && is_pdf_url && keycode == 80 && (modifiers & 8) != 0 && performPdfPrintOperationDiagnostic(contents, keycode, modifiers))
        return;
    if (type == 0 && is_pdf_url && performPdfProductionZoomForKeyEvent(contents, keycode, modifiers))
        return;
    if (type == 0 && is_pdf_url && keycode == 83 && (modifiers & 8) != 0) {
        bool invoked = performPdfHudSavePrivateHook(contents);
        tracePdfHudSave(contents, invoked ? @"private-hook-result" : @"private-hook-failed", [NSString stringWithFormat:@"source=command-s invoked=%d", invoked ? 1 : 0]);
        if (invoked) {
            return;
        }
    }
    if (type == 0 && is_pdf_url && keycode == 70 && (modifiers & 8) != 0) {
        contents->pdf_find_session_active = true;
        contents->pdf_find_query = [NSMutableString string];
        tracePdfFind(contents, @"begin", @"source=command-f");
        return;
    }
    if (type == 0 && is_pdf_url && contents->pdf_find_session_active) {
        if (keycode == 13 || keycode == 36) {
            submitPdfFindQuery(contents);
            contents->pdf_find_session_active = false;
            return;
        }
        if (keycode == 53 || keycode == 27) {
            tracePdfFind(contents, @"end", @"source=escape");
            contents->pdf_find_session_active = false;
            contents->pdf_find_query = nil;
            return;
        }
        if ((keycode == 51 || keycode == 8) && contents->pdf_find_query.length > 0) {
            [contents->pdf_find_query deleteCharactersInRange:NSMakeRange(contents->pdf_find_query.length - 1, 1)];
            tracePdfFind(contents, @"edit", @"source=backspace");
            return;
        }
        if (characters.length > 0 && modifiers == 0) {
            if (!contents->pdf_find_query)
                contents->pdf_find_query = [NSMutableString string];
            [contents->pdf_find_query appendString:characters];
            tracePdfFind(contents, @"edit", [NSString stringWithFormat:@"append_len=%lu", (unsigned long)characters.length]);
            return;
        }
    }
    if (!is_pdf_url && contents->pdf_find_session_active) {
        contents->pdf_find_session_active = false;
        contents->pdf_find_query = nil;
    }
    if (performPdfKeyboardPageScrollForKeyEvent(contents, type, keycode, modifiers))
        return;
    unsigned short macKeyCode = macKeyCodeForWindowsKeyCode(keycode);
    NSEvent *event = [NSEvent keyEventWithType:eventType
        location:NSMakePoint(0, 0)
        modifierFlags:cocoaModifiers(modifiers)
        timestamp:[[NSDate date] timeIntervalSince1970]
        windowNumber:contents->window.windowNumber
        context:nil
        characters:characters
        charactersIgnoringModifiers:characters
        isARepeat:type == 2
        keyCode:macKeyCode];

    bool is_copy_key_down = type == 0 && keycode == 67 && (modifiers & 8) != 0;
    if (type == 0 && !is_copy_key_down)
        clearPdfSelectedTextCache(contents, @"non-copy-key");
    bool route_pdf_text_input = type == 0
        && is_pdf_url
        && modifiers == 0
        && isPrintableTextInput(characters);
    NSString *copy_bridge_mode = is_copy_key_down ? pdfCopyBridgeMode() : nil;
    PdfCopyBridgeState copy_bridge_state;
    if (is_copy_key_down) {
        applyPdfResponderProbe(contents, @"before-copy");
        if (pdfSelectedTextCacheCopyEnabled())
            contents->pdf_selected_text_copy_start_pasteboard = [[NSPasteboard.generalPasteboard stringForType:NSPasteboardTypeString] copy] ?: @"";
        if (pdfCopyBridgeEnabled())
            tracePdfCopyBridgeState(contents, @"before-copy", copy_bridge_mode, &copy_bridge_state);
        copy_bridge_state = applyPdfCopyBridge(contents, copy_bridge_mode);
        traceCopyState(contents, @"before-external-copy");
        tracePdfSelectionSurface(contents, @"before-external-copy");
        tracePdfSelectedTextRoutes(contents, @"before-external-copy");
        tracePdfViewGeometry(contents, @"before-external-copy", 0, 0, NSMakePoint(0, 0));
        runPdfCopyBridgePreKeyRoutes(contents, copy_bridge_mode, event);
    }

    if (eventType == NSEventTypeKeyUp)
    {
        [NSApp _setCurrentEvent:event];
        [contents->web_view keyUp:event];
        [NSApp _setCurrentEvent:nil];
        if (is_pdf_url && keycode == 13)
            dispatchPdfPasswordReturnKeyUp(contents);
    } else {
        [NSApp _setCurrentEvent:event];
        if (route_pdf_text_input)
            [contents->web_view insertText:characters];
        else
            [contents->web_view keyDown:event];
        [NSApp _setCurrentEvent:nil];
    }
    if (is_pdf_url && type == 1)
        tracePdfPasswordOracle(contents, @"key-up");
    if (is_pdf_url && type == 1)
        tracePdfFormOracle(contents, @"key-up");
    if (is_pdf_url && type == 1)
        tracePdfViewHierarchy(contents, @"post-deterministic-input");
    if (is_pdf_url && type == 1)
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
            tracePdfFormOracle(contents, @"key-up-delayed");
        });
    if (is_copy_key_down) {
        traceCopyState(contents, @"after-external-copy");
        tracePdfSelectionSurface(contents, @"after-external-copy");
        tracePdfSelectedTextRoutes(contents, @"after-external-copy");
        schedulePdfSelectedTextCacheCopyDecision(contents, @"after-external-copy");
        tracePdfViewGeometry(contents, @"after-external-copy", 0, 0, NSMakePoint(0, 0));
        if ([pdfResponderProbeMode() isEqualToString:@"explicit-copy-target"]) {
            traceCopyState(contents, @"before-explicit-copy-target");
            tracePdfSelectionSurface(contents, @"before-explicit-copy-target");
            tracePdfSelectedTextRoutes(contents, @"before-explicit-copy-target");
            tracePdfViewGeometry(contents, @"before-explicit-copy-target", 0, 0, NSMakePoint(0, 0));
            BOOL ok_webview = [NSApp sendAction:@selector(copy:) to:contents->web_view from:nil];
            appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-explicit-copy-target tab=%d route=sendActionWebView ok=%d clipboard={%@}", contents->tab_id, ok_webview ? 1 : 0, clipboardSample()]);
            if ([contents->web_view respondsToSelector:@selector(copy:)]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
                [contents->web_view performSelector:@selector(copy:) withObject:nil];
#pragma clang diagnostic pop
                appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-explicit-copy-target tab=%d route=performWebViewCopy responds=1 invoked=1 clipboard={%@}", contents->tab_id, clipboardSample()]);
            } else {
                appendPdfCopyTrace([NSString stringWithFormat:@"webkit-pdf-explicit-copy-target tab=%d route=performWebViewCopy responds=0 invoked=0 reason=not-responds clipboard={%@}", contents->tab_id, clipboardSample()]);
            }
            traceCopyState(contents, @"after-explicit-copy-target");
            tracePdfSelectionSurface(contents, @"after-explicit-copy-target");
            tracePdfSelectedTextRoutes(contents, @"after-explicit-copy-target");
            tracePdfViewGeometry(contents, @"after-explicit-copy-target", 0, 0, NSMakePoint(0, 0));
        }
        if (pdfCopyInProcessProbeEnabled() || pdfCopyDirectEnabled()) {
            NSString *copyTraceEvent = pdfCopyDirectEnabled() ? @"webkit-pdf-copy-direct" : @"webkit-pdf-copy-inprocess";
            traceCopyState(contents, pdfCopyDirectEnabled() ? @"before-direct-copy" : @"before-inprocess-copy");
            tracePdfSelectionSurface(contents, pdfCopyDirectEnabled() ? @"before-direct-copy" : @"before-inprocess-copy");
            tracePdfSelectedTextRoutes(contents, pdfCopyDirectEnabled() ? @"before-direct-copy" : @"before-inprocess-copy");
            tracePdfViewGeometry(contents, pdfCopyDirectEnabled() ? @"before-direct-copy" : @"before-inprocess-copy", 0, 0, NSMakePoint(0, 0));
            BOOL ok_nil = [NSApp sendAction:@selector(copy:) to:nil from:nil];
            appendPdfCopyTrace([NSString stringWithFormat:@"%@ tab=%d route=sendActionNil ok=%d clipboard={%@}", copyTraceEvent, contents->tab_id, ok_nil ? 1 : 0, clipboardSample()]);
            BOOL ok_webview = [NSApp sendAction:@selector(copy:) to:contents->web_view from:nil];
            appendPdfCopyTrace([NSString stringWithFormat:@"%@ tab=%d route=sendActionWebView ok=%d clipboard={%@}", copyTraceEvent, contents->tab_id, ok_webview ? 1 : 0, clipboardSample()]);
            if ([contents->web_view respondsToSelector:@selector(copy:)]) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
                [contents->web_view performSelector:@selector(copy:) withObject:nil];
#pragma clang diagnostic pop
                appendPdfCopyTrace([NSString stringWithFormat:@"%@ tab=%d route=performWebViewCopy responds=1 invoked=1 clipboard={%@}", copyTraceEvent, contents->tab_id, clipboardSample()]);
            } else {
                appendPdfCopyTrace([NSString stringWithFormat:@"%@ tab=%d route=performWebViewCopy responds=0 invoked=0 reason=not-responds clipboard={%@}", copyTraceEvent, contents->tab_id, clipboardSample()]);
            }
            traceCopyState(contents, pdfCopyDirectEnabled() ? @"after-direct-copy" : @"after-inprocess-copy");
            tracePdfSelectionSurface(contents, pdfCopyDirectEnabled() ? @"after-direct-copy" : @"after-inprocess-copy");
            tracePdfSelectedTextRoutes(contents, pdfCopyDirectEnabled() ? @"after-direct-copy" : @"after-inprocess-copy");
            tracePdfViewGeometry(contents, pdfCopyDirectEnabled() ? @"after-direct-copy" : @"after-inprocess-copy", 0, 0, NSMakePoint(0, 0));
        }
        restorePdfCopyBridge(contents, copy_bridge_mode, &copy_bridge_state);
    }
}

void ts_set_focus(ts_web_contents_t wc, bool focused)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return;

    contents->focused = focused;
    traceCopyState(contents, focused ? @"focus-true" : @"focus-false");
    tracePdfSelectionSurface(contents, focused ? @"focus-true" : @"focus-false");
    if (!focused)
        clearPdfSelectedTextCache(contents, @"focus-false");
    if (!focused) {
        if (contents->window) {
            [contents->window makeFirstResponder:nil];
            if ([NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_ALLOW_HOST_MOUSE"] isEqualToString:@"1"])
                contents->window.ignoresMouseEvents = YES;
        }
        // Accessory host is not a key window under normal product path; do not
        // call resignKeyWindow / activateIgnoringOtherApps (would fight Terminal).
        return;
    }

    // Logical focus true: prefer web view as first responder without stealing
    // app activation or forcing TSHostWindow to become key (canBecomeKeyWindow
    // remains NO outside PDF probe/copy-bridge modes).
    if (contents->window && contents->web_view) {
        // Keep accessory pass-through by default. Smoke may set
        // ASTROHACKER_WEBKIT_ALLOW_HOST_MOUSE=1 for synthetic hit testing.
        if ([NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_ALLOW_HOST_MOUSE"] isEqualToString:@"1"])
            contents->window.ignoresMouseEvents = NO;
        [contents->window makeFirstResponder:contents->web_view];
    }
}

bool ts_web_contents_is_focused(ts_web_contents_t wc)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents)
        return false;
    return contents->focused;
}

void ts_set_presentation_visible(ts_web_contents_t wc, bool visible)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents || !contents->web_view)
        return;

    if (!contents->live_context_id)
        exportContext(contents);

    contents->presentation_visible = visible;
    [contents->web_view _setTermSurfExternalPresentationVisible:visible];
    fprintf(stderr,
        "[libtermsurf_webkit] presentation-visible tab_id=%d visible=%d context_id=%u\n",
        contents->tab_id,
        visible ? 1 : 0,
        contents->live_context_id);
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

void ts_set_on_navigation_state(ts_navigation_state_cb cb, void *user_data)
{
    g_callbacks.on_navigation_state = cb;
    g_callbacks.on_navigation_state_data = user_data;
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

extern "C" int ts_webkit_test_write_pdf_save(
    const char *download_dir,
    const char *suggested_filename,
    const uint8_t *data,
    size_t data_len,
    char *out_path,
    size_t out_path_len)
{
    if (!download_dir || !*download_dir || !data || !data_len)
        return 0;

    NSString *previousDownloadDir = NSProcessInfo.processInfo.environment[@"ASTROHACKER_WEBKIT_DOWNLOAD_DIR"];
    setenv("ASTROHACKER_WEBKIT_DOWNLOAD_DIR", download_dir, 1);
    NSData *pdfData = [NSData dataWithBytes:data length:data_len];
    NSError *error = nil;
    NSString *savedPath = savePdfDataToDownloads(pdfData, stringFromCString(suggested_filename), &error);
    if (previousDownloadDir.length)
        setenv("ASTROHACKER_WEBKIT_DOWNLOAD_DIR", previousDownloadDir.UTF8String, 1);
    else
        unsetenv("ASTROHACKER_WEBKIT_DOWNLOAD_DIR");

    if (!savedPath.length)
        return 0;
    if (out_path && out_path_len) {
        const char *utf8 = savedPath.UTF8String;
        size_t len = strlen(utf8);
        if (len >= out_path_len)
            len = out_path_len - 1;
        memcpy(out_path, utf8, len);
        out_path[len] = '\0';
    }
    return 1;
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

extern "C" int ts_webkit_test_host_ignores_mouse_events(ts_web_contents_t wc)
{
    WebContents *contents = static_cast<WebContents *>(wc);
    if (!contents || !contents->window)
        return -1;
    return contents->window.ignoresMouseEvents ? 1 : 0;
}
