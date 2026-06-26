#ifndef TERMSURF_LIBTERMSURF_WEBKIT_H
#define TERMSURF_LIBTERMSURF_WEBKIT_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void *ts_browser_context_t;
typedef void *ts_web_contents_t;

typedef void (*ts_initialized_cb)(void *user_data);
typedef void (*ts_task_cb)(void *user_data);
typedef void (*ts_tab_ready_cb)(ts_web_contents_t wc, int tab_id, void *user_data);
typedef void (*ts_ca_context_id_cb)(ts_web_contents_t wc, uint32_t context_id, int width, int height, void *user_data);
typedef void (*ts_url_changed_cb)(ts_web_contents_t wc, const char *url, void *user_data);
typedef void (*ts_loading_state_cb)(ts_web_contents_t wc, const char *url, int loading, void *user_data);
typedef void (*ts_title_changed_cb)(ts_web_contents_t wc, const char *title, void *user_data);
typedef void (*ts_cursor_changed_cb)(ts_web_contents_t wc, int cursor, void *user_data);
typedef void (*ts_target_url_changed_cb)(ts_web_contents_t wc, const char *url, void *user_data);
typedef void (*ts_javascript_dialog_request_cb)(
    ts_web_contents_t wc,
    uint64_t request_id,
    const char *dialog_type,
    const char *origin_url,
    const char *message,
    const char *default_prompt_text,
    void *user_data);
typedef void (*ts_console_message_cb)(
    ts_web_contents_t wc,
    const char *level,
    const char *message,
    int line_number,
    const char *source,
    void *user_data);
typedef void (*ts_http_auth_request_cb)(
    ts_web_contents_t wc,
    uint64_t request_id,
    const char *url,
    const char *auth_scheme,
    const char *challenger,
    const char *realm,
    bool is_proxy,
    bool first_auth_attempt,
    bool is_primary_main_frame_navigation,
    bool is_navigation,
    void *user_data);
typedef void (*ts_renderer_crashed_cb)(
    ts_web_contents_t wc,
    const char *reason,
    int exit_code,
    const char *url,
    bool visible,
    void *user_data);
typedef void (*ts_render_probe_cb)(
    ts_web_contents_t wc,
    const char *method,
    const char *status,
    int width,
    int height,
    int magenta,
    int cyan,
    int yellow,
    int webkit_green,
    const char *error,
    void *user_data);
typedef void (*ts_webkit_test_task_cb)(void *user_data);

int ts_content_main(int argc, const char *const *argv);
void ts_set_on_initialized(ts_initialized_cb callback, void *user_data);
void ts_post_task(ts_task_cb task, void *user_data);
void ts_quit(void);

ts_browser_context_t ts_create_browser_context(const char *path);
ts_browser_context_t ts_create_incognito_browser_context(void);
void ts_destroy_browser_context(ts_browser_context_t ctx);

ts_web_contents_t ts_create_web_contents(
    ts_browser_context_t ctx,
    const char *url,
    int width,
    int height,
    bool dark);
ts_web_contents_t ts_create_devtools_web_contents(
    ts_browser_context_t ctx,
    int inspected_tab_id,
    int width,
    int height,
    bool dark);
void ts_destroy_web_contents(ts_web_contents_t wc);

void ts_load_url(ts_web_contents_t wc, const char *url);

void ts_forward_mouse_event(
    ts_web_contents_t wc,
    int type,
    int button,
    int x,
    int y,
    int click_count,
    int modifiers);
void ts_forward_mouse_move(ts_web_contents_t wc, int x, int y, int modifiers);
void ts_forward_scroll_event(
    ts_web_contents_t wc,
    int x,
    int y,
    float delta_x,
    float delta_y,
    int phase,
    int momentum_phase,
    bool precise,
    int modifiers);
void ts_forward_key_event(ts_web_contents_t wc, int type, int keycode, const char *utf8, int modifiers);

void ts_set_focus(ts_web_contents_t wc, bool focused);
void ts_set_gui_active(ts_web_contents_t wc, bool active, const char *reason);
void ts_set_color_scheme(ts_web_contents_t wc, bool dark);
void ts_set_view_size(
    ts_web_contents_t wc,
    int width,
    int height,
    double screen_x,
    double screen_y,
    double screen_width,
    double screen_height,
    double screen_scale);

bool ts_reply_javascript_dialog(
    ts_web_contents_t wc,
    uint64_t request_id,
    bool accepted,
    const char *prompt_text);
bool ts_reply_http_auth(
    ts_web_contents_t wc,
    uint64_t request_id,
    bool accepted,
    const char *username,
    const char *password);

void ts_set_on_tab_ready(ts_tab_ready_cb cb, void *user_data);
void ts_set_on_ca_context_id(ts_ca_context_id_cb cb, void *user_data);
void ts_set_on_url_changed(ts_url_changed_cb cb, void *user_data);
void ts_set_on_loading_state(ts_loading_state_cb cb, void *user_data);
void ts_set_on_title_changed(ts_title_changed_cb cb, void *user_data);
void ts_set_on_cursor_changed(ts_cursor_changed_cb cb, void *user_data);
void ts_set_on_target_url_changed(ts_target_url_changed_cb cb, void *user_data);
void ts_set_on_javascript_dialog_request(ts_javascript_dialog_request_cb cb, void *user_data);
void ts_set_on_console_message(ts_console_message_cb cb, void *user_data);
void ts_set_on_http_auth_request(ts_http_auth_request_cb cb, void *user_data);
void ts_set_on_renderer_crashed(ts_renderer_crashed_cb cb, void *user_data);
void ts_set_on_render_probe(ts_render_probe_cb cb, void *user_data);

void ts_webkit_test_capture_render_probe(ts_web_contents_t wc);
void ts_webkit_test_post_delayed_task(double seconds, ts_webkit_test_task_cb callback, void *user_data);
int ts_webkit_test_write_pdf_save(
    const char *download_dir,
    const char *suggested_filename,
    const uint8_t *data,
    size_t data_len,
    char *out_path,
    size_t out_path_len);

#ifdef __cplusplus
}
#endif

#endif
