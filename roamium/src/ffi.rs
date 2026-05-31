use std::ffi::c_void;
use std::os::raw::{c_char, c_double, c_int, c_ulonglong};

pub type TsBrowserContext = *mut c_void;
pub type TsWebContents = *mut c_void;

extern "C" {
    // --- Lifecycle ---

    pub fn ts_content_main(argc: c_int, argv: *const *const c_char) -> c_int;

    pub fn ts_set_on_initialized(
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_post_task(task: Option<unsafe extern "C" fn(*mut c_void)>, user_data: *mut c_void);

    pub fn ts_quit();

    // --- Profiles ---

    pub fn ts_create_browser_context(path: *const c_char) -> TsBrowserContext;

    // --- Tabs ---

    pub fn ts_create_web_contents(
        ctx: TsBrowserContext,
        url: *const c_char,
        width: c_int,
        height: c_int,
        dark: bool,
    ) -> TsWebContents;

    pub fn ts_create_devtools_web_contents(
        ctx: TsBrowserContext,
        inspected_tab_id: c_int,
        width: c_int,
        height: c_int,
        dark: bool,
    ) -> TsWebContents;

    pub fn ts_destroy_web_contents(wc: TsWebContents);

    // --- Navigation ---

    pub fn ts_load_url(wc: TsWebContents, url: *const c_char);

    // --- Input ---

    pub fn ts_forward_mouse_event(
        wc: TsWebContents,
        r#type: c_int,
        button: c_int,
        x: c_int,
        y: c_int,
        click_count: c_int,
        modifiers: c_int,
    );

    pub fn ts_forward_mouse_move(wc: TsWebContents, x: c_int, y: c_int, modifiers: c_int);

    pub fn ts_forward_scroll_event(
        wc: TsWebContents,
        x: c_int,
        y: c_int,
        delta_x: f32,
        delta_y: f32,
        phase: c_int,
        momentum_phase: c_int,
        precise: bool,
        modifiers: c_int,
    );

    pub fn ts_forward_key_event(
        wc: TsWebContents,
        r#type: c_int,
        keycode: c_int,
        utf8: *const c_char,
        modifiers: c_int,
    );

    // --- State ---

    pub fn ts_set_focus(wc: TsWebContents, focused: bool);
    pub fn ts_set_gui_active(wc: TsWebContents, active: bool, reason: *const c_char);
    pub fn ts_set_color_scheme(wc: TsWebContents, dark: bool);
    pub fn ts_set_view_size(
        wc: TsWebContents,
        width: c_int,
        height: c_int,
        screen_x: c_double,
        screen_y: c_double,
        screen_width: c_double,
        screen_height: c_double,
        screen_scale: c_double,
    );

    pub fn ts_reply_javascript_dialog(
        wc: TsWebContents,
        request_id: c_ulonglong,
        accepted: bool,
        prompt_text: *const c_char,
    ) -> bool;

    // --- Callbacks ---

    pub fn ts_set_on_tab_ready(
        cb: Option<unsafe extern "C" fn(TsWebContents, c_int, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_ca_context_id(
        cb: Option<unsafe extern "C" fn(TsWebContents, u32, c_int, c_int, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_url_changed(
        cb: Option<unsafe extern "C" fn(TsWebContents, *const c_char, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_loading_state(
        cb: Option<unsafe extern "C" fn(TsWebContents, *const c_char, c_int, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_title_changed(
        cb: Option<unsafe extern "C" fn(TsWebContents, *const c_char, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_cursor_changed(
        cb: Option<unsafe extern "C" fn(TsWebContents, c_int, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_target_url_changed(
        cb: Option<unsafe extern "C" fn(TsWebContents, *const c_char, *mut c_void)>,
        user_data: *mut c_void,
    );

    pub fn ts_set_on_javascript_dialog_request(
        cb: Option<
            unsafe extern "C" fn(
                TsWebContents,
                c_ulonglong,
                *const c_char,
                *const c_char,
                *const c_char,
                *const c_char,
                *mut c_void,
            ),
        >,
        user_data: *mut c_void,
    );
}
