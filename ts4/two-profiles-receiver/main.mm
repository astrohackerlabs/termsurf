// Copyright 2025 TermSurf
// Metal receiver: receives IOSurface Mach ports via XPC and renders them.
// Part of Issue 414 Experiment 3: hidden sender, visible receiver.

#import <Cocoa/Cocoa.h>
#import <IOSurface/IOSurface.h>
#import <Metal/Metal.h>
#import <QuartzCore/CAMetalLayer.h>
#import <CoreVideo/CVDisplayLink.h>
#import <mach/mach.h>
#import <xpc/xpc.h>

#include <atomic>
#include <cstdio>
#include <ctime>
#include <mutex>

// --- XPC state (must be static to prevent ARC from releasing them) ---

static xpc_connection_t g_listener = nil;
static xpc_connection_t g_peer = nil;

// --- Shared state between XPC thread and render thread ---

static std::mutex g_surface_mutex;
static IOSurfaceRef g_pending_surface = nullptr;
static int g_frame_count = 0;
static struct timespec g_last_log_time;

// --- Metal state ---

static id<MTLDevice> g_device = nil;
static id<MTLCommandQueue> g_command_queue = nil;
static id<MTLRenderPipelineState> g_pipeline = nil;
static id<MTLSamplerState> g_sampler = nil;
static CAMetalLayer *g_metal_layer = nil;
static id<MTLTexture> g_current_texture = nil;

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

        // Swap in the new surface.
        {
            std::lock_guard<std::mutex> lock(g_surface_mutex);
            if (g_pending_surface)
                CFRelease(g_pending_surface);
            g_pending_surface = surface;
        }

        // FPS logging.
        g_frame_count++;
        struct timespec now;
        clock_gettime(CLOCK_MONOTONIC, &now);
        double elapsed = (now.tv_sec - g_last_log_time.tv_sec) +
                         (now.tv_nsec - g_last_log_time.tv_nsec) / 1e9;
        if (elapsed >= 1.0) {
            size_t w = IOSurfaceGetWidth(surface);
            size_t h = IOSurfaceGetHeight(surface);
            fprintf(stderr,
                    "[Receiver] %d frames in %.2fs (%.1f fps) | IOSurface %zux%zu\n",
                    g_frame_count, elapsed, g_frame_count / elapsed, w, h);
            g_frame_count = 0;
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
    // Check for a new IOSurface.
    IOSurfaceRef surface = nullptr;
    {
        std::lock_guard<std::mutex> lock(g_surface_mutex);
        surface = g_pending_surface;
        if (surface)
            CFRetain(surface);
    }

    if (surface) {
        // Create a Metal texture from the IOSurface.
        MTLTextureDescriptor *desc = [MTLTextureDescriptor
            texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm_sRGB
                                        width:IOSurfaceGetWidth(surface)
                                       height:IOSurfaceGetHeight(surface)
                                    mipmapped:NO];
        desc.usage = MTLTextureUsageShaderRead;
        id<MTLTexture> newTexture = [g_device newTextureWithDescriptor:desc
                                                            iosurface:surface
                                                                plane:0];
        CFRelease(surface);

        if (newTexture)
            g_current_texture = newTexture;
    }

    if (!g_current_texture)
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
    [encoder setFragmentTexture:g_current_texture atIndex:0];
    [encoder setFragmentSamplerState:g_sampler atIndex:0];
    [encoder drawPrimitives:MTLPrimitiveTypeTriangleStrip
                vertexStart:0
                vertexCount:4];
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
            fprintf(stderr, "[Receiver] Profile server connected\n");
            g_peer = (xpc_connection_t)peer;
            xpc_connection_set_event_handler(
                g_peer, ^(xpc_object_t event) {
                    if (xpc_get_type(event) == XPC_TYPE_DICTIONARY) {
                        handle_message(event);
                    } else if (xpc_get_type(event) == XPC_TYPE_ERROR) {
                        if (event == XPC_ERROR_CONNECTION_INVALID)
                            fprintf(stderr, "[Receiver] Connection closed\n");
                        else
                            fprintf(stderr, "[Receiver] XPC error\n");
                    }
                });
            xpc_connection_resume(g_peer);
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
    // Create window.
    NSRect frame = NSMakeRect(100, 100, 800, 600);
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
