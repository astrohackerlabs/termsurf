#include "termsurf_gecko_tiles.h"

#if defined(__APPLE__) && defined(__MACH__)
#include <TargetConditionals.h>
#endif

#if defined(__APPLE__) && defined(__MACH__) && defined(TARGET_OS_OSX) && \
    TARGET_OS_OSX

#include <CoreFoundation/CoreFoundation.h>
#include <IOSurface/IOSurface.h>
#include <errno.h>
#include <mach/mach.h>
#include <servers/bootstrap.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

enum { kPixelFormatBGRA = 0x42475241 };

typedef struct {
  mach_msg_header_t header;
  mach_msg_body_t body;
  mach_msg_port_descriptor_t surface_port;
  uint64_t generation;
  uint64_t token;
  uint32_t surface_id;
  uint32_t width;
  uint32_t height;
  uint32_t pixel_format;
  uint32_t opaque;
  double device_scale;
  int32_t exporter_pid;
  char exporter_role[16];
  uint64_t stream_id;
  uint64_t layer_id;
  int32_t position_x;
  int32_t position_y;
  int32_t display_x;
  int32_t display_y;
  int32_t display_width;
  int32_t display_height;
  uint32_t resize_epoch;
  uint32_t expected_width;
  uint32_t expected_height;
  mach_msg_trailer_t trailer;
} tsgt_surface_message_t;

typedef struct {
  mach_msg_header_t header;
  uint64_t generation;
  uint64_t token;
  uint32_t surface_id;
  int32_t receiver_pid;
} tsgt_ack_message_t;

typedef struct {
  mach_msg_header_t header;
  uint64_t stream_id;
  int32_t exporter_pid;
  uint32_t count;
  uint64_t layer_ids[TSGT_MAX_TILES];
  mach_msg_trailer_t trailer;
} tsgt_layer_set_message_t;

typedef struct {
  int live;
  uint64_t stream_id;
  int32_t exporter_pid;
  uint32_t count;
  uint64_t layer_ids[TSGT_MAX_TILES];
} tsgt_layer_set_t;

typedef union {
  mach_msg_header_t header;
  tsgt_surface_message_t surface;
  tsgt_layer_set_message_t layer_set;
} tsgt_receive_message_t;

struct tsgt_host {
  char service[256];
  mach_port_t receive_port;
  tsgt_tile_t tiles[TSGT_MAX_TILES];
  tsgt_layer_set_t layer_sets[TSGT_MAX_TILES];
  tsgt_present_fn present;
  void *present_ctx;
  tsgt_stats_t stats;
  int pane_w, pane_h;
  double scale;
  uint8_t last_marker;
  int have_marker;
};

static int send_ack(mach_port_t reply_port, const tsgt_surface_message_t *msg) {
  if (reply_port == MACH_PORT_NULL) return 0;
  tsgt_ack_message_t ack = {};
  ack.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_MOVE_SEND_ONCE, 0);
  ack.header.msgh_size = sizeof(ack);
  ack.header.msgh_remote_port = reply_port;
  ack.header.msgh_local_port = MACH_PORT_NULL;
  ack.header.msgh_id = (mach_msg_id_t)TSGT_ACK_ID;
  ack.generation = msg->generation;
  ack.token = msg->token;
  ack.surface_id = msg->surface_id;
  ack.receiver_pid = getpid();
  kern_return_t kr = mach_msg(&ack.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT,
                              sizeof(ack), 0, MACH_PORT_NULL, 1000,
                              MACH_PORT_NULL);
  if (kr != KERN_SUCCESS) {
    mach_port_deallocate(mach_task_self(), reply_port);
    return 0;
  }
  return 1;
}

static tsgt_tile_t *find_tile(tsgt_host_t *h, uint64_t stream_id,
                              uint64_t layer_id) {
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (h->tiles[i].live && h->tiles[i].stream_id == stream_id &&
        h->tiles[i].layer_id == layer_id)
      return &h->tiles[i];
  }
  return NULL;
}

static tsgt_tile_t *alloc_tile(tsgt_host_t *h, uint64_t stream_id,
                               uint64_t layer_id) {
  tsgt_tile_t *t = find_tile(h, stream_id, layer_id);
  if (t) return t;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (!h->tiles[i].live) {
      memset(&h->tiles[i], 0, sizeof(h->tiles[i]));
      h->tiles[i].live = 1;
      h->tiles[i].stream_id = stream_id;
      h->tiles[i].layer_id = layer_id;
      h->tiles[i].order_index = -1;
      return &h->tiles[i];
    }
  }
  return NULL;
}

static void release_tile(tsgt_host_t *h, tsgt_tile_t *t) {
  if (!t || !t->live) return;
  if (t->iosurface) {
    CFRelease((IOSurfaceRef)t->iosurface);
    t->iosurface = NULL;
    h->stats.released++;
  }
  h->stats.removes++;
  memset(t, 0, sizeof(*t));
}

static int live_count(const tsgt_host_t *h) {
  int n = 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++)
    if (h->tiles[i].live) n++;
  return n;
}

static int stream_count(const tsgt_host_t *h) {
  uint64_t streams[TSGT_MAX_TILES];
  int n = 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (!h->tiles[i].live) continue;
    int seen = 0;
    for (int j = 0; j < n; j++) {
      if (streams[j] == h->tiles[i].stream_id) {
        seen = 1;
        break;
      }
    }
    if (!seen) streams[n++] = h->tiles[i].stream_id;
  }
  return n;
}

static tsgt_layer_set_t *find_layer_set(tsgt_host_t *h, uint64_t stream_id,
                                        int create) {
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (h->layer_sets[i].live && h->layer_sets[i].stream_id == stream_id)
      return &h->layer_sets[i];
  }
  if (!create) return NULL;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (!h->layer_sets[i].live) {
      memset(&h->layer_sets[i], 0, sizeof(h->layer_sets[i]));
      h->layer_sets[i].live = 1;
      h->layer_sets[i].stream_id = stream_id;
      return &h->layer_sets[i];
    }
  }
  return NULL;
}

static int layer_order(const tsgt_layer_set_t *set, uint64_t layer_id) {
  if (!set) return -1;
  for (uint32_t i = 0; i < set->count; i++) {
    if (set->layer_ids[i] == layer_id) return (int)i;
  }
  return -1;
}

static void call_present(tsgt_host_t *h) {
  if (!h->present) return;
  tsgt_tile_t snap[TSGT_MAX_TILES];
  uint8_t used[TSGT_MAX_TILES] = {};
  int n = 0;
  // Firefox's SetLayers array is ordered back-to-front. Preserve that order
  // so the product CALayers reproduce Firefox's own compositor stacking.
  for (int s = 0; s < TSGT_MAX_TILES; s++) {
    const tsgt_layer_set_t *set = &h->layer_sets[s];
    if (!set->live) continue;
    for (uint32_t order = 0; order < set->count; order++) {
      for (int i = 0; i < TSGT_MAX_TILES; i++) {
        if (!used[i] && h->tiles[i].live &&
            h->tiles[i].stream_id == set->stream_id &&
            h->tiles[i].layer_id == set->layer_ids[order]) {
          snap[n++] = h->tiles[i];
          used[i] = 1;
          break;
        }
      }
    }
  }
  // Surface messages can precede their commit's SetLayers message. Keep those
  // provisional tiles visible until the ordered reconciliation arrives.
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (h->tiles[i].live && !used[i]) snap[n++] = h->tiles[i];
  }
  int pw = h->pane_w > 0 ? h->pane_w
                         : (int)(h->stats.last_expected_w
                                     ? h->stats.last_expected_w
                                     : 1600);
  int ph = h->pane_h > 0 ? h->pane_h
                         : (int)(h->stats.last_expected_h
                                     ? h->stats.last_expected_h
                                     : 1000);
  double sc = h->scale > 0 ? h->scale : h->stats.last_scale;
  if (sc <= 0) sc = 2.0;
  h->present(h->present_ctx, snap, n, pw, ph, sc);
}

static int handle_msg(tsgt_host_t *h, tsgt_surface_message_t *msg) {
  if (strcmp(msg->exporter_role, "parent") != 0) return 0;
  if (msg->pixel_format != kPixelFormatBGRA) return 0;
  if (msg->stream_id == 0) return 0;
  if (h->stats.exporter_pid == 0) h->stats.exporter_pid = msg->exporter_pid;
  if (msg->exporter_pid == getpid()) return 0;

  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (h->tiles[i].live && h->tiles[i].stream_id == msg->stream_id &&
        h->tiles[i].exporter_pid != 0 &&
        h->tiles[i].exporter_pid != msg->exporter_pid)
      return 0;
  }

  if (msg->device_scale > 0) h->stats.last_scale = msg->device_scale;
  if (msg->expected_width > 0) h->stats.last_expected_w = msg->expected_width;
  if (msg->expected_height > 0) h->stats.last_expected_h = msg->expected_height;
  if (msg->resize_epoch != 0) h->stats.last_epoch = msg->resize_epoch;
  if (msg->expected_width > 0 && msg->expected_height > 0) {
    h->pane_w = (int)msg->expected_width;
    h->pane_h = (int)msg->expected_height;
  }
  if (msg->device_scale > 0) h->scale = msg->device_scale;

  mach_port_t sport = msg->surface_port.name;
  IOSurfaceRef surface = IOSurfaceLookupFromMachPort(sport);
  mach_port_deallocate(mach_task_self(), sport);
  if (!surface) return 0;

  tsgt_tile_t *t = alloc_tile(h, msg->stream_id, msg->layer_id);
  if (!t) {
    CFRelease(surface);
    return 0;
  }
  if (t->iosurface) {
    CFRelease((IOSurfaceRef)t->iosurface);
    h->stats.released++;
    t->iosurface = NULL;
  }
  if (t->generation != 0 && msg->generation <= t->generation) {
    h->stats.gen_monotonic_ok = 0;
    CFRelease(surface);
    return 0;
  }
  t->iosurface = surface;
  h->stats.acquired++;
  t->generation = msg->generation;
  t->surface_id = msg->surface_id;
  t->pos_x = msg->position_x;
  t->pos_y = msg->position_y;
  t->disp_x = msg->display_x;
  t->disp_y = msg->display_y;
  t->disp_w = msg->display_width > 0 ? msg->display_width : (int32_t)msg->width;
  t->disp_h =
      msg->display_height > 0 ? msg->display_height : (int32_t)msg->height;
  t->resize_epoch = msg->resize_epoch;
  t->expected_width = msg->expected_width;
  t->expected_height = msg->expected_height;
  t->device_scale = msg->device_scale;
  t->exporter_pid = msg->exporter_pid;
  t->order_index = layer_order(find_layer_set(h, msg->stream_id, 0),
                               msg->layer_id);
  if (t->disp_w > 0 && t->disp_h > 0 &&
      (t->disp_w < (int32_t)msg->width || t->disp_h < (int32_t)msg->height ||
       t->disp_x != 0 || t->disp_y != 0)) {
    h->stats.partial_tile_ok = 1;
  }
  h->stats.updates++;
  h->stats.stream_count = (uint64_t)stream_count(h);
  int live = live_count(h);
  if ((uint64_t)live > h->stats.max_live) h->stats.max_live = (uint64_t)live;
  call_present(h);
  return 1;
}

static int handle_layer_set(tsgt_host_t *h, tsgt_layer_set_message_t *msg) {
  if (msg->stream_id == 0 || msg->exporter_pid <= 0 ||
      msg->exporter_pid == getpid() || msg->count > TSGT_MAX_TILES)
    return 0;

  tsgt_layer_set_t *set = find_layer_set(h, msg->stream_id, 1);
  if (!set) return 0;
  if (set->exporter_pid != 0 && set->exporter_pid != msg->exporter_pid)
    return 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (h->tiles[i].live && h->tiles[i].stream_id == msg->stream_id &&
        h->tiles[i].exporter_pid != 0 &&
        h->tiles[i].exporter_pid != msg->exporter_pid)
      return 0;
  }

  set->exporter_pid = msg->exporter_pid;
  set->count = msg->count;
  if (msg->count > 0)
    memcpy(set->layer_ids, msg->layer_ids,
           (size_t)msg->count * sizeof(msg->layer_ids[0]));

  uint64_t removed = 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    tsgt_tile_t *tile = &h->tiles[i];
    if (!tile->live || tile->stream_id != msg->stream_id) continue;
    int order = layer_order(set, tile->layer_id);
    if (order < 0) {
      release_tile(h, tile);
      removed++;
    } else {
      tile->order_index = order;
    }
  }
  h->stats.layer_sets++;
  h->stats.reconciled_removes += removed;
  h->stats.stream_count = (uint64_t)stream_count(h);
  call_present(h);
  return 1;
}

tsgt_host_t *tsgt_host_create(const char *service_name, tsgt_present_fn present,
                              void *present_ctx) {
  if (!service_name || !service_name[0] ||
      strlen(service_name) >= sizeof(((tsgt_host_t *)0)->service)) {
    return NULL;
  }
  tsgt_host_t *h = calloc(1, sizeof(*h));
  if (!h) return NULL;
  strncpy(h->service, service_name, sizeof(h->service) - 1);
  h->present = present;
  h->present_ctx = present_ctx;
  h->stats.gen_monotonic_ok = 1;
  h->scale = 2.0;

  if (mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE,
                         &h->receive_port) != KERN_SUCCESS) {
    free(h);
    return NULL;
  }
  if (mach_port_insert_right(mach_task_self(), h->receive_port, h->receive_port,
                             MACH_MSG_TYPE_MAKE_SEND) != KERN_SUCCESS) {
    mach_port_mod_refs(mach_task_self(), h->receive_port,
                       MACH_PORT_RIGHT_RECEIVE, -1);
    free(h);
    return NULL;
  }
  mach_port_limits_t limits = {};
  limits.mpl_qlimit = MACH_PORT_QLIMIT_LARGE;
  mach_port_set_attributes(mach_task_self(), h->receive_port,
                           MACH_PORT_LIMITS_INFO, (mach_port_info_t)&limits,
                           MACH_PORT_LIMITS_INFO_COUNT);
  name_t name = {};
  strncpy(name, service_name, sizeof(name) - 1);
  if (bootstrap_register(bootstrap_port, name, h->receive_port) !=
      KERN_SUCCESS) {
    mach_port_mod_refs(mach_task_self(), h->receive_port,
                       MACH_PORT_RIGHT_RECEIVE, -1);
    free(h);
    return NULL;
  }
  fprintf(stderr, "tsgt_host_ready=1 pid=%d service=%s\n", getpid(),
          service_name);
  return h;
}

void tsgt_host_destroy(tsgt_host_t *host) {
  if (!host) return;
  tsgt_host_clear(host);
  if (host->receive_port != MACH_PORT_NULL) {
    mach_port_mod_refs(mach_task_self(), host->receive_port,
                       MACH_PORT_RIGHT_RECEIVE, -1);
    host->receive_port = MACH_PORT_NULL;
  }
  free(host);
}

int tsgt_host_poll(tsgt_host_t *host, uint32_t timeout_ms) {
  if (!host) return 0;
  int accepted = 0;
  for (;;) {
    tsgt_receive_message_t message = {};
    message.header.msgh_local_port = host->receive_port;
    message.header.msgh_size = sizeof(message);
    kern_return_t kr =
        mach_msg(&message.header, MACH_RCV_MSG | MACH_RCV_TIMEOUT, 0,
                 sizeof(message), host->receive_port, timeout_ms,
                 MACH_PORT_NULL);
    if (kr != KERN_SUCCESS) break;
    if (message.header.msgh_id == (mach_msg_id_t)TSGT_LAYER_SET_MSG_ID) {
      if (!(message.header.msgh_bits & MACH_MSGH_BITS_COMPLEX) &&
          handle_layer_set(host, &message.layer_set))
        accepted++;
      timeout_ms = 0;
      continue;
    }
    if (message.header.msgh_id != (mach_msg_id_t)TSGT_MSG_ID ||
        !(message.header.msgh_bits & MACH_MSGH_BITS_COMPLEX) ||
        message.surface.body.msgh_descriptor_count != 1) {
      continue;
    }
    int ok = handle_msg(host, &message.surface);
    if (send_ack(message.header.msgh_remote_port, &message.surface)) {
      host->stats.acked++;
    }
    if (ok) accepted++;
    timeout_ms = 0; /* drain remaining without blocking */
  }
  return accepted;
}

void tsgt_host_stats(const tsgt_host_t *host, tsgt_stats_t *out) {
  if (!host || !out) return;
  *out = host->stats;
}

int tsgt_host_live_count(const tsgt_host_t *host) {
  return host ? live_count(host) : 0;
}

const char *tsgt_host_service(const tsgt_host_t *host) {
  return host ? host->service : "";
}

void tsgt_host_sample(tsgt_host_t *host) {
  if (!host) return;
  size_t red = 0, green = 0, blue = 0;
  uint8_t band = 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    tsgt_tile_t *t = &host->tiles[i];
    if (!t->live || !t->iosurface) continue;
    IOSurfaceRef s = (IOSurfaceRef)t->iosurface;
    if (IOSurfaceLock(s, kIOSurfaceLockReadOnly, NULL) != kIOReturnSuccess)
      continue;
    size_t w = IOSurfaceGetWidth(s);
    size_t h = IOSurfaceGetHeight(s);
    size_t stride = IOSurfaceGetBytesPerRow(s);
    size_t bpe = IOSurfaceGetBytesPerElement(s);
    const uint8_t *base = (const uint8_t *)IOSurfaceGetBaseAddress(s);
    if (!base || bpe < 4) {
      IOSurfaceUnlock(s, kIOSurfaceLockReadOnly, NULL);
      continue;
    }
    int x0 = t->disp_x < 0 ? 0 : t->disp_x;
    int y0 = t->disp_y < 0 ? 0 : t->disp_y;
    int x1 = x0 + (t->disp_w > 0 ? t->disp_w : (int)w);
    int y1 = y0 + (t->disp_h > 0 ? t->disp_h : (int)h);
    if (x1 > (int)w) x1 = (int)w;
    if (y1 > (int)h) y1 = (int)h;
    for (int y = y0; y < y1; y += 8) {
      for (int x = x0; x < x1; x += 8) {
        const uint8_t *px = base + (size_t)y * stride + (size_t)x * bpe;
        int B = px[0], G = px[1], R = px[2];
        if (R >= 247 && G <= 8 && B <= 8) {
          red++;
          band |= 0x1;
        } else if (G >= 247 && R <= 8 && B <= 8) {
          green++;
          band |= 0x2;
        } else if (B >= 247 && R <= 8 && G <= 8) {
          blue++;
          band |= 0x4;
        } else if ((R >= 247 && G >= 247 && B >= 247) ||
                   (R <= 8 && G <= 8 && B <= 8)) {
          uint8_t mv = (R + G + B) > 400 ? 255 : 0;
          if (host->have_marker &&
              abs((int)mv - (int)host->last_marker) >= 200) {
            host->stats.marker_changes++;
          }
          host->last_marker = mv;
          host->have_marker = 1;
        }
      }
    }
    IOSurfaceUnlock(s, kIOSurfaceLockReadOnly, NULL);
  }
  host->stats.band_mask |= band;
  fprintf(stderr,
          "tsgt_sample=1 band_mask=0x%x red=%zu green=%zu blue=%zu "
          "marker_changes=%llu live=%d\n",
          host->stats.band_mask, red, green, blue,
          (unsigned long long)host->stats.marker_changes, live_count(host));
}

void tsgt_host_clear_stream(tsgt_host_t *host, uint64_t stream_id) {
  if (!host || stream_id == 0) return;
  int changed = 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (host->tiles[i].live && host->tiles[i].stream_id == stream_id) {
      release_tile(host, &host->tiles[i]);
      changed = 1;
    }
  }
  tsgt_layer_set_t *set = find_layer_set(host, stream_id, 0);
  if (set) memset(set, 0, sizeof(*set));
  host->stats.stream_count = (uint64_t)stream_count(host);
  if (changed) call_present(host);
}

int tsgt_host_reap_dead_streams(tsgt_host_t *host) {
  if (!host) return 0;
  uint64_t streams[TSGT_MAX_TILES];
  int32_t pids[TSGT_MAX_TILES];
  int n = 0;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (!host->tiles[i].live) continue;
    int seen = 0;
    for (int j = 0; j < n; j++) {
      if (streams[j] == host->tiles[i].stream_id) {
        seen = 1;
        break;
      }
    }
    if (!seen) {
      streams[n] = host->tiles[i].stream_id;
      pids[n] = host->tiles[i].exporter_pid;
      n++;
    }
  }
  int reaped = 0;
  for (int i = 0; i < n; i++) {
    if (pids[i] > 0 && kill(pids[i], 0) != 0 && errno == ESRCH) {
      tsgt_host_clear_stream(host, streams[i]);
      reaped++;
    }
  }
  host->stats.reaped_streams += (uint64_t)reaped;
  return reaped;
}

void tsgt_host_clear(tsgt_host_t *host) {
  if (!host) return;
  for (int i = 0; i < TSGT_MAX_TILES; i++) {
    if (host->tiles[i].live) release_tile(host, &host->tiles[i]);
  }
  memset(host->layer_sets, 0, sizeof(host->layer_sets));
  call_present(host);
}

#else /* !macOS */

#include <stddef.h>
#include <string.h>

struct tsgt_host {
  int unused;
};

tsgt_host_t *tsgt_host_create(const char *service_name, tsgt_present_fn present,
                              void *present_ctx) {
  (void)service_name;
  (void)present;
  (void)present_ctx;
  return NULL;
}
void tsgt_host_destroy(tsgt_host_t *host) { (void)host; }
int tsgt_host_poll(tsgt_host_t *host, uint32_t timeout_ms) {
  (void)host;
  (void)timeout_ms;
  return 0;
}
void tsgt_host_stats(const tsgt_host_t *host, tsgt_stats_t *out) {
  (void)host;
  if (out) memset(out, 0, sizeof(*out));
}
int tsgt_host_live_count(const tsgt_host_t *host) {
  (void)host;
  return 0;
}
const char *tsgt_host_service(const tsgt_host_t *host) {
  (void)host;
  return "";
}
void tsgt_host_sample(tsgt_host_t *host) { (void)host; }
int tsgt_host_reap_dead_streams(tsgt_host_t *host) {
  (void)host;
  return 0;
}
void tsgt_host_clear_stream(tsgt_host_t *host, uint64_t stream_id) {
  (void)host;
  (void)stream_id;
}
void tsgt_host_clear(tsgt_host_t *host) { (void)host; }

#endif
