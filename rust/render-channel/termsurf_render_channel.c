#include "termsurf_render_channel.h"

#if defined(__APPLE__) && defined(__MACH__)
#include <TargetConditionals.h>
#endif

#if defined(__APPLE__) && defined(__MACH__) && defined(TARGET_OS_OSX) &&       \
    TARGET_OS_OSX
#include <CoreFoundation/CoreFoundation.h>
#include <IOSurface/IOSurface.h>
#include <mach/mach.h>
#include <servers/bootstrap.h>
#include <stddef.h>
#include <stdlib.h>

typedef struct {
  mach_msg_header_t header;
  mach_msg_body_t body;
  mach_msg_port_descriptor_t port;
} tsrc_port_message_t;

typedef struct {
  tsrc_port_message_t message;
  mach_msg_trailer_t trailer;
} tsrc_port_message_receive_t;

typedef struct {
  mach_msg_header_t header;
  mach_msg_body_t body;
  mach_msg_port_descriptor_t surface_port;
  uint32_t width;
  uint32_t height;
  uint32_t bytes_per_row;
  uint32_t pixel_format;
  uint32_t generation;
  uint64_t attachment_id;
} tsrc_surface_message_t;

typedef struct {
  tsrc_surface_message_t message;
  mach_msg_trailer_t trailer;
} tsrc_surface_message_receive_t;

struct tsrc_received_surface {
  tsrc_surface_metadata_t metadata;
  IOSurfaceRef surface;
};

enum {
  TSRC_TEST_BYTES_PER_ELEMENT = 4,
  TSRC_TEST_PIXEL_FORMAT_BGRA = 0x42475241u,
};

static int tsrc_valid_name(const char *service_name) {
  return service_name != NULL && service_name[0] != '\0';
}

static void tsrc_destroy_receive_right(mach_port_t port) {
  if (!MACH_PORT_VALID(port))
    return;
  mach_port_mod_refs(mach_task_self(), port, MACH_PORT_RIGHT_RECEIVE, -1);
  mach_port_deallocate(mach_task_self(), port);
}

static void tsrc_set_i32(CFMutableDictionaryRef dict, const void *key,
                         int32_t value) {
  CFNumberRef number =
      CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &value);
  CFDictionarySetValue(dict, key, number);
  CFRelease(number);
}

static IOSurfaceRef tsrc_create_test_surface(uint32_t width, uint32_t height) {
  CFMutableDictionaryRef dict = CFDictionaryCreateMutable(
      kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
      &kCFTypeDictionaryValueCallBacks);
  tsrc_set_i32(dict, kIOSurfaceWidth, (int32_t)width);
  tsrc_set_i32(dict, kIOSurfaceHeight, (int32_t)height);
  tsrc_set_i32(dict, kIOSurfaceBytesPerElement, TSRC_TEST_BYTES_PER_ELEMENT);
  tsrc_set_i32(dict, kIOSurfacePixelFormat, TSRC_TEST_PIXEL_FORMAT_BGRA);
  IOSurfaceRef surface = IOSurfaceCreate(dict);
  CFRelease(dict);
  return surface;
}

static int tsrc_send_port_message(mach_port_t remote_port, mach_port_t port,
                                  mach_msg_id_t message_id,
                                  uint32_t timeout_ms) {
  tsrc_port_message_t message = {0};
  message.header.msgh_bits =
      MACH_MSGH_BITS_COMPLEX | MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
  message.header.msgh_size = sizeof(message);
  message.header.msgh_remote_port = remote_port;
  message.header.msgh_local_port = MACH_PORT_NULL;
  message.header.msgh_id = message_id;
  message.body.msgh_descriptor_count = 1;
  message.port.name = port;
  message.port.disposition = MACH_MSG_TYPE_MOVE_SEND;
  message.port.type = MACH_MSG_PORT_DESCRIPTOR;

  kern_return_t kr =
      mach_msg(&message.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT,
               message.header.msgh_size, 0, MACH_PORT_NULL,
               (mach_msg_timeout_t)timeout_ms, MACH_PORT_NULL);
  return kr == KERN_SUCCESS ? TSRC_OK : TSRC_SEND_FAILED;
}

static int tsrc_receive_port_message(mach_port_t receive_port,
                                     mach_msg_id_t message_id,
                                     uint32_t timeout_ms,
                                     mach_port_t *out_port) {
  tsrc_port_message_receive_t received = {0};
  kern_return_t kr =
      mach_msg(&received.message.header, MACH_RCV_MSG | MACH_RCV_TIMEOUT, 0,
               sizeof(received), receive_port, (mach_msg_timeout_t)timeout_ms,
               MACH_PORT_NULL);
  if (kr != KERN_SUCCESS)
    return TSRC_RECEIVE_FAILED;

  tsrc_port_message_t message = received.message;
  if (message.header.msgh_id != message_id)
    return TSRC_BAD_MESSAGE;
  if (message.body.msgh_descriptor_count != 1)
    return TSRC_BAD_MESSAGE;
  if (message.port.type != MACH_MSG_PORT_DESCRIPTOR)
    return TSRC_BAD_MESSAGE;
  if (message.port.disposition != MACH_MSG_TYPE_MOVE_SEND &&
      message.port.disposition != MACH_MSG_TYPE_PORT_SEND)
    return TSRC_BAD_MESSAGE;
  if (!MACH_PORT_VALID(message.port.name))
    return TSRC_BAD_MESSAGE;

  *out_port = message.port.name;
  return TSRC_OK;
}

int tsrc_register_service(const char *service_name,
                          tsrc_port_t *out_control_port) {
  if (!tsrc_valid_name(service_name) || out_control_port == NULL)
    return TSRC_INVALID_ARGUMENT;

  mach_port_t control_port = MACH_PORT_NULL;
  kern_return_t kr = mach_port_allocate(mach_task_self(),
                                        MACH_PORT_RIGHT_RECEIVE, &control_port);
  if (kr != KERN_SUCCESS)
    return TSRC_ALLOCATE_FAILED;

  kr = mach_port_insert_right(mach_task_self(), control_port, control_port,
                              MACH_MSG_TYPE_MAKE_SEND);
  if (kr != KERN_SUCCESS) {
    tsrc_destroy_receive_right(control_port);
    return TSRC_INSERT_RIGHT_FAILED;
  }

  kr = bootstrap_register(bootstrap_port, (char *)service_name, control_port);
  if (kr != KERN_SUCCESS) {
    tsrc_destroy_receive_right(control_port);
    return TSRC_REGISTER_FAILED;
  }

  *out_control_port = (tsrc_port_t)control_port;
  return TSRC_OK;
}

int tsrc_wait_for_child_port(tsrc_port_t control_port_value,
                             uint32_t timeout_ms, tsrc_port_t *out_child_port) {
  if (control_port_value == 0 || out_child_port == NULL)
    return TSRC_INVALID_ARGUMENT;

  mach_port_t child_port = MACH_PORT_NULL;
  int result = tsrc_receive_port_message((mach_port_t)control_port_value,
                                         TSRC_BOOTSTRAP_MESSAGE_ID, timeout_ms,
                                         &child_port);
  if (result == TSRC_OK)
    *out_child_port = (tsrc_port_t)child_port;
  return result;
}

int tsrc_child_connect_and_send(const char *service_name, uint32_t timeout_ms,
                                tsrc_port_t *out_receive_port) {
  if (!tsrc_valid_name(service_name) || out_receive_port == NULL)
    return TSRC_INVALID_ARGUMENT;

  mach_port_t control_send_right = MACH_PORT_NULL;
  kern_return_t kr = bootstrap_look_up(bootstrap_port, (char *)service_name,
                                       &control_send_right);
  if (kr != KERN_SUCCESS)
    return TSRC_LOOKUP_FAILED;

  mach_port_t receive_port = MACH_PORT_NULL;
  kr = mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE,
                          &receive_port);
  if (kr != KERN_SUCCESS) {
    mach_port_deallocate(mach_task_self(), control_send_right);
    return TSRC_ALLOCATE_FAILED;
  }

  kr = mach_port_insert_right(mach_task_self(), receive_port, receive_port,
                              MACH_MSG_TYPE_MAKE_SEND);
  if (kr != KERN_SUCCESS) {
    tsrc_destroy_receive_right(receive_port);
    mach_port_deallocate(mach_task_self(), control_send_right);
    return TSRC_INSERT_RIGHT_FAILED;
  }

  int send_result = tsrc_send_port_message(
      control_send_right, receive_port, TSRC_BOOTSTRAP_MESSAGE_ID, timeout_ms);
  mach_port_deallocate(mach_task_self(), control_send_right);
  if (send_result != TSRC_OK) {
    tsrc_destroy_receive_right(receive_port);
    return send_result;
  }

  *out_receive_port = (tsrc_port_t)receive_port;
  return TSRC_OK;
}

int tsrc_send_surface_receiver(tsrc_port_t child_port_value,
                               uint32_t timeout_ms,
                               tsrc_port_t *out_surface_receive_port) {
  if (child_port_value == 0 || out_surface_receive_port == NULL)
    return TSRC_INVALID_ARGUMENT;

  mach_port_t surface_receive_port = MACH_PORT_NULL;
  kern_return_t kr = mach_port_allocate(
      mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &surface_receive_port);
  if (kr != KERN_SUCCESS)
    return TSRC_ALLOCATE_FAILED;

  kr = mach_port_insert_right(mach_task_self(), surface_receive_port,
                              surface_receive_port, MACH_MSG_TYPE_MAKE_SEND);
  if (kr != KERN_SUCCESS) {
    tsrc_destroy_receive_right(surface_receive_port);
    return TSRC_INSERT_RIGHT_FAILED;
  }

  int result = tsrc_send_port_message(
      (mach_port_t)child_port_value, surface_receive_port,
      TSRC_SURFACE_RECEIVER_MESSAGE_ID, timeout_ms);
  if (result != TSRC_OK) {
    tsrc_destroy_receive_right(surface_receive_port);
    return result;
  }

  *out_surface_receive_port = (tsrc_port_t)surface_receive_port;
  return TSRC_OK;
}

int tsrc_wait_for_surface_receiver(tsrc_port_t child_receive_port_value,
                                   uint32_t timeout_ms,
                                   tsrc_port_t *out_surface_send_port) {
  if (child_receive_port_value == 0 || out_surface_send_port == NULL)
    return TSRC_INVALID_ARGUMENT;

  mach_port_t surface_send_port = MACH_PORT_NULL;
  int result = tsrc_receive_port_message((mach_port_t)child_receive_port_value,
                                         TSRC_SURFACE_RECEIVER_MESSAGE_ID,
                                         timeout_ms, &surface_send_port);
  if (result == TSRC_OK)
    *out_surface_send_port = (tsrc_port_t)surface_send_port;
  return result;
}

int tsrc_send_surface(tsrc_port_t surface_send_port_value,
                      tsrc_port_t exported_surface_port_value, uint32_t width,
                      uint32_t height, uint32_t bytes_per_row,
                      uint32_t pixel_format, uint32_t generation,
                      uint64_t attachment_id, uint32_t timeout_ms) {
  if (surface_send_port_value == 0 || exported_surface_port_value == 0 ||
      width == 0 || height == 0)
    return TSRC_INVALID_ARGUMENT;

  mach_port_t surface_port = (mach_port_t)exported_surface_port_value;

  tsrc_surface_message_t message = {0};
  message.header.msgh_bits =
      MACH_MSGH_BITS_COMPLEX | MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, 0);
  message.header.msgh_size = sizeof(message);
  message.header.msgh_remote_port = (mach_port_t)surface_send_port_value;
  message.header.msgh_local_port = MACH_PORT_NULL;
  message.header.msgh_id = TSRC_TEST_SURFACE_MESSAGE_ID;
  message.body.msgh_descriptor_count = 1;
  message.surface_port.name = surface_port;
  message.surface_port.disposition = MACH_MSG_TYPE_MOVE_SEND;
  message.surface_port.type = MACH_MSG_PORT_DESCRIPTOR;
  message.width = width;
  message.height = height;
  message.bytes_per_row = bytes_per_row;
  message.pixel_format = pixel_format;
  message.generation = generation;
  message.attachment_id = attachment_id;

  kern_return_t kr =
      mach_msg(&message.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT,
               message.header.msgh_size, 0, MACH_PORT_NULL,
               (mach_msg_timeout_t)timeout_ms, MACH_PORT_NULL);
  if (kr != KERN_SUCCESS) {
    mach_port_deallocate(mach_task_self(), surface_port);
    return TSRC_SEND_FAILED;
  }

  return TSRC_OK;
}

int tsrc_send_test_surface(tsrc_port_t surface_send_port_value, uint32_t width,
                           uint32_t height, uint32_t generation,
                           uint32_t timeout_ms) {
  if (surface_send_port_value == 0 || width == 0 || height == 0)
    return TSRC_INVALID_ARGUMENT;

  IOSurfaceRef surface = tsrc_create_test_surface(width, height);
  if (surface == NULL)
    return TSRC_ALLOCATE_FAILED;

  mach_port_t surface_port = IOSurfaceCreateMachPort(surface);
  if (!MACH_PORT_VALID(surface_port)) {
    CFRelease(surface);
    return TSRC_ALLOCATE_FAILED;
  }

  int result = tsrc_send_surface(
      surface_send_port_value, (tsrc_port_t)surface_port, width, height,
      (uint32_t)IOSurfaceGetBytesPerRow(surface),
      (uint32_t)IOSurfaceGetPixelFormat(surface), generation, 0, timeout_ms);
  CFRelease(surface);
  mach_port_deallocate(mach_task_self(), (mach_port_t)surface_send_port_value);
  return result;
}

int tsrc_receive_surface(tsrc_port_t surface_receive_port_value,
                         uint32_t timeout_ms,
                         tsrc_received_surface_t **out_surface) {
  if (surface_receive_port_value == 0 || out_surface == NULL)
    return TSRC_INVALID_ARGUMENT;
  *out_surface = NULL;

  tsrc_surface_message_receive_t received = {0};
  kern_return_t kr =
      mach_msg(&received.message.header, MACH_RCV_MSG | MACH_RCV_TIMEOUT, 0,
               sizeof(received), (mach_port_t)surface_receive_port_value,
               (mach_msg_timeout_t)timeout_ms, MACH_PORT_NULL);
  if (kr != KERN_SUCCESS)
    return TSRC_RECEIVE_FAILED;

  tsrc_surface_message_t message = received.message;
  if (message.header.msgh_id != TSRC_TEST_SURFACE_MESSAGE_ID)
    return TSRC_BAD_MESSAGE;
  if (message.body.msgh_descriptor_count != 1)
    return TSRC_BAD_MESSAGE;
  if (message.surface_port.type != MACH_MSG_PORT_DESCRIPTOR)
    return TSRC_BAD_MESSAGE;
  if (message.surface_port.disposition != MACH_MSG_TYPE_MOVE_SEND &&
      message.surface_port.disposition != MACH_MSG_TYPE_PORT_SEND)
    return TSRC_BAD_MESSAGE;
  if (!MACH_PORT_VALID(message.surface_port.name))
    return TSRC_BAD_MESSAGE;

  IOSurfaceRef imported =
      IOSurfaceLookupFromMachPort(message.surface_port.name);
  if (imported == NULL) {
    mach_port_deallocate(mach_task_self(), message.surface_port.name);
    return TSRC_BAD_MESSAGE;
  }

  tsrc_received_surface_t *surface =
      (tsrc_received_surface_t *)calloc(1, sizeof(tsrc_received_surface_t));
  if (surface == NULL) {
    CFRelease(imported);
    mach_port_deallocate(mach_task_self(), message.surface_port.name);
    return TSRC_ALLOCATE_FAILED;
  }

  surface->metadata.width = message.width;
  surface->metadata.height = message.height;
  surface->metadata.bytes_per_row = message.bytes_per_row;
  surface->metadata.pixel_format = message.pixel_format;
  surface->metadata.generation = message.generation;
  surface->metadata.attachment_id = message.attachment_id;
  surface->metadata.imported_width = (uint32_t)IOSurfaceGetWidth(imported);
  surface->metadata.imported_height = (uint32_t)IOSurfaceGetHeight(imported);
  surface->metadata.imported_bytes_per_row =
      (uint32_t)IOSurfaceGetBytesPerRow(imported);
  surface->metadata.imported_pixel_format =
      (uint32_t)IOSurfaceGetPixelFormat(imported);
  surface->surface = imported;
  mach_port_deallocate(mach_task_self(), message.surface_port.name);
  *out_surface = surface;
  return TSRC_OK;
}

void tsrc_received_surface_metadata(const tsrc_received_surface_t *surface,
                                    tsrc_surface_metadata_t *out_metadata) {
  if (out_metadata == NULL)
    return;
  if (surface == NULL) {
    *out_metadata = (tsrc_surface_metadata_t){0};
    return;
  }
  *out_metadata = surface->metadata;
}

void *tsrc_received_surface_iosurface(const tsrc_received_surface_t *surface) {
  if (surface == NULL)
    return NULL;
  return surface->surface;
}

void *
tsrc_retain_received_surface_iosurface(const tsrc_received_surface_t *surface) {
  if (surface == NULL || surface->surface == NULL)
    return NULL;
  CFRetain(surface->surface);
  return surface->surface;
}

void tsrc_release_iosurface(void *surface) {
  if (surface == NULL)
    return;
  CFRelease(surface);
}

void tsrc_release_received_surface(tsrc_received_surface_t *surface) {
  if (surface == NULL)
    return;
  if (surface->surface != NULL)
    CFRelease(surface->surface);
  free(surface);
}

int tsrc_receive_test_surface(tsrc_port_t surface_receive_port_value,
                              uint32_t timeout_ms,
                              tsrc_surface_metadata_t *out_metadata) {
  if (out_metadata == NULL)
    return TSRC_INVALID_ARGUMENT;
  tsrc_received_surface_t *surface = NULL;
  int result =
      tsrc_receive_surface(surface_receive_port_value, timeout_ms, &surface);
  if (result == TSRC_OK) {
    tsrc_received_surface_metadata(surface, out_metadata);
    tsrc_release_received_surface(surface);
  }
  return result;
}

void tsrc_deallocate_port(tsrc_port_t port) {
  if (port != 0)
    mach_port_deallocate(mach_task_self(), (mach_port_t)port);
}

void tsrc_destroy_receive_port(tsrc_port_t port) {
  tsrc_destroy_receive_right((mach_port_t)port);
}

#else

int tsrc_register_service(const char *service_name,
                          tsrc_port_t *out_control_port) {
  (void)service_name;
  if (out_control_port != 0)
    *out_control_port = 0;
  return TSRC_UNSUPPORTED;
}

int tsrc_wait_for_child_port(tsrc_port_t control_port, uint32_t timeout_ms,
                             tsrc_port_t *out_child_port) {
  (void)control_port;
  (void)timeout_ms;
  if (out_child_port != 0)
    *out_child_port = 0;
  return TSRC_UNSUPPORTED;
}

int tsrc_child_connect_and_send(const char *service_name, uint32_t timeout_ms,
                                tsrc_port_t *out_receive_port) {
  (void)service_name;
  (void)timeout_ms;
  if (out_receive_port != 0)
    *out_receive_port = 0;
  return TSRC_UNSUPPORTED;
}

int tsrc_send_surface_receiver(tsrc_port_t child_port, uint32_t timeout_ms,
                               tsrc_port_t *out_surface_receive_port) {
  (void)child_port;
  (void)timeout_ms;
  if (out_surface_receive_port != 0)
    *out_surface_receive_port = 0;
  return TSRC_UNSUPPORTED;
}

int tsrc_wait_for_surface_receiver(tsrc_port_t child_receive_port,
                                   uint32_t timeout_ms,
                                   tsrc_port_t *out_surface_send_port) {
  (void)child_receive_port;
  (void)timeout_ms;
  if (out_surface_send_port != 0)
    *out_surface_send_port = 0;
  return TSRC_UNSUPPORTED;
}

int tsrc_send_test_surface(tsrc_port_t surface_send_port, uint32_t width,
                           uint32_t height, uint32_t generation,
                           uint32_t timeout_ms) {
  (void)surface_send_port;
  (void)width;
  (void)height;
  (void)generation;
  (void)timeout_ms;
  return TSRC_UNSUPPORTED;
}

int tsrc_send_surface(tsrc_port_t surface_send_port,
                      tsrc_port_t exported_surface_port, uint32_t width,
                      uint32_t height, uint32_t bytes_per_row,
                      uint32_t pixel_format, uint32_t generation,
                      uint64_t attachment_id, uint32_t timeout_ms) {
  (void)surface_send_port;
  (void)exported_surface_port;
  (void)width;
  (void)height;
  (void)bytes_per_row;
  (void)pixel_format;
  (void)generation;
  (void)attachment_id;
  (void)timeout_ms;
  return TSRC_UNSUPPORTED;
}

int tsrc_receive_surface(tsrc_port_t surface_receive_port, uint32_t timeout_ms,
                         tsrc_received_surface_t **out_surface) {
  (void)surface_receive_port;
  (void)timeout_ms;
  if (out_surface != 0)
    *out_surface = 0;
  return TSRC_UNSUPPORTED;
}

void tsrc_received_surface_metadata(const tsrc_received_surface_t *surface,
                                    tsrc_surface_metadata_t *out_metadata) {
  (void)surface;
  if (out_metadata != 0)
    *out_metadata = (tsrc_surface_metadata_t){0};
}

void *tsrc_received_surface_iosurface(const tsrc_received_surface_t *surface) {
  (void)surface;
  return 0;
}

void *
tsrc_retain_received_surface_iosurface(const tsrc_received_surface_t *surface) {
  (void)surface;
  return 0;
}

void tsrc_release_iosurface(void *surface) { (void)surface; }

void tsrc_release_received_surface(tsrc_received_surface_t *surface) {
  (void)surface;
}

int tsrc_receive_test_surface(tsrc_port_t surface_receive_port,
                              uint32_t timeout_ms,
                              tsrc_surface_metadata_t *out_metadata) {
  (void)surface_receive_port;
  (void)timeout_ms;
  if (out_metadata != 0)
    *out_metadata = (tsrc_surface_metadata_t){0};
  return TSRC_UNSUPPORTED;
}

void tsrc_deallocate_port(tsrc_port_t port) { (void)port; }

void tsrc_destroy_receive_port(tsrc_port_t port) { (void)port; }

#endif

const char *tsrc_result_name(int result) {
  switch (result) {
  case TSRC_OK:
    return "ok";
  case TSRC_UNSUPPORTED:
    return "unsupported";
  case TSRC_INVALID_ARGUMENT:
    return "invalid-argument";
  case TSRC_ALLOCATE_FAILED:
    return "allocate-failed";
  case TSRC_INSERT_RIGHT_FAILED:
    return "insert-right-failed";
  case TSRC_REGISTER_FAILED:
    return "register-failed";
  case TSRC_LOOKUP_FAILED:
    return "lookup-failed";
  case TSRC_SEND_FAILED:
    return "send-failed";
  case TSRC_RECEIVE_FAILED:
    return "receive-failed";
  case TSRC_BAD_MESSAGE:
    return "bad-message";
  default:
    return "unknown";
  }
}
