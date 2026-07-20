#import <AppKit/AppKit.h>

#include "libtermsurf_webkit.h"
#include "test_support.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum Phase {
    PhaseInitial,
    PhaseFirstA2,
    PhaseFirstBack,
    PhaseDisabled,
    PhaseFirstForward,
    PhaseBackBeforeFresh,
    PhaseSecondA2,
    PhasePushState,
    PhaseSameDocumentBack,
    PhaseSameDocumentForward,
    PhaseCrash,
    PhaseRecoveryA1,
    PhaseRecoveryBack,
    PhaseCleanup,
    PhaseDone,
};

struct NavigationState {
    bool can_go_back;
    bool can_go_forward;
    bool can_refresh;
    int events;
    bool precommit_false;
};

struct State {
    ts_browser_context_t context;
    ts_web_contents_t views[2];
    struct NavigationState navigation[2];
    bool initial_loaded[2];
    int creating_index;
    enum Phase phase;
    bool query_pending;
    int disabled_event_count;
    bool crash_false_seen;
    bool crash_callback_seen;
    bool crash_peer_query_started;
    bool cleanup_started;
    int late_callbacks;
    bool finished;
    char a1[1024];
    char a2[1024];
    char b1[1024];
};

static struct State *global_state;

static void fail(const char *reason);
static void query_page(struct State *state, ts_web_contents_t view, ts_webkit_test_eval_cb callback);
static void phase_page_ready(const char *result, void *user_data);
static void maybe_finish_crash_observation(struct State *state);

static void cleanup(void)
{
    struct State *state = global_state;
    if (!state)
        return;
    if (state->views[0]) {
        ts_destroy_web_contents(state->views[0]);
        state->views[0] = NULL;
    }
    if (state->views[1]) {
        ts_destroy_web_contents(state->views[1]);
        state->views[1] = NULL;
    }
    if (state->context) {
        ts_destroy_browser_context(state->context);
        state->context = NULL;
    }
}

static void fail(const char *reason)
{
    fprintf(stderr, "FORWARD_ACTION_SMOKE_FAIL engine=webkit phase=%d reason=%s\n",
        global_state ? global_state->phase : -1, reason);
    fflush(stderr);
    exit(1);
}

static int view_index(struct State *state, ts_web_contents_t view)
{
    if (view == state->views[0])
        return 0;
    if (view == state->views[1])
        return 1;
    fail("unknown_view_callback");
    return -1;
}

static void make_url(char *out, size_t size, const char *base, const char *path)
{
    if (snprintf(out, size, "%s%s", base, path) >= (int)size)
        fail("fixture_url_too_long");
}

static void expect_page(const char *result, const char *path, const char *title, const char *marker)
{
    if (!result || !*result || strstr(result, "ERROR:"))
        fail("page_oracle_error");
    if (!strstr(result, path) || !strstr(result, title) || !strstr(result, marker)) {
        fprintf(stderr,
            "page_oracle_mismatch expected_path=%s expected_title=%s expected_marker=%s actual=%s\n",
            path, title, marker, result);
        fail("page_oracle_mismatch");
    }
    printf("FORWARD_PAGE phase=%d path=%s back_a=%d forward_a=%d back_b=%d forward_b=%d result=%s\n",
        global_state->phase,
        path,
        global_state->navigation[0].can_go_back,
        global_state->navigation[0].can_go_forward,
        global_state->navigation[1].can_go_back,
        global_state->navigation[1].can_go_forward,
        result);
    fflush(stdout);
}

static void expect_states(
    struct State *state,
    bool a_back,
    bool a_forward,
    bool b_back,
    bool b_forward,
    const char *where)
{
    if (state->navigation[0].can_go_back != a_back
        || state->navigation[0].can_go_forward != a_forward
        || state->navigation[1].can_go_back != b_back
        || state->navigation[1].can_go_forward != b_forward) {
        fprintf(stderr,
            "state_mismatch where=%s expected=(%d,%d,%d,%d) actual=(%d,%d,%d,%d) events=(%d,%d)\n",
            where,
            a_back,
            a_forward,
            b_back,
            b_forward,
            state->navigation[0].can_go_back,
            state->navigation[0].can_go_forward,
            state->navigation[1].can_go_back,
            state->navigation[1].can_go_forward,
            state->navigation[0].events,
            state->navigation[1].events);
        fail("navigation_state_mismatch");
    }
}

static void query_page(struct State *state, ts_web_contents_t view, ts_webkit_test_eval_cb callback)
{
    state->query_pending = true;
    ts_webkit_test_evaluate_javascript(
        view,
        "window.backSmokeSnapshot ? window.backSmokeSnapshot() : 'ERROR:missing-snapshot'",
        callback,
        state);
}

static void load_a(struct State *state, enum Phase phase, const char *url)
{
    state->phase = phase;
    state->query_pending = false;
    ts_load_url(state->views[0], url);
}

static void finish_after_cleanup(void *user_data)
{
    struct State *state = user_data;
    if (state->late_callbacks != 0)
        fail("late_callback_after_cleanup");
    state->phase = PhaseDone;
    state->finished = true;
    puts("FORWARD_ACTION_SMOKE_PASS engine=webkit tabs=2 history_round_trip=1 back_action=1 forward_action=1 state=1 disabled=1 isolation=1 same_document=1 fresh_navigation_clears_forward=1 wrong_tab_rejected=1 crash_recovery=1 cleanup=1 future_actions_rejected=1");
    fflush(stdout);
    ts_quit();
}

static void finish(struct State *state)
{
    state->phase = PhaseCleanup;
    state->cleanup_started = true;
    ts_destroy_web_contents(state->views[0]);
    state->views[0] = NULL;
    ts_destroy_web_contents(state->views[1]);
    state->views[1] = NULL;
    ts_destroy_browser_context(state->context);
    state->context = NULL;
    ts_webkit_test_post_delayed_task(0.0, finish_after_cleanup, state);
}

static void start_disabled_checks(struct State *state)
{
    state->phase = PhaseDisabled;
    state->disabled_event_count = state->navigation[0].events;
    if (ts_navigation_action(state->views[0], "back"))
        fail("disabled_back_accepted");
    if (ts_navigation_action(state->views[1], "forward"))
        fail("wrong_tab_forward_accepted");
    if (ts_navigation_action(state->views[0], "unknown"))
        fail("unknown_action_accepted");
    if (ts_navigation_action(state->views[0], NULL))
        fail("null_action_accepted");
    if (ts_navigation_action(NULL, "back"))
        fail("null_view_accepted");
    query_page(state, state->views[0], phase_page_ready);
}

static void pushed_state_ready(const char *result, void *user_data)
{
    struct State *state = user_data;
    state->query_pending = false;
    expect_page(result, "/a2#state", "A2 pushed", "marker=a2");
    expect_states(state, true, false, false, false, "after-push-state");
    state->phase = PhaseSameDocumentBack;
    if (!ts_navigation_action(state->views[0], "back"))
        fail("same_document_back_rejected");
}

static void after_crash_peer_ready(const char *result, void *user_data)
{
    struct State *state = user_data;
    state->query_pending = false;
    expect_page(result, "/b1", "B1", "marker=b1");
    expect_states(state, false, false, false, false, "after-a-crash");
    load_a(state, PhaseRecoveryA1, state->a1);
}

static void maybe_finish_crash_observation(struct State *state)
{
    if (!state->crash_false_seen || !state->crash_callback_seen || state->crash_peer_query_started)
        return;
    state->crash_peer_query_started = true;
    query_page(state, state->views[1], after_crash_peer_ready);
}

static void phase_page_ready(const char *result, void *user_data)
{
    struct State *state = user_data;
    state->query_pending = false;
    switch (state->phase) {
    case PhaseFirstA2:
        expect_page(result, "/a2", "A2", "marker=a2");
        expect_states(state, true, false, false, false, "first-a2");
        state->phase = PhaseFirstBack;
        if (!ts_navigation_action(state->views[0], "back"))
            fail("first_back_rejected");
        break;
    case PhaseFirstBack:
        expect_page(result, "/a1", "A1", "marker=a1");
        expect_states(state, false, true, false, false, "first-back");
        start_disabled_checks(state);
        break;
    case PhaseDisabled:
        expect_page(result, "/a1", "A1", "marker=a1");
        expect_states(state, false, true, false, false, "disabled-actions");
        if (state->navigation[0].events != state->disabled_event_count)
            fail("disabled_action_fabricated_state");
        state->phase = PhaseFirstForward;
        if (!ts_navigation_action(state->views[0], "forward"))
            fail("first_forward_rejected");
        break;
    case PhaseFirstForward:
        expect_page(result, "/a2", "A2", "marker=a2");
        expect_states(state, true, false, false, false, "first-forward");
        state->phase = PhaseBackBeforeFresh;
        if (!ts_navigation_action(state->views[0], "back"))
            fail("back_before_fresh_rejected");
        break;
    case PhaseBackBeforeFresh:
        expect_page(result, "/a1", "A1", "marker=a1");
        expect_states(state, false, true, false, false, "back-before-fresh");
        load_a(state, PhaseSecondA2, state->a2);
        break;
    case PhaseSecondA2:
        expect_page(result, "/a2", "A2", "marker=a2");
        expect_states(state, true, false, false, false, "second-a2");
        state->phase = PhasePushState;
        state->query_pending = true;
        ts_webkit_test_evaluate_javascript(
            state->views[0],
            "history.pushState({backSmoke:true}, '', '/a2#state');"
            "document.title='A2 pushed';"
            "window.backSmokeSnapshot()",
            pushed_state_ready,
            state);
        break;
    case PhaseSameDocumentBack:
        expect_page(result, "/a2", "A2", "marker=a2");
        if (strstr(result, "#state"))
            fail("same_document_back_kept_fragment");
        expect_states(state, true, true, false, false, "same-document-back");
        state->phase = PhaseSameDocumentForward;
        if (!ts_navigation_action(state->views[0], "forward"))
            fail("same_document_forward_rejected");
        break;
    case PhaseSameDocumentForward:
        expect_page(result, "/a2#state", "A2 pushed", "marker=a2");
        expect_states(state, true, false, false, false, "same-document-forward");
        state->phase = PhaseCrash;
        state->crash_false_seen = false;
        state->crash_callback_seen = false;
        ts_webkit_test_kill_web_content_process(state->views[0]);
        break;
    case PhaseRecoveryA1:
        expect_page(result, "/a1", "A1", "marker=a1");
        expect_states(state, true, false, false, false, "recovery-a1-retained-history");
        state->phase = PhaseRecoveryBack;
        if (!ts_navigation_action(state->views[0], "back"))
            fail("recovery_back_rejected");
        break;
    case PhaseRecoveryBack:
        expect_page(result, "/a2", "A2", "marker=a2");
        expect_states(state, true, true, false, false, "recovery-back-retained-a2");
        finish(state);
        break;
    default:
        fail("unexpected_phase_page_result");
    }
}

static void initial_b_ready(const char *result, void *user_data)
{
    struct State *state = user_data;
    state->query_pending = false;
    expect_page(result, "/b1", "B1", "marker=b1");
    expect_states(state, false, false, false, false, "initial-b1");
    if (!state->navigation[0].precommit_false || !state->navigation[1].precommit_false)
        fail("missing_precommit_false");
    load_a(state, PhaseFirstA2, state->a2);
}

static void initial_a_ready(const char *result, void *user_data)
{
    struct State *state = user_data;
    state->query_pending = false;
    expect_page(result, "/a1", "A1", "marker=a1");
    expect_states(state, false, false, false, false, "initial-a1");
    query_page(state, state->views[1], initial_b_ready);
}

static void on_tab_ready(ts_web_contents_t view, int tab_id, void *user_data)
{
    struct State *state = user_data;
    if (state->cleanup_started) {
        state->late_callbacks++;
        return;
    }
    if (state->creating_index < 0 || state->creating_index > 1)
        fail("tab_ready_without_creator");
    state->views[state->creating_index] = view;
    printf("BACK_TAB_READY index=%d tab_id=%d\n", state->creating_index, tab_id);
    fflush(stdout);
}

static void on_navigation_state(
    ts_web_contents_t view,
    bool can_go_back,
    bool can_go_forward,
    bool can_refresh,
    void *user_data)
{
    struct State *state = user_data;
    if (state->cleanup_started) {
        state->late_callbacks++;
        return;
    }
    int index = view_index(state, view);
    state->navigation[index].can_go_back = can_go_back;
    state->navigation[index].can_go_forward = can_go_forward;
    state->navigation[index].can_refresh = can_refresh;
    state->navigation[index].events++;
    if (!state->initial_loaded[index] && !can_go_back && !can_go_forward)
        state->navigation[index].precommit_false = true;
    printf("FORWARD_STATE phase=%d index=%d can_go_back=%d can_go_forward=%d events=%d\n",
        state->phase, index, can_go_back, can_go_forward, state->navigation[index].events);
    fflush(stdout);
    if (state->phase == PhaseCrash && index == 0 && !can_go_back && !can_go_forward) {
        state->crash_false_seen = true;
        maybe_finish_crash_observation(state);
    }
    if (state->phase == PhaseSameDocumentBack && index == 0 && can_go_back
        && can_go_forward && !state->query_pending) {
        query_page(state, state->views[0], phase_page_ready);
    }
    if (state->phase == PhaseSameDocumentForward && index == 0 && can_go_back
        && !can_go_forward && !state->query_pending) {
        query_page(state, state->views[0], phase_page_ready);
    }
}

static void on_url_changed(ts_web_contents_t view, const char *url, void *user_data)
{
    struct State *state = user_data;
    if (state->cleanup_started) {
        state->late_callbacks++;
        return;
    }
    int index = view_index(state, view);
    printf("BACK_URL phase=%d index=%d url=%s\n", state->phase, index, url ?: "");
    fflush(stdout);
}

static void on_loading_state(ts_web_contents_t view, const char *url, int loading, void *user_data)
{
    (void)url;
    struct State *state = user_data;
    if (state->cleanup_started) {
        state->late_callbacks++;
        return;
    }
    int index = view_index(state, view);
    if (loading)
        return;
    if (state->phase == PhaseInitial) {
        state->initial_loaded[index] = true;
        if (state->initial_loaded[0] && state->initial_loaded[1] && !state->query_pending)
            query_page(state, state->views[0], initial_a_ready);
        return;
    }
    if (index != 0 || state->query_pending)
        return;
    switch (state->phase) {
    case PhaseFirstA2:
    case PhaseFirstBack:
    case PhaseFirstForward:
    case PhaseBackBeforeFresh:
    case PhaseSecondA2:
    case PhaseRecoveryA1:
    case PhaseRecoveryBack:
        query_page(state, state->views[0], phase_page_ready);
        break;
    default:
        break;
    }
}

static void on_renderer_crashed(
    ts_web_contents_t view,
    const char *reason,
    int exit_code,
    const char *url,
    bool can_reload,
    void *user_data)
{
    struct State *state = user_data;
    if (state->cleanup_started) {
        state->late_callbacks++;
        return;
    }
    int index = view_index(state, view);
    printf("BACK_CRASH phase=%d index=%d reason=%s code=%d url=%s can_reload=%d\n",
        state->phase, index, reason ?: "", exit_code, url ?: "", can_reload);
    fflush(stdout);
    if (state->phase != PhaseCrash || index != 0)
        fail("wrong_view_or_phase_crashed");
    state->crash_callback_seen = true;
    maybe_finish_crash_observation(state);
}

static void watchdog(void *user_data)
{
    struct State *state = user_data;
    if (!state->finished)
        fail("watchdog_timeout");
}

static void on_initialized(void *user_data)
{
    struct State *state = user_data;
    state->context = ts_create_incognito_browser_context();
    if (!state->context)
        fail("context_creation");

    state->creating_index = 0;
    ts_web_contents_t returned_a = ts_create_web_contents(state->context, state->a1, 640, 480, false);
    if (!returned_a || returned_a != state->views[0])
        fail("view_a_creation");

    state->creating_index = 1;
    ts_web_contents_t returned_b = ts_create_web_contents(state->context, state->b1, 640, 480, false);
    if (!returned_b || returned_b != state->views[1] || returned_b == returned_a)
        fail("view_b_creation");
    state->creating_index = -1;

    ts_set_view_size(state->views[0], 640, 480, 0, 0, 640, 480, 1);
    ts_set_view_size(state->views[1], 640, 480, 640, 0, 640, 480, 1);
    ts_webkit_test_post_delayed_task(45.0, watchdog, state);
}

int main(int argc, const char **argv)
{
    @autoreleasepool {
        if (argc != 2) {
            fprintf(stderr, "usage: %s http://127.0.0.1:PORT\n", argv[0]);
            return 2;
        }
        struct State *state = calloc(1, sizeof(*state));
        if (!state)
            return 2;
        global_state = state;
        state->creating_index = -1;
        state->phase = PhaseInitial;
        make_url(state->a1, sizeof(state->a1), argv[1], "/a1");
        make_url(state->a2, sizeof(state->a2), argv[1], "/a2");
        make_url(state->b1, sizeof(state->b1), argv[1], "/b1");
        atexit(cleanup);

        ts_set_on_initialized(on_initialized, state);
        ts_set_on_tab_ready(on_tab_ready, state);
        ts_set_on_url_changed(on_url_changed, state);
        ts_set_on_loading_state(on_loading_state, state);
        ts_set_on_navigation_state(on_navigation_state, state);
        ts_set_on_renderer_crashed(on_renderer_crashed, state);
        return ts_content_main(argc, argv);
    }
}
