#import <Cocoa/Cocoa.h>
#import <QuartzCore/QuartzCore.h>
#import <WebKit/WebKit.h>

@interface CAContext : NSObject
+ (instancetype)remoteContextWithOptions:(NSDictionary *)options;
@property(nonatomic, readonly) uint32_t contextId;
@property(nonatomic, retain) CALayer *layer;
- (void)invalidate;
@end

@interface CALayerHost : CALayer
@property(nonatomic) uint32_t contextId;
@end

static NSString *proofDirectory(void)
{
    NSString *exe = [[NSBundle mainBundle] executablePath];
    return [[exe stringByDeletingLastPathComponent] stringByDeletingLastPathComponent];
}

static NSURL *testPageURL(void)
{
    NSString *path = [proofDirectory() stringByAppendingPathComponent:@"test-content/index.html"];
    return [NSURL fileURLWithPath:path];
}

static NSURL *navigationPageURL(void)
{
    NSString *path = [proofDirectory() stringByAppendingPathComponent:@"test-content/navigation.html"];
    return [NSURL fileURLWithPath:path];
}

@interface HostDelegate : NSObject <NSApplicationDelegate>
@property(nonatomic) uint32_t contextId;
@property(nonatomic) BOOL stressMode;
@property(nonatomic, strong) NSWindow *window;
@end

@implementation HostDelegate
- (instancetype)initWithContextId:(uint32_t)contextId stressMode:(BOOL)stressMode
{
    self = [super init];
    if (self) {
        _contextId = contextId;
        _stressMode = stressMode;
    }
    return self;
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification
{
    (void)notification;

    NSRect frame = NSMakeRect(760, 180, 720, 560);
    self.window = [[NSWindow alloc] initWithContentRect:frame styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable) backing:NSBackingStoreBuffered defer:NO];
    self.window.title = [NSString stringWithFormat:@"Host process rendering context %u", self.contextId];

    NSView *contentView = self.window.contentView;
    contentView.wantsLayer = YES;
    contentView.layer.backgroundColor = NSColor.blackColor.CGColor;

    CALayerHost *hostLayer = [CALayerHost layer];
    hostLayer.contextId = self.contextId;
    hostLayer.frame = contentView.bounds;
    hostLayer.autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable;
    hostLayer.backgroundColor = NSColor.darkGrayColor.CGColor;
    [contentView.layer addSublayer:hostLayer];

    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];

    NSLog(@"HOST_READY pid=%d context_id=%u host_has_no_wkwebview=1", getpid(), self.contextId);

    if (self.stressMode)
        [self scheduleStressLifecycle];
}

- (void)scheduleStressLifecycle
{
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(3.2 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self resizeHostWindow];
    });

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(5.4 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self runHideShowCycle:1];
    });

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(7.0 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self runHideShowCycle:2];
    });

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(8.6 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        NSLog(@"HOST_FINAL_VISIBLE pid=%d context_id=%u visible=%d", getpid(), self.contextId, self.window.visible);
    });

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(10.0 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        NSLog(@"HOST_TERMINATING pid=%d context_id=%u", getpid(), self.contextId);
        [NSApp terminate:nil];
    });
}

- (void)resizeHostWindow
{
    NSRect frame = self.window.frame;
    frame.size = NSMakeSize(820, 620);
    [self.window setFrame:frame display:YES animate:NO];
    NSLog(@"HOST_RESIZED pid=%d size=%0.0fx%0.0f", getpid(), self.window.contentView.bounds.size.width, self.window.contentView.bounds.size.height);
}

- (void)runHideShowCycle:(NSInteger)cycle
{
    [self.window orderOut:nil];
    NSLog(@"HOST_HIDDEN pid=%d cycle=%ld", getpid(), (long)cycle);

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.55 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self.window makeKeyAndOrderFront:nil];
        NSLog(@"HOST_HIDE_SHOW_CYCLE_%ld_COMPLETE pid=%d visible=%d", (long)cycle, getpid(), self.window.visible);
    });
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender
{
    (void)sender;
    return !self.stressMode;
}
@end

@interface OwnerDelegate : NSObject <NSApplicationDelegate, WKNavigationDelegate, WKScriptMessageHandler>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) WKWebView *webView;
@property(nonatomic, strong) CAContext *remoteContext;
@property(nonatomic, strong) NSTask *hostTask;
@property(nonatomic) BOOL exportedInitialContext;
@property(nonatomic) BOOL stressMode;
@end

@implementation OwnerDelegate
- (void)applicationDidFinishLaunching:(NSNotification *)notification
{
    (void)notification;

    NSRect frame = NSMakeRect(40, 180, 720, 560);
    self.window = [[NSWindow alloc] initWithContentRect:frame styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable) backing:NSBackingStoreBuffered defer:NO];
    self.window.title = @"Owner process WKWebView";

    WKWebViewConfiguration *configuration = [[WKWebViewConfiguration alloc] init];
    [configuration.userContentController addScriptMessageHandler:self name:@"proof"];
    self.webView = [[WKWebView alloc] initWithFrame:self.window.contentView.bounds configuration:configuration];
    self.webView.navigationDelegate = self;
    self.webView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    self.webView.wantsLayer = YES;
    [self.window.contentView addSubview:self.webView];
    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];

    NSLog(@"OWNER_LOADING pid=%d url=%@", getpid(), testPageURL().path);
    [self.webView loadFileURL:testPageURL() allowingReadAccessToURL:[NSURL fileURLWithPath:proofDirectory() isDirectory:YES]];
}

- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation
{
    (void)webView;
    (void)navigation;

    NSLog(@"OWNER_NAVIGATION_FINISHED pid=%d url=%@", getpid(), self.webView.URL.absoluteString);

    if (self.exportedInitialContext)
        return;
    self.exportedInitialContext = YES;

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.8 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self exportContextAndLaunchHost];
    });
}

- (void)exportContextAndLaunchHost
{
    if (self.remoteContext)
        return;

    [self.webView layoutSubtreeIfNeeded];

    self.remoteContext = [CAContext remoteContextWithOptions:@{
        @"kCAContextCIFilterBehavior" : @"ignore",
    }];
    self.remoteContext.layer = self.webView.layer;

    uint32_t contextId = self.remoteContext.contextId;
    NSLog(@"OWNER_EXPORTED_CONTEXT pid=%d context_id=%u webview_layer=%p", getpid(), contextId, self.webView.layer);

    NSString *executablePath = [[NSBundle mainBundle] executablePath];
    NSTask *task = [[NSTask alloc] init];
    task.executableURL = [NSURL fileURLWithPath:executablePath];
    if (self.stressMode)
        task.arguments = @[ @"--host", [NSString stringWithFormat:@"%u", contextId], @"--stress" ];
    else
        task.arguments = @[ @"--host", [NSString stringWithFormat:@"%u", contextId] ];
    task.standardOutput = NSFileHandle.fileHandleWithStandardOutput;
    task.standardError = NSFileHandle.fileHandleWithStandardError;
    task.terminationHandler = ^(NSTask *finishedTask) {
        NSLog(@"OWNER_OBSERVED_HOST_TERMINATION host_pid=%d status=%d", finishedTask.processIdentifier, finishedTask.terminationStatus);
        if (self.stressMode) {
            dispatch_async(dispatch_get_main_queue(), ^{
                NSLog(@"OWNER_TERMINATING pid=%d", getpid());
                [NSApp terminate:nil];
            });
        }
    };

    NSError *error = nil;
    if (![task launchAndReturnError:&error]) {
        NSLog(@"OWNER_HOST_LAUNCH_FAILED error=%@", error);
        return;
    }

    self.hostTask = task;
    NSLog(@"OWNER_LAUNCHED_HOST host_pid=%d context_id=%u", task.processIdentifier, contextId);

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(2.4 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self resizeOwnerWebView];
    });

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(4.2 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
        [self navigateAfterExport];
    });
}

- (void)userContentController:(WKUserContentController *)userContentController didReceiveScriptMessage:(WKScriptMessage *)message
{
    (void)userContentController;
    NSLog(@"OWNER_SCRIPT_MESSAGE pid=%d name=%@ body=%@", getpid(), message.name, message.body);
}

- (void)resizeOwnerWebView
{
    NSRect frame = self.window.frame;
    frame.size = NSMakeSize(620, 420);
    [self.window setFrame:frame display:YES animate:NO];
    self.webView.frame = self.window.contentView.bounds;
    [self.webView layoutSubtreeIfNeeded];
    NSLog(@"OWNER_RESIZED_WEBVIEW pid=%d size=%0.0fx%0.0f", getpid(), self.webView.bounds.size.width, self.webView.bounds.size.height);
}

- (void)navigateAfterExport
{
    NSLog(@"OWNER_NAVIGATING_AFTER_EXPORT pid=%d url=%@", getpid(), navigationPageURL().path);
    [self.webView loadFileURL:navigationPageURL() allowingReadAccessToURL:[NSURL fileURLWithPath:proofDirectory() isDirectory:YES]];
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender
{
    (void)sender;
    if (self.hostTask.running)
        [self.hostTask terminate];
    [self.remoteContext invalidate];
    return YES;
}
@end

static void runHost(uint32_t contextId, BOOL stressMode)
{
    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        [application setActivationPolicy:NSApplicationActivationPolicyRegular];
        HostDelegate *delegate = [[HostDelegate alloc] initWithContextId:contextId stressMode:stressMode];
        application.delegate = delegate;
        [application run];
    }
}

static void runOwner(BOOL stressMode)
{
    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        [application setActivationPolicy:NSApplicationActivationPolicyRegular];
        OwnerDelegate *delegate = [[OwnerDelegate alloc] init];
        delegate.stressMode = stressMode;
        application.delegate = delegate;
        [application run];
    }
}

int main(int argc, const char *argv[])
{
    if ((argc == 3 || argc == 4) && strcmp(argv[1], "--host") == 0) {
        uint32_t contextId = (uint32_t)strtoul(argv[2], NULL, 10);
        if (!contextId) {
            fprintf(stderr, "invalid context id: %s\n", argv[2]);
            return 2;
        }
        BOOL stressMode = argc == 4 && strcmp(argv[3], "--stress") == 0;
        if (argc == 4 && !stressMode) {
            fprintf(stderr, "unknown host argument: %s\n", argv[3]);
            return 2;
        }
        runHost(contextId, stressMode);
        return 0;
    }

    if ((argc == 2 || argc == 3) && strcmp(argv[1], "--owner") == 0) {
        BOOL stressMode = argc == 3 && strcmp(argv[2], "--stress") == 0;
        if (argc == 3 && !stressMode) {
            fprintf(stderr, "unknown owner argument: %s\n", argv[2]);
            return 2;
        }
        runOwner(stressMode);
        return 0;
    }

    fprintf(stderr, "usage: %s --owner [--stress] | --host <context-id> [--stress]\n", argv[0]);
    return 2;
}
