#ifndef TERMSURF_LIBTERMSURF_WEBKIT_TEST_SUPPORT_H
#define TERMSURF_LIBTERMSURF_WEBKIT_TEST_SUPPORT_H

#include "libtermsurf_webkit.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*ts_webkit_test_eval_cb)(const char *result, void *user_data);
typedef void (*ts_webkit_test_task_cb)(void *user_data);

void ts_webkit_test_evaluate_javascript(
    ts_web_contents_t wc,
    const char *script,
    ts_webkit_test_eval_cb callback,
    void *user_data);

void ts_webkit_test_post_delayed_task(double seconds, ts_webkit_test_task_cb callback, void *user_data);

void ts_webkit_test_kill_web_content_process(ts_web_contents_t wc);
int ts_webkit_test_renderer_crash_delegate_count(void);

/* 1 if host window ignoresMouseEvents, 0 if not, -1 if unavailable */
int ts_webkit_test_host_ignores_mouse_events(ts_web_contents_t wc);

uint64_t ts_webkit_test_authoritative_mouse_move_count(ts_web_contents_t wc);
int ts_webkit_test_has_authoritative_mouse_move_selector(ts_web_contents_t wc);

#ifdef __cplusplus
}
#endif

#endif
