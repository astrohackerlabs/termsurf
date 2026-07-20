#ifndef TERMSURF_RENDER_CHANNEL_H
#define TERMSURF_RENDER_CHANNEL_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define TSRC_BOOTSTRAP_MESSAGE_ID 0x54535243u
#define TSRC_SURFACE_RECEIVER_MESSAGE_ID 0x54535244u
#define TSRC_TEST_SURFACE_MESSAGE_ID 0x54535245u
#define TSRC_DEFAULT_TIMEOUT_MS 1000u

typedef uint32_t tsrc_port_t;

typedef struct tsrc_surface_metadata {
  uint32_t width;
  uint32_t height;
  uint32_t bytes_per_row;
  uint32_t pixel_format;
  uint32_t generation;
  uint64_t attachment_id;
  uint32_t imported_width;
  uint32_t imported_height;
  uint32_t imported_bytes_per_row;
  uint32_t imported_pixel_format;
} tsrc_surface_metadata_t;

typedef struct tsrc_received_surface tsrc_received_surface_t;

enum tsrc_result {
  TSRC_OK = 0,
  TSRC_UNSUPPORTED = 1,
  TSRC_INVALID_ARGUMENT = 2,
  TSRC_ALLOCATE_FAILED = 3,
  TSRC_INSERT_RIGHT_FAILED = 4,
  TSRC_REGISTER_FAILED = 5,
  TSRC_LOOKUP_FAILED = 6,
  TSRC_SEND_FAILED = 7,
  TSRC_RECEIVE_FAILED = 8,
  TSRC_BAD_MESSAGE = 9,
};

int tsrc_register_service(const char *service_name,
                          tsrc_port_t *out_control_port);
int tsrc_wait_for_child_port(tsrc_port_t control_port, uint32_t timeout_ms,
                             tsrc_port_t *out_child_port);
int tsrc_child_connect_and_send(const char *service_name, uint32_t timeout_ms,
                                tsrc_port_t *out_receive_port);
int tsrc_send_surface_receiver(tsrc_port_t child_port, uint32_t timeout_ms,
                               tsrc_port_t *out_surface_receive_port);
int tsrc_wait_for_surface_receiver(tsrc_port_t child_receive_port,
                                   uint32_t timeout_ms,
                                   tsrc_port_t *out_surface_send_port);
int tsrc_send_surface(tsrc_port_t surface_send_port,
                      tsrc_port_t exported_surface_port, uint32_t width,
                      uint32_t height, uint32_t bytes_per_row,
                      uint32_t pixel_format, uint32_t generation,
                      uint64_t attachment_id, uint32_t timeout_ms);
int tsrc_send_test_surface(tsrc_port_t surface_send_port, uint32_t width,
                           uint32_t height, uint32_t generation,
                           uint32_t timeout_ms);
int tsrc_receive_surface(tsrc_port_t surface_receive_port, uint32_t timeout_ms,
                         tsrc_received_surface_t **out_surface);
void tsrc_received_surface_metadata(const tsrc_received_surface_t *surface,
                                    tsrc_surface_metadata_t *out_metadata);
void *tsrc_received_surface_iosurface(const tsrc_received_surface_t *surface);
void *
tsrc_retain_received_surface_iosurface(const tsrc_received_surface_t *surface);
void tsrc_release_iosurface(void *surface);
void tsrc_release_received_surface(tsrc_received_surface_t *surface);
int tsrc_receive_test_surface(tsrc_port_t surface_receive_port,
                              uint32_t timeout_ms,
                              tsrc_surface_metadata_t *out_metadata);
void tsrc_deallocate_port(tsrc_port_t port);
void tsrc_destroy_receive_port(tsrc_port_t port);
const char *tsrc_result_name(int result);

#ifdef __cplusplus
}
#endif

#endif
