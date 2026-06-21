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
};

static void run_input_sequence(void *user_data);
static void query_focus_state(void *user_data);
static void run_pointer_key_sequence(void *user_data);
static void run_target_url_sequence(void *user_data);
static void check_target_url_sequence(void *user_data);
static void run_javascript_dialog_sequence(void *user_data);
static void run_http_auth_sequence(void *user_data);
static void query_http_auth_accept_state(void *user_data);

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
        "WWW-Authenticate: Basic realm=\"surfari\"\r\n"
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
        bool authorized = strstr(request, "Authorization: Basic c3VyZmFyaTpzZWNyZXQ=") != NULL;
        if (accept_path && authorized)
            respond_ok(client, "<!doctype html><title>Surfari Auth OK</title><body>auth-ok</body>");
        else if (accept_path || reject_path)
            respond_unauthorized(client);
        else
            respond_ok(client, "<!doctype html><title>Surfari Auth Server</title><body>auth-server</body>");

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
    if (!state->context_id_count)
        fail("ca context id callback missing");
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

    ts_destroy_web_contents(state->web_contents);
    ts_destroy_browser_context(state->persistent_context);
    ts_destroy_browser_context(state->incognito_context);
    stop_auth_server(state);
    printf("SMOKE_PASS initialized=%d tab_ready=%d ca_context=%d url=%d loading_started=%d loading_finished=%d title=%d navigations=%d resized=%d focus=%d input=%d target_url=%d js_dialogs=%d http_auth=%d\n",
        state->initialized,
        state->tab_ready,
        state->context_id_count,
        state->url_changed,
        state->loading_started,
        state->loading_finished,
        state->title_changed,
        state->navigations_finished,
        state->resized,
        state->focus_checked,
        state->input_checked,
        state->target_url_checked,
        state->javascript_dialog_checked,
        state->http_auth_accept_checked && state->http_auth_reject_checked);
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

static void check_input_result(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK input_state %s\n", result ? result : "");
    if (!result)
        fail("input state result missing");
    if (!strstr(result, "\"blur\":true"))
        fail("blur was not observed");
    if (!strstr(result, "\"move\":\"120,130\""))
        fail("mousemove was not observed");
    if (!strstr(result, "\"click\":\"140,150,0\""))
        fail("click was not observed");
    if (strstr(result, "\"scroll\":0"))
        fail("scroll was not observed");
    if (!strstr(result, "\"key\":\"a\""))
        fail("keyboard input was not observed");
    if (!strstr(result, "\"colorScheme\":\"dark\""))
        fail("dark color scheme was not observed");
    state->input_checked = 1;
    ts_post_task(run_target_url_sequence, state);
}

static void check_target_url_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    if (state->target_url_events != 2)
        fail("target URL callback count mismatch");
    if (strcmp(state->target_url_sequence[0], "https://example.test/surfari-target") != 0)
        fail("target URL link callback mismatch");
    if (strcmp(state->target_url_sequence[1], "") != 0)
        fail("target URL clear callback mismatch");
    state->target_url_checked = 1;
    ts_post_task(run_javascript_dialog_sequence, state);
}

static void run_target_url_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_forward_mouse_move(state->web_contents, 392, 112, 0);
    ts_forward_mouse_move(state->web_contents, 408, 116, 0);
    ts_forward_mouse_move(state->web_contents, 20, 20, 0);
    ts_webkit_test_post_delayed_task(0.5, check_target_url_sequence, state);
}

static void query_input_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "JSON.stringify(window.__surfariState)",
        check_input_result,
        state);
}

static void check_focus_result(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK focus_state %s\n", result ? result : "");
    if (!result)
        fail("focus state result missing");
    if (!strstr(result, "\"focus\":true") && !strstr(result, "\"hasFocus\":true"))
        fail("focus was not observed");
    state->focus_checked = 1;
    ts_post_task(run_pointer_key_sequence, state);
}

static void query_focus_state(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_webkit_test_evaluate_javascript(
        state->web_contents,
        "JSON.stringify({ focus: window.__surfariState.focus, focusIn: window.__surfariState.focusIn, hasFocus: document.hasFocus(), activeElement: document.activeElement ? document.activeElement.id : \"\" })",
        check_focus_result,
        state);
}

static void run_input_sequence(void *user_data)
{
    struct State *state = (struct State *)user_data;
    ts_set_color_scheme(state->web_contents, true);
    ts_set_gui_active(state->web_contents, true, "smoke-test-active");
    ts_set_focus(state->web_contents, true);
    ts_webkit_test_post_delayed_task(0.2, query_focus_state, state);
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
    if (!strstr(result, "\"prompt\":\"surfari-prompt-reply\""))
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
        "JSON.stringify({ alert: (alert('surfari-alert'), 'done'), confirm: confirm('surfari-confirm'), prompt: prompt('surfari-prompt', 'default-prompt') })",
        check_javascript_dialog_result,
        state);
}

static void check_http_auth_accept_result(const char *result, void *user_data)
{
    struct State *state = (struct State *)user_data;
    printf("CALLBACK http_auth_accept_state %s\n", result ? result : "");
    if (!result)
        fail("http auth accept result missing");
    if (!strstr(result, "Surfari Auth OK"))
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
    if (ts_reply_http_auth(state->web_contents, 999999, true, "surfari", "secret"))
        fail("stale http auth reply unexpectedly succeeded");
    ts_load_url(state->web_contents, state->auth_url);
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
    state->context_id_count++;
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
            ts_post_task(resize_after_navigation, state);
        } else if (state->navigations_finished == 3) {
            ts_webkit_test_post_delayed_task(0.2, query_http_auth_accept_state, state);
        } else if (state->navigations_finished == 4) {
            state->http_auth_reject_checked = 1;
            ts_post_task(finish, state);
        }
    }
    printf("CALLBACK loading_state loading=%d url=%s\n", loading, url ? url : "");
}

static void on_title_changed(ts_web_contents_t wc, const char *title, void *user_data)
{
    (void)wc;
    struct State *state = (struct State *)user_data;
    if (title && strstr(title, "Surfari"))
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
        prompt_text = "surfari-prompt-reply";
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
    if (!realm || strcmp(realm, "surfari") != 0)
        fail("http auth realm mismatch");
    if (is_proxy)
        fail("http auth proxy flag was incorrect");
    if (!first_auth_attempt)
        fail("http auth first attempt flag was incorrect");
    if (!is_primary_main_frame_navigation || !is_navigation)
        fail("http auth navigation flags were incorrect");

    bool accept = state->http_auth_requests == 1;
    if (!ts_reply_http_auth(wc, request_id, accept, "surfari", "secret"))
        fail("http auth reply failed");
    if (!ts_reply_http_auth(wc, request_id, accept, "surfari", "secret"))
        state->stale_http_auth_replies++;
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
