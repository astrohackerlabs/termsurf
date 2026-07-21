#include "libtermsurf_webkit.h"
#include "test_support.h"

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <unistd.h>
#include <pthread.h>

struct State {
    ts_browser_context_t persistent_context;
    ts_browser_context_t incognito_context;
    ts_web_contents_t web_contents;
    const char *first_url;
    const char *second_url;
    int initialized;
    int tab_ready;
    int context_id_count;
    uint32_t context_id;
    int live_visible_raf_count;
    int live_hidden_raf_count;
    int live_visibility_checked;
    int url_changed;
    int loading_started;
    int loading_finished;
    int title_changed;
    int navigations_finished;
    int resized;
    int focus_checked;
    int input_checked;
    int target_url_checked;
    int target_url_events;
    char target_url_sequence[2][256];
    int hover_checked;
    int hover_link_x;
    int hover_link_y;
    int hover_neutral_x;
    int hover_neutral_y;
    uint64_t hover_move_count_before;
    uint64_t authoritative_mouse_moves;
    int authoritative_selector_checked;
    int drag_checked;
    uint64_t drag_move_count_before;
    int cursor_checked;
    int cursor_events;
    int cursor_sequence[3];
    double cursor_target_x;
    double cursor_target_y;
    int cursor_expected_type;
    int console_checked;
    int console_events;
    char console_levels[4][16];
    char console_messages[4][256];
    char console_sources[4][512];
    int console_lines[4];
    int javascript_dialog_requests;
    int javascript_dialog_checked;
    int stale_javascript_dialog_replies;
    int auth_server_fd;
    int auth_server_port;
    pthread_t auth_server_thread;
    char auth_url[128];
    int http_auth_requests;
    int http_auth_accept_checked;
    int http_auth_reject_checked;
    int stale_http_auth_replies;
    int renderer_crash_checked;
    int renderer_crash_events;
    int renderer_crash_delegate_count_before;
    char renderer_crash_reason[64];
    int renderer_crash_exit_code;
    char renderer_crash_url[512];
    int renderer_crash_visible;
};

static void run_input_sequence(void *user_data);
static void check_logical_focus(void *user_data);
static void run_pointer_key_sequence(void *user_data);
static void run_target_url_sequence(void *user_data);
static void target_url_move_to_link(const char *result, void *user_data);
static void query_hover_on_state(void *user_data);
static void check_hover_on_state(const char *result, void *user_data);
static void query_hover_off_state(void *user_data);
static void check_hover_off_state(const char *result, void *user_data);
static void run_drag_sequence(void *user_data);
static void drag_to_region(const char *result, void *user_data);
static void query_drag_state(void *user_data);
static void check_drag_state(const char *result, void *user_data);
static void check_target_url_sequence(void *user_data);
static void run_cursor_sequence(void *user_data);
static void capture_cursor_sequence(void *user_data);
static void cursor_move_to_element_center(const char *result, void *user_data);
static void cursor_after_pointer(void *user_data);
static void cursor_after_hand(void *user_data);
static void cursor_query_pointer(void *user_data);
static void cursor_query_hand(void *user_data);
static void cursor_query_ibeam(void *user_data);
static void check_cursor_sequence(void *user_data);
static void run_console_sequence(void *user_data);
static void console_sequence_started(const char *result, void *user_data);
static void check_console_sequence(void *user_data);
static void run_javascript_dialog_sequence(void *user_data);
static void run_http_auth_sequence(void *user_data);
static void query_http_auth_accept_state(void *user_data);
static void run_renderer_crash_sequence(void *user_data);
static void check_renderer_crash_sequence(void *user_data);
static void begin_live_visibility_sequence(void *user_data);
static void query_hidden_live_state(void *user_data);
static void query_restored_live_state(void *user_data);
static void on_cursor_changed(ts_web_contents_t wc, int cursor_type, void *user_data);
static void on_console_message(
    ts_web_contents_t wc,
    const char *level,
    const char *message,
    int line_number,
    const char *source,
    void *user_data);
static void on_renderer_crashed(
    ts_web_contents_t wc,
    const char *reason,
    int exit_code,
    const char *url,
    bool visible,
    void *user_data);

static void fail(const char *message)
{
    fprintf(stderr, "SMOKE_FAIL %s\n", message);
    fflush(stderr);
    exit(1);
}

static void write_all(int fd, const char *data)
{
    size_t len = strlen(data);
    while (len) {
        ssize_t written = write(fd, data, len);
        if (written <= 0)
            return;
        data += written;
        len -= (size_t)written;
    }
}

static void respond_unauthorized(int fd)
{
    write_all(fd,
        "HTTP/1.1 401 Unauthorized\r\n"
        "WWW-Authenticate: Basic realm=\"webkit\"\r\n"
        "Content-Length: 0\r\n"
        "Connection: close\r\n"
        "\r\n");
}

static void respond_ok(int fd, const char *body)
{
    char response[512];
    snprintf(response, sizeof(response),
        "HTTP/1.1 200 OK\r\n"
        "Content-Type: text/html\r\n"
        "Content-Length: %zu\r\n"
        "Connection: close\r\n"
        "\r\n"
        "%s",
        strlen(body),
        body);
    write_all(fd, response);
}

static void *auth_server_main(void *user_data)
{
    struct State *state = (struct State *)user_data;
    for (;;) {
        int client = accept(state->auth_server_fd, NULL, NULL);
        if (client < 0)
            return NULL;

        char request[4096];
        ssize_t read_count = read(client, request, sizeof(request) - 1);
        if (read_count <= 0) {
            close(client);
            continue;
        }
        request[read_count] = '\0';

        bool accept_path = strstr(request, "GET /auth-accept ") != NULL;
        bool reject_path = strstr(request, "GET /auth-reject ") != NULL;
        bool authorized = strstr(request, "Authorization: Basic d2Via2l0OnNlY3JldA==") != NULL;
        if (accept_path && authorized)
            respond_ok(client, "<!doctype html><title>WebKit Auth OK</title><body>auth-ok</body>");
        else if (accept_path || reject_path)
            respond_unauthorized(client);
        else
            respond_ok(client, "<!doctype html><title>WebKit Auth Server</title><body>auth-server</body>");

        close(client);
    }
}

static void start_auth_server(struct State *state)
{
    state->auth_server_fd = socket(AF_INET, SOCK_STREAM, 0);
    if (state->auth_server_fd < 0)
        fail("auth server socket failed");

    int reuse = 1;
    setsockopt(state->auth_server_fd, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = 0;
    if (bind(state->auth_server_fd, (struct sockaddr *)&addr, sizeof(addr)) != 0)
        fail("auth server bind failed");
    if (listen(state->auth_server_fd, 16) != 0)
        fail("auth server listen failed");

    socklen_t len = sizeof(addr);
    if (getsockname(state->auth_server_fd, (struct sockaddr *)&addr, &len) != 0)
        fail("auth server getsockname failed");
    state->auth_server_port = ntohs(addr.sin_port);
    snprintf(state->auth_url, sizeof(state->auth_url), "http://127.0.0.1:%d/auth-accept", state->auth_server_port);

    if (pthread_create(&state->auth_server_thread, NULL, auth_server_main, state) != 0)
        fail("auth server thread failed");
}

static void stop_auth_server(struct State *state)
{
    if (state->auth_server_fd >= 0) {
        close(state->auth_server_fd);
        state->auth_server_fd = -1;
        pthread_join(state->auth_server_thread, NULL);
    }
}

static void finish(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (!state->initialized)
        fail("initialized callback missing");
    if (!state->persistent_context || !state->incognito_context)
        fail("context creation failed");
    if (!state->web_contents)
        fail("web contents creation failed");
    if (!state->tab_ready)
        fail("tab ready callback missing");
    if (state->context_id_count < 3)
        fail("ca context id callback missing");
    if (!state->context_id)
        fail("stable ca context id missing");
    if (!state->live_visibility_checked)
        fail("live presentation visibility check missing");
    if (!state->url_changed)
        fail("url changed callback missing");
    if (!state->loading_started || !state->loading_finished)
        fail("loading callbacks missing");
    if (!state->title_changed)
        fail("title changed callback missing");
    if (state->navigations_finished < 2)
        fail("second navigation did not finish");
    if (!state->resized)
        fail("resize callback missing");
    if (!state->focus_checked)
        fail("focus check missing");
    if (!state->input_checked)
        fail("input check missing");
    if (!state->target_url_checked)
        fail("target URL check missing");
    if (!state->hover_checked)
        fail("hover check missing");
    if (!state->drag_checked)
        fail("drag check missing");
    if (!state->cursor_checked)
        fail("cursor check missing");
    if (!state->console_checked)
        fail("console check missing");
    if (state->javascript_dialog_requests != 3)
        fail("javascript dialog request count mismatch");
    if (!state->javascript_dialog_checked)
        fail("javascript dialog check missing");
    if (state->stale_javascript_dialog_replies != 3)
        fail("stale javascript dialog replies were not rejected");
    if (state->http_auth_requests != 2)
        fail("http auth request count mismatch");
    if (!state->http_auth_accept_checked)
        fail("http auth accepted navigation missing");
    if (!state->http_auth_reject_checked)
        fail("http auth rejected navigation missing");
    if (state->stale_http_auth_replies != 2)
        fail("stale http auth replies were not rejected");
    if (!state->renderer_crash_checked)
        fail("renderer crash check missing");

    ts_destroy_web_contents(state->web_contents);
    ts_destroy_browser_context(state->persistent_context);
    ts_destroy_browser_context(state->incognito_context);
    stop_auth_server(state);
    printf("SMOKE_PASS initialized=%d tab_ready=%d ca_context=%d stable_context_id=%u live_visibility=%d url=%d loading_started=%d loading_finished=%d title=%d navigations=%d resized=%d focus=%d input=%d hover=%d authoritative_moves=%llu selector=%d target_url=%d cursor=%d drag=%d console=%d js_dialogs=%d http_auth=%d renderer_crash=%d\n",
        state->initialized,
        state->tab_ready,
        state->context_id_count,
        state->context_id,
        state->live_visibility_checked,
        state->url_changed,
        state->loading_started,
        state->loading_finished,
        state->title_changed,
        state->navigations_finished,
        state->resized,
        state->focus_checked,
        state->input_checked,
        state->hover_checked,
        (unsigned long long)state->authoritative_mouse_moves,
        state->authoritative_selector_checked,
        state->target_url_checked,
        state->cursor_checked,
        state->drag_checked,
        state->console_checked,
        state->javascript_dialog_checked,
        state->http_auth_accept_checked && state->http_auth_reject_checked,
        state->renderer_crash_checked);
    fflush(stdout);
    ts_quit();
}

static void resize_after_navigation(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_set_view_size(state->web_contents, 640, 480, 0, 0, 640, 480, 2.0);
    state->resized = 1;
    ts_post_task(run_input_sequence, state);
}

static void parse_live_state(const char *result, int *count, char *visibility, size_t visibility_size)
{
    if (!result || sscanf(result, "%d:%31s", count, visibility) != 2)
        fail("live presentation state result missing");
    visibility[visibility_size - 1] = '\0';
}

static void check_restored_live_state(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    int count = 0;
    char visibility[32] = { 0 };
    parse_live_state(result, &count, visibility, sizeof(visibility));
    printf("CALLBACK live_state phase=restored count=%d visibility=%s\n", count, visibility);
    if (strcmp(visibility, "visible") != 0)
        fail("restored external presentation did not become visible");
    if (count - state->live_hidden_raf_count < 10)
        fail("requestAnimationFrame did not resume for restored external presentation");
    state->live_visibility_checked = 1;
    ts_post_task(resize_after_navigation, state);
}

static void query_restored_live_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "String(window.__termsurfRafCount) + ':' + document.visibilityState",
        check_restored_live_state,
        state);
}

static void check_hidden_live_state(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    char visibility[32] = { 0 };
    parse_live_state(result, &state->live_hidden_raf_count, visibility, sizeof(visibility));
    printf("CALLBACK live_state phase=hidden count=%d visibility=%s\n", state->live_hidden_raf_count, visibility);
    if (strcmp(visibility, "hidden") != 0)
        fail("hidden external presentation did not become hidden");
    if (state->live_hidden_raf_count - state->live_visible_raf_count > 3)
        fail("requestAnimationFrame continued at foreground rate while hidden");
    ts_set_presentation_visible(state->web_contents, true);
    ts_webkit_test_post_delayed_task(1.0, query_restored_live_state, state);
}

static void query_hidden_live_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "String(window.__termsurfRafCount) + ':' + document.visibilityState",
        check_hidden_live_state,
        state);
}

static void check_visible_live_state(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    char visibility[32] = { 0 };
    parse_live_state(result, &state->live_visible_raf_count, visibility, sizeof(visibility));
    printf("CALLBACK live_state phase=visible count=%d visibility=%s\n", state->live_visible_raf_count, visibility);
    if (strcmp(visibility, "visible") != 0)
        fail("external presentation did not become visible");
    if (state->live_visible_raf_count < 10)
        fail("requestAnimationFrame did not run for visible external presentation");
    ts_set_presentation_visible(state->web_contents, false);
    ts_webkit_test_post_delayed_task(1.0, query_hidden_live_state, state);
}

static void begin_live_visibility_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "String(window.__termsurfRafCount) + ':' + document.visibilityState",
        check_visible_live_state,
        state);
}

static void check_input_result(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK input_state %s\n", result ? result : "");
    if (!result)
        fail("input state result missing");
    /* Pane focus is a TermSurf routing state. The accessory host is never a
     * key window, so DOM focus/blur is not an oracle for it. */
    if (!strstr(result, "\"move\":\"120,130\""))
        fail("authoritative mousemove was not observed");
    if (!strstr(result, "\"click\":\"140,150,0\""))
        fail("click was not observed");
    if (strstr(result, "\"scroll\":0"))
        fail("scroll was not observed");
    if (!strstr(result, "\"key\":\"a\""))
        fail("keyboard input was not observed");
    if (!strstr(result, "\"colorScheme\":\"dark\""))
        fail("dark color scheme was not observed");
    state->input_checked = 1;
    ts_post_task(run_console_sequence, state);
}

static void check_target_url_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (state->target_url_events != 2)
        fail("target URL callback count mismatch");
    if (strcmp(state->target_url_sequence[0], "https://example.test/webkit-target") != 0)
        fail("target URL link callback mismatch");
    if (strcmp(state->target_url_sequence[1], "") != 0)
        fail("target URL clear callback mismatch");
    state->target_url_checked = 1;
    ts_post_task(run_drag_sequence, state);
}

static void check_drag_state(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK drag_state %s\n", result ? result : "");
    if (!result)
        fail("drag state missing");
    if (!strstr(result, "\"down\":1") || !strstr(result, "\"up\":1"))
        fail("drag down/up events missing");
    if (!strstr(result, "Authoritative WebKit drag selection"))
        fail("native text selection missing after drag");
    if (ts_webkit_test_authoritative_mouse_move_count(state->web_contents) - state->drag_move_count_before != 1)
        fail("active drag incorrectly used the ordinary authoritative-move path");
    state->drag_checked = 1;
    ts_post_task(run_pointer_key_sequence, state);
}

static void query_drag_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "JSON.stringify({drag:window.__webkitDrag,selection:String(getSelection())})",
        check_drag_state,
        state);
}

static void drag_to_region(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (!result)
        fail("drag coordinates missing");
    const char *x1p = strstr(result, "\"x1\":");
    const char *x2p = strstr(result, "\"x2\":");
    const char *yp = strstr(result, "\"y\":");
    if (!x1p || !x2p || !yp)
        fail("drag coordinates parse failed");
    int x1 = (int)(atof(x1p + 5) + 0.5);
    int x2 = (int)(atof(x2p + 5) + 0.5);
    int y = (int)(atof(yp + 4) + 0.5);
    printf("CALLBACK drag_coords x1=%d x2=%d y=%d\n", x1, x2, y);
    state->drag_move_count_before = ts_webkit_test_authoritative_mouse_move_count(state->web_contents);
    ts_forward_mouse_move(state->web_contents, x1, y, 0);
    ts_forward_mouse_event(state->web_contents, 0, 0, x1, y, 1, 0);
    ts_forward_mouse_move(state->web_contents, x2, y, 0);
    ts_forward_mouse_event(state->web_contents, 1, 0, x2, y, 1, 0);
    ts_webkit_test_post_delayed_task(0.8, query_drag_state, state);
}

static void run_drag_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el=document.getElementById('drag-region'); el.scrollIntoView({block:'center'}); const r=el.getBoundingClientRect(); return JSON.stringify({x1:r.left+8,x2:r.right-8,y:r.top+r.height/2}); })()",
        drag_to_region,
        state);
}

static void check_cursor_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (state->cursor_events != 3)
        fail("cursor callback count mismatch");
    if (state->cursor_sequence[0] != 2)
        fail("hand cursor callback mismatch");
    if (state->cursor_sequence[1] != 0)
        fail("pointer cursor callback mismatch");
    if (state->cursor_sequence[2] != 3)
        fail("i-beam cursor callback mismatch");
    state->cursor_checked = 1;
    ts_set_on_cursor_changed(NULL, NULL);
    ts_post_task(run_target_url_sequence, state);
}

static void check_hover_off_state(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK hover_state phase=off %s\n", result ? result : "");
    if (!result)
        fail("hover-off state missing");
    if (!strstr(result, "\"hover\":false"))
        fail("CSS hover did not clear");
    if (!strstr(result, "\"background\":\"rgb(47, 129, 247)\""))
        fail("hover background did not restore");
    if (!strstr(result, "mouseout:target-link") || !strstr(result, "mouseleave:target-link"))
        fail("hover exit events missing");
    state->authoritative_mouse_moves = ts_webkit_test_authoritative_mouse_move_count(state->web_contents);
    if (state->authoritative_mouse_moves - state->hover_move_count_before != 2)
        fail("hover sequence did not inject exactly one authoritative move per coordinate");
    state->hover_checked = 1;
    check_target_url_sequence(state);
}

static void query_hover_off_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el=document.getElementById('target-link'); const s=getComputedStyle(el); return JSON.stringify({hover:el.matches(':hover'),background:s.backgroundColor,color:s.color,events:window.__webkitHoverEvents}); })()",
        check_hover_off_state,
        state);
}

static void check_hover_on_state(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK hover_state phase=on %s\n", result ? result : "");
    if (!result)
        fail("hover-on state missing");
    if (!strstr(result, "\"hover\":true"))
        fail("CSS hover did not activate");
    if (!strstr(result, "\"background\":\"rgb(249, 115, 22)\""))
        fail("hover background did not change");
    const char *over = strstr(result, "mouseover:target-link");
    const char *enter = strstr(result, "mouseenter:target-link");
    const char *move = strstr(result, "mousemove:target-link");
    if (!over || !enter || !move)
        fail("hover entry events missing");
    if (!(over < enter && enter < move))
        fail("hover entry events out of order");
    ts_forward_mouse_move(state->web_contents, state->hover_neutral_x, state->hover_neutral_y, 0);
    ts_webkit_test_post_delayed_task(0.8, query_hover_off_state, state);
}

static void query_hover_on_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el=document.getElementById('target-link'); const s=getComputedStyle(el); return JSON.stringify({hover:el.matches(':hover'),background:s.backgroundColor,color:s.color,events:window.__webkitHoverEvents}); })()",
        check_hover_on_state,
        state);
}

static void target_url_move_to_link(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    double x = 0, y = 0, neutral_x = 0, neutral_y = 0;
    if (!result)
        fail("target-link coords missing");
    if (strstr(result, "\"ok\":false") || strstr(result, "elementsFromPoint-miss"))
        fail("target-link not in elementsFromPoint hit stack");
    const char *xp = strstr(result, "\"x\":");
    const char *yp = strstr(result, "\"y\":");
    const char *nxp = strstr(result, "\"neutralX\":");
    const char *nyp = strstr(result, "\"neutralY\":");
    if (!xp || !yp || !nxp || !nyp)
        fail("target-link coords parse failed");
    x = atof(xp + 4);
    y = atof(yp + 4);
    neutral_x = atof(nxp + 11);
    neutral_y = atof(nyp + 11);
    state->hover_link_x = (int)(x + 0.5);
    state->hover_link_y = (int)(y + 0.5);
    state->hover_neutral_x = (int)(neutral_x + 0.5);
    state->hover_neutral_y = (int)(neutral_y + 0.5);
    state->authoritative_selector_checked = ts_webkit_test_has_authoritative_mouse_move_selector(state->web_contents);
    if (!state->authoritative_selector_checked)
        fail("WKWebView _simulateMouseMove: selector unavailable");
    state->hover_move_count_before = ts_webkit_test_authoritative_mouse_move_count(state->web_contents);
    printf("CALLBACK hover_coords link_x=%d link_y=%d neutral_x=%d neutral_y=%d count_before=%llu\n",
        state->hover_link_x,
        state->hover_link_y,
        state->hover_neutral_x,
        state->hover_neutral_y,
        (unsigned long long)state->hover_move_count_before);
    ts_forward_mouse_move(state->web_contents, state->hover_link_x, state->hover_link_y, 0);
    ts_webkit_test_post_delayed_task(0.8, query_hover_on_state, state);
}

static void run_target_url_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el=document.getElementById('target-link'), neutral=document.getElementById('pointer-region'); if(!el||!neutral)return null; el.scrollIntoView({block:'nearest'}); const r=el.getBoundingClientRect(), nr=neutral.getBoundingClientRect(); const x=r.left+r.width/2,y=r.top+r.height/2,neutralX=nr.left+nr.width/2,neutralY=nr.top+nr.height/2; const top=document.elementFromPoint(x,y),neutralTop=document.elementFromPoint(neutralX,neutralY); return JSON.stringify({x,y,neutralX,neutralY,ok:!!top&&el.contains(top)||top===el,neutralOk:!!neutralTop&&neutral.contains(neutralTop)||neutralTop===neutral,href:el.href}); })()",
        target_url_move_to_link,
        state);
}

static void cursor_move_to_element_center(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    double x = 0, y = 0;
    char cursor[64];
    cursor[0] = 0;
    if (!result)
        fail("cursor target element query missing result");
    if (strstr(result, "\"ok\":false") || strstr(result, "elementsFromPoint-miss"))
        fail("cursor target not in elementsFromPoint hit stack");
    /* result like {"x":1.2,"y":3.4,"cursor":"pointer","ok":true} */
    const char *xp = strstr(result, "\"x\":");
    const char *yp = strstr(result, "\"y\":");
    if (!xp || !yp)
        fail("cursor target element parse failed");
    x = atof(xp + 4);
    y = atof(yp + 4);
    const char *cp = strstr(result, "\"cursor\":\"");
    if (cp) {
        cp += 10; /* length of "cursor":" */
        size_t n = 0;
        while (cp[n] && cp[n] != '"' && n + 1 < sizeof(cursor)) {
            cursor[n] = cp[n];
            n++;
        }
        cursor[n] = 0;
    }
    printf("CALLBACK cursor_target raw=%s expected_type=%d\n", result ? result : "", state->cursor_expected_type);
    state->cursor_target_x = x;
    state->cursor_target_y = y;
    int ix = (int)(x + 0.5);
    int iy = (int)(y + 0.5);
    ts_forward_mouse_move(state->web_contents, ix, iy, 0);
    if (state->cursor_expected_type == 0)
        ts_webkit_test_post_delayed_task(1.5, cursor_after_pointer, state);
    else if (state->cursor_expected_type == 2)
        ts_webkit_test_post_delayed_task(1.5, cursor_after_hand, state);
    else
        ts_webkit_test_post_delayed_task(1.5, check_cursor_sequence, state);
}

static void capture_cursor_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_set_on_cursor_changed(on_cursor_changed, state);
    ts_post_task(cursor_query_hand, state);
}

static void cursor_query_pointer(void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->cursor_expected_type = 0;
    /* pointer-region */
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el = document.getElementById('pointer-region'); if (!el) return null; const r = el.getBoundingClientRect(); const pts = [[r.left+r.width/2,r.top+r.height/2],[r.left+4,r.top+4],[r.right-4,r.top+4],[r.left+4,r.bottom-4],[r.right-4,r.bottom-4],[r.left+r.width/2,r.top+4],[r.left+r.width/2,r.bottom-4]]; for (const [x,y] of pts) { const list = document.elementsFromPoint ? document.elementsFromPoint(x,y) : []; const top = list[0] || document.elementFromPoint(x,y); if (!top) continue; let n=top, hit=false; while(n){ if(n===el){ hit=true; break; } n=n.parentElement; } if (hit) { return JSON.stringify({ x:x, y:y, cursor:getComputedStyle(el).cursor, id:el.id||'', hit:(top.id||top.tagName||''), n:list.length, ok:true }); } } return JSON.stringify({ x:r.left+r.width/2, y:r.top+r.height/2, cursor:getComputedStyle(el).cursor, id:el.id||'', hit:'null', ok:false, reason:'elementsFromPoint-miss' }); })()",
        cursor_move_to_element_center,
        state);
}

static void cursor_after_pointer(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (state->cursor_events < 1)
        fail("pointer cursor callback missing before hand stage");
    ts_post_task(cursor_query_ibeam, state);
}

static void cursor_query_hand(void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->cursor_expected_type = 2;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el = document.getElementById('hand-region'); if (!el) return null; el.scrollIntoView({block:'nearest', inline:'nearest'}); const r = el.getBoundingClientRect(); const pts = [[r.left+r.width/2,r.top+r.height/2],[r.left+Math.min(8,r.width/2),r.top+Math.min(8,r.height/2)],[r.right-Math.min(8,r.width/2),r.bottom-Math.min(8,r.height/2)],[r.left+r.width/2,r.top+Math.min(8,r.height/2)],[r.left+r.width/2,r.bottom-Math.min(8,r.height/2)]]; for (const p of pts) { const x=p[0], y=p[1]; const list = document.elementsFromPoint ? document.elementsFromPoint(x,y) : []; const top = list[0] || document.elementFromPoint(x,y); if (!top) continue; let n=top, hit=false; while (n) { if (n===el) { hit=true; break; } n=n.parentElement; } if (!hit && !(el.contains && el.contains(top))) continue; return JSON.stringify({ x:x, y:y, cursor:getComputedStyle(el).cursor, id:el.id||'', hit:(top.id||top.tagName||''), n:list.length, ok:true }); } return JSON.stringify({ ok:false, reason:'elementsFromPoint-miss', id:'hand-region', r:{l:r.left,t:r.top,w:r.width,h:r.height} }); })()",
        cursor_move_to_element_center,
        state);
}

static void cursor_after_hand(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (state->cursor_events < 1)
        fail("hand cursor callback missing before pointer stage");
    if (state->cursor_sequence[state->cursor_events - 1] != 2 && state->cursor_sequence[1] != 2) {
        /* allow any position in sequence so far to contain type 2 */
        int found = 0;
        for (int i = 0; i < state->cursor_events; i++)
            if (state->cursor_sequence[i] == 2)
                found = 1;
        if (!found)
            fail("hand cursor type 2 not observed");
    }
    ts_post_task(cursor_query_pointer, state);
}

static void cursor_query_ibeam(void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->cursor_expected_type = 3;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "(() => { const el = document.getElementById('input-target'); if (!el) return null; el.scrollIntoView({block:'nearest', inline:'nearest'}); const r = el.getBoundingClientRect(); const pts = [[r.left+r.width/2,r.top+r.height/2],[r.left+Math.min(8,r.width/2),r.top+Math.min(8,r.height/2)],[r.right-Math.min(8,r.width/2),r.bottom-Math.min(8,r.height/2)],[r.left+r.width/2,r.top+Math.min(8,r.height/2)],[r.left+r.width/2,r.bottom-Math.min(8,r.height/2)]]; for (const p of pts) { const x=p[0], y=p[1]; const list = document.elementsFromPoint ? document.elementsFromPoint(x,y) : []; const top = list[0] || document.elementFromPoint(x,y); if (!top) continue; let n=top, hit=false; while (n) { if (n===el) { hit=true; break; } n=n.parentElement; } if (!hit && !(el.contains && el.contains(top))) continue; return JSON.stringify({ x:x, y:y, cursor:getComputedStyle(el).cursor, id:el.id||'', hit:(top.id||top.tagName||''), n:list.length, ok:true }); } return JSON.stringify({ ok:false, reason:'elementsFromPoint-miss', id:'input-target', r:{l:r.left,t:r.top,w:r.width,h:r.height} }); })()",
        cursor_move_to_element_center,
        state);
}

static void run_cursor_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_post_delayed_task(0.2, capture_cursor_sequence, state);
}

static void check_console_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    const char *expected_levels[] = { "log", "info", "warn", "error" };
    const char *expected_messages[] = {
        "webkit-log 42 true",
        "webkit-info [\"alpha\",7]",
        "webkit-warn {\"kind\":\"object\",\"count\":2}",
        "webkit-error null",
    };

    if (state->console_events != 4)
        fail("console callback count mismatch");
    for (int i = 0; i < 4; i++) {
        if (strcmp(state->console_levels[i], expected_levels[i]) != 0)
            fail("console level mismatch");
        if (strcmp(state->console_messages[i], expected_messages[i]) != 0)
            fail("console message mismatch");
        if (!strstr(state->console_sources[i], "navigation.html"))
            fail("console source mismatch");
        if (state->console_lines[i] <= 0)
            fail("console line number missing");
    }

    state->console_checked = 1;
    ts_set_on_console_message(NULL, NULL);
    ts_post_task(run_javascript_dialog_sequence, state);
}

static void console_sequence_started(const char *result, void *user_data)
{
    if (!result || !strstr(result, "console-started"))
        fail("console sequence did not start");
    ts_webkit_test_post_delayed_task(0.5, check_console_sequence, user_data);
}

static void run_console_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_set_on_console_message(on_console_message, state);
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "window.__webkitEmitConsoleMessages();",
        console_sequence_started,
        state);
}

static void query_input_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "JSON.stringify(window.__webkitState)",
        check_input_result,
        state);
}

static void check_logical_focus(void *user_data)
{
    struct State *state = (struct State *)user_data;
    /* Accessory TSHostWindow cannot become key; durable gate is logical pane focus. */
    if (!ts_web_contents_is_focused(state->web_contents))
        fail("logical focus was not observed after ts_set_focus(true)");
    printf("CALLBACK focus_state logical=true\n");
    state->focus_checked = 1;
    /* Cursor probes use fixed elementFromPoint coords; run before input scroll. */
    ts_post_task(run_cursor_sequence, state);
}

static void run_input_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_set_color_scheme(state->web_contents, true);
    ts_set_gui_active(state->web_contents, true, "smoke-test-active");
    ts_set_focus(state->web_contents, true);
    if (!ts_web_contents_is_focused(state->web_contents))
        fail("logical focus false immediately after ts_set_focus(true)");
    {
        int ignores = ts_webkit_test_host_ignores_mouse_events(state->web_contents);
        const char *allow = getenv("ASTROHACKER_WEBKIT_ALLOW_HOST_MOUSE");
        if (allow && strcmp(allow, "1") == 0) {
            if (ignores != 0)
                fail("ALLOW_HOST_MOUSE=1 but host still ignoresMouseEvents");
            printf("CALLBACK host_mouse allows_mouse=1 ignoresMouseEvents=%d\n", ignores);
        } else {
            if (ignores != 1)
                fail("default host must ignoreMouseEvents after focus");
            printf("CALLBACK host_mouse allows_mouse=0 ignoresMouseEvents=%d\n", ignores);
        }
    }
    ts_webkit_test_post_delayed_task(0.2, check_logical_focus, state);
}

static void run_pointer_key_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_forward_mouse_move(state->web_contents, 120, 130, 0);
    ts_forward_mouse_event(state->web_contents, 0, 0, 140, 150, 1, 0);
    ts_forward_mouse_event(state->web_contents, 1, 0, 140, 150, 1, 0);
    ts_forward_scroll_event(state->web_contents, 180, 160, 0, 120, 0, 0, true, 0);
    ts_forward_key_event(state->web_contents, 0, 0, "a", 0);
    ts_forward_key_event(state->web_contents, 1, 0, "a", 0);
    ts_set_gui_active(state->web_contents, false, "smoke-test-inactive");
    ts_set_focus(state->web_contents, false);
    if (ts_web_contents_is_focused(state->web_contents))
        fail("logical focus still true after ts_set_focus(false)");
    ts_webkit_test_post_delayed_task(0.5, query_input_state, state);
}

static void check_javascript_dialog_result(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK javascript_dialog_state %s\n", result ? result : "");
    if (!result)
        fail("javascript dialog state result missing");
    if (!strstr(result, "\"alert\":\"done\""))
        fail("javascript alert did not complete");
    if (!strstr(result, "\"confirm\":true"))
        fail("javascript confirm did not receive accepted reply");
    if (!strstr(result, "\"prompt\":\"webkit-prompt-reply\""))
        fail("javascript prompt did not receive prompt reply");
    state->javascript_dialog_checked = 1;
    ts_post_task(run_http_auth_sequence, state);
}

static void run_javascript_dialog_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (ts_reply_javascript_dialog(state->web_contents, 999999, true, "stale"))
        fail("stale javascript dialog reply unexpectedly succeeded");
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "JSON.stringify({ alert: (alert('webkit-alert'), 'done'), confirm: confirm('webkit-confirm'), prompt: prompt('webkit-prompt', 'default-prompt') })",
        check_javascript_dialog_result,
        state);
}

static void check_http_auth_accept_result(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK http_auth_accept_state %s\n", result ? result : "");
    if (!result)
        fail("http auth accept result missing");
    if (!strstr(result, "WebKit Auth OK"))
        fail("http auth accepted page title missing");
    state->http_auth_accept_checked = 1;

    snprintf(state->auth_url, sizeof(state->auth_url), "http://127.0.0.1:%d/auth-reject", state->auth_server_port);
    ts_load_url(state->web_contents, state->auth_url);
}

static void query_http_auth_accept_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "document.title + ':' + document.body.textContent",
        check_http_auth_accept_result,
        state);
}

static void run_http_auth_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (ts_reply_http_auth(state->web_contents, 999999, true, "webkit", "secret"))
        fail("stale http auth reply unexpectedly succeeded");
    ts_load_url(state->web_contents, state->auth_url);
}

static void check_renderer_crash_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    int delegate_count = ts_webkit_test_renderer_crash_delegate_count() - state->renderer_crash_delegate_count_before;
    if (delegate_count != 1)
        fail("renderer crash delegate count mismatch");
    if (state->renderer_crash_events != 1)
        fail("renderer crash callback count mismatch");
    if (strcmp(state->renderer_crash_reason, "requested") != 0)
        fail("renderer crash reason mismatch");
    if (state->renderer_crash_exit_code != 0)
        fail("renderer crash exit code mismatch");
    if (!strstr(state->renderer_crash_url, "/auth-reject") && !strstr(state->renderer_crash_url, "/auth-accept") && !strstr(state->renderer_crash_url, "navigation.html"))
        fail("renderer crash URL mismatch");
    if (!state->renderer_crash_visible)
        fail("renderer crash visible flag mismatch");

    state->renderer_crash_checked = 1;
    ts_set_on_renderer_crashed(NULL, NULL);
    ts_post_task(finish, state);
}

static void run_renderer_crash_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->renderer_crash_delegate_count_before = ts_webkit_test_renderer_crash_delegate_count();
    ts_set_on_renderer_crashed(on_renderer_crashed, state);
    printf("CALLBACK renderer_crash_trigger helper=_killWebContentProcessAndResetState\n");
    ts_webkit_test_kill_web_content_process(state->web_contents);
    ts_webkit_test_post_delayed_task(1.0, check_renderer_crash_sequence, state);
}

static void on_initialized(void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->initialized = 1;
    puts("CALLBACK initialized");
    state->persistent_context = ts_create_browser_context(NULL);
    state->incognito_context = ts_create_incognito_browser_context();
    state->web_contents = ts_create_web_contents(state->persistent_context, state->first_url, 320, 240, false);
}

static void on_tab_ready(ts_web_contents_t wc, int tab_id, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    if (tab_id <= 0)
        fail("tab id was not positive");
    state->tab_ready = 1;
    printf("CALLBACK tab_ready tab_id=%d\n", tab_id);
}

static void on_ca_context_id(ts_web_contents_t wc, uint32_t context_id, int width, int height, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    if (!context_id)
        fail("context id was zero");
    if (width <= 0 || height <= 0)
        fail("context size was invalid");
    if (state->context_id && state->context_id != context_id)
        fail("ca context id changed across lifecycle");
    state->context_id = context_id;
    state->context_id_count++;
    ts_set_presentation_visible(state->web_contents, true);
    printf("CALLBACK ca_context_id context_id=%u width=%d height=%d\n", context_id, width, height);
}

static void on_url_changed(ts_web_contents_t wc, const char *url, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    state->url_changed++;
    printf("CALLBACK url_changed url=%s\n", url ? url : "");
}

static void on_loading_state(ts_web_contents_t wc, const char *url, int loading, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    if (loading)
        state->loading_started++;
    else {
        state->loading_finished++;
        state->navigations_finished++;
        if (state->navigations_finished == 1) {
            ts_load_url(state->web_contents, state->second_url);
        } else if (state->navigations_finished == 2) {
            ts_webkit_test_post_delayed_task(1.0, begin_live_visibility_sequence, state);
        } else if (state->navigations_finished == 3) {
            ts_webkit_test_post_delayed_task(1.0, query_http_auth_accept_state, state);
        } else if (state->navigations_finished == 4) {
            state->http_auth_reject_checked = 1;
            ts_post_task(run_renderer_crash_sequence, state);
        }
    }
    printf("CALLBACK loading_state loading=%d url=%s\n", loading, url ? url : "");
}

static void on_title_changed(ts_web_contents_t wc, const char *title, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    if (title && strstr(title, "WebKit"))
        state->title_changed++;
    printf("CALLBACK title_changed title=%s\n", title ? title : "");
}

static void on_target_url_changed(ts_web_contents_t wc, const char *url, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    printf("CALLBACK target_url url=%s\n", url ? url : "");
    if (state->target_url_events < 2) {
        snprintf(
            state->target_url_sequence[state->target_url_events],
            sizeof(state->target_url_sequence[state->target_url_events]),
            "%s",
            url ? url : "");
    }
    state->target_url_events++;
}

static void on_cursor_changed(ts_web_contents_t wc, int cursor_type, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    printf("CALLBACK cursor cursor_type=%d\n", cursor_type);
    if (state->cursor_events < 3)
        state->cursor_sequence[state->cursor_events] = cursor_type;
    state->cursor_events++;
}

static void on_console_message(
    ts_web_contents_t wc,
    const char *level,
    const char *message,
    int line_number,
    const char *source,
    void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    printf("CALLBACK console level=%s line=%d source=%s message=%s\n",
        level ? level : "",
        line_number,
        source ? source : "",
        message ? message : "");

    if (state->console_events < 4) {
        snprintf(
            state->console_levels[state->console_events],
            sizeof(state->console_levels[state->console_events]),
            "%s",
            level ? level : "");
        snprintf(
            state->console_messages[state->console_events],
            sizeof(state->console_messages[state->console_events]),
            "%s",
            message ? message : "");
        snprintf(
            state->console_sources[state->console_events],
            sizeof(state->console_sources[state->console_events]),
            "%s",
            source ? source : "");
        state->console_lines[state->console_events] = line_number;
    }
    state->console_events++;
}

static void on_javascript_dialog_request(
    ts_web_contents_t wc,
    uint64_t request_id,
    const char *dialog_type,
    const char *origin_url,
    const char *message,
    const char *default_prompt_text,
    void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->javascript_dialog_requests++;
    printf("CALLBACK javascript_dialog request_id=%llu type=%s origin=%s message=%s default=%s\n",
        (unsigned long long)request_id,
        dialog_type ? dialog_type : "",
        origin_url ? origin_url : "",
        message ? message : "",
        default_prompt_text ? default_prompt_text : "");

    bool accepted = true;
    const char *prompt_text = "";
    if (dialog_type && strcmp(dialog_type, "prompt") == 0)
        prompt_text = "webkit-prompt-reply";
    if (!ts_reply_javascript_dialog(wc, request_id, accepted, prompt_text))
        fail("javascript dialog reply failed");
    if (!ts_reply_javascript_dialog(wc, request_id, accepted, prompt_text))
        state->stale_javascript_dialog_replies++;
}

static void on_http_auth_request(
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
    void *user_data)
{
    struct State *state = (struct State *)user_data;
    state->http_auth_requests++;
    printf("CALLBACK http_auth request_id=%llu url=%s scheme=%s challenger=%s realm=%s proxy=%d first=%d primary=%d navigation=%d\n",
        (unsigned long long)request_id,
        url ? url : "",
        auth_scheme ? auth_scheme : "",
        challenger ? challenger : "",
        realm ? realm : "",
        is_proxy,
        first_auth_attempt,
        is_primary_main_frame_navigation,
        is_navigation);

    char expected_challenger[128];
    snprintf(expected_challenger, sizeof(expected_challenger), "http://127.0.0.1:%d", state->auth_server_port);
    if (!auth_scheme || strcmp(auth_scheme, "basic") != 0)
        fail("http auth scheme was not normalized");
    if (!challenger || strcmp(challenger, expected_challenger) != 0)
        fail("http auth challenger mismatch");
    if (!realm || strcmp(realm, "webkit") != 0)
        fail("http auth realm mismatch");
    if (is_proxy)
        fail("http auth proxy flag was incorrect");
    /* First challenge must report first_auth_attempt; retries may clear it. */
    if (state->http_auth_requests == 1 && !first_auth_attempt)
        fail("http auth first attempt flag was incorrect");
    if (!is_primary_main_frame_navigation || !is_navigation)
        fail("http auth navigation flags were incorrect");

    bool accept = state->http_auth_requests == 1;
    if (!ts_reply_http_auth(wc, request_id, accept, "webkit", "secret"))
        fail("http auth reply failed");
    if (!ts_reply_http_auth(wc, request_id, accept, "webkit", "secret"))
        state->stale_http_auth_replies++;
}

static void on_renderer_crashed(
    ts_web_contents_t wc,
    const char *reason,
    int exit_code,
    const char *url,
    bool visible,
    void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    printf("CALLBACK renderer_crashed reason=%s exit_code=%d url=%s visible=%d\n",
        reason ? reason : "",
        exit_code,
        url ? url : "",
        visible);

    if (state->renderer_crash_events < 1) {
        snprintf(state->renderer_crash_reason, sizeof(state->renderer_crash_reason), "%s", reason ? reason : "");
        state->renderer_crash_exit_code = exit_code;
        snprintf(state->renderer_crash_url, sizeof(state->renderer_crash_url), "%s", url ? url : "");
        state->renderer_crash_visible = visible;
    }
    state->renderer_crash_events++;
}

int main(int argc, const char **argv)
{
    if (argc != 3) {
        fprintf(stderr, "usage: %s <first-url> <second-url>\n", argv[0]);
        return 2;
    }

    struct State state = {
        .first_url = argv[1],
        .second_url = argv[2],
        .auth_server_fd = -1,
    };
    start_auth_server(&state);

    ts_set_on_initialized(on_initialized, &state);
    ts_set_on_tab_ready(on_tab_ready, &state);
    ts_set_on_ca_context_id(on_ca_context_id, &state);
    ts_set_on_url_changed(on_url_changed, &state);
    ts_set_on_loading_state(on_loading_state, &state);
    ts_set_on_title_changed(on_title_changed, &state);
    ts_set_on_target_url_changed(on_target_url_changed, &state);
    ts_set_on_javascript_dialog_request(on_javascript_dialog_request, &state);
    ts_set_on_http_auth_request(on_http_auth_request, &state);

    return ts_content_main(argc, argv);
}
