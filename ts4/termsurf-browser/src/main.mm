#import <Metal/Metal.h>
#import <IOSurface/IOSurface.h>
#import <CoreFoundation/CoreFoundation.h>
#include <cstdio>
#include <mach/mach.h>

int main() {
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
        fprintf(stderr, "ERROR: Failed to create IOSurface\n");
        return 1;
    }

    printf("IOSurface created: %zux%zu\n",
           IOSurfaceGetWidth(surface),
           IOSurfaceGetHeight(surface));

    // Step 2: Create Metal device and command queue
    id<MTLDevice> device = MTLCreateSystemDefaultDevice();
    if (!device) {
        fprintf(stderr, "ERROR: No Metal device available\n");
        return 1;
    }

    id<MTLCommandQueue> commandQueue = [device newCommandQueue];
    printf("Metal device: %s\n", [[device name] UTF8String]);

    // Step 3: Create Metal texture backed by the IOSurface
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
        fprintf(stderr, "ERROR: Failed to create Metal texture from IOSurface\n");
        return 1;
    }

    // Step 4: Render a clear-to-green pass
    MTLRenderPassDescriptor *passDesc = [MTLRenderPassDescriptor renderPassDescriptor];
    passDesc.colorAttachments[0].texture = texture;
    passDesc.colorAttachments[0].loadAction = MTLLoadActionClear;
    passDesc.colorAttachments[0].storeAction = MTLStoreActionStore;
    passDesc.colorAttachments[0].clearColor = MTLClearColorMake(0.0, 1.0, 0.0, 1.0);

    id<MTLCommandBuffer> commandBuffer = [commandQueue commandBuffer];
    id<MTLRenderCommandEncoder> encoder =
        [commandBuffer renderCommandEncoderWithDescriptor:passDesc];

    // Empty pass — just clears to green
    [encoder endEncoding];
    [commandBuffer commit];
    [commandBuffer waitUntilCompleted];

    printf("Rendered green (0, 255, 0, 255)\n");

    // Step 5: Read back pixels from IOSurface and verify
    IOSurfaceLock(surface, kIOSurfaceLockReadOnly, nullptr);

    auto *base = static_cast<const uint8_t *>(IOSurfaceGetBaseAddress(surface));
    size_t stride = IOSurfaceGetBytesPerRow(surface);

    // Read pixel at (0,0) — BGRA format
    uint8_t b0 = base[0];
    uint8_t g0 = base[1];
    uint8_t r0 = base[2];
    uint8_t a0 = base[3];

    if (r0 == 0 && g0 == 255 && b0 == 0 && a0 == 255) {
        printf("Pixel at (0,0): (%u, %u, %u, %u) ✓\n", r0, g0, b0, a0);
    } else {
        printf("Pixel at (0,0): (%u, %u, %u, %u) ✗ (expected 0, 255, 0, 255)\n",
               r0, g0, b0, a0);
    }

    // Check middle pixel
    size_t midOffset = 300 * stride + 400 * bytesPerElement;
    uint8_t bm = base[midOffset];
    uint8_t gm = base[midOffset + 1];
    uint8_t rm = base[midOffset + 2];
    uint8_t am = base[midOffset + 3];
    printf("Pixel at (400,300): (%u, %u, %u, %u)\n", rm, gm, bm, am);

    IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly, nullptr);

    // Step 6: Create Mach port from IOSurface
    mach_port_t port = IOSurfaceCreateMachPort(surface);
    printf("Mach port: %u\n", port);

    if (port != 0) {
        printf("Phase 4 complete: IOSurface + Metal + Mach port verified\n");
    } else {
        printf("ERROR: Mach port creation failed\n");
    }

    CFRelease(surface);
    return 0;
}
