#include "termsurf_ladybird.h"

#include <stdio.h>
#include <string.h>

struct TsLadybirdRuntime {
  bool active;
  bool destroyed;
  const char *last_error;
};

struct TsLadybirdView {
  TsLadybirdRuntime *runtime;
  bool active;
  bool destroyed;
  bool did_finish_load;
  bool did_crash;
  int width;
  int height;
  bool dark;
  bool gui_active;
  unsigned long long mouse_button_events;
  unsigned long long mouse_move_events;
  unsigned long long scroll_events;
  unsigned long long key_events;
  char last_url[1024];
  char pending_title[1024];
  TsLadybirdConsoleMessage pending_console_message;
  TsLadybirdJavaScriptDialogRequest pending_javascript_dialog_request;
  TsLadybirdRendererCrash pending_renderer_crash;
  char pending_target_url[1024];
  int pending_cursor_type;
  unsigned long long next_javascript_dialog_request_id;
  unsigned long long active_javascript_dialog_request_id;
  char active_javascript_dialog_type[32];
  bool title_changed;
  bool console_message_pending;
  bool javascript_dialog_request_pending;
  bool renderer_crash_pending;
  bool cursor_changed;
  bool target_url_changed;
};

static TsLadybirdRuntime stub_runtime;
static TsLadybirdView stub_view;
static const char *stub_global_last_error = "";

static void set_error(TsLadybirdRuntime *runtime, const char *error) {
  if (runtime) {
    runtime->last_error = error;
  } else {
    stub_global_last_error = error;
  }
}

static void copy_c_string(char *out, size_t out_len, const char *value) {
  if (!out || out_len == 0) {
    return;
  }
  snprintf(out, out_len, "%s", value ? value : "");
}

const char *ts_ladybird_runtime_name(void) {
  return "libtermsurf_ladybird-stub";
}

const char *ts_ladybird_runtime_version(void) { return "0.0.0-stub"; }

const char *ts_ladybird_runtime_resource_root(void) { return ""; }

bool ts_ladybird_warmup(void) {
  TsLadybirdRuntime *runtime = ts_ladybird_runtime_create();
  if (!runtime) {
    return false;
  }
  printf("PROBE: TermSurf Ladybird stub runtime created\n");

  TsLadybirdView *view = ts_ladybird_view_create(runtime, 800, 600);
  if (!view) {
    ts_ladybird_runtime_destroy(runtime);
    return false;
  }
  printf("PROBE: TermSurf Ladybird stub view created\n");

  if (!ts_ladybird_view_load_url(
          view,
          "data:text/html,%3Ctitle%3ELadybird%20ABI%3C/title%3E%3Cp%3Eok%3C/"
          "p%3E")) {
    ts_ladybird_view_destroy(view);
    ts_ladybird_runtime_destroy(runtime);
    return false;
  }
  printf("PROBE: TermSurf Ladybird stub navigation completed immediately\n");

  bool ok = ts_ladybird_runtime_pump(runtime) &&
            ts_ladybird_view_did_finish_load(view) &&
            !ts_ladybird_view_did_crash(view);

  ts_ladybird_view_destroy(view);
  printf("PROBE: TermSurf Ladybird stub view destroyed\n");
  ts_ladybird_runtime_destroy(runtime);
  printf("PROBE: TermSurf Ladybird stub runtime destroyed\n");
  return ok;
}

bool ts_ladybird_initialize_runtime(void) { return true; }

void ts_ladybird_shutdown_runtime(void) {}

TsLadybirdRuntime *ts_ladybird_runtime_create(void) {
  if (stub_runtime.active && !stub_runtime.destroyed) {
    set_error(NULL, "stub runtime already exists");
    return NULL;
  }

  stub_runtime.active = true;
  stub_runtime.destroyed = false;
  stub_runtime.last_error = "";
  return &stub_runtime;
}

void ts_ladybird_runtime_destroy(TsLadybirdRuntime *runtime) {
  if (!runtime || runtime->destroyed) {
    return;
  }
  runtime->active = false;
  runtime->destroyed = true;
}

bool ts_ladybird_runtime_pump(TsLadybirdRuntime *runtime) {
  if (!runtime) {
    set_error(NULL, "stub runtime is null");
    return false;
  }
  if (runtime->destroyed) {
    set_error(runtime, "stub runtime is destroyed");
    return false;
  }
  return true;
}

const char *ts_ladybird_runtime_last_error(const TsLadybirdRuntime *runtime) {
  if (runtime) {
    return runtime->last_error;
  }
  return stub_global_last_error;
}

TsLadybirdView *ts_ladybird_view_create(TsLadybirdRuntime *runtime, int width,
                                        int height) {
  if (!runtime || runtime->destroyed) {
    set_error(runtime, "stub runtime is invalid");
    return NULL;
  }
  if (width <= 0 || height <= 0) {
    set_error(runtime, "stub view size must be positive");
    return NULL;
  }

  stub_view.runtime = runtime;
  stub_view.active = true;
  stub_view.destroyed = false;
  stub_view.did_finish_load = false;
  stub_view.did_crash = false;
  stub_view.width = width;
  stub_view.height = height;
  stub_view.dark = false;
  stub_view.gui_active = true;
  stub_view.mouse_button_events = 0;
  stub_view.mouse_move_events = 0;
  stub_view.scroll_events = 0;
  stub_view.key_events = 0;
  stub_view.last_url[0] = '\0';
  stub_view.pending_title[0] = '\0';
  stub_view.pending_target_url[0] = '\0';
  memset(&stub_view.pending_console_message, 0,
         sizeof(stub_view.pending_console_message));
  memset(&stub_view.pending_javascript_dialog_request, 0,
         sizeof(stub_view.pending_javascript_dialog_request));
  memset(&stub_view.pending_renderer_crash, 0,
         sizeof(stub_view.pending_renderer_crash));
  stub_view.pending_cursor_type = 0;
  stub_view.next_javascript_dialog_request_id = 1;
  stub_view.active_javascript_dialog_request_id = 0;
  stub_view.active_javascript_dialog_type[0] = '\0';
  stub_view.title_changed = false;
  stub_view.console_message_pending = false;
  stub_view.javascript_dialog_request_pending = false;
  stub_view.renderer_crash_pending = false;
  stub_view.cursor_changed = false;
  stub_view.target_url_changed = false;
  return &stub_view;
}

void ts_ladybird_view_destroy(TsLadybirdView *view) {
  if (!view || view->destroyed) {
    return;
  }
  view->active = false;
  view->destroyed = true;
}

bool ts_ladybird_view_load_url(TsLadybirdView *view, const char *url) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!url) {
    set_error(view->runtime, "stub url is null");
    return false;
  }

  snprintf(view->last_url, sizeof(view->last_url), "%s", url);
  snprintf(view->pending_title, sizeof(view->pending_title),
           "Ladybird Stub Title");
  view->title_changed = true;
  copy_c_string(view->pending_console_message.level,
                sizeof(view->pending_console_message.level), "log");
  copy_c_string(view->pending_console_message.message,
                sizeof(view->pending_console_message.message),
                "\"Ladybird Stub Console\"");
  view->pending_console_message.line_no = 0;
  copy_c_string(view->pending_console_message.source_id,
                sizeof(view->pending_console_message.source_id), "<stub>");
  view->console_message_pending = true;
  if (strstr(url, "ladybird-dialog-message")) {
    unsigned long long request_id = view->next_javascript_dialog_request_id++;
    memset(&view->pending_javascript_dialog_request, 0,
           sizeof(view->pending_javascript_dialog_request));
    view->pending_javascript_dialog_request.request_id = request_id;
    copy_c_string(view->pending_javascript_dialog_request.dialog_type,
                  sizeof(view->pending_javascript_dialog_request.dialog_type),
                  "prompt");
    copy_c_string(view->pending_javascript_dialog_request.origin_url,
                  sizeof(view->pending_javascript_dialog_request.origin_url),
                  url);
    copy_c_string(view->pending_javascript_dialog_request.message,
                  sizeof(view->pending_javascript_dialog_request.message),
                  "ladybird-dialog-message");
    copy_c_string(
        view->pending_javascript_dialog_request.default_prompt_text,
        sizeof(view->pending_javascript_dialog_request.default_prompt_text),
        "ladybird-dialog-default");
    view->active_javascript_dialog_request_id = request_id;
    copy_c_string(view->active_javascript_dialog_type,
                  sizeof(view->active_javascript_dialog_type), "prompt");
    view->javascript_dialog_request_pending = true;
    view->did_finish_load = false;
  } else {
    view->did_finish_load = true;
  }
  view->did_crash = false;
  return true;
}

bool ts_ladybird_view_resize(TsLadybirdView *view, int width, int height) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (width <= 0 || height <= 0) {
    set_error(view->runtime, "stub view size must be positive");
    return false;
  }

  view->width = width;
  view->height = height;
  return true;
}

bool ts_ladybird_view_set_color_scheme(TsLadybirdView *view, bool dark) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  view->dark = dark;
  return true;
}

bool ts_ladybird_view_set_gui_active(TsLadybirdView *view, bool active) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  view->gui_active = active;
  return true;
}

bool ts_ladybird_view_mouse_event(TsLadybirdView *view, const char *type,
                                  const char *button, double x, double y,
                                  int click_count,
                                  unsigned long long modifiers) {
  (void)x;
  (void)y;
  (void)click_count;
  (void)modifiers;
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!type || (strcmp(type, "down") != 0 && strcmp(type, "up") != 0)) {
    set_error(view->runtime, "stub mouse event type is unsupported");
    return false;
  }
  if (!button || (strcmp(button, "left") != 0 && strcmp(button, "right") != 0 &&
                  strcmp(button, "middle") != 0)) {
    set_error(view->runtime, "stub mouse button is unsupported");
    return false;
  }

  view->mouse_button_events++;
  return true;
}

bool ts_ladybird_view_mouse_move(TsLadybirdView *view, double x, double y,
                                 unsigned long long modifiers) {
  (void)x;
  (void)y;
  (void)modifiers;
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  view->mouse_move_events++;
  view->pending_cursor_type = 2;
  copy_c_string(view->pending_target_url, sizeof(view->pending_target_url),
                "https://example.com/ladybird-stub-hover");
  view->cursor_changed = true;
  view->target_url_changed = true;
  return true;
}

bool ts_ladybird_view_scroll_event(TsLadybirdView *view, double x, double y,
                                   double delta_x, double delta_y,
                                   unsigned long long phase,
                                   unsigned long long momentum_phase,
                                   bool precise, unsigned long long modifiers) {
  (void)x;
  (void)y;
  (void)delta_x;
  (void)delta_y;
  (void)phase;
  (void)momentum_phase;
  (void)precise;
  (void)modifiers;
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  view->scroll_events++;
  return true;
}

bool ts_ladybird_view_key_event(TsLadybirdView *view, const char *type,
                                int windows_key_code, const char *utf8,
                                unsigned long long modifiers) {
  (void)windows_key_code;
  (void)utf8;
  (void)modifiers;
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!type || (strcmp(type, "down") != 0 && strcmp(type, "up") != 0 &&
                strcmp(type, "repeat") != 0)) {
    set_error(view->runtime, "stub key event type is unsupported");
    return false;
  }

  view->key_events++;
  return true;
}

bool ts_ladybird_view_run_javascript_for_testing(TsLadybirdView *view,
                                                 const char *script) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!script) {
    set_error(view->runtime, "stub script is null");
    return false;
  }
  return true;
}

bool ts_ladybird_view_navigation_action(TsLadybirdView *view,
                                        const char *action) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!action) {
    set_error(view->runtime, "stub navigation action is null");
    return false;
  }
  if (strcmp(action, "back") == 0) {
    set_error(view->runtime, "stub back navigation is unavailable");
    return false;
  }
  if (strcmp(action, "refresh") == 0) {
    set_error(view->runtime, "stub refresh navigation is unavailable");
    return false;
  }
  set_error(view->runtime, "stub navigation action is unsupported");
  return false;
}

bool ts_ladybird_view_navigation_state(
    const TsLadybirdView *view, TsLadybirdNavigationState *out_state) {
  if (!out_state) {
    set_error(view ? view->runtime : NULL,
              "stub navigation state output is null");
    return false;
  }
  memset(out_state, 0, sizeof(*out_state));
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  return true;
}

bool ts_ladybird_view_take_title_changed(TsLadybirdView *view, char *out_title,
                                         size_t out_title_len) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!view->title_changed) {
    return false;
  }

  copy_c_string(out_title, out_title_len, view->pending_title);
  view->pending_title[0] = '\0';
  view->title_changed = false;
  return true;
}

bool ts_ladybird_view_take_console_message(
    TsLadybirdView *view, TsLadybirdConsoleMessage *out_message) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!out_message) {
    set_error(view->runtime, "stub console message output is null");
    return false;
  }
  if (!view->console_message_pending) {
    return false;
  }

  *out_message = view->pending_console_message;
  memset(&view->pending_console_message, 0,
         sizeof(view->pending_console_message));
  view->console_message_pending = false;
  return true;
}

bool ts_ladybird_view_take_cursor_changed(TsLadybirdView *view,
                                          int *out_cursor_type) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!out_cursor_type) {
    set_error(view->runtime, "stub cursor change output is null");
    return false;
  }
  if (!view->cursor_changed) {
    return false;
  }

  *out_cursor_type = view->pending_cursor_type;
  view->pending_cursor_type = 0;
  view->cursor_changed = false;
  return true;
}

bool ts_ladybird_view_take_target_url_changed(TsLadybirdView *view,
                                              char *out_url,
                                              size_t out_url_len) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!out_url || out_url_len == 0) {
    set_error(view->runtime, "stub target URL output is null");
    return false;
  }
  if (!view->target_url_changed) {
    return false;
  }

  copy_c_string(out_url, out_url_len, view->pending_target_url);
  view->pending_target_url[0] = '\0';
  view->target_url_changed = false;
  return true;
}

bool ts_ladybird_view_take_javascript_dialog_request(
    TsLadybirdView *view, TsLadybirdJavaScriptDialogRequest *out_request) {
  if (!out_request) {
    set_error(view ? view->runtime : NULL,
              "stub JavaScript dialog request output is null");
    return false;
  }
  memset(out_request, 0, sizeof(*out_request));
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!view->javascript_dialog_request_pending) {
    return false;
  }

  *out_request = view->pending_javascript_dialog_request;
  memset(&view->pending_javascript_dialog_request, 0,
         sizeof(view->pending_javascript_dialog_request));
  view->javascript_dialog_request_pending = false;
  return true;
}

bool ts_ladybird_view_reply_javascript_dialog(TsLadybirdView *view,
                                              unsigned long long request_id,
                                              bool accepted,
                                              const char *prompt_text) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (view->active_javascript_dialog_request_id == 0 ||
      view->active_javascript_dialog_request_id != request_id) {
    set_error(view->runtime, "stub stale JavaScript dialog request id");
    return false;
  }

  if (strcmp(view->active_javascript_dialog_type, "prompt") == 0 && accepted) {
    copy_c_string(view->pending_console_message.level,
                  sizeof(view->pending_console_message.level), "log");
    copy_c_string(view->pending_console_message.message,
                  sizeof(view->pending_console_message.message),
                  prompt_text ? prompt_text : "");
    view->pending_console_message.line_no = 0;
    copy_c_string(view->pending_console_message.source_id,
                  sizeof(view->pending_console_message.source_id), "<stub>");
    view->console_message_pending = true;
  }

  view->active_javascript_dialog_request_id = 0;
  view->active_javascript_dialog_type[0] = '\0';
  view->did_finish_load = true;
  return true;
}

bool ts_ladybird_view_take_renderer_crashed(
    TsLadybirdView *view, TsLadybirdRendererCrash *out_crash) {
  if (!out_crash) {
    set_error(view ? view->runtime : NULL,
              "stub renderer crash output is null");
    return false;
  }
  memset(out_crash, 0, sizeof(*out_crash));
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }
  if (!view->renderer_crash_pending) {
    return false;
  }

  *out_crash = view->pending_renderer_crash;
  memset(&view->pending_renderer_crash, 0,
         sizeof(view->pending_renderer_crash));
  view->renderer_crash_pending = false;
  return true;
}

bool ts_ladybird_view_crash_current_page_for_testing(TsLadybirdView *view) {
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  copy_c_string(view->pending_renderer_crash.termination_status,
                sizeof(view->pending_renderer_crash.termination_status),
                "crashed");
  view->pending_renderer_crash.termination_status_code = 0;
  copy_c_string(view->pending_renderer_crash.url,
                sizeof(view->pending_renderer_crash.url), view->last_url);
  view->pending_renderer_crash.can_reload = true;
  view->renderer_crash_pending = true;
  view->did_crash = true;
  return true;
}

const char *ts_ladybird_view_last_url(const TsLadybirdView *view) {
  if (!view) {
    return "";
  }
  return view->last_url;
}

bool ts_ladybird_view_did_finish_load(const TsLadybirdView *view) {
  return view && view->did_finish_load;
}

bool ts_ladybird_view_did_crash(const TsLadybirdView *view) {
  return view && view->did_crash;
}

bool ts_ladybird_view_render_surface_probe(
    TsLadybirdView *view, TsLadybirdRenderSurfaceProbe *out_probe) {
  if (!out_probe) {
    set_error(view ? view->runtime : NULL, "stub render surface probe is null");
    return false;
  }

  memset(out_probe, 0, sizeof(*out_probe));
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  return true;
}

bool ts_ladybird_view_export_render_surface(
    TsLadybirdView *view, TsLadybirdRenderSurfaceExport *out_export) {
  if (!out_export) {
    set_error(view ? view->runtime : NULL,
              "stub render surface export output is null");
    return false;
  }

  memset(out_export, 0, sizeof(*out_export));
  if (!view || view->destroyed) {
    set_error(view ? view->runtime : NULL, "stub view is invalid");
    return false;
  }

  return true;
}
