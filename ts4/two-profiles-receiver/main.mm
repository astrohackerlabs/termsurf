// Copyright 2025 TermSurf
// Metal receiver: receives IOSurface Mach ports via XPC and renders them.
// Part of Issue 414 Experiment 7: two profile servers side by side.

#import <Cocoa/Cocoa.h>
#import <IOSurface/IOSurface.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <CoreVideo/CVDisplayLink.h>
#import <mach/mach.h>
#import <xpc/xpc.h>

#include <atomic>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <mutex>
#include <vector>

// --- Pane indices ---

enum Pane { LEFT = 0, RIGHT = 1, PANE_COUNT = 2 };

// --- XPC state (must be static to prevent ARC from releasing them) ---

static xpc_connection_t g_listener = nil;
static std::vector<xpc_connection_t> g_peers;

// --- Shared state between XPC thread and render thread ---

static std::mutex g_surface_mutex;
static IOSurfaceRef g_pending_surface[PANE_COUNT] = { nullptr, nullptr };
static int g_frame_count[PANE_COUNT] = { 0, 0 };
static struct timespec g_last_log_time;

// --- Metal state ---

static id<MTLDevice> g_device = nil;
static id<MTLCommandQueue> g_command_queue = nil;
static id<MTLRenderPipelineState> g_pipeline = nil;
static id<MTLSamplerState> g_sampler = nil;
static CAMetalLayer *g_metal_layer = nil;
static id<MTLTexture> g_current_texture[PANE_COUNT] = { nil, nil };

// --- Session ID → pane mapping ---

static Pane pane_for_session(const char *session_id) {
    if (session_id && strcmp(session_id, "profile-b") == 0)
        return RIGHT;
    return LEFT;  // default: profile-a or unknown → left
}

// --- XPC message handler ---

static void handle_message(xpc_object_t msg) {
    const char *action = xpc_dictionary_get_string(msg, "action");
    if (!action)
        return;

    if (strcmp(action, "display_surface") == 0) {
        mach_port_t port = xpc_dictionary_copy_mach_send(msg, "iosurface_port");
        if (port == MACH_PORT_NULL) {
            fprintf(stderr, "[Receiver] null Mach port\n");
            return;
        }

        IOSurfaceRef surface = IOSurfaceLookupFromMachPort(port);
        mach_port_deallocate(mach_task_self(), port);

        if (!surface) {
            fprintf(stderr, "[Receiver] IOSurfaceLookupFromMachPort failed\n");
            return;
        }

        // Map session_id to pane.
        const char *session_id = xpc_dictionary_get_string(msg, "session_id");
        Pane pane = pane_for_session(session_id);

        // Swap in the new surface for this pane.
        {
            std::lock_guard<std::mutex> lock(g_surface_mutex);
            if (g_pending_surface[pane])
                CFRelease(g_pending_surface[pane]);
            g_pending_surface[pane] = surface;
        }

        // FPS logging (per-pane counts, single log line).
        g_frame_count[pane]++;
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        double elapsed = (now.tv_sec - g_last_log_time.tv_sec) +
                         (now.tv_nsec - g_last_log_time.tv_nsec) / 1e9;
        if (elapsed >= 1.0) {
            size_t w = IOSurfaceGetWidth(surface);
            size_t h = IOSurfaceGetHeight(surface);
            double fps_l = g_frame_count[LEFT] / elapsed;
            double fps_r = g_frame_count[RIGHT] / elapsed;
            fprintf(stderr,
                    "[Receiver] L: %d (%.1f fps) R: %d (%.1f fps) | "
                    "IOSurface %zux%zu\n",
                    g_frame_count[LEFT], fps_l,
                    g_frame_count[RIGHT], fps_r, w, h);
            g_frame_count[LEFT] = 0;
            g_frame_count[RIGHT] = 0;
            g_last_log_time = now;
        }
    } else if (strcmp(action, "register") == 0) {
        const char *session_id = xpc_dictionary_get_string(msg, "session_id");
        fprintf(stderr, "[Receiver] Profile server registered: %s\n",
                session_id ? session_id : "(no session_id)");
    }
}

// --- Metal setup ---

static void setup_metal(NSView *view) {
    g_device = MTLCreateSystemDefaultDevice();
    g_command_queue = [g_device newCommandQueue];

    g_metal_layer = [CAMetalLayer layer];
    g_metal_layer.device = g_device;
    g_metal_layer.pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB;
    g_metal_layer.framebufferOnly = NO;
    g_metal_layer.displaySyncEnabled = YES;

    // Render at Retina resolution (2x on HiDPI screens).
    CGFloat scale = [[NSScreen mainScreen] backingScaleFactor];
    g_metal_layer.contentsScale = scale;
    CGSize viewSize = view.bounds.size;
    g_metal_layer.drawableSize = CGSizeMake(
        viewSize.width * scale, viewSize.height * scale);

    [view setWantsLayer:YES];
    [view setLayer:g_metal_layer];

    // Load shaders from metallib next to the binary.
    NSString *path = [[[NSBundle mainBundle] executablePath]
        stringByDeletingLastPathComponent];
    NSString *libPath = [path stringByAppendingPathComponent:@"shaders.metallib"];
    NSError *error = nil;
    id<MTLLibrary> library = [g_device newLibraryWithFile:libPath error:&error];
    if (!library) {
        fprintf(stderr, "[Receiver] Failed to load shaders.metallib: %s\n",
                [[error localizedDescription] UTF8String]);
        exit(1);
    }

    id<MTLFunction> vertexFunc = [library newFunctionWithName:@"vertex_main"];
    id<MTLFunction> fragmentFunc = [library newFunctionWithName:@"fragment_main"];

    MTLRenderPipelineDescriptor *pipelineDesc =
        [[MTLRenderPipelineDescriptor alloc] init];
    pipelineDesc.vertexFunction = vertexFunc;
    pipelineDesc.fragmentFunction = fragmentFunc;
    pipelineDesc.colorAttachments[0].pixelFormat = MTLPixelFormatBGRA8Unorm_sRGB;

    g_pipeline = [g_device newRenderPipelineStateWithDescriptor:pipelineDesc
                                                         error:&error];
    if (!g_pipeline) {
        fprintf(stderr, "[Receiver] Failed to create pipeline: %s\n",
                [[error localizedDescription] UTF8String]);
        exit(1);
    }

    MTLSamplerDescriptor *samplerDesc = [[MTLSamplerDescriptor alloc] init];
    samplerDesc.magFilter = MTLSamplerMinMagFilterLinear;
    samplerDesc.minFilter = MTLSamplerMinMagFilterLinear;
    g_sampler = [g_device newSamplerStateWithDescriptor:samplerDesc];
}

// --- Render one frame ---

static void render_frame() {
    // Grab the latest IOSurface for each pane.
    IOSurfaceRef surfaces[PANE_COUNT] = { nullptr, nullptr };
    {
        std::lock_guard<std::mutex> lock(g_surface_mutex);
        for (int i = 0; i < PANE_COUNT; i++) {
            surfaces[i] = g_pending_surface[i];
            if (surfaces[i])
                CFRetain(surfaces[i]);
        }
    }

    // Update Metal textures from new IOSurfaces.
    for (int i = 0; i < PANE_COUNT; i++) {
        if (surfaces[i]) {
            MTLTextureDescriptor *desc = [MTLTextureDescriptor
                texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm_sRGB
                                            width:IOSurfaceGetWidth(surfaces[i])
                                           height:IOSurfaceGetHeight(surfaces[i])
                                        mipmapped:NO];
            desc.usage = MTLTextureUsageShaderRead;
            id<MTLTexture> newTexture = [g_device newTextureWithDescriptor:desc
                                                                iosurface:surfaces[i]
                                                                    plane:0];
            CFRelease(surfaces[i]);

            if (newTexture)
                g_current_texture[i] = newTexture;
        }
    }

    // Need at least one texture to render.
    if (!g_current_texture[LEFT] && !g_current_texture[RIGHT])
        return;

    id<CAMetalDrawable> drawable = [g_metal_layer nextDrawable];
    if (!drawable)
        return;

    MTLRenderPassDescriptor *passDesc = [MTLRenderPassDescriptor renderPassDescriptor];
    passDesc.colorAttachments[0].texture = drawable.texture;
    passDesc.colorAttachments[0].loadAction = MTLLoadActionClear;
    passDesc.colorAttachments[0].storeAction = MTLStoreActionStore;
    passDesc.colorAttachments[0].clearColor = MTLClearColorMake(0, 0, 0, 1);

    id<MTLCommandBuffer> cmdBuf = [g_command_queue commandBuffer];
    id<MTLRenderCommandEncoder> encoder =
        [cmdBuf renderCommandEncoderWithDescriptor:passDesc];

    [encoder setRenderPipelineState:g_pipeline];
    [encoder setFragmentSamplerState:g_sampler atIndex:0];

    double drawableW = g_metal_layer.drawableSize.width;
    double drawableH = g_metal_layer.drawableSize.height;
    double halfW = drawableW / 2.0;

    // Left pane (profile-a).
    if (g_current_texture[LEFT]) {
        MTLViewport vp = { 0, 0, halfW, drawableH, 0, 1 };
        [encoder setViewport:vp];
        [encoder setFragmentTexture:g_current_texture[LEFT] atIndex:0];
        [encoder drawPrimitives:MTLPrimitiveTypeTriangleStrip
                    vertexStart:0
                    vertexCount:4];
    }

    // Right pane (profile-b).
    if (g_current_texture[RIGHT]) {
        MTLViewport vp = { halfW, 0, halfW, drawableH, 0, 1 };
        [encoder setViewport:vp];
        [encoder setFragmentTexture:g_current_texture[RIGHT] atIndex:0];
        [encoder drawPrimitives:MTLPrimitiveTypeTriangleStrip
                    vertexStart:0
                    vertexCount:4];
    }

    [encoder endEncoding];

    [cmdBuf presentDrawable:drawable];
    [cmdBuf commit];
}

// --- CVDisplayLink callback ---

static CVReturn display_link_callback(CVDisplayLinkRef displayLink,
                                       const CVTimeStamp *now,
                                       const CVTimeStamp *outputTime,
                                       CVOptionFlags flagsIn,
                                       CVOptionFlags *flagsOut,
                                       void *context) {
    @autoreleasepool {
        render_frame();
    }
    return kCVReturnSuccess;
}

// --- XPC listener setup ---

static void start_xpc_listener() {
    clock_gettime(CLOCK_MONOTONIC, &g_last_log_time);

    dispatch_queue_t queue = dispatch_queue_create(
        "com.termsurf.two-profiles.xpc", DISPATCH_QUEUE_SERIAL);

    g_listener = xpc_connection_create_mach_service(
        "com.termsurf.two-profiles", queue,
        XPC_CONNECTION_MACH_SERVICE_LISTENER);

    if (!g_listener) {
        fprintf(stderr, "[Receiver] Failed to create Mach service listener\n");
        exit(1);
    }

    xpc_connection_set_event_handler(g_listener, ^(xpc_object_t peer) {
        if (xpc_get_type(peer) == XPC_TYPE_CONNECTION) {
            fprintf(stderr, "[Receiver] Profile server connected (%zu total)\n",
                    g_peers.size() + 1);
            xpc_connection_t peer_conn = (xpc_connection_t)peer;
            g_peers.push_back(peer_conn);
            xpc_connection_set_event_handler(
                peer_conn, ^(xpc_object_t event) {
                    if (xpc_get_type(event) == XPC_TYPE_DICTIONARY) {
                        handle_message(event);
                    } else if (xpc_get_type(event) == XPC_TYPE_ERROR) {
                        if (event == XPC_ERROR_CONNECTION_INVALID)
                            fprintf(stderr, "[Receiver] Connection closed\n");
                        else
                            fprintf(stderr, "[Receiver] XPC error\n");
                    }
                });
            xpc_connection_resume(peer_conn);
        } else if (xpc_get_type(peer) == XPC_TYPE_ERROR) {
            fprintf(stderr, "[Receiver] Listener error\n");
        }
    });

    xpc_connection_resume(g_listener);
    fprintf(stderr, "[Receiver] Listening on com.termsurf.two-profiles...\n");
}

// --- App delegate ---

@interface ReceiverAppDelegate : NSObject <NSApplicationDelegate>
@end

@implementation ReceiverAppDelegate {
    NSWindow *_window;
    CVDisplayLinkRef _displayLink;
}

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    // Create window: 1600x600 logical = two 800x600 panes side by side.
    NSRect frame = NSMakeRect(100, 100, 1600, 600);
    NSUInteger style = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                       NSWindowStyleMaskResizable;
    _window = [[NSWindow alloc] initWithContentRect:frame
                                          styleMask:style
                                            backing:NSBackingStoreBuffered
                                              defer:NO];
    _window.title = @"Two Profiles Receiver";
    [_window makeKeyAndOrderFront:nil];

    // Setup Metal on the content view.
    setup_metal(_window.contentView);

    // Start CVDisplayLink for vsync-driven rendering.
    CVDisplayLinkCreateWithActiveCGDisplays(&_displayLink);
    CVDisplayLinkSetOutputCallback(_displayLink, display_link_callback, nullptr);
    CVDisplayLinkStart(_displayLink);

    fprintf(stderr, "[Receiver] Window and Metal pipeline ready\n");
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    if (_displayLink) {
        CVDisplayLinkStop(_displayLink);
        CVDisplayLinkRelease(_displayLink);
    }
}

- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender {
    return YES;
}

@end

// --- main ---

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        // Start XPC listener first — before NSApplication — so it's ready
        // the instant launchd delivers the pending connection.
        start_xpc_listener();

        NSApplication *app = [NSApplication sharedApplication];
        [app setActivationPolicy:NSApplicationActivationPolicyRegular];
        ReceiverAppDelegate *delegate = [[ReceiverAppDelegate alloc] init];
        app.delegate = delegate;
        [app activateIgnoringOtherApps:YES];
        [app run];
    }
    return 0;
}
