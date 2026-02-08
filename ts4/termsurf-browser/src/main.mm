#import <Metal/Metal.h>
#import <IOSurface/IOSurface.h>
#import <CoreFoundation/CoreFoundation.h>
#include <xpc/xpc.h>
#include <dispatch/dispatch.h>
#include <cstdio>
#include <mach/mach.h>

int main() {
    fprintf(stderr, "[Browser] Starting...\n");

    // Step 1: Create IOSurface (BGRA8, 800x600)
    int width = 800;
    int height = 600;
    int bytesPerElement = 4;
    int bytesPerRow = width * bytesPerElement;
    OSType pixelFormat = 'BGRA';

    NSDictionary *properties = @{
        (id)kIOSurfaceWidth: @(width),
        (id)kIOSurfaceHeight: @(height),
        (id)kIOSurfaceBytesPerElement: @(bytesPerElement),
        (id)kIOSurfaceBytesPerRow: @(bytesPerRow),
        (id)kIOSurfacePixelFormat: @(pixelFormat),
    };

    IOSurfaceRef surface = IOSurfaceCreate((__bridge CFDictionaryRef)properties);
    if (!surface) {
        fprintf(stderr, "[Browser] ERROR: Failed to create IOSurface\n");
        return 1;
    }

    fprintf(stderr, "[Browser] IOSurface created: %zux%zu\n",
            IOSurfaceGetWidth(surface),
            IOSurfaceGetHeight(surface));

    // Step 2: Create Metal device and render green
    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    if (!device) {
        fprintf(stderr, "[Browser] ERROR: No Metal device available\n");
        return 1;
    }

    id<MTLCommandQueue> commandQueue = [device newCommandQueue];
    fprintf(stderr, "[Browser] Metal device: %s\n", [[device name] UTF8String]);

    // Create Metal texture backed by the IOSurface (zero-copy)
    MTLTextureDescriptor *texDesc = [MTLTextureDescriptor
        texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm
                                     width:width
                                    height:height
                                 mipmapped:NO];
    texDesc.usage = MTLTextureUsageRenderTarget;
    texDesc.storageMode = MTLStorageModeShared;

    id<MTLTexture> texture = [device newTextureWithDescriptor:texDesc
                                                    iosurface:surface
                                                        plane:0];
    if (!texture) {
        fprintf(stderr, "[Browser] ERROR: Failed to create Metal texture from IOSurface\n");
        return 1;
    }

    // Render clear-to-green
    MTLRenderPassDescriptor *passDesc = [MTLRenderPassDescriptor renderPassDescriptor];
    passDesc.colorAttachments[0].texture = texture;
    passDesc.colorAttachments[0].loadAction = MTLLoadActionClear;
    passDesc.colorAttachments[0].storeAction = MTLStoreActionStore;
    passDesc.colorAttachments[0].clearColor = MTLClearColorMake(0.0, 1.0, 0.0, 1.0);

    id<MTLCommandBuffer> commandBuffer = [commandQueue commandBuffer];
    id<MTLRenderCommandEncoder> encoder =
        [commandBuffer renderCommandEncoderWithDescriptor:passDesc];
    [encoder endEncoding];
    [commandBuffer commit];
    [commandBuffer waitUntilCompleted];

    // Verify pixel
    IOSurfaceLock(surface, kIOSurfaceLockReadOnly, nullptr);
    auto *base = static_cast<const uint8_t *>(IOSurfaceGetBaseAddress(surface));
    uint8_t b0 = base[0], g0 = base[1], r0 = base[2], a0 = base[3];
    IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly, nullptr);
    fprintf(stderr, "[Browser] Rendered green, pixel (0,0): (%u, %u, %u, %u)\n", r0, g0, b0, a0);

    // Step 3: Set up XPC listener
    const char *service_name = "com.termsurf.ts4.browser";
    xpc_connection_t listener = xpc_connection_create_mach_service(
        service_name,
        dispatch_get_main_queue(),
        XPC_CONNECTION_MACH_SERVICE_LISTENER
    );

    if (!listener) {
        fprintf(stderr, "[Browser] Failed to create XPC listener\n");
        return 1;
    }

    xpc_connection_set_event_handler(listener, ^(xpc_object_t peer) {
        if (xpc_get_type(peer) == XPC_TYPE_ERROR) {
            fprintf(stderr, "[Browser] Listener error\n");
            return;
        }

        fprintf(stderr, "[Browser] New client connected\n");

        xpc_connection_set_event_handler(peer, ^(xpc_object_t event) {
            if (xpc_get_type(event) == XPC_TYPE_ERROR) {
                if (event == XPC_ERROR_CONNECTION_INVALID) {
                    fprintf(stderr, "[Browser] Client disconnected\n");
                }
                return;
            }

            if (xpc_get_type(event) != XPC_TYPE_DICTIONARY) return;

            const char *action = xpc_dictionary_get_string(event, "action");
            if (action) {
                fprintf(stderr, "[Browser] Received: %s\n", action);
            }
        });
        xpc_connection_resume(peer);

        // Create Mach port and send frame
        mach_port_t port = IOSurfaceCreateMachPort(surface);
        fprintf(stderr, "[Browser] Created Mach port: %u\n", port);

        xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
        xpc_dictionary_set_string(msg, "action", "frame");
        xpc_dictionary_set_mach_send(msg, "iosurface_port", port);
        xpc_dictionary_set_uint64(msg, "width", (uint64_t)width);
        xpc_dictionary_set_uint64(msg, "height", (uint64_t)height);
        xpc_connection_send_message(peer, msg);
        // msg is released by ARC (xpc objects are Obj-C objects under ARC)

        fprintf(stderr, "[Browser] Frame sent: %dx%d\n", width, height);
    });

    xpc_connection_resume(listener);
    fprintf(stderr, "[Browser] Listening on %s\n", service_name);

    // Block forever, processing XPC events
    dispatch_main();
}
