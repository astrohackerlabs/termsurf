#ifndef TERMSURF_GECKO_TILES_H
#define TERMSURF_GECKO_TILES_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Exp1/2 multi-tile Mach protocol host for Ghostboard (Exp4).
 * Ghostboard registers the bootstrap service; Gecko looks up and sends. */

#define TSGT_MAX_TILES 64
#define TSGT_MSG_ID 0x41534847u
#define TSGT_ACK_ID 0x41534841u
#define TSGT_LAYER_SET_MSG_ID 0x4153484cu

typedef struct tsgt_tile {
  int live;
  uint64_t stream_id;
  uint64_t layer_id;
  uint64_t generation;
  uint32_t surface_id;
  void *iosurface; /* IOSurfaceRef */
  int32_t pos_x, pos_y;
  int32_t disp_x, disp_y, disp_w, disp_h;
  uint32_t resize_epoch;
  uint32_t expected_width, expected_height;
  double device_scale;
  int32_t exporter_pid;
  int32_t order_index;
} tsgt_tile_t;

typedef struct tsgt_host tsgt_host_t;

/* Present callback: host owns tiles snapshot; caller must not free tiles. */
typedef void (*tsgt_present_fn)(void *ctx, const tsgt_tile_t *tiles, int count,
                                int pane_w_phys, int pane_h_phys,
                                double scale);

/* Create host, bootstrap_register(service_name), ready to receive. */
tsgt_host_t *tsgt_host_create(const char *service_name, tsgt_present_fn present,
                              void *present_ctx);
void tsgt_host_destroy(tsgt_host_t *host);

/* Non-blocking drain of pending tile messages (call from poll thread).
 * Returns number of accepted updates this call. */
int tsgt_host_poll(tsgt_host_t *host, uint32_t timeout_ms);

/* Stats for fail-closed markers. */
typedef struct tsgt_stats {
  uint64_t updates;
  uint64_t acquired;
  uint64_t released;
  uint64_t acked;
  uint64_t removes;
  uint64_t max_live;
  int exporter_pid;
  int gen_monotonic_ok;
  int partial_tile_ok;
  uint32_t last_epoch;
  uint32_t last_expected_w;
  uint32_t last_expected_h;
  double last_scale;
  uint8_t band_mask;
  uint64_t marker_changes;
  uint64_t stream_count;
  uint64_t reaped_streams;
  uint64_t layer_sets;
  uint64_t reconciled_removes;
} tsgt_stats_t;

void tsgt_host_stats(const tsgt_host_t *host, tsgt_stats_t *out);
int tsgt_host_live_count(const tsgt_host_t *host);
const char *tsgt_host_service(const tsgt_host_t *host);

/* Sample live tile IOSurfaces for RGB bands + marker (oracle). */
void tsgt_host_sample(tsgt_host_t *host);

/* Release streams whose exporter process has exited. Returns streams reaped. */
int tsgt_host_reap_dead_streams(tsgt_host_t *host);

/* Release one tab-owned stream without affecting peer streams. */
void tsgt_host_clear_stream(tsgt_host_t *host, uint64_t stream_id);

/* Release all tiles (pane teardown). */
void tsgt_host_clear(tsgt_host_t *host);

#ifdef __cplusplus
}
#endif

#endif /* TERMSURF_GECKO_TILES_H */
