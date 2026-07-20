#ifndef TERMSURF_LADYBIRD_H
#define TERMSURF_LADYBIRD_H

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct TsLadybirdRuntime TsLadybirdRuntime;
typedef struct TsLadybirdView TsLadybirdView;

typedef struct TsLadybirdRenderSurfaceProbe {
  bool has_surface;
  bool can_export_shared_image;
  int pixel_width;
  int pixel_height;
  unsigned long long generation;
  bool ready_to_paint_seen;
  bool has_usable_bitmap;
} TsLadybirdRenderSurfaceProbe;

typedef struct TsLadybirdRenderSurfaceExport {
  bool has_surface;
  unsigned int surface_port;
  int pixel_width;
  int pixel_height;
  unsigned long long bytes_per_row;
  unsigned int pixel_format;
  unsigned long long generation;
} TsLadybirdRenderSurfaceExport;

typedef struct TsLadybirdConsoleMessage {
  char level[32];
  char message[1024];
  int line_no;
  char source_id[512];
} TsLadybirdConsoleMessage;

typedef struct TsLadybirdJavaScriptDialogRequest {
  unsigned long long request_id;
  char dialog_type[32];
  char origin_url[1024];
  char message[1024];
  char default_prompt_text[1024];
} TsLadybirdJavaScriptDialogRequest;

typedef struct TsLadybirdRendererCrash {
  char termination_status[64];
  int termination_status_code;
  char url[1024];
  bool can_reload;
} TsLadybirdRendererCrash;

typedef struct TsLadybirdNavigationState {
  bool can_go_back;
  bool can_go_forward;
  bool can_refresh;
} TsLadybirdNavigationState;

const char *ts_ladybird_runtime_name(void);
const char *ts_ladybird_runtime_version(void);
const char *ts_ladybird_runtime_resource_root(void);
bool ts_ladybird_warmup(void);
bool ts_ladybird_initialize_runtime(void);
void ts_ladybird_shutdown_runtime(void);
TsLadybirdRuntime *ts_ladybird_runtime_create(void);
void ts_ladybird_runtime_destroy(TsLadybirdRuntime *runtime);
bool ts_ladybird_runtime_pump(TsLadybirdRuntime *runtime);
const char *ts_ladybird_runtime_last_error(const TsLadybirdRuntime *runtime);
TsLadybirdView *ts_ladybird_view_create(TsLadybirdRuntime *runtime, int width,
                                        int height);
void ts_ladybird_view_destroy(TsLadybirdView *view);
bool ts_ladybird_view_load_url(TsLadybirdView *view, const char *url);
bool ts_ladybird_view_resize(TsLadybirdView *view, int width, int height);
bool ts_ladybird_view_set_color_scheme(TsLadybirdView *view, bool dark);
bool ts_ladybird_view_set_gui_active(TsLadybirdView *view, bool active);
bool ts_ladybird_view_mouse_event(TsLadybirdView *view, const char *type,
                                  const char *button, double x, double y,
                                  int click_count,
                                  unsigned long long modifiers);
bool ts_ladybird_view_mouse_move(TsLadybirdView *view, double x, double y,
                                 unsigned long long modifiers);
bool ts_ladybird_view_scroll_event(TsLadybirdView *view, double x, double y,
                                   double delta_x, double delta_y,
                                   unsigned long long phase,
                                   unsigned long long momentum_phase,
                                   bool precise, unsigned long long modifiers);
bool ts_ladybird_view_key_event(TsLadybirdView *view, const char *type,
                                int windows_key_code, const char *utf8,
                                unsigned long long modifiers);
bool ts_ladybird_view_run_javascript_for_testing(TsLadybirdView *view,
                                                 const char *script);
bool ts_ladybird_view_navigation_action(TsLadybirdView *view,
                                        const char *action);
bool ts_ladybird_view_navigation_state(
    const TsLadybirdView *view, TsLadybirdNavigationState *out_state);
bool ts_ladybird_view_take_title_changed(TsLadybirdView *view, char *out_title,
                                         size_t out_title_len);
bool ts_ladybird_view_take_console_message(
    TsLadybirdView *view, TsLadybirdConsoleMessage *out_message);
bool ts_ladybird_view_take_cursor_changed(TsLadybirdView *view,
                                          int *out_cursor_type);
bool ts_ladybird_view_take_target_url_changed(TsLadybirdView *view,
                                              char *out_url,
                                              size_t out_url_len);
bool ts_ladybird_view_take_javascript_dialog_request(
    TsLadybirdView *view, TsLadybirdJavaScriptDialogRequest *out_request);
bool ts_ladybird_view_reply_javascript_dialog(TsLadybirdView *view,
                                              unsigned long long request_id,
                                              bool accepted,
                                              const char *prompt_text);
bool ts_ladybird_view_take_renderer_crashed(TsLadybirdView *view,
                                            TsLadybirdRendererCrash *out_crash);
bool ts_ladybird_view_crash_current_page_for_testing(TsLadybirdView *view);
const char *ts_ladybird_view_last_url(const TsLadybirdView *view);
bool ts_ladybird_view_did_finish_load(const TsLadybirdView *view);
bool ts_ladybird_view_did_crash(const TsLadybirdView *view);
bool ts_ladybird_view_render_surface_probe(
    TsLadybirdView *view, TsLadybirdRenderSurfaceProbe *out_probe);
/* On macOS, a successful export returns a Mach send right in surface_port.
   The caller must move it into the render side-channel shim or release it. */
bool ts_ladybird_view_export_render_surface(
    TsLadybirdView *view, TsLadybirdRenderSurfaceExport *out_export);

#ifdef __cplusplus
}
#endif

#endif
