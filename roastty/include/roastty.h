#ifndef ROASTTY_H
#define ROASTTY_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#ifdef ROASTTY_STATIC
#define ROASTTY_API
#else
#define ROASTTY_API __declspec(dllimport)
#endif
#else
#define ROASTTY_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef void* roastty_app_t;
typedef void* roastty_config_t;
typedef void* roastty_surface_t;

enum {
  ROASTTY_SUCCESS = 0,
};

typedef enum {
  ROASTTY_BUILD_MODE_DEBUG,
  ROASTTY_BUILD_MODE_RELEASE_SAFE,
  ROASTTY_BUILD_MODE_RELEASE_FAST,
  ROASTTY_BUILD_MODE_RELEASE_SMALL,
} roastty_build_mode_e;

typedef struct {
  roastty_build_mode_e build_mode;
  const char* version;
  uintptr_t version_len;
} roastty_info_s;

typedef struct {
  const char* message;
} roastty_diagnostic_s;

typedef struct {
  const char* ptr;
  uintptr_t len;
  bool sentinel;
} roastty_string_s;

typedef struct {
  const char* key;
  const char* value;
} roastty_env_var_s;

typedef enum {
  ROASTTY_PLATFORM_INVALID,
  ROASTTY_PLATFORM_MACOS,
  ROASTTY_PLATFORM_IOS,
} roastty_platform_e;

typedef struct {
  void* nsview;
} roastty_platform_macos_s;

typedef struct {
  void* uiview;
} roastty_platform_ios_s;

typedef union {
  roastty_platform_macos_s macos;
  roastty_platform_ios_s ios;
} roastty_platform_u;

typedef enum {
  ROASTTY_SURFACE_CONTEXT_WINDOW = 0,
  ROASTTY_SURFACE_CONTEXT_TAB = 1,
  ROASTTY_SURFACE_CONTEXT_SPLIT = 2,
} roastty_surface_context_e;

typedef struct {
  roastty_platform_e platform_tag;
  roastty_platform_u platform;
  void* userdata;
  double scale_factor;
  float font_size;
  const char* working_directory;
  const char* command;
  roastty_env_var_s* env_vars;
  size_t env_var_count;
  const char* initial_input;
  bool wait_after_command;
  roastty_surface_context_e context;
} roastty_surface_config_s;

typedef struct {
  uint16_t columns;
  uint16_t rows;
  uint32_t width_px;
  uint32_t height_px;
  uint32_t cell_width_px;
  uint32_t cell_height_px;
} roastty_surface_size_s;

typedef enum {
  ROASTTY_CLIPBOARD_STANDARD,
  ROASTTY_CLIPBOARD_SELECTION,
} roastty_clipboard_e;

typedef enum {
  ROASTTY_CLIPBOARD_REQUEST_STANDARD,
  ROASTTY_CLIPBOARD_REQUEST_SELECTION,
} roastty_clipboard_request_e;

typedef struct {
  const char* mime;
  const char* data;
} roastty_clipboard_content_s;

typedef enum {
  ROASTTY_TARGET_APP,
  ROASTTY_TARGET_SURFACE,
} roastty_target_tag_e;

typedef struct {
  roastty_target_tag_e tag;
  roastty_surface_t surface;
} roastty_target_s;

typedef struct {
  int tag;
  uintptr_t storage[8];
} roastty_action_s;

typedef enum {
  ROASTTY_COLOR_SCHEME_LIGHT,
  ROASTTY_COLOR_SCHEME_DARK,
} roastty_color_scheme_e;

typedef void (*roastty_runtime_wakeup_cb)(void*);
typedef bool (*roastty_runtime_action_cb)(roastty_app_t,
                                          roastty_target_s,
                                          roastty_action_s);
typedef bool (*roastty_runtime_read_clipboard_cb)(void*,
                                                  roastty_clipboard_e,
                                                  void*);
typedef void (*roastty_runtime_confirm_read_clipboard_cb)(
    void*,
    const char*,
    void*,
    roastty_clipboard_request_e);
typedef void (*roastty_runtime_write_clipboard_cb)(void*,
                                                   roastty_clipboard_e,
                                                   const roastty_clipboard_content_s*,
                                                   size_t,
                                                   bool);
typedef void (*roastty_runtime_close_surface_cb)(void*, bool);

typedef struct {
  void* userdata;
  bool supports_selection_clipboard;
  roastty_runtime_wakeup_cb wakeup_cb;
  roastty_runtime_action_cb action_cb;
  roastty_runtime_read_clipboard_cb read_clipboard_cb;
  roastty_runtime_confirm_read_clipboard_cb confirm_read_clipboard_cb;
  roastty_runtime_write_clipboard_cb write_clipboard_cb;
  roastty_runtime_close_surface_cb close_surface_cb;
} roastty_runtime_config_s;

ROASTTY_API int roastty_init(uintptr_t, char**);
ROASTTY_API roastty_info_s roastty_info(void);
ROASTTY_API void roastty_string_free(roastty_string_s);

ROASTTY_API roastty_config_t roastty_config_new(void);
ROASTTY_API void roastty_config_free(roastty_config_t);
ROASTTY_API roastty_config_t roastty_config_clone(roastty_config_t);
ROASTTY_API void roastty_config_load_cli_args(roastty_config_t);
ROASTTY_API void roastty_config_load_file(roastty_config_t, const char*);
ROASTTY_API void roastty_config_load_default_files(roastty_config_t);
ROASTTY_API void roastty_config_load_recursive_files(roastty_config_t);
ROASTTY_API void roastty_config_finalize(roastty_config_t);
ROASTTY_API uint32_t roastty_config_diagnostics_count(roastty_config_t);
ROASTTY_API roastty_diagnostic_s roastty_config_get_diagnostic(roastty_config_t,
                                                               uint32_t);
ROASTTY_API roastty_string_s roastty_config_open_path(void);

ROASTTY_API roastty_app_t roastty_app_new(const roastty_runtime_config_s*,
                                          roastty_config_t);
ROASTTY_API void roastty_app_free(roastty_app_t);
ROASTTY_API void roastty_app_tick(roastty_app_t);
ROASTTY_API void* roastty_app_userdata(roastty_app_t);
ROASTTY_API void roastty_app_set_focus(roastty_app_t, bool);
ROASTTY_API void roastty_app_update_config(roastty_app_t, roastty_config_t);
ROASTTY_API bool roastty_app_needs_confirm_quit(roastty_app_t);
ROASTTY_API bool roastty_app_has_global_keybinds(roastty_app_t);
ROASTTY_API void roastty_app_set_color_scheme(roastty_app_t,
                                              roastty_color_scheme_e);

ROASTTY_API roastty_surface_config_s roastty_surface_config_new(void);
ROASTTY_API roastty_surface_t roastty_surface_new(roastty_app_t,
                                                  const roastty_surface_config_s*);
ROASTTY_API void roastty_surface_free(roastty_surface_t);
ROASTTY_API void* roastty_surface_userdata(roastty_surface_t);
ROASTTY_API roastty_app_t roastty_surface_app(roastty_surface_t);
ROASTTY_API void roastty_surface_update_config(roastty_surface_t,
                                               roastty_config_t);
ROASTTY_API bool roastty_surface_needs_confirm_quit(roastty_surface_t);
ROASTTY_API bool roastty_surface_process_exited(roastty_surface_t);
ROASTTY_API void roastty_surface_set_content_scale(roastty_surface_t,
                                                   double,
                                                   double);
ROASTTY_API void roastty_surface_set_focus(roastty_surface_t, bool);
ROASTTY_API void roastty_surface_set_occlusion(roastty_surface_t, bool);
ROASTTY_API void roastty_surface_set_size(roastty_surface_t, uint32_t, uint32_t);
ROASTTY_API roastty_surface_size_s roastty_surface_size(roastty_surface_t);
ROASTTY_API uint64_t roastty_surface_foreground_pid(roastty_surface_t);
ROASTTY_API roastty_string_s roastty_surface_tty_name(roastty_surface_t);
ROASTTY_API void roastty_surface_set_color_scheme(roastty_surface_t,
                                                  roastty_color_scheme_e);
ROASTTY_API void roastty_surface_request_close(roastty_surface_t);

#ifdef __cplusplus
}
#endif

#endif
