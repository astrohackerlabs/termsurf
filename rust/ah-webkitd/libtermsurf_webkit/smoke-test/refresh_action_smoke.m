#import <AppKit/AppKit.h>

#include "libtermsurf_webkit.h"
#include "test_support.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum Phase { PhaseInitial, PhaseReload500, PhaseCrash, PhaseCrashReload, PhaseCleanup, PhaseDone };

struct Tab {
    ts_web_contents_t view;
    char title[128];
    bool back;
    bool forward;
    bool refresh;
    bool initial_disabled;
    bool crashed;
    bool crash_reloadable;
    int loading_edges;
    int terminal_edges;
};

struct State {
    ts_browser_context_t context;
    struct Tab tabs[2];
    enum Phase phase;
    int creating_index;
    bool cleanup_scheduled;
    bool finished;
    char a_url[1024];
    char b_url[1024];
};

static struct State *global_state;

static void fail(const char *reason)
{
    fprintf(stderr, "REFRESH_ACTION_SMOKE_FAIL engine=webkit phase=%d reason=%s a_title=%s b_title=%s\n",
        global_state ? global_state->phase : -1, reason ?: "", global_state ? global_state->tabs[0].title : "",
        global_state ? global_state->tabs[1].title : "");
    fflush(stderr);
    exit(1);
}

static int tab_index(struct State *state, ts_web_contents_t view)
{
    if (state->tabs[0].view == view)
        return 0;
    if (state->tabs[1].view == view)
        return 1;
    fail("unknown_tab");
    return -1;
}

static bool ready(struct Tab *tab, const char *title)
{
    return !strcmp(tab->title, title) && !tab->back && !tab->forward && tab->refresh;
}

static bool peer_unchanged(struct State *state)
{
    struct Tab *peer = &state->tabs[1];
    return ready(peer, "B reload=1 status=200") && !peer->crashed
        && peer->loading_edges == 1 && peer->terminal_edges == 1;
}

static void advance(struct State *state);

static void cleanup_and_finish(void *user_data)
{
    struct State *state = user_data;
    ts_web_contents_t stale = state->tabs[0].view;
    ts_destroy_web_contents(stale);
    state->tabs[0].view = NULL;
    if (ts_navigation_action(stale, "refresh"))
        fail("closed_tab_accepted_refresh");
    if (!peer_unchanged(state))
        fail("closed_tab_action_mutated_peer");
    ts_destroy_web_contents(state->tabs[1].view);
    state->tabs[1].view = NULL;
    ts_destroy_browser_context(state->context);
    state->context = NULL;
    state->phase = PhaseDone;
    state->finished = true;
    puts("REFRESH_ACTION_SMOKE_PASS engine=webkit tabs=2 reload=1 capability=1 history_unchanged=1 request_correlation=1 disabled=1 isolation=1 failed_reload=1 crash_recovery=1 cleanup=1 future_actions_rejected=1");
    fflush(stdout);
    ts_quit();
}

static void advance(struct State *state)
{
    struct Tab *a = &state->tabs[0];
    switch (state->phase) {
    case PhaseInitial:
        if (ready(a, "A reload=1 status=200") && peer_unchanged(state)
            && a->initial_disabled && state->tabs[1].initial_disabled) {
            if (ts_navigation_action(a->view, "future") || ts_navigation_action(NULL, "refresh"))
                fail("future_or_null_action_accepted");
            state->phase = PhaseReload500;
            if (!ts_navigation_action(a->view, "refresh"))
                fail("refresh_rejected");
        }
        break;
    case PhaseReload500:
        if (ready(a, "A reload=2 status=500") && a->loading_edges == 2
            && a->terminal_edges == 2 && peer_unchanged(state)) {
            state->phase = PhaseCrash;
            ts_webkit_test_kill_web_content_process(a->view);
        }
        break;
    case PhaseCrash:
        if (a->crashed && a->crash_reloadable && !a->back && !a->forward
            && a->refresh && peer_unchanged(state)) {
            state->phase = PhaseCrashReload;
            if (!ts_navigation_action(a->view, "refresh"))
                fail("crash_refresh_rejected");
        }
        break;
    case PhaseCrashReload:
        if (ready(a, "A reload=3 status=200") && a->loading_edges == 3
            && a->terminal_edges == 3 && peer_unchanged(state) && !state->cleanup_scheduled) {
            state->cleanup_scheduled = true;
            state->phase = PhaseCleanup;
            ts_webkit_test_post_delayed_task(0.0, cleanup_and_finish, state);
        }
        break;
    case PhaseCleanup:
    case PhaseDone:
        break;
    }
}

static void on_tab_ready(ts_web_contents_t view, int tab_id, void *user_data)
{
    (void)tab_id;
    struct State *state = user_data;
    if (state->creating_index < 0 || state->creating_index > 1)
        fail("tab_ready_without_creator");
    state->tabs[state->creating_index].view = view;
    state->tabs[state->creating_index].initial_disabled = !ts_navigation_action(view, "refresh");
}

static void on_title_changed(ts_web_contents_t view, const char *title, void *user_data)
{
    struct State *state = user_data;
    struct Tab *tab = &state->tabs[tab_index(state, view)];
    snprintf(tab->title, sizeof(tab->title), "%s", title ?: "");
    advance(state);
}

static void on_loading_state(ts_web_contents_t view, const char *url, int loading, void *user_data)
{
    (void)url;
    struct State *state = user_data;
    struct Tab *tab = &state->tabs[tab_index(state, view)];
    if (loading)
        tab->loading_edges++;
    else
        tab->terminal_edges++;
    advance(state);
}

static void on_navigation_state(ts_web_contents_t view, bool back, bool forward, bool refresh, void *user_data)
{
    struct State *state = user_data;
    struct Tab *tab = &state->tabs[tab_index(state, view)];
    tab->back = back;
    tab->forward = forward;
    tab->refresh = refresh;
    advance(state);
}

static void on_renderer_crashed(ts_web_contents_t view, const char *reason, int exit_code, const char *url, bool can_reload, void *user_data)
{
    (void)reason;
    (void)exit_code;
    (void)url;
    struct State *state = user_data;
    struct Tab *tab = &state->tabs[tab_index(state, view)];
    tab->crashed = true;
    tab->crash_reloadable = can_reload;
    advance(state);
}

static void watchdog(void *user_data)
{
    struct State *state = user_data;
    if (!state->finished)
        fail("timeout");
}

static void on_initialized(void *user_data)
{
    struct State *state = user_data;
    state->context = ts_create_incognito_browser_context();
    state->creating_index = 0;
    ts_web_contents_t a = ts_create_web_contents(state->context, state->a_url, 640, 480, false);
    state->creating_index = 1;
    ts_web_contents_t b = ts_create_web_contents(state->context, state->b_url, 640, 480, false);
    state->creating_index = -1;
    if (!state->context || !a || !b || a != state->tabs[0].view || b != state->tabs[1].view || a == b)
        fail("distinct_tabs_not_created");
    ts_set_view_size(a, 640, 480, 0, 0, 640, 480, 1);
    ts_set_view_size(b, 640, 480, 640, 0, 640, 480, 1);
    ts_webkit_test_post_delayed_task(45.0, watchdog, state);
}

int main(int argc, const char **argv)
{
    @autoreleasepool {
        if (argc != 2)
            return 2;
        struct State *state = calloc(1, sizeof(*state));
        if (!state)
            return 2;
        global_state = state;
        state->creating_index = -1;
        snprintf(state->a_url, sizeof(state->a_url), "%s/a", argv[1]);
        snprintf(state->b_url, sizeof(state->b_url), "%s/b", argv[1]);
        ts_set_on_initialized(on_initialized, state);
        ts_set_on_tab_ready(on_tab_ready, state);
        ts_set_on_loading_state(on_loading_state, state);
        ts_set_on_navigation_state(on_navigation_state, state);
        ts_set_on_title_changed(on_title_changed, state);
        ts_set_on_renderer_crashed(on_renderer_crashed, state);
        return ts_content_main(argc, argv);
    }
}
