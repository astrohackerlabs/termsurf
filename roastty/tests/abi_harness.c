#include <assert.h>
#include <stdint.h>
#include <string.h>

#include "roastty.h"

static void wakeup_cb(void *userdata) {
  (void)userdata;
}

static bool action_cb(roastty_app_t app,
                      roastty_target_s target,
                      roastty_action_s action) {
  (void)app;
  (void)target;
  (void)action;
  return false;
}

static bool read_clipboard_cb(void *userdata,
                              roastty_clipboard_e clipboard,
                              void *state) {
  (void)userdata;
  (void)clipboard;
  (void)state;
  return false;
}

static void confirm_read_clipboard_cb(void *userdata,
                                      const char *str,
                                      void *state,
                                      roastty_clipboard_request_e request) {
  (void)userdata;
  (void)str;
  (void)state;
  (void)request;
}

static void write_clipboard_cb(void *userdata,
                               roastty_clipboard_e clipboard,
                               const roastty_clipboard_content_s *content,
                               size_t len,
                               bool confirm) {
  (void)userdata;
  (void)clipboard;
  (void)content;
  (void)len;
  (void)confirm;
}

static void close_surface_cb(void *userdata, bool process_alive) {
  (void)userdata;
  (void)process_alive;
}

int main(int argc, char **argv) {
  assert(roastty_init((uintptr_t)argc, argv) == ROASTTY_SUCCESS);

  roastty_config_free(NULL);
  roastty_app_free(NULL);
  roastty_surface_free(NULL);
  roastty_config_load_cli_args(NULL);
  roastty_config_load_default_files(NULL);
  roastty_config_load_recursive_files(NULL);
  roastty_config_load_file(NULL, NULL);
  roastty_config_finalize(NULL);
  assert(roastty_config_diagnostics_count(NULL) == 0);
  assert(roastty_config_get_diagnostic(NULL, 0).message != NULL);
  assert(roastty_app_userdata(NULL) == NULL);
  roastty_app_tick(NULL);
  roastty_app_set_focus(NULL, true);
  roastty_app_set_color_scheme(NULL, ROASTTY_COLOR_SCHEME_DARK);
  roastty_app_update_config(NULL, NULL);
  assert(!roastty_app_needs_confirm_quit(NULL));
  assert(!roastty_app_has_global_keybinds(NULL));
  assert(roastty_surface_userdata(NULL) == NULL);
  assert(roastty_surface_app(NULL) == NULL);
  roastty_surface_update_config(NULL, NULL);
  assert(!roastty_surface_needs_confirm_quit(NULL));
  assert(!roastty_surface_process_exited(NULL));
  roastty_surface_set_content_scale(NULL, 1.0, 1.0);
  roastty_surface_set_focus(NULL, true);
  roastty_surface_set_occlusion(NULL, true);
  roastty_surface_set_color_scheme(NULL, ROASTTY_COLOR_SCHEME_DARK);
  roastty_surface_set_size(NULL, 1, 1);
  roastty_surface_size_s null_size = roastty_surface_size(NULL);
  assert(null_size.width_px == 0);
  assert(null_size.height_px == 0);
  assert(null_size.columns == 0);
  assert(null_size.rows == 0);
  assert(null_size.cell_width_px == 0);
  assert(null_size.cell_height_px == 0);
  assert(roastty_surface_foreground_pid(NULL) == 0);
  roastty_surface_request_close(NULL);

  roastty_info_s info = roastty_info();
  assert(info.version != NULL);
  assert(info.version_len > 0);

  roastty_config_t config = roastty_config_new();
  assert(config != NULL);
  roastty_config_load_cli_args(config);
  roastty_config_load_default_files(config);
  roastty_config_load_recursive_files(config);
  roastty_config_load_file(config, "/tmp/nonexistent-roastty-config");
  roastty_config_finalize(config);
  assert(roastty_config_diagnostics_count(config) == 0);
  roastty_diagnostic_s diagnostic = roastty_config_get_diagnostic(config, 0);
  assert(diagnostic.message != NULL);

  roastty_config_t clone = roastty_config_clone(config);
  assert(clone != NULL);
  roastty_config_free(clone);

  roastty_string_s open_path = roastty_config_open_path();
  assert(open_path.ptr != NULL);
  assert(open_path.len == strlen("roastty-config"));
  assert(open_path.sentinel == false);
  roastty_string_free(open_path);

  uintptr_t app_userdata = 0xA991;
  roastty_runtime_config_s runtime = {
      .userdata = (void *)app_userdata,
      .supports_selection_clipboard = true,
      .wakeup_cb = wakeup_cb,
      .action_cb = action_cb,
      .read_clipboard_cb = read_clipboard_cb,
      .confirm_read_clipboard_cb = confirm_read_clipboard_cb,
      .write_clipboard_cb = write_clipboard_cb,
      .close_surface_cb = close_surface_cb,
  };

  roastty_app_t app = roastty_app_new(&runtime, config);
  assert(app != NULL);
  assert((uintptr_t)roastty_app_userdata(app) == app_userdata);
  roastty_app_tick(app);
  roastty_app_set_focus(app, true);
  roastty_app_set_color_scheme(app, ROASTTY_COLOR_SCHEME_DARK);
  roastty_app_update_config(app, config);
  assert(!roastty_app_needs_confirm_quit(app));
  assert(!roastty_app_has_global_keybinds(app));

  uintptr_t surface_userdata = 0x5A5A;
  roastty_surface_config_s surface_config = roastty_surface_config_new();
  surface_config.userdata = (void *)surface_userdata;
  surface_config.scale_factor = 2.0;
  surface_config.context = ROASTTY_SURFACE_CONTEXT_WINDOW;

  roastty_app_t app_with_null_runtime = roastty_app_new(NULL, config);
  assert(app_with_null_runtime != NULL);
  assert(roastty_app_userdata(app_with_null_runtime) == NULL);
  roastty_app_free(app_with_null_runtime);

  roastty_surface_t surface_with_null_config = roastty_surface_new(app, NULL);
  assert(surface_with_null_config != NULL);
  assert(roastty_surface_app(surface_with_null_config) == app);
  roastty_surface_free(surface_with_null_config);

  assert(roastty_surface_new(NULL, &surface_config) == NULL);

  roastty_surface_t surface = roastty_surface_new(app, &surface_config);
  assert(surface != NULL);
  assert(roastty_surface_app(surface) == app);
  assert((uintptr_t)roastty_surface_userdata(surface) == surface_userdata);

  roastty_surface_set_content_scale(surface, 2.0, 2.0);
  roastty_surface_set_focus(surface, true);
  roastty_surface_set_occlusion(surface, false);
  roastty_surface_set_color_scheme(surface, ROASTTY_COLOR_SCHEME_LIGHT);
  roastty_surface_set_size(surface, 1024, 768);

  roastty_surface_size_s size = roastty_surface_size(surface);
  assert(size.width_px == 1024);
  assert(size.height_px == 768);
  assert(size.columns == 0);
  assert(size.rows == 0);
  assert(size.cell_width_px == 0);
  assert(size.cell_height_px == 0);

  assert(roastty_surface_foreground_pid(surface) == 0);
  assert(!roastty_surface_needs_confirm_quit(surface));
  assert(!roastty_surface_process_exited(surface));

  roastty_string_s tty_name = roastty_surface_tty_name(surface);
  assert(tty_name.ptr != NULL);
  assert(tty_name.len == strlen("roastty-skeleton-tty"));
  assert(tty_name.sentinel == true);
  roastty_string_free(tty_name);

  roastty_surface_request_close(surface);
  roastty_surface_free(surface);

  roastty_string_s empty_tty = roastty_surface_tty_name(NULL);
  assert(empty_tty.ptr == NULL);
  assert(empty_tty.len == 0);
  assert(empty_tty.sentinel == false);
  roastty_string_free(empty_tty);

  for (int i = 0; i < 16; i++) {
    roastty_surface_t loop_surface = roastty_surface_new(app, &surface_config);
    assert(loop_surface != NULL);
    roastty_surface_free(loop_surface);
  }

  roastty_app_free(app);
  roastty_config_free(config);
  return 0;
}
