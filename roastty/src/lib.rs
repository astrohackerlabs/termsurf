use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;

// ABI ownership model:
// - Config/app/surface handles returned by Roastty are heap-owned by Roastty and
//   released only by their matching free function.
// - Runtime callback userdata, surface config pointers, strings, env arrays,
//   platform pointers, and app pointers stored on surfaces are borrowed from the
//   caller; this skeleton records scalar values but never frees borrowed data.
// - RoasttyString values are freed only by roastty_string_free and only when
//   they were returned by Roastty string-returning functions.
pub type RoasttyApp = *mut c_void;
pub type RoasttyConfig = *mut c_void;
pub type RoasttySurface = *mut c_void;

const ROASTTY_SUCCESS: c_int = 0;
const ROASTTY_BUILD_MODE_DEBUG: c_int = 0;

#[repr(C)]
pub struct RoasttyInfo {
    build_mode: c_int,
    version: *const c_char,
    version_len: usize,
}

#[repr(C)]
pub struct RoasttyDiagnostic {
    message: *const c_char,
}

#[repr(C)]
pub struct RoasttyString {
    ptr: *const c_char,
    len: usize,
    sentinel: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyEnvVar {
    key: *const c_char,
    value: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyPlatformMacos {
    nsview: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyPlatformIos {
    uiview: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union RoasttyPlatform {
    macos: RoasttyPlatformMacos,
    ios: RoasttyPlatformIos,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttySurfaceConfig {
    platform_tag: c_int,
    platform: RoasttyPlatform,
    userdata: *mut c_void,
    scale_factor: f64,
    font_size: f32,
    working_directory: *const c_char,
    command: *const c_char,
    env_vars: *mut RoasttyEnvVar,
    env_var_count: usize,
    initial_input: *const c_char,
    wait_after_command: bool,
    context: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoasttySurfaceSize {
    columns: u16,
    rows: u16,
    width_px: u32,
    height_px: u32,
    cell_width_px: u32,
    cell_height_px: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyClipboardContent {
    mime: *const c_char,
    data: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyTarget {
    tag: c_int,
    surface: RoasttySurface,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyAction {
    tag: c_int,
    storage: [usize; 8],
}

type WakeupCallback = Option<unsafe extern "C" fn(*mut c_void)>;
type ActionCallback =
    Option<unsafe extern "C" fn(RoasttyApp, RoasttyTarget, RoasttyAction) -> bool>;
type ReadClipboardCallback = Option<unsafe extern "C" fn(*mut c_void, c_int, *mut c_void) -> bool>;
type ConfirmReadClipboardCallback =
    Option<unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void, c_int)>;
type WriteClipboardCallback =
    Option<unsafe extern "C" fn(*mut c_void, c_int, *const RoasttyClipboardContent, usize, bool)>;
type CloseSurfaceCallback = Option<unsafe extern "C" fn(*mut c_void, bool)>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyRuntimeConfig {
    userdata: *mut c_void,
    supports_selection_clipboard: bool,
    wakeup_cb: WakeupCallback,
    action_cb: ActionCallback,
    read_clipboard_cb: ReadClipboardCallback,
    confirm_read_clipboard_cb: ConfirmReadClipboardCallback,
    write_clipboard_cb: WriteClipboardCallback,
    close_surface_cb: CloseSurfaceCallback,
}

struct Config {
    finalized: bool,
}

struct App {
    runtime: RoasttyRuntimeConfig,
    focused: bool,
    color_scheme: c_int,
}

struct Surface {
    app: RoasttyApp,
    userdata: *mut c_void,
    scale_factor_x: f64,
    scale_factor_y: f64,
    focused: bool,
    occluded: bool,
    size: RoasttySurfaceSize,
    color_scheme: c_int,
}

static VERSION: &[u8] = b"0.1.0-roastty\0";
static EMPTY_DIAGNOSTIC: &[u8] = b"\0";

fn config_from_handle<'a>(handle: RoasttyConfig) -> Option<&'a mut Config> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<Config>()) })
    }
}

fn app_from_handle<'a>(handle: RoasttyApp) -> Option<&'a mut App> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<App>()) })
    }
}

fn surface_from_handle<'a>(handle: RoasttySurface) -> Option<&'a mut Surface> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<Surface>()) })
    }
}

fn empty_string() -> RoasttyString {
    RoasttyString {
        ptr: ptr::null(),
        len: 0,
        sentinel: false,
    }
}

fn allocated_string(bytes: &[u8]) -> RoasttyString {
    let owned = bytes.to_vec().into_boxed_slice();
    let len = owned.len();
    let ptr = Box::into_raw(owned).cast::<u8>();
    RoasttyString {
        ptr: ptr.cast::<c_char>(),
        len,
        sentinel: false,
    }
}

fn allocated_c_string(value: &str) -> RoasttyString {
    let c_string = CString::new(value).expect("static strings must not contain interior nuls");
    let len = c_string.as_bytes().len();
    let ptr = c_string.into_raw();
    RoasttyString {
        ptr,
        len,
        sentinel: true,
    }
}

#[no_mangle]
pub extern "C" fn roastty_init(_argc: usize, _argv: *mut *mut c_char) -> c_int {
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_info() -> RoasttyInfo {
    RoasttyInfo {
        build_mode: ROASTTY_BUILD_MODE_DEBUG,
        version: VERSION.as_ptr().cast::<c_char>(),
        version_len: VERSION.len() - 1,
    }
}

#[no_mangle]
pub extern "C" fn roastty_string_free(value: RoasttyString) {
    if value.ptr.is_null() || value.len == 0 {
        return;
    }

    unsafe {
        if value.sentinel {
            drop(CString::from_raw(value.ptr.cast_mut()));
        } else {
            let slice = ptr::slice_from_raw_parts_mut(value.ptr.cast::<u8>().cast_mut(), value.len);
            drop(Box::from_raw(slice));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_config_new() -> RoasttyConfig {
    Box::into_raw(Box::new(Config { finalized: false })).cast()
}

#[no_mangle]
pub extern "C" fn roastty_config_free(config: RoasttyConfig) {
    if !config.is_null() {
        unsafe {
            drop(Box::from_raw(config.cast::<Config>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_config_clone(config: RoasttyConfig) -> RoasttyConfig {
    let finalized = config_from_handle(config)
        .map(|config| config.finalized)
        .unwrap_or(false);
    Box::into_raw(Box::new(Config { finalized })).cast()
}

#[no_mangle]
pub extern "C" fn roastty_config_load_cli_args(_config: RoasttyConfig) {}

#[no_mangle]
pub extern "C" fn roastty_config_load_file(_config: RoasttyConfig, _path: *const c_char) {}

#[no_mangle]
pub extern "C" fn roastty_config_load_default_files(_config: RoasttyConfig) {}

#[no_mangle]
pub extern "C" fn roastty_config_load_recursive_files(_config: RoasttyConfig) {}

#[no_mangle]
pub extern "C" fn roastty_config_finalize(config: RoasttyConfig) {
    if let Some(config) = config_from_handle(config) {
        config.finalized = true;
    }
}

#[no_mangle]
pub extern "C" fn roastty_config_diagnostics_count(_config: RoasttyConfig) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn roastty_config_get_diagnostic(
    _config: RoasttyConfig,
    _index: u32,
) -> RoasttyDiagnostic {
    RoasttyDiagnostic {
        message: EMPTY_DIAGNOSTIC.as_ptr().cast::<c_char>(),
    }
}

#[no_mangle]
pub extern "C" fn roastty_config_open_path() -> RoasttyString {
    allocated_string(b"roastty-config")
}

#[no_mangle]
pub extern "C" fn roastty_app_new(
    runtime: *const RoasttyRuntimeConfig,
    _config: RoasttyConfig,
) -> RoasttyApp {
    let runtime = if runtime.is_null() {
        RoasttyRuntimeConfig {
            userdata: ptr::null_mut(),
            supports_selection_clipboard: false,
            wakeup_cb: None,
            action_cb: None,
            read_clipboard_cb: None,
            confirm_read_clipboard_cb: None,
            write_clipboard_cb: None,
            close_surface_cb: None,
        }
    } else {
        unsafe { *runtime }
    };

    Box::into_raw(Box::new(App {
        runtime,
        focused: false,
        color_scheme: 0,
    }))
    .cast()
}

#[no_mangle]
pub extern "C" fn roastty_app_free(app: RoasttyApp) {
    if !app.is_null() {
        unsafe {
            drop(Box::from_raw(app.cast::<App>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_app_tick(_app: RoasttyApp) {}

#[no_mangle]
pub extern "C" fn roastty_app_userdata(app: RoasttyApp) -> *mut c_void {
    app_from_handle(app)
        .map(|app| app.runtime.userdata)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn roastty_app_set_focus(app: RoasttyApp, focused: bool) {
    if let Some(app) = app_from_handle(app) {
        app.focused = focused;
    }
}

#[no_mangle]
pub extern "C" fn roastty_app_update_config(_app: RoasttyApp, _config: RoasttyConfig) {}

#[no_mangle]
pub extern "C" fn roastty_app_needs_confirm_quit(_app: RoasttyApp) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn roastty_app_has_global_keybinds(_app: RoasttyApp) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn roastty_app_set_color_scheme(app: RoasttyApp, color_scheme: c_int) {
    if let Some(app) = app_from_handle(app) {
        app.color_scheme = color_scheme;
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_config_new() -> RoasttySurfaceConfig {
    RoasttySurfaceConfig {
        platform_tag: 0,
        platform: RoasttyPlatform {
            macos: RoasttyPlatformMacos {
                nsview: ptr::null_mut(),
            },
        },
        userdata: ptr::null_mut(),
        scale_factor: 1.0,
        font_size: 0.0,
        working_directory: ptr::null(),
        command: ptr::null(),
        env_vars: ptr::null_mut(),
        env_var_count: 0,
        initial_input: ptr::null(),
        wait_after_command: false,
        context: 0,
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_new(
    app: RoasttyApp,
    config: *const RoasttySurfaceConfig,
) -> RoasttySurface {
    if app.is_null() {
        return ptr::null_mut();
    }

    let config = if config.is_null() {
        roastty_surface_config_new()
    } else {
        unsafe { *config }
    };

    Box::into_raw(Box::new(Surface {
        app,
        userdata: config.userdata,
        scale_factor_x: config.scale_factor,
        scale_factor_y: config.scale_factor,
        focused: false,
        occluded: false,
        size: RoasttySurfaceSize {
            columns: 0,
            rows: 0,
            width_px: 0,
            height_px: 0,
            cell_width_px: 0,
            cell_height_px: 0,
        },
        color_scheme: 0,
    }))
    .cast()
}

#[no_mangle]
pub extern "C" fn roastty_surface_free(surface: RoasttySurface) {
    if !surface.is_null() {
        unsafe {
            drop(Box::from_raw(surface.cast::<Surface>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_userdata(surface: RoasttySurface) -> *mut c_void {
    surface_from_handle(surface)
        .map(|surface| surface.userdata)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn roastty_surface_app(surface: RoasttySurface) -> RoasttyApp {
    surface_from_handle(surface)
        .map(|surface| surface.app)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn roastty_surface_update_config(_surface: RoasttySurface, _config: RoasttyConfig) {}

#[no_mangle]
pub extern "C" fn roastty_surface_needs_confirm_quit(_surface: RoasttySurface) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn roastty_surface_process_exited(_surface: RoasttySurface) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn roastty_surface_set_content_scale(surface: RoasttySurface, x: f64, y: f64) {
    if let Some(surface) = surface_from_handle(surface) {
        surface.scale_factor_x = x;
        surface.scale_factor_y = y;
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_set_focus(surface: RoasttySurface, focused: bool) {
    if let Some(surface) = surface_from_handle(surface) {
        surface.focused = focused;
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_set_occlusion(surface: RoasttySurface, occluded: bool) {
    if let Some(surface) = surface_from_handle(surface) {
        surface.occluded = occluded;
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_set_size(surface: RoasttySurface, width: u32, height: u32) {
    if let Some(surface) = surface_from_handle(surface) {
        surface.size.width_px = width;
        surface.size.height_px = height;
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_size(surface: RoasttySurface) -> RoasttySurfaceSize {
    surface_from_handle(surface)
        .map(|surface| surface.size)
        .unwrap_or(RoasttySurfaceSize {
            columns: 0,
            rows: 0,
            width_px: 0,
            height_px: 0,
            cell_width_px: 0,
            cell_height_px: 0,
        })
}

#[no_mangle]
pub extern "C" fn roastty_surface_foreground_pid(_surface: RoasttySurface) -> u64 {
    0
}

#[no_mangle]
pub extern "C" fn roastty_surface_tty_name(surface: RoasttySurface) -> RoasttyString {
    if surface.is_null() {
        empty_string()
    } else {
        allocated_c_string("roastty-skeleton-tty")
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_set_color_scheme(surface: RoasttySurface, color_scheme: c_int) {
    if let Some(surface) = surface_from_handle(surface) {
        surface.color_scheme = color_scheme;
    }
}

#[no_mangle]
pub extern "C" fn roastty_surface_request_close(_surface: RoasttySurface) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_shape_matches_roastty() {
        let value = empty_string();
        assert!(value.ptr.is_null());
        assert_eq!(value.len, 0);
        assert!(!value.sentinel);
        roastty_string_free(value);
    }

    #[test]
    fn allocated_non_sentinel_string_can_be_freed() {
        let value = roastty_config_open_path();
        assert!(!value.ptr.is_null());
        assert_eq!(value.len, "roastty-config".len());
        assert!(!value.sentinel);
        roastty_string_free(value);
    }

    #[test]
    fn allocated_sentinel_string_can_be_freed() {
        let config = roastty_config_new();
        let runtime = RoasttyRuntimeConfig {
            userdata: ptr::null_mut(),
            supports_selection_clipboard: false,
            wakeup_cb: None,
            action_cb: None,
            read_clipboard_cb: None,
            confirm_read_clipboard_cb: None,
            write_clipboard_cb: None,
            close_surface_cb: None,
        };
        let app = roastty_app_new(&runtime, config);
        let surface_config = roastty_surface_config_new();
        let surface = roastty_surface_new(app, &surface_config);

        let value = roastty_surface_tty_name(surface);
        assert!(!value.ptr.is_null());
        assert_eq!(value.len, "roastty-skeleton-tty".len());
        assert!(value.sentinel);
        roastty_string_free(value);

        roastty_surface_free(surface);
        roastty_app_free(app);
        roastty_config_free(config);
    }
}
