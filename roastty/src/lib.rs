use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::slice;

use input::{key, key_encode, key_mods};
use terminal::kitty::KeyFlags;
use terminal::terminal::{
    Terminal as InnerTerminal, TerminalColorKind, TerminalScreen, TerminalStreamError,
};
use terminal::{mouse, mouse_encode, osc, point};

mod input;
mod terminal;

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
pub type RoasttyKeyEncoder = *mut c_void;
pub type RoasttyKeyEvent = *mut c_void;
pub type RoasttyMouseEncoder = *mut c_void;
pub type RoasttyMouseEvent = *mut c_void;
pub type RoasttyOscCommand = *mut c_void;
pub type RoasttyOscParser = *mut c_void;
pub type RoasttySurface = *mut c_void;
pub type RoasttyTerminal = *mut c_void;

const ROASTTY_MODE_TAG_VALUE_MASK: u16 = 0x7fff;
const ROASTTY_MODE_TAG_ANSI_BIT: u16 = 0x8000;

const ROASTTY_SUCCESS: c_int = 0;
#[allow(dead_code)]
const ROASTTY_OUT_OF_MEMORY: c_int = 1;
const ROASTTY_INVALID_VALUE: c_int = 2;
const ROASTTY_OUT_OF_SPACE: c_int = 3;
const ROASTTY_NO_VALUE: c_int = 4;
const ROASTTY_BUILD_MODE_DEBUG: c_int = 0;

const ROASTTY_TERMINAL_DATA_INVALID: c_int = 0;
const ROASTTY_TERMINAL_DATA_COLS: c_int = 1;
const ROASTTY_TERMINAL_DATA_ROWS: c_int = 2;
const ROASTTY_TERMINAL_DATA_CURSOR_X: c_int = 3;
const ROASTTY_TERMINAL_DATA_CURSOR_Y: c_int = 4;
const ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP: c_int = 5;
const ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN: c_int = 6;
const ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE: c_int = 7;
const ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS: c_int = 8;
const ROASTTY_TERMINAL_DATA_SCROLLBAR: c_int = 9;
const ROASTTY_TERMINAL_DATA_CURSOR_STYLE: c_int = 10;
const ROASTTY_TERMINAL_DATA_MOUSE_TRACKING: c_int = 11;
const ROASTTY_TERMINAL_DATA_TITLE: c_int = 12;
const ROASTTY_TERMINAL_DATA_PWD: c_int = 13;
const ROASTTY_TERMINAL_DATA_TOTAL_ROWS: c_int = 14;
const ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS: c_int = 15;
const ROASTTY_TERMINAL_DATA_WIDTH_PX: c_int = 16;
const ROASTTY_TERMINAL_DATA_HEIGHT_PX: c_int = 17;
const ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND: c_int = 18;
const ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND: c_int = 19;
const ROASTTY_TERMINAL_DATA_COLOR_CURSOR: c_int = 20;
const ROASTTY_TERMINAL_DATA_COLOR_PALETTE: c_int = 21;
const ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT: c_int = 22;
const ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT: c_int = 23;
const ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT: c_int = 24;
const ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT: c_int = 25;
const ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT: c_int = 26;
const ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE: c_int = 27;
const ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE: c_int = 28;
const ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM: c_int = 29;
const ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS: c_int = 30;
const ROASTTY_TERMINAL_DATA_SELECTION: c_int = 31;
const ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE: c_int = 32;

const ROASTTY_TERMINAL_SCREEN_PRIMARY: c_int = 0;
const ROASTTY_TERMINAL_SCREEN_ALTERNATE: c_int = 1;

const ROASTTY_TERMINAL_OPTION_TITLE: c_int = 9;
const ROASTTY_TERMINAL_OPTION_PWD: c_int = 10;
const ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND: c_int = 11;
const ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND: c_int = 12;
const ROASTTY_TERMINAL_OPTION_COLOR_CURSOR: c_int = 13;
const ROASTTY_TERMINAL_OPTION_COLOR_PALETTE: c_int = 14;

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
pub struct RoasttyConfigPath {
    path: *const c_char,
    optional: bool,
}

#[repr(C)]
pub struct RoasttyString {
    ptr: *const c_char,
    len: usize,
    sentinel: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyRgb {
    r: u8,
    g: u8,
    b: u8,
}

type RoasttyPalette = [RoasttyRgb; 256];

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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyMouseMods {
    shift: bool,
    alt: bool,
    ctrl: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyMousePosition {
    x: f32,
    y: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyMouseEncoderSize {
    size: usize,
    screen_width: u32,
    screen_height: u32,
    cell_width: u32,
    cell_height: u32,
    padding_top: u32,
    padding_bottom: u32,
    padding_right: u32,
    padding_left: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoasttyKeyMods {
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_: bool,
    caps_lock: bool,
    num_lock: bool,
    shift_side: c_int,
    ctrl_side: c_int,
    alt_side: c_int,
    super_side: c_int,
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

struct Terminal {
    terminal: InnerTerminal,
}

struct MouseEvent {
    event: mouse_encode::Event,
}

struct MouseEncoder {
    event: mouse::MouseEventMode,
    format: mouse::MouseFormat,
    geometry: mouse_encode::Geometry,
    any_button_pressed: bool,
    track_last_cell: bool,
    last_cell: Option<point::Coordinate>,
}

struct KeyEvent {
    event: key::KeyEvent,
}

struct KeyEncoder {
    opts: key_encode::Options,
}

struct OscParser {
    parser: osc::Parser,
    last_command: Option<OwnedOscCommand>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct OwnedOscCommand {
    tag: c_int,
    title: Option<Vec<u8>>,
    terminator: Option<c_int>,
}

static VERSION: &[u8] = b"0.1.0-roastty\0";
static EMPTY_DIAGNOSTIC: &[u8] = b"\0";
static WINDOW_SAVE_STATE_DEFAULT: &[u8] = b"default\0";
static WINDOW_DECORATION_AUTO: &[u8] = b"auto\0";
static WINDOW_THEME_AUTO: &[u8] = b"auto\0";

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

fn terminal_from_handle<'a>(handle: RoasttyTerminal) -> Option<&'a mut Terminal> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<Terminal>()) })
    }
}

fn mouse_event_from_handle<'a>(handle: RoasttyMouseEvent) -> Option<&'a mut MouseEvent> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<MouseEvent>()) })
    }
}

fn mouse_encoder_from_handle<'a>(handle: RoasttyMouseEncoder) -> Option<&'a mut MouseEncoder> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<MouseEncoder>()) })
    }
}

fn key_event_from_handle<'a>(handle: RoasttyKeyEvent) -> Option<&'a mut KeyEvent> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<KeyEvent>()) })
    }
}

fn key_encoder_from_handle<'a>(handle: RoasttyKeyEncoder) -> Option<&'a mut KeyEncoder> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<KeyEncoder>()) })
    }
}

fn osc_parser_from_handle<'a>(handle: RoasttyOscParser) -> Option<&'a mut OscParser> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<OscParser>()) })
    }
}

fn osc_command_from_handle<'a>(handle: RoasttyOscCommand) -> Option<&'a OwnedOscCommand> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &*(handle.cast::<OwnedOscCommand>()) })
    }
}

fn osc_terminator_from_int(value: c_int) -> Option<osc::Terminator> {
    match value {
        0 | 92 => Some(osc::Terminator::St),
        0x07 => Some(osc::Terminator::Bel),
        _ => None,
    }
}

fn osc_terminator_to_int(value: osc::Terminator) -> c_int {
    match value {
        osc::Terminator::Bel => 0x07,
        osc::Terminator::St => b'\\' as c_int,
    }
}

fn owned_osc_command(command: osc::Command<'_>) -> Option<OwnedOscCommand> {
    match command {
        osc::Command::WindowTitle { title } => {
            if title.as_bytes().contains(&0) {
                return None;
            }
            let mut title = title.as_bytes().to_vec();
            title.push(0);
            Some(OwnedOscCommand {
                tag: 1,
                title: Some(title),
                terminator: None,
            })
        }
        osc::Command::ClipboardContents { .. } => Some(owned_osc_tag(4)),
        osc::Command::ReportPwd { .. } => Some(owned_osc_tag(5)),
        osc::Command::MouseShape { .. } => Some(owned_osc_tag(6)),
        osc::Command::ColorOperation { requests } => {
            let terminator = requests.iter().find_map(|request| match request {
                osc::ColorRequest::QueryPalette { terminator, .. }
                | osc::ColorRequest::QueryDynamic { terminator, .. } => {
                    Some(osc_terminator_to_int(terminator))
                }
                _ => None,
            });
            Some(OwnedOscCommand {
                tag: 7,
                title: None,
                terminator,
            })
        }
        osc::Command::KittyColor { terminator, .. } => Some(OwnedOscCommand {
            tag: 8,
            title: None,
            terminator: Some(osc_terminator_to_int(terminator)),
        }),
        osc::Command::DesktopNotification { .. } => Some(owned_osc_tag(9)),
        osc::Command::StartHyperlink { .. } => Some(owned_osc_tag(10)),
        osc::Command::EndHyperlink => Some(owned_osc_tag(11)),
        osc::Command::KittyTextSizing { .. } => Some(owned_osc_tag(22)),
        osc::Command::KittyClipboard { value } => Some(OwnedOscCommand {
            tag: 23,
            title: None,
            terminator: Some(osc_terminator_to_int(value.terminator)),
        }),
        osc::Command::ContextSignal { .. } => Some(owned_osc_tag(24)),
        osc::Command::SemanticPrompt { .. } => Some(owned_osc_tag(3)),
    }
}

fn owned_osc_tag(tag: c_int) -> OwnedOscCommand {
    OwnedOscCommand {
        tag,
        title: None,
        terminator: None,
    }
}

fn key_action_from_int(value: c_int) -> Option<key::KeyAction> {
    match value {
        0 => Some(key::KeyAction::Release),
        1 => Some(key::KeyAction::Press),
        2 => Some(key::KeyAction::Repeat),
        _ => None,
    }
}

fn key_action_to_int(value: key::KeyAction) -> c_int {
    value as c_int
}

fn key_from_int(value: c_int) -> Option<key::Key> {
    let index = usize::try_from(value).ok()?;
    key::ALL_KEYS.get(index).copied()
}

fn key_to_int(value: key::Key) -> c_int {
    value as c_int
}

fn key_side_from_int(value: c_int) -> Option<key_mods::Side> {
    match value {
        0 => Some(key_mods::Side::Left),
        1 => Some(key_mods::Side::Right),
        _ => None,
    }
}

fn key_side_to_int(value: key_mods::Side) -> c_int {
    match value {
        key_mods::Side::Left => 0,
        key_mods::Side::Right => 1,
    }
}

fn key_mods_from_abi(value: RoasttyKeyMods) -> Option<key_mods::Mods> {
    Some(key_mods::Mods {
        shift: value.shift,
        ctrl: value.ctrl,
        alt: value.alt,
        super_: value.super_,
        caps_lock: value.caps_lock,
        num_lock: value.num_lock,
        sides: key_mods::ModSides {
            shift: key_side_from_int(value.shift_side)?,
            ctrl: key_side_from_int(value.ctrl_side)?,
            alt: key_side_from_int(value.alt_side)?,
            super_: key_side_from_int(value.super_side)?,
        },
    })
}

fn key_mods_to_abi(value: key_mods::Mods) -> RoasttyKeyMods {
    RoasttyKeyMods {
        shift: value.shift,
        ctrl: value.ctrl,
        alt: value.alt,
        super_: value.super_,
        caps_lock: value.caps_lock,
        num_lock: value.num_lock,
        shift_side: key_side_to_int(value.sides.shift),
        ctrl_side: key_side_to_int(value.sides.ctrl),
        alt_side: key_side_to_int(value.sides.alt),
        super_side: key_side_to_int(value.sides.super_),
    }
}

fn option_as_alt_from_int(value: c_int) -> Option<key_mods::OptionAsAlt> {
    match value {
        0 => Some(key_mods::OptionAsAlt::False),
        1 => Some(key_mods::OptionAsAlt::True),
        2 => Some(key_mods::OptionAsAlt::Left),
        3 => Some(key_mods::OptionAsAlt::Right),
        _ => None,
    }
}

fn mouse_action_from_int(value: c_int) -> Option<mouse::MouseAction> {
    match value {
        0 => Some(mouse::MouseAction::Press),
        1 => Some(mouse::MouseAction::Release),
        2 => Some(mouse::MouseAction::Motion),
        _ => None,
    }
}

fn mouse_action_to_int(value: mouse::MouseAction) -> c_int {
    match value {
        mouse::MouseAction::Press => 0,
        mouse::MouseAction::Release => 1,
        mouse::MouseAction::Motion => 2,
    }
}

fn mouse_button_from_int(value: c_int) -> Option<mouse::MouseButton> {
    match value {
        0 => Some(mouse::MouseButton::Unknown),
        1 => Some(mouse::MouseButton::Left),
        2 => Some(mouse::MouseButton::Right),
        3 => Some(mouse::MouseButton::Middle),
        4 => Some(mouse::MouseButton::Four),
        5 => Some(mouse::MouseButton::Five),
        6 => Some(mouse::MouseButton::Six),
        7 => Some(mouse::MouseButton::Seven),
        8 => Some(mouse::MouseButton::Eight),
        9 => Some(mouse::MouseButton::Nine),
        10 => Some(mouse::MouseButton::Ten),
        11 => Some(mouse::MouseButton::Eleven),
        _ => None,
    }
}

fn mouse_button_to_int(value: mouse::MouseButton) -> c_int {
    match value {
        mouse::MouseButton::Unknown => 0,
        mouse::MouseButton::Left => 1,
        mouse::MouseButton::Right => 2,
        mouse::MouseButton::Middle => 3,
        mouse::MouseButton::Four => 4,
        mouse::MouseButton::Five => 5,
        mouse::MouseButton::Six => 6,
        mouse::MouseButton::Seven => 7,
        mouse::MouseButton::Eight => 8,
        mouse::MouseButton::Nine => 9,
        mouse::MouseButton::Ten => 10,
        mouse::MouseButton::Eleven => 11,
    }
}

fn mouse_event_mode_from_int(value: c_int) -> Option<mouse::MouseEventMode> {
    match value {
        0 => Some(mouse::MouseEventMode::None),
        1 => Some(mouse::MouseEventMode::X10),
        2 => Some(mouse::MouseEventMode::Normal),
        3 => Some(mouse::MouseEventMode::Button),
        4 => Some(mouse::MouseEventMode::Any),
        _ => None,
    }
}

fn mouse_format_from_int(value: c_int) -> Option<mouse::MouseFormat> {
    match value {
        0 => Some(mouse::MouseFormat::X10),
        1 => Some(mouse::MouseFormat::Utf8),
        2 => Some(mouse::MouseFormat::Sgr),
        3 => Some(mouse::MouseFormat::Urxvt),
        4 => Some(mouse::MouseFormat::SgrPixels),
        _ => None,
    }
}

fn default_mouse_geometry() -> mouse_encode::Geometry {
    mouse_encode::Geometry {
        screen: mouse_encode::PixelSize {
            width: 1,
            height: 1,
        },
        cell: mouse_encode::PixelSize {
            width: 1,
            height: 1,
        },
        padding: mouse_encode::Padding::default(),
    }
}

fn mouse_geometry_from_abi(size: &RoasttyMouseEncoderSize) -> Option<mouse_encode::Geometry> {
    if size.cell_width == 0 || size.cell_height == 0 {
        return None;
    }

    Some(mouse_encode::Geometry {
        screen: mouse_encode::PixelSize {
            width: size.screen_width,
            height: size.screen_height,
        },
        cell: mouse_encode::PixelSize {
            width: size.cell_width,
            height: size.cell_height,
        },
        padding: mouse_encode::Padding {
            top: size.padding_top,
            bottom: size.padding_bottom,
            right: size.padding_right,
            left: size.padding_left,
        },
    })
}

fn mouse_geometry_from_abi_ptr(value: *const c_void) -> Option<mouse_encode::Geometry> {
    let provided_size = unsafe { value.cast::<usize>().read() };
    if provided_size < std::mem::size_of::<RoasttyMouseEncoderSize>() {
        return None;
    }

    let size = unsafe { value.cast::<RoasttyMouseEncoderSize>().read() };
    mouse_geometry_from_abi(&size)
}

fn empty_string() -> RoasttyString {
    RoasttyString {
        ptr: ptr::null(),
        len: 0,
        sentinel: false,
    }
}

fn write_empty_string(out: *mut RoasttyString) {
    if !out.is_null() {
        unsafe {
            out.write(empty_string());
        }
    }
}

fn try_allocated_string(bytes: &[u8]) -> Result<RoasttyString, c_int> {
    if bytes.is_empty() {
        return Ok(empty_string());
    }

    let mut owned = Vec::new();
    owned
        .try_reserve_exact(bytes.len())
        .map_err(|_| ROASTTY_OUT_OF_MEMORY)?;
    owned.extend_from_slice(bytes);
    let len = owned.len();
    let ptr = Box::into_raw(owned.into_boxed_slice()).cast::<u8>();
    Ok(RoasttyString {
        ptr: ptr.cast::<c_char>(),
        len,
        sentinel: false,
    })
}

fn write_copied_string(out: *mut RoasttyString, bytes: &[u8]) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    write_empty_string(out);
    match try_allocated_string(bytes) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            ROASTTY_SUCCESS
        }
        Err(error) => error,
    }
}

fn terminal_data_selector_is_valid(data: c_int) -> bool {
    data != ROASTTY_TERMINAL_DATA_INVALID
        && matches!(
            data,
            ROASTTY_TERMINAL_DATA_COLS..=ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE
        )
}

fn mode_tag_parts(tag: u16) -> (u16, bool) {
    (
        tag & ROASTTY_MODE_TAG_VALUE_MASK,
        tag & ROASTTY_MODE_TAG_ANSI_BIT != 0,
    )
}

fn staged_terminal_string(value: *const c_void) -> Result<Option<String>, c_int> {
    if value.is_null() {
        return Ok(None);
    }

    let value = unsafe { value.cast::<RoasttyString>().read() };
    if value.ptr.is_null() {
        return if value.len == 0 {
            Ok(Some(String::new()))
        } else {
            Err(ROASTTY_INVALID_VALUE)
        };
    }

    let bytes = unsafe { slice::from_raw_parts(value.ptr.cast::<u8>(), value.len) };
    let text = std::str::from_utf8(bytes).map_err(|_| ROASTTY_INVALID_VALUE)?;
    let mut owned = String::new();
    owned
        .try_reserve_exact(text.len())
        .map_err(|_| ROASTTY_OUT_OF_MEMORY)?;
    owned.push_str(text);
    Ok(Some(owned))
}

fn staged_terminal_pwd(value: *const c_void) -> Result<Option<String>, c_int> {
    let Some(text) = staged_terminal_string(value)? else {
        return Ok(None);
    };
    if text.is_empty() {
        return Ok(Some(text));
    }

    let mut stored = String::new();
    stored
        .try_reserve_exact(text.len() + 1)
        .map_err(|_| ROASTTY_OUT_OF_MEMORY)?;
    stored.push_str(&text);
    stored.push('\0');
    Ok(Some(stored))
}

fn read_rgb(value: *const c_void) -> Option<(u8, u8, u8)> {
    if value.is_null() {
        return None;
    }
    let rgb = unsafe { value.cast::<RoasttyRgb>().read() };
    Some((rgb.r, rgb.g, rgb.b))
}

fn read_palette(value: *const c_void) -> Option<[(u8, u8, u8); 256]> {
    if value.is_null() {
        return None;
    }
    let palette = unsafe { value.cast::<RoasttyPalette>().read() };
    Some(palette_to_tuples(palette))
}

fn palette_to_tuples(palette: RoasttyPalette) -> [(u8, u8, u8); 256] {
    let mut result = [(0, 0, 0); 256];
    for (index, rgb) in palette.into_iter().enumerate() {
        result[index] = (rgb.r, rgb.g, rgb.b);
    }
    result
}

fn palette_from_tuples(palette: [(u8, u8, u8); 256]) -> RoasttyPalette {
    let mut result = [RoasttyRgb::default(); 256];
    for (index, rgb) in palette.into_iter().enumerate() {
        result[index] = RoasttyRgb {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
        };
    }
    result
}

fn write_rgb(out: *mut c_void, rgb: (u8, u8, u8)) {
    unsafe {
        out.cast::<RoasttyRgb>().write(RoasttyRgb {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
        });
    }
}

fn write_palette(out: *mut c_void, palette: [(u8, u8, u8); 256]) {
    unsafe {
        out.cast::<RoasttyPalette>()
            .write(palette_from_tuples(palette));
    }
}

unsafe fn terminal_get_write(terminal: &InnerTerminal, data: c_int, out: *mut c_void) -> c_int {
    match data {
        ROASTTY_TERMINAL_DATA_COLS => out.cast::<u16>().write(terminal.columns()),
        ROASTTY_TERMINAL_DATA_ROWS => out.cast::<u16>().write(terminal.rows()),
        ROASTTY_TERMINAL_DATA_CURSOR_X => out.cast::<u16>().write(terminal.cursor_position().0),
        ROASTTY_TERMINAL_DATA_CURSOR_Y => out.cast::<u16>().write(terminal.cursor_position().1),
        ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP => {
            out.cast::<bool>().write(terminal.cursor_pending_wrap())
        }
        ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN => {
            let value = match terminal.active_screen() {
                TerminalScreen::Primary => ROASTTY_TERMINAL_SCREEN_PRIMARY,
                TerminalScreen::Alternate => ROASTTY_TERMINAL_SCREEN_ALTERNATE,
            };
            out.cast::<c_int>().write(value);
        }
        ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE => out.cast::<bool>().write(terminal.cursor_visible()),
        ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS => {
            out.cast::<u8>().write(terminal.kitty_keyboard_flags())
        }
        ROASTTY_TERMINAL_DATA_MOUSE_TRACKING => out.cast::<bool>().write(terminal.mouse_tracking()),
        ROASTTY_TERMINAL_DATA_TOTAL_ROWS => out.cast::<usize>().write(terminal.total_rows()),
        ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS => {
            out.cast::<usize>().write(terminal.scrollback_rows())
        }
        ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND => {
            let Some(rgb) = terminal.color_effective(TerminalColorKind::Foreground) else {
                return ROASTTY_NO_VALUE;
            };
            write_rgb(out, rgb);
        }
        ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND => {
            let Some(rgb) = terminal.color_effective(TerminalColorKind::Background) else {
                return ROASTTY_NO_VALUE;
            };
            write_rgb(out, rgb);
        }
        ROASTTY_TERMINAL_DATA_COLOR_CURSOR => {
            let Some(rgb) = terminal.color_effective(TerminalColorKind::Cursor) else {
                return ROASTTY_NO_VALUE;
            };
            write_rgb(out, rgb);
        }
        ROASTTY_TERMINAL_DATA_COLOR_PALETTE => write_palette(out, terminal.palette_current()),
        ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT => {
            let Some(rgb) = terminal.color_default(TerminalColorKind::Foreground) else {
                return ROASTTY_NO_VALUE;
            };
            write_rgb(out, rgb);
        }
        ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT => {
            let Some(rgb) = terminal.color_default(TerminalColorKind::Background) else {
                return ROASTTY_NO_VALUE;
            };
            write_rgb(out, rgb);
        }
        ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT => {
            let Some(rgb) = terminal.color_default(TerminalColorKind::Cursor) else {
                return ROASTTY_NO_VALUE;
            };
            write_rgb(out, rgb);
        }
        ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT => {
            write_palette(out, terminal.palette_default())
        }
        ROASTTY_TERMINAL_DATA_SCROLLBAR
        | ROASTTY_TERMINAL_DATA_CURSOR_STYLE
        | ROASTTY_TERMINAL_DATA_TITLE
        | ROASTTY_TERMINAL_DATA_PWD
        | ROASTTY_TERMINAL_DATA_WIDTH_PX
        | ROASTTY_TERMINAL_DATA_HEIGHT_PX
        | ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT
        | ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE
        | ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE
        | ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM
        | ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS
        | ROASTTY_TERMINAL_DATA_SELECTION
        | ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE => return ROASTTY_NO_VALUE,
        _ => return ROASTTY_INVALID_VALUE,
    }

    ROASTTY_SUCCESS
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

fn terminal_stream_error_result(error: TerminalStreamError) -> c_int {
    match error {
        TerminalStreamError::PageAlloc => ROASTTY_OUT_OF_MEMORY,
        TerminalStreamError::ManagedCellUnsupported
        | TerminalStreamError::InvalidPoint
        | TerminalStreamError::UnsupportedCodepoint(_) => ROASTTY_INVALID_VALUE,
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
pub extern "C" fn roastty_config_get(
    config: RoasttyConfig,
    output: *mut c_void,
    key: *const c_char,
    key_len: usize,
) -> bool {
    if config.is_null() || output.is_null() || key.is_null() {
        return false;
    }

    let key = unsafe { slice::from_raw_parts(key.cast::<u8>(), key_len) };
    unsafe {
        match key {
            b"initial-window" => {
                output.cast::<bool>().write(true);
                true
            }
            b"quit-after-last-window-closed" => {
                output.cast::<bool>().write(false);
                true
            }
            b"window-save-state" => {
                output
                    .cast::<*const c_char>()
                    .write(WINDOW_SAVE_STATE_DEFAULT.as_ptr().cast());
                true
            }
            b"window-decoration" => {
                output
                    .cast::<*const c_char>()
                    .write(WINDOW_DECORATION_AUTO.as_ptr().cast());
                true
            }
            b"window-theme" => {
                output
                    .cast::<*const c_char>()
                    .write(WINDOW_THEME_AUTO.as_ptr().cast());
                true
            }
            b"background-opacity" => {
                output.cast::<f64>().write(1.0);
                true
            }
            b"bell-audio-volume" => {
                output.cast::<f64>().write(0.5);
                true
            }
            b"notify-on-command-finish-after" => {
                output.cast::<usize>().write(5000);
                true
            }
            b"title" => {
                output.cast::<*const c_char>().write(ptr::null());
                true
            }
            b"window-position-x" | b"window-position-y" | b"bell-audio-path" => false,
            _ => false,
        }
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
pub extern "C" fn roastty_terminal_new(
    columns: u16,
    rows: u16,
    max_scrollback_rows: usize,
    out: *mut RoasttyTerminal,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    unsafe {
        out.write(ptr::null_mut());
    }
    if columns == 0 || rows == 0 {
        return ROASTTY_INVALID_VALUE;
    }

    let max_scrollback_rows = if max_scrollback_rows == usize::MAX {
        None
    } else {
        Some(max_scrollback_rows)
    };
    let terminal = match InnerTerminal::init(columns, rows, max_scrollback_rows) {
        Ok(terminal) => terminal,
        Err(_) => return ROASTTY_OUT_OF_MEMORY,
    };

    unsafe {
        out.write(Box::into_raw(Box::new(Terminal { terminal })).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_free(terminal: RoasttyTerminal) {
    if !terminal.is_null() {
        unsafe {
            drop(Box::from_raw(terminal.cast::<Terminal>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_reset(terminal: RoasttyTerminal) {
    if let Some(terminal) = terminal_from_handle(terminal) {
        terminal.terminal.reset();
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_vt_write(
    terminal: RoasttyTerminal,
    bytes: *const u8,
    len: usize,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if bytes.is_null() && len > 0 {
        return ROASTTY_INVALID_VALUE;
    }
    let input = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(bytes, len) }
    };
    match terminal.terminal.next_slice(input) {
        Ok(()) => ROASTTY_SUCCESS,
        Err(error) => terminal_stream_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_set(
    terminal: RoasttyTerminal,
    option: c_int,
    value: *const c_void,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };

    match option {
        ROASTTY_TERMINAL_OPTION_TITLE => match staged_terminal_string(value) {
            Ok(value) => {
                terminal.terminal.set_title(value);
                ROASTTY_SUCCESS
            }
            Err(error) => error,
        },
        ROASTTY_TERMINAL_OPTION_PWD => match staged_terminal_pwd(value) {
            Ok(value) => {
                terminal.terminal.set_pwd(value);
                ROASTTY_SUCCESS
            }
            Err(error) => error,
        },
        ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND => {
            terminal
                .terminal
                .set_color_default(TerminalColorKind::Foreground, read_rgb(value));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND => {
            terminal
                .terminal
                .set_color_default(TerminalColorKind::Background, read_rgb(value));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_COLOR_CURSOR => {
            terminal
                .terminal
                .set_color_default(TerminalColorKind::Cursor, read_rgb(value));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_COLOR_PALETTE => {
            terminal.terminal.set_palette_default(read_palette(value));
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_mode_get(
    terminal: RoasttyTerminal,
    tag: u16,
    out: *mut bool,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let (value, ansi) = mode_tag_parts(tag);
    let Some(enabled) = terminal.terminal.mode_get(value, ansi) else {
        return ROASTTY_INVALID_VALUE;
    };
    unsafe {
        out.write(enabled);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_mode_set(
    terminal: RoasttyTerminal,
    tag: u16,
    enabled: bool,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };

    let (value, ansi) = mode_tag_parts(tag);
    if !terminal.terminal.mode_set(value, ansi, enabled) {
        return ROASTTY_INVALID_VALUE;
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_read_screen_plain(
    terminal: RoasttyTerminal,
    unwrap: bool,
    out: *mut RoasttyString,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        write_empty_string(out);
        return ROASTTY_INVALID_VALUE;
    };
    let text = terminal.terminal.plain_screen(unwrap);
    write_copied_string(out, text.as_bytes())
}

#[no_mangle]
pub extern "C" fn roastty_terminal_title(
    terminal: RoasttyTerminal,
    out: *mut RoasttyString,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        write_empty_string(out);
        return ROASTTY_INVALID_VALUE;
    };
    write_copied_string(out, terminal.terminal.title().as_bytes())
}

#[no_mangle]
pub extern "C" fn roastty_terminal_pwd(
    terminal: RoasttyTerminal,
    out: *mut RoasttyString,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        write_empty_string(out);
        return ROASTTY_INVALID_VALUE;
    };
    write_copied_string(out, terminal.terminal.pwd().unwrap_or("").as_bytes())
}

#[no_mangle]
pub extern "C" fn roastty_terminal_cursor_position(
    terminal: RoasttyTerminal,
    column: *mut u16,
    row: *mut u16,
) -> bool {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return false;
    };
    if column.is_null() || row.is_null() {
        return false;
    }
    let (x, y) = terminal.terminal.cursor_position();
    unsafe {
        column.write(x);
        row.write(y);
    }
    true
}

#[no_mangle]
pub extern "C" fn roastty_terminal_get(
    terminal: RoasttyTerminal,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !terminal_data_selector_is_valid(data) {
        return ROASTTY_INVALID_VALUE;
    }
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    unsafe { terminal_get_write(&terminal.terminal, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_get_multi(
    terminal: RoasttyTerminal,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> c_int {
    if !out_written.is_null() {
        unsafe {
            out_written.write(0);
        }
    }
    if terminal_from_handle(terminal).is_none() || keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if count == 0 {
        return ROASTTY_SUCCESS;
    }

    for index in 0..count {
        let key = unsafe { *keys.add(index) };
        if !terminal_data_selector_is_valid(key) {
            return ROASTTY_INVALID_VALUE;
        }

        let value = unsafe { *values.add(index) };
        if value.is_null() {
            return ROASTTY_INVALID_VALUE;
        }

        let result = roastty_terminal_get(terminal, key, value);
        if result != ROASTTY_SUCCESS {
            return result;
        }

        if !out_written.is_null() {
            unsafe {
                out_written.write(index + 1);
            }
        }
    }

    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_take_pty_response(
    terminal: RoasttyTerminal,
    out: *mut RoasttyString,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        write_empty_string(out);
        return ROASTTY_INVALID_VALUE;
    };
    let result = write_copied_string(out, terminal.terminal.pty_response());
    if result == ROASTTY_SUCCESS {
        terminal.terminal.clear_pty_response();
    }
    result
}

#[no_mangle]
pub extern "C" fn roastty_key_event_new(out: *mut RoasttyKeyEvent) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let event = Box::new(KeyEvent {
        event: key::KeyEvent::default(),
    });
    unsafe {
        out.write(Box::into_raw(event).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_free(event: RoasttyKeyEvent) {
    if !event.is_null() {
        unsafe {
            drop(Box::from_raw(event.cast::<KeyEvent>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_action(event: RoasttyKeyEvent, action: c_int) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(action) = key_action_from_int(action) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.action = action;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_action(event: RoasttyKeyEvent) -> c_int {
    key_event_from_handle(event)
        .map(|event| key_action_to_int(event.event.action))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_key(event: RoasttyKeyEvent, key: c_int) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(key) = key_from_int(key) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.key = key;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_key(event: RoasttyKeyEvent) -> c_int {
    key_event_from_handle(event)
        .map(|event| key_to_int(event.event.key))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_mods(
    event: RoasttyKeyEvent,
    mods: RoasttyKeyMods,
) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(mods) = key_mods_from_abi(mods) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.mods = mods;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_mods(event: RoasttyKeyEvent) -> RoasttyKeyMods {
    key_event_from_handle(event)
        .map(|event| key_mods_to_abi(event.event.mods))
        .unwrap_or_else(|| key_mods_to_abi(key_mods::Mods::new()))
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_consumed_mods(
    event: RoasttyKeyEvent,
    mods: RoasttyKeyMods,
) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(mods) = key_mods_from_abi(mods) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.consumed_mods = mods;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_consumed_mods(event: RoasttyKeyEvent) -> RoasttyKeyMods {
    key_event_from_handle(event)
        .map(|event| key_mods_to_abi(event.event.consumed_mods))
        .unwrap_or_else(|| key_mods_to_abi(key_mods::Mods::new()))
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_composing(
    event: RoasttyKeyEvent,
    composing: bool,
) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.composing = composing;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_composing(event: RoasttyKeyEvent) -> bool {
    key_event_from_handle(event)
        .map(|event| event.event.composing)
        .unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_utf8(
    event: RoasttyKeyEvent,
    bytes: *const u8,
    len: usize,
) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    if bytes.is_null() {
        if len == 0 {
            event.event.utf8.clear();
            return ROASTTY_SUCCESS;
        }
        return ROASTTY_INVALID_VALUE;
    }

    let bytes = unsafe { slice::from_raw_parts(bytes, len) };
    if std::str::from_utf8(bytes).is_err() {
        return ROASTTY_INVALID_VALUE;
    }

    event.event.utf8.clear();
    event.event.utf8.extend_from_slice(bytes);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_utf8(event: RoasttyKeyEvent, len: *mut usize) -> *const u8 {
    let Some(event) = key_event_from_handle(event) else {
        if !len.is_null() {
            unsafe {
                len.write(0);
            }
        }
        return ptr::null();
    };
    if !len.is_null() {
        unsafe {
            len.write(event.event.utf8.len());
        }
    }
    if event.event.utf8.is_empty() {
        ptr::null()
    } else {
        event.event.utf8.as_ptr()
    }
}

#[no_mangle]
pub extern "C" fn roastty_key_event_set_unshifted_codepoint(
    event: RoasttyKeyEvent,
    codepoint: u32,
) -> c_int {
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.unshifted_codepoint = codepoint;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_event_get_unshifted_codepoint(event: RoasttyKeyEvent) -> u32 {
    key_event_from_handle(event)
        .map(|event| event.event.unshifted_codepoint)
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn roastty_key_encoder_new(out: *mut RoasttyKeyEncoder) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let encoder = Box::new(KeyEncoder {
        opts: key_encode::Options::default(),
    });
    unsafe {
        out.write(Box::into_raw(encoder).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_encoder_free(encoder: RoasttyKeyEncoder) {
    if !encoder.is_null() {
        unsafe {
            drop(Box::from_raw(encoder.cast::<KeyEncoder>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_key_encoder_setopt(
    encoder: RoasttyKeyEncoder,
    option: c_int,
    value: *const c_void,
) -> c_int {
    let Some(encoder) = key_encoder_from_handle(encoder) else {
        return ROASTTY_INVALID_VALUE;
    };
    if value.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match option {
        0 => encoder.opts.cursor_key_application = unsafe { value.cast::<bool>().read() },
        1 => encoder.opts.keypad_key_application = unsafe { value.cast::<bool>().read() },
        2 => encoder.opts.ignore_keypad_with_numlock = unsafe { value.cast::<bool>().read() },
        3 => encoder.opts.alt_esc_prefix = unsafe { value.cast::<bool>().read() },
        4 => encoder.opts.modify_other_keys_state_2 = unsafe { value.cast::<bool>().read() },
        5 => {
            let value = unsafe { value.cast::<u8>().read() };
            let Some(flags) = KeyFlags::from_raw_int(value) else {
                return ROASTTY_INVALID_VALUE;
            };
            encoder.opts.kitty_flags = flags;
        }
        6 => {
            let value = unsafe { value.cast::<c_int>().read() };
            let Some(option_as_alt) = option_as_alt_from_int(value) else {
                return ROASTTY_INVALID_VALUE;
            };
            encoder.opts.macos_option_as_alt = option_as_alt;
        }
        7 => encoder.opts.backarrow_key_mode = unsafe { value.cast::<bool>().read() },
        _ => return ROASTTY_INVALID_VALUE,
    }

    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_key_encoder_encode(
    encoder: RoasttyKeyEncoder,
    event: RoasttyKeyEvent,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    let Some(encoder) = key_encoder_from_handle(encoder) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(event) = key_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if out.is_null() && out_len != 0 {
        return ROASTTY_INVALID_VALUE;
    }

    let encoded = key_encode::encode(&event.event, encoder.opts);
    unsafe {
        out_written.write(encoded.len());
    }

    if encoded.len() > out_len || (!encoded.is_empty() && out.is_null()) {
        return ROASTTY_OUT_OF_SPACE;
    }

    if !encoded.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
        }
    }

    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_new(out: *mut RoasttyMouseEvent) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let event = Box::new(MouseEvent {
        event: mouse_encode::Event::default(),
    });
    unsafe {
        out.write(Box::into_raw(event).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_free(event: RoasttyMouseEvent) {
    if !event.is_null() {
        unsafe {
            drop(Box::from_raw(event.cast::<MouseEvent>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_set_action(event: RoasttyMouseEvent, action: c_int) -> c_int {
    let Some(event) = mouse_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(action) = mouse_action_from_int(action) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.action = action;
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_get_action(event: RoasttyMouseEvent) -> c_int {
    mouse_event_from_handle(event)
        .map(|event| mouse_action_to_int(event.event.action))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_set_button(event: RoasttyMouseEvent, button: c_int) -> c_int {
    let Some(event) = mouse_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(button) = mouse_button_from_int(button) else {
        return ROASTTY_INVALID_VALUE;
    };

    event.event.button = Some(button);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_clear_button(event: RoasttyMouseEvent) {
    if let Some(event) = mouse_event_from_handle(event) {
        event.event.button = None;
    }
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_get_button(
    event: RoasttyMouseEvent,
    out: *mut c_int,
) -> bool {
    let Some(event) = mouse_event_from_handle(event) else {
        return false;
    };
    let Some(button) = event.event.button else {
        return false;
    };

    if !out.is_null() {
        unsafe {
            out.write(mouse_button_to_int(button));
        }
    }
    true
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_set_mods(event: RoasttyMouseEvent, mods: RoasttyMouseMods) {
    if let Some(event) = mouse_event_from_handle(event) {
        event.event.mods = mouse::MouseMods {
            shift: mods.shift,
            alt: mods.alt,
            ctrl: mods.ctrl,
        };
    }
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_get_mods(event: RoasttyMouseEvent) -> RoasttyMouseMods {
    mouse_event_from_handle(event)
        .map(|event| RoasttyMouseMods {
            shift: event.event.mods.shift,
            alt: event.event.mods.alt,
            ctrl: event.event.mods.ctrl,
        })
        .unwrap_or(RoasttyMouseMods {
            shift: false,
            alt: false,
            ctrl: false,
        })
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_set_position(
    event: RoasttyMouseEvent,
    pos: RoasttyMousePosition,
) {
    if let Some(event) = mouse_event_from_handle(event) {
        event.event.pos = mouse_encode::Position { x: pos.x, y: pos.y };
    }
}

#[no_mangle]
pub extern "C" fn roastty_mouse_event_get_position(
    event: RoasttyMouseEvent,
) -> RoasttyMousePosition {
    mouse_event_from_handle(event)
        .map(|event| RoasttyMousePosition {
            x: event.event.pos.x,
            y: event.event.pos.y,
        })
        .unwrap_or(RoasttyMousePosition { x: 0.0, y: 0.0 })
}

#[no_mangle]
pub extern "C" fn roastty_mouse_encoder_new(out: *mut RoasttyMouseEncoder) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let encoder = Box::new(MouseEncoder {
        event: mouse::MouseEventMode::None,
        format: mouse::MouseFormat::X10,
        geometry: default_mouse_geometry(),
        any_button_pressed: false,
        track_last_cell: false,
        last_cell: None,
    });
    unsafe {
        out.write(Box::into_raw(encoder).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_mouse_encoder_free(encoder: RoasttyMouseEncoder) {
    if !encoder.is_null() {
        unsafe {
            drop(Box::from_raw(encoder.cast::<MouseEncoder>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_mouse_encoder_setopt(
    encoder: RoasttyMouseEncoder,
    option: c_int,
    value: *const c_void,
) -> c_int {
    let Some(encoder) = mouse_encoder_from_handle(encoder) else {
        return ROASTTY_INVALID_VALUE;
    };
    if value.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match option {
        0 => {
            let value = unsafe { value.cast::<c_int>().read() };
            let Some(event) = mouse_event_mode_from_int(value) else {
                return ROASTTY_INVALID_VALUE;
            };
            if encoder.event != event {
                encoder.last_cell = None;
            }
            encoder.event = event;
        }
        1 => {
            let value = unsafe { value.cast::<c_int>().read() };
            let Some(format) = mouse_format_from_int(value) else {
                return ROASTTY_INVALID_VALUE;
            };
            if encoder.format != format {
                encoder.last_cell = None;
            }
            encoder.format = format;
        }
        2 => {
            let Some(geometry) = mouse_geometry_from_abi_ptr(value) else {
                return ROASTTY_INVALID_VALUE;
            };
            encoder.geometry = geometry;
            encoder.last_cell = None;
        }
        3 => {
            encoder.any_button_pressed = unsafe { value.cast::<bool>().read() };
        }
        4 => {
            encoder.track_last_cell = unsafe { value.cast::<bool>().read() };
            if !encoder.track_last_cell {
                encoder.last_cell = None;
            }
        }
        _ => return ROASTTY_INVALID_VALUE,
    }

    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_mouse_encoder_reset(encoder: RoasttyMouseEncoder) {
    if let Some(encoder) = mouse_encoder_from_handle(encoder) {
        encoder.last_cell = None;
    }
}

#[no_mangle]
pub extern "C" fn roastty_mouse_encoder_encode(
    encoder: RoasttyMouseEncoder,
    event: RoasttyMouseEvent,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    let Some(encoder) = mouse_encoder_from_handle(encoder) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(event) = mouse_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if out.is_null() && out_len != 0 {
        return ROASTTY_INVALID_VALUE;
    }

    let mut next_last_cell = encoder.last_cell;
    let encoded = mouse_encode::encode(
        event.event,
        mouse_encode::Options {
            event: encoder.event,
            format: encoder.format,
            geometry: encoder.geometry,
            any_button_pressed: encoder.any_button_pressed,
            last_cell: encoder.track_last_cell.then_some(&mut next_last_cell),
        },
    )
    .unwrap_or_default();

    unsafe {
        out_written.write(encoded.len());
    }

    if encoded.len() > out_len || (!encoded.is_empty() && out.is_null()) {
        return ROASTTY_OUT_OF_SPACE;
    }

    if !encoded.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
        }
    }
    if encoder.track_last_cell {
        encoder.last_cell = next_last_cell;
    }

    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_osc_new(out: *mut RoasttyOscParser) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let parser = Box::new(OscParser {
        parser: osc::Parser::new(),
        last_command: None,
    });
    unsafe {
        out.write(Box::into_raw(parser).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_osc_free(parser: RoasttyOscParser) {
    if !parser.is_null() {
        unsafe {
            drop(Box::from_raw(parser.cast::<OscParser>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_osc_reset(parser: RoasttyOscParser) {
    if let Some(parser) = osc_parser_from_handle(parser) {
        parser.parser.reset();
        parser.last_command = None;
    }
}

#[no_mangle]
pub extern "C" fn roastty_osc_next(parser: RoasttyOscParser, byte: u8) {
    if let Some(parser) = osc_parser_from_handle(parser) {
        parser.last_command = None;
        parser.parser.push(byte);
    }
}

#[no_mangle]
pub extern "C" fn roastty_osc_end(
    parser: RoasttyOscParser,
    terminator: c_int,
) -> RoasttyOscCommand {
    let Some(parser) = osc_parser_from_handle(parser) else {
        return ptr::null_mut();
    };
    let Some(terminator) = osc_terminator_from_int(terminator) else {
        parser.last_command = None;
        parser.parser.reset();
        return ptr::null_mut();
    };

    parser.last_command = parser
        .parser
        .command(terminator)
        .and_then(owned_osc_command);
    parser.parser.reset();

    parser
        .last_command
        .as_ref()
        .map(|command| {
            let ptr: *const OwnedOscCommand = command;
            ptr.cast_mut().cast()
        })
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn roastty_osc_command_type(command: RoasttyOscCommand) -> c_int {
    osc_command_from_handle(command)
        .map(|command| command.tag)
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn roastty_osc_command_data(
    command: RoasttyOscCommand,
    data: c_int,
    out: *mut c_void,
) -> bool {
    let Some(command) = osc_command_from_handle(command) else {
        return false;
    };
    if out.is_null() {
        return false;
    }

    match data {
        1 if command.tag == 1 => {
            let Some(title) = command.title.as_ref() else {
                return false;
            };
            unsafe {
                out.cast::<*const c_char>().write(title.as_ptr().cast());
            }
            true
        }
        _ => false,
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

    fn new_key_event() -> RoasttyKeyEvent {
        let mut event = ptr::null_mut();
        assert_eq!(roastty_key_event_new(&mut event), ROASTTY_SUCCESS);
        assert!(!event.is_null());
        event
    }

    fn new_key_encoder() -> RoasttyKeyEncoder {
        let mut encoder = ptr::null_mut();
        assert_eq!(roastty_key_encoder_new(&mut encoder), ROASTTY_SUCCESS);
        assert!(!encoder.is_null());
        encoder
    }

    fn new_osc_parser() -> RoasttyOscParser {
        let mut parser = ptr::null_mut();
        assert_eq!(roastty_osc_new(&mut parser), ROASTTY_SUCCESS);
        assert!(!parser.is_null());
        parser
    }

    fn new_terminal(cols: u16, rows: u16) -> RoasttyTerminal {
        let mut terminal = ptr::null_mut();
        assert_eq!(
            roastty_terminal_new(cols, rows, usize::MAX, &mut terminal),
            ROASTTY_SUCCESS
        );
        assert!(!terminal.is_null());
        terminal
    }

    fn write_terminal(terminal: RoasttyTerminal, bytes: &[u8]) {
        assert_eq!(
            roastty_terminal_vt_write(terminal, bytes.as_ptr(), bytes.len()),
            ROASTTY_SUCCESS
        );
    }

    fn take_roastty_string(value: RoasttyString) -> Vec<u8> {
        if value.ptr.is_null() || value.len == 0 {
            return Vec::new();
        }
        let bytes = unsafe { slice::from_raw_parts(value.ptr.cast::<u8>(), value.len) }.to_vec();
        roastty_string_free(value);
        bytes
    }

    fn terminal_string(
        terminal: RoasttyTerminal,
        f: extern "C" fn(RoasttyTerminal, *mut RoasttyString) -> c_int,
    ) -> Vec<u8> {
        let mut out = empty_string();
        assert_eq!(f(terminal, &mut out), ROASTTY_SUCCESS);
        take_roastty_string(out)
    }

    fn terminal_plain_string(terminal: RoasttyTerminal) -> Vec<u8> {
        let mut out = empty_string();
        assert_eq!(
            roastty_terminal_read_screen_plain(terminal, false, &mut out),
            ROASTTY_SUCCESS
        );
        take_roastty_string(out)
    }

    fn terminal_get_rgb_result(
        terminal: RoasttyTerminal,
        data: c_int,
        out: &mut RoasttyRgb,
    ) -> c_int {
        roastty_terminal_get(terminal, data, out as *mut _ as *mut c_void)
    }

    fn terminal_get_rgb(terminal: RoasttyTerminal, data: c_int) -> RoasttyRgb {
        let mut out = RoasttyRgb::default();
        assert_eq!(
            terminal_get_rgb_result(terminal, data, &mut out),
            ROASTTY_SUCCESS
        );
        out
    }

    fn terminal_get_palette(terminal: RoasttyTerminal, data: c_int) -> RoasttyPalette {
        let mut out = [RoasttyRgb::default(); 256];
        assert_eq!(
            roastty_terminal_get(terminal, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn assert_rgb_override_survives_default_path(
        option: c_int,
        effective_data: c_int,
        default_data: c_int,
        set_override: &[u8],
        reset_override: &[u8],
        default: RoasttyRgb,
        changed_default: RoasttyRgb,
        override_rgb: RoasttyRgb,
    ) {
        let terminal = new_terminal(5, 3);
        assert_eq!(
            roastty_terminal_set(terminal, option, &default as *const _ as *const c_void),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, set_override);
        assert_eq!(terminal_get_rgb(terminal, effective_data), override_rgb);

        assert_eq!(
            roastty_terminal_set(
                terminal,
                option,
                &changed_default as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(terminal_get_rgb(terminal, effective_data), override_rgb);
        assert_eq!(terminal_get_rgb(terminal, default_data), changed_default);

        assert_eq!(
            roastty_terminal_set(terminal, option, ptr::null()),
            ROASTTY_SUCCESS
        );
        assert_eq!(terminal_get_rgb(terminal, effective_data), override_rgb);
        let mut out = RoasttyRgb::default();
        assert_eq!(
            terminal_get_rgb_result(terminal, default_data, &mut out),
            ROASTTY_NO_VALUE
        );

        write_terminal(terminal, reset_override);
        assert_eq!(
            terminal_get_rgb_result(terminal, effective_data, &mut out),
            ROASTTY_NO_VALUE
        );

        assert_eq!(
            roastty_terminal_set(terminal, option, &default as *const _ as *const c_void),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, set_override);
        write_terminal(terminal, reset_override);
        assert_eq!(terminal_get_rgb(terminal, effective_data), default);

        roastty_terminal_free(terminal);
    }

    fn borrowed_roastty_string(bytes: &[u8]) -> RoasttyString {
        RoasttyString {
            ptr: bytes.as_ptr().cast::<c_char>(),
            len: bytes.len(),
            sentinel: false,
        }
    }

    fn feed_osc(parser: RoasttyOscParser, bytes: &[u8]) {
        for &byte in bytes {
            roastty_osc_next(parser, byte);
        }
    }

    fn parse_osc(parser: RoasttyOscParser, bytes: &[u8], terminator: c_int) -> RoasttyOscCommand {
        feed_osc(parser, bytes);
        roastty_osc_end(parser, terminator)
    }

    fn key_mods() -> RoasttyKeyMods {
        RoasttyKeyMods {
            shift: false,
            ctrl: false,
            alt: false,
            super_: false,
            caps_lock: false,
            num_lock: false,
            shift_side: 0,
            ctrl_side: 0,
            alt_side: 0,
            super_side: 0,
        }
    }

    const fn ansi_mode_tag(value: u16) -> u16 {
        value | ROASTTY_MODE_TAG_ANSI_BIT
    }

    const fn dec_mode_tag(value: u16) -> u16 {
        value
    }

    fn terminal_get_bool(terminal: RoasttyTerminal, data: c_int) -> bool {
        let mut out = false;
        assert_eq!(
            roastty_terminal_get(terminal, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn terminal_get_u16(terminal: RoasttyTerminal, data: c_int) -> u16 {
        let mut out = 0u16;
        assert_eq!(
            roastty_terminal_get(terminal, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn terminal_get_screen(terminal: RoasttyTerminal) -> c_int {
        let mut out = ROASTTY_TERMINAL_SCREEN_ALTERNATE;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                &mut out as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        out
    }

    fn terminal_mode_get(terminal: RoasttyTerminal, tag: u16) -> bool {
        let mut out = false;
        assert_eq!(
            roastty_terminal_mode_get(terminal, tag, &mut out),
            ROASTTY_SUCCESS
        );
        out
    }

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

    #[test]
    fn terminal_abi_new_rejects_invalid_inputs() {
        let mut terminal = ptr::null_mut();
        assert_eq!(
            roastty_terminal_new(5, 3, usize::MAX, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_new(0, 3, usize::MAX, &mut terminal),
            ROASTTY_INVALID_VALUE
        );
        assert!(terminal.is_null());
        assert_eq!(
            roastty_terminal_new(5, 0, usize::MAX, &mut terminal),
            ROASTTY_INVALID_VALUE
        );
        assert!(terminal.is_null());
        roastty_terminal_free(ptr::null_mut());
    }

    #[test]
    fn terminal_abi_write_validation_and_plain_output() {
        let terminal = new_terminal(5, 3);
        assert_eq!(
            roastty_terminal_vt_write(ptr::null_mut(), ptr::null(), 0),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_vt_write(terminal, ptr::null(), 1),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_vt_write(terminal, ptr::null(), 0),
            ROASTTY_SUCCESS
        );

        write_terminal(terminal, b"abc");
        let mut plain = empty_string();
        assert_eq!(
            roastty_terminal_read_screen_plain(terminal, false, &mut plain),
            ROASTTY_SUCCESS
        );
        assert_eq!(take_roastty_string(plain), b"abc");
        assert_eq!(
            roastty_terminal_read_screen_plain(terminal, false, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        let mut column = 0u16;
        let mut row = 0u16;
        assert!(roastty_terminal_cursor_position(
            terminal,
            &mut column,
            &mut row
        ));
        assert_eq!((column, row), (3, 0));
        assert!(!roastty_terminal_cursor_position(
            terminal,
            ptr::null_mut(),
            &mut row
        ));
        assert!(!roastty_terminal_cursor_position(
            ptr::null_mut(),
            &mut column,
            &mut row
        ));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_abi_parser_state_survives_split_writes() {
        let terminal = new_terminal(10, 4);

        write_terminal(terminal, &[0xc3]);
        write_terminal(terminal, &[0xa9]);
        let mut plain = empty_string();
        assert_eq!(
            roastty_terminal_read_screen_plain(terminal, false, &mut plain),
            ROASTTY_SUCCESS
        );
        assert_eq!(take_roastty_string(plain), "é".as_bytes());

        write_terminal(terminal, b"\x1b]0;split ");
        write_terminal(terminal, b"title\x07");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_title),
            b"split title"
        );

        write_terminal(terminal, b"\x1b]1337;CurrentDir=file://host/");
        write_terminal(terminal, b"split\x07");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_pwd),
            b"file://host/split"
        );

        write_terminal(terminal, b"\x1b[");
        write_terminal(terminal, b"6n");
        let response = terminal_string(terminal, roastty_terminal_take_pty_response);
        assert_eq!(response, b"\x1b[1;2R");
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_abi_string_helpers_validate_outputs() {
        let terminal = new_terminal(5, 3);
        let mut out = RoasttyString {
            ptr: ptr::dangling(),
            len: 99,
            sentinel: true,
        };

        assert_eq!(
            roastty_terminal_title(ptr::null_mut(), &mut out),
            ROASTTY_INVALID_VALUE
        );
        assert!(out.ptr.is_null());
        assert_eq!(out.len, 0);
        assert!(!out.sentinel);

        assert_eq!(
            roastty_terminal_title(terminal, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert!(terminal_string(terminal, roastty_terminal_title).is_empty());
        assert!(terminal_string(terminal, roastty_terminal_pwd).is_empty());
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_metadata_setters_abi_option_values_are_stable() {
        assert_eq!(ROASTTY_TERMINAL_OPTION_TITLE, 9);
        assert_eq!(ROASTTY_TERMINAL_OPTION_PWD, 10);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND, 11);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND, 12);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_CURSOR, 13);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_PALETTE, 14);
    }

    #[test]
    fn terminal_metadata_setters_abi_validate_inputs_without_mutation() {
        let terminal = new_terminal(5, 3);
        let title = borrowed_roastty_string(b"old title");
        let pwd = borrowed_roastty_string(b"file://host/old");
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_PWD,
                &pwd as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );

        assert_eq!(
            roastty_terminal_set(ptr::null_mut(), ROASTTY_TERMINAL_OPTION_TITLE, ptr::null()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_set(terminal, 9999, &title as *const _ as *const c_void),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_set(terminal, 0, &title as *const _ as *const c_void),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_set(terminal, 15, &title as *const _ as *const c_void),
            ROASTTY_INVALID_VALUE
        );

        let invalid_null = RoasttyString {
            ptr: ptr::null(),
            len: 1,
            sentinel: false,
        };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &invalid_null as *const _ as *const c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            terminal_string(terminal, roastty_terminal_title),
            b"old title"
        );

        let invalid_utf8 = borrowed_roastty_string(&[0xff]);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_PWD,
                &invalid_utf8 as *const _ as *const c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            terminal_string(terminal, roastty_terminal_pwd),
            b"file://host/old"
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_metadata_setters_abi_set_clear_and_copy_strings() {
        let terminal = new_terminal(5, 3);

        let mut title_bytes = b"new title".to_vec();
        let title = borrowed_roastty_string(&title_bytes);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        title_bytes.fill(b'X');
        assert_eq!(
            terminal_string(terminal, roastty_terminal_title),
            b"new title"
        );

        let mut pwd_bytes = b"file://host/new".to_vec();
        let pwd = borrowed_roastty_string(&pwd_bytes);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_PWD,
                &pwd as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        pwd_bytes.fill(b'Y');
        assert_eq!(
            terminal_string(terminal, roastty_terminal_pwd),
            b"file://host/new"
        );

        let nul_title = borrowed_roastty_string(b"a\0b");
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &nul_title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(terminal_string(terminal, roastty_terminal_title), b"a\0b");

        let empty = RoasttyString {
            ptr: ptr::null(),
            len: 0,
            sentinel: false,
        };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &empty as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert!(terminal_string(terminal, roastty_terminal_title).is_empty());

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_PWD, ptr::null()),
            ROASTTY_SUCCESS
        );
        assert!(terminal_string(terminal, roastty_terminal_pwd).is_empty());

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_metadata_setters_abi_do_not_mutate_terminal_state() {
        let terminal = new_terminal(5, 3);
        write_terminal(terminal, b"abc");
        write_terminal(terminal, b"\x1b[?1049hALT");
        write_terminal(terminal, b"\x1b[?7l");

        let before_plain = terminal_plain_string(terminal);
        let before_cursor = {
            let mut x = 0u16;
            let mut y = 0u16;
            assert!(roastty_terminal_cursor_position(terminal, &mut x, &mut y));
            (x, y)
        };
        let before_screen = terminal_get_screen(terminal);
        let before_wraparound = terminal_mode_get(terminal, dec_mode_tag(7));

        let title = borrowed_roastty_string(b"metadata only");
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        let pwd = borrowed_roastty_string(b"file://host/metadata");
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_PWD,
                &pwd as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );

        assert_eq!(terminal_plain_string(terminal), before_plain);
        let mut x = 0u16;
        let mut y = 0u16;
        assert!(roastty_terminal_cursor_position(terminal, &mut x, &mut y));
        assert_eq!((x, y), before_cursor);
        assert_eq!(terminal_get_screen(terminal), before_screen);
        assert_eq!(
            terminal_mode_get(terminal, dec_mode_tag(7)),
            before_wraparound
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_metadata_setters_abi_interoperate_with_osc_updates() {
        let terminal = new_terminal(10, 3);
        let direct_title = borrowed_roastty_string(b"direct title");
        let direct_pwd = borrowed_roastty_string(b"file://host/direct");

        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &direct_title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_PWD,
                &direct_pwd as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b]0;osc title\x07");
        write_terminal(terminal, b"\x1b]1337;CurrentDir=file://host/osc\x07");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_title),
            b"osc title"
        );
        assert_eq!(
            terminal_string(terminal, roastty_terminal_pwd),
            b"file://host/osc"
        );

        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &direct_title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_PWD,
                &direct_pwd as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            terminal_string(terminal, roastty_terminal_title),
            b"direct title"
        );
        assert_eq!(
            terminal_string(terminal, roastty_terminal_pwd),
            b"file://host/direct"
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_color_set_get_abi_rgb_layout_and_option_values_are_stable() {
        assert_eq!(std::mem::size_of::<RoasttyRgb>(), 3);
        assert_eq!(std::mem::align_of::<RoasttyRgb>(), 1);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND, 11);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND, 12);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_CURSOR, 13);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_PALETTE, 14);
    }

    #[test]
    fn terminal_color_set_get_abi_rgb_defaults_are_initially_unset() {
        let terminal = new_terminal(5, 3);
        let mut out = RoasttyRgb { r: 1, g: 2, b: 3 };

        for data in [
            ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
            ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND,
            ROASTTY_TERMINAL_DATA_COLOR_CURSOR,
            ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT,
            ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT,
            ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT,
        ] {
            assert_eq!(
                terminal_get_rgb_result(terminal, data, &mut out),
                ROASTTY_NO_VALUE
            );
            assert_eq!(out, RoasttyRgb { r: 1, g: 2, b: 3 });
        }

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_color_set_get_abi_set_get_and_clear_rgb_defaults() {
        let terminal = new_terminal(5, 3);

        for (option, effective_data, default_data, rgb) in [
            (
                ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT,
                RoasttyRgb { r: 1, g: 2, b: 3 },
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT,
                RoasttyRgb { r: 4, g: 5, b: 6 },
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_CURSOR,
                ROASTTY_TERMINAL_DATA_COLOR_CURSOR,
                ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT,
                RoasttyRgb { r: 7, g: 8, b: 9 },
            ),
        ] {
            assert_eq!(
                roastty_terminal_set(terminal, option, &rgb as *const _ as *const c_void),
                ROASTTY_SUCCESS
            );
            assert_eq!(terminal_get_rgb(terminal, effective_data), rgb);
            assert_eq!(terminal_get_rgb(terminal, default_data), rgb);
            assert_eq!(
                roastty_terminal_set(terminal, option, ptr::null()),
                ROASTTY_SUCCESS
            );
            let mut out = RoasttyRgb { r: 9, g: 8, b: 7 };
            assert_eq!(
                terminal_get_rgb_result(terminal, effective_data, &mut out),
                ROASTTY_NO_VALUE
            );
            assert_eq!(out, RoasttyRgb { r: 9, g: 8, b: 7 });
            assert_eq!(
                terminal_get_rgb_result(terminal, default_data, &mut out),
                ROASTTY_NO_VALUE
            );
            assert_eq!(out, RoasttyRgb { r: 9, g: 8, b: 7 });
        }

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_color_set_get_abi_runtime_overrides_survive_default_changes() {
        let default = RoasttyRgb {
            r: 0x10,
            g: 0x20,
            b: 0x30,
        };
        let changed_default = RoasttyRgb {
            r: 0x40,
            g: 0x50,
            b: 0x60,
        };
        let override_rgb = RoasttyRgb {
            r: 0xaa,
            g: 0xbb,
            b: 0xcc,
        };

        for (
            option,
            effective_data,
            default_data,
            set_override,
            reset_override,
            default,
            changed_default,
            override_rgb,
        ) in [
            (
                ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT,
                b"\x1b]10;#aabbcc\x1b\\".as_slice(),
                b"\x1b]110\x1b\\".as_slice(),
                default,
                changed_default,
                override_rgb,
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT,
                b"\x1b]21;foreground=#aabbcc\x1b\\".as_slice(),
                b"\x1b]21;foreground=\x1b\\".as_slice(),
                default,
                changed_default,
                override_rgb,
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT,
                b"\x1b]11;#aabbcc\x1b\\".as_slice(),
                b"\x1b]111\x1b\\".as_slice(),
                default,
                changed_default,
                override_rgb,
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_CURSOR,
                ROASTTY_TERMINAL_DATA_COLOR_CURSOR,
                ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT,
                b"\x1b]12;#aabbcc\x1b\\".as_slice(),
                b"\x1b]112\x1b\\".as_slice(),
                default,
                changed_default,
                override_rgb,
            ),
        ] {
            assert_rgb_override_survives_default_path(
                option,
                effective_data,
                default_data,
                set_override,
                reset_override,
                default,
                changed_default,
                override_rgb,
            );
        }
    }

    #[test]
    fn terminal_color_set_get_abi_kitty_background_and_cursor_override_paths() {
        let default = RoasttyRgb {
            r: 0x10,
            g: 0x20,
            b: 0x30,
        };
        let changed_default = RoasttyRgb {
            r: 0x40,
            g: 0x50,
            b: 0x60,
        };
        let override_rgb = RoasttyRgb {
            r: 0xaa,
            g: 0xbb,
            b: 0xcc,
        };

        for (option, effective_data, default_data, set_override, reset_override) in [
            (
                ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT,
                b"\x1b]21;background=#aabbcc\x1b\\".as_slice(),
                b"\x1b]21;background=\x1b\\".as_slice(),
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
                ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT,
                b"\x1b]21;foreground=#aabbcc\x1b\\".as_slice(),
                b"\x1b]21;foreground=\x1b\\".as_slice(),
            ),
            (
                ROASTTY_TERMINAL_OPTION_COLOR_CURSOR,
                ROASTTY_TERMINAL_DATA_COLOR_CURSOR,
                ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT,
                b"\x1b]21;cursor=#aabbcc\x1b\\".as_slice(),
                b"\x1b]21;cursor=\x1b\\".as_slice(),
            ),
        ] {
            assert_rgb_override_survives_default_path(
                option,
                effective_data,
                default_data,
                set_override,
                reset_override,
                default,
                changed_default,
                override_rgb,
            );
        }
    }

    #[test]
    fn terminal_color_set_get_abi_palette_current_default_and_copy_semantics() {
        let terminal = new_terminal(5, 3);
        let initial_current = terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE);
        let initial_default =
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT);
        assert_eq!(initial_current, initial_default);

        let mut custom = initial_default;
        custom[1] = RoasttyRgb {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        };
        custom[2] = RoasttyRgb {
            r: 0x44,
            g: 0x55,
            b: 0x66,
        };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                &custom as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        custom[1] = RoasttyRgb { r: 0, g: 0, b: 0 };
        assert_eq!(custom[1], RoasttyRgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE)[1],
            RoasttyRgb {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT)[2],
            RoasttyRgb {
                r: 0x44,
                g: 0x55,
                b: 0x66,
            }
        );

        write_terminal(terminal, b"\x1b]4;1;#aabbcc\x1b\\");
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE)[1],
            RoasttyRgb {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
            }
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT)[1],
            RoasttyRgb {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }
        );

        let mut replacement = initial_default;
        replacement[1] = RoasttyRgb {
            r: 0x77,
            g: 0x88,
            b: 0x99,
        };
        replacement[3] = RoasttyRgb {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                &replacement as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE)[1],
            RoasttyRgb {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
            }
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE)[3],
            RoasttyRgb {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT)[1],
            RoasttyRgb {
                r: 0x77,
                g: 0x88,
                b: 0x99,
            }
        );

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_COLOR_PALETTE, ptr::null(),),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE)[1],
            RoasttyRgb {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc,
            }
        );
        assert_eq!(
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT),
            initial_default
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_get_abi_result_and_selector_values_are_stable() {
        assert_eq!(ROASTTY_SUCCESS, 0);
        assert_eq!(ROASTTY_OUT_OF_MEMORY, 1);
        assert_eq!(ROASTTY_INVALID_VALUE, 2);
        assert_eq!(ROASTTY_OUT_OF_SPACE, 3);
        assert_eq!(ROASTTY_NO_VALUE, 4);
        assert_eq!(ROASTTY_TERMINAL_DATA_INVALID, 0);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLS, 1);
        assert_eq!(ROASTTY_TERMINAL_DATA_ROWS, 2);
        assert_eq!(ROASTTY_TERMINAL_DATA_CURSOR_X, 3);
        assert_eq!(ROASTTY_TERMINAL_DATA_CURSOR_Y, 4);
        assert_eq!(ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP, 5);
        assert_eq!(ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN, 6);
        assert_eq!(ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE, 7);
        assert_eq!(ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS, 8);
        assert_eq!(ROASTTY_TERMINAL_DATA_SCROLLBAR, 9);
        assert_eq!(ROASTTY_TERMINAL_DATA_CURSOR_STYLE, 10);
        assert_eq!(ROASTTY_TERMINAL_DATA_MOUSE_TRACKING, 11);
        assert_eq!(ROASTTY_TERMINAL_DATA_TITLE, 12);
        assert_eq!(ROASTTY_TERMINAL_DATA_PWD, 13);
        assert_eq!(ROASTTY_TERMINAL_DATA_TOTAL_ROWS, 14);
        assert_eq!(ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS, 15);
        assert_eq!(ROASTTY_TERMINAL_DATA_WIDTH_PX, 16);
        assert_eq!(ROASTTY_TERMINAL_DATA_HEIGHT_PX, 17);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND, 18);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND, 19);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_CURSOR, 20);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_PALETTE, 21);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT, 22);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT, 23);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT, 24);
        assert_eq!(ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT, 25);
        assert_eq!(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT, 26);
        assert_eq!(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE, 27);
        assert_eq!(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE, 28);
        assert_eq!(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM, 29);
        assert_eq!(ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS, 30);
        assert_eq!(ROASTTY_TERMINAL_DATA_SELECTION, 31);
        assert_eq!(ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE, 32);
        assert_eq!(ROASTTY_TERMINAL_SCREEN_PRIMARY, 0);
        assert_eq!(ROASTTY_TERMINAL_SCREEN_ALTERNATE, 1);
    }

    #[test]
    fn terminal_get_abi_validates_terminal_selector_and_output() {
        let terminal = new_terminal(5, 3);
        let mut value = 0u16;

        assert_eq!(
            roastty_terminal_get(
                ptr::null_mut(),
                ROASTTY_TERMINAL_DATA_COLS,
                &mut value as *mut _ as *mut c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_get(terminal, -1, &mut value as *mut _ as *mut c_void),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_get(terminal, 33, &mut value as *mut _ as *mut c_void),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_INVALID,
                &mut value as *mut _ as *mut c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_get(terminal, ROASTTY_TERMINAL_DATA_COLS, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_get_abi_reads_fresh_scalar_fields() {
        let terminal = new_terminal(10, 4);

        let mut cols = 0u16;
        let mut rows = 0u16;
        let mut cursor_x = 99u16;
        let mut cursor_y = 99u16;
        let mut pending_wrap = true;
        let mut active_screen = ROASTTY_TERMINAL_SCREEN_ALTERNATE;
        let mut cursor_visible = false;
        let mut key_flags = 99u8;
        let mut mouse_tracking = true;
        let mut total_rows = 0usize;
        let mut scrollback_rows = 99usize;

        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_COLS,
                &mut cols as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_ROWS,
                &mut rows as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_X,
                &mut cursor_x as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_Y,
                &mut cursor_y as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP,
                &mut pending_wrap as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                &mut active_screen as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE,
                &mut cursor_visible as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS,
                &mut key_flags as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                &mut mouse_tracking as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_TOTAL_ROWS,
                &mut total_rows as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS,
                &mut scrollback_rows as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );

        assert_eq!(cols, 10);
        assert_eq!(rows, 4);
        assert_eq!((cursor_x, cursor_y), (0, 0));
        assert!(!pending_wrap);
        assert_eq!(active_screen, ROASTTY_TERMINAL_SCREEN_PRIMARY);
        assert!(cursor_visible);
        assert_eq!(key_flags, 0);
        assert!(!mouse_tracking);
        assert_eq!(total_rows, 4);
        assert_eq!(scrollback_rows, 0);

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_get_abi_tracks_runtime_scalar_changes() {
        let terminal = new_terminal(5, 3);

        write_terminal(terminal, b"abcde");
        let mut cursor_x = 0u16;
        let mut cursor_y = 99u16;
        let mut pending_wrap = false;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_X,
                &mut cursor_x as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_Y,
                &mut cursor_y as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP,
                &mut pending_wrap as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((cursor_x, cursor_y), (4, 0));
        assert!(pending_wrap);

        write_terminal(terminal, b"\x1b[?1049h");
        let mut active_screen = ROASTTY_TERMINAL_SCREEN_PRIMARY;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                &mut active_screen as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(active_screen, ROASTTY_TERMINAL_SCREEN_ALTERNATE);
        write_terminal(terminal, b"\x1b[?1049l");
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                &mut active_screen as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(active_screen, ROASTTY_TERMINAL_SCREEN_PRIMARY);

        write_terminal(terminal, b"\x1b[?25l");
        let mut cursor_visible = true;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE,
                &mut cursor_visible as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert!(!cursor_visible);
        write_terminal(terminal, b"\x1b[?25h");
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE,
                &mut cursor_visible as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert!(cursor_visible);

        write_terminal(terminal, b"\x1b[>4u");
        let mut key_flags = 0u8;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS,
                &mut key_flags as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(key_flags, 4);

        write_terminal(terminal, b"\x1b[?1000h");
        let mut mouse_tracking = false;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                &mut mouse_tracking as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert!(mouse_tracking);
        write_terminal(terminal, b"\x1b[?1000l");
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                &mut mouse_tracking as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert!(!mouse_tracking);

        write_terminal(terminal, b"\r\n1\r\n2\r\n3\r\n4");
        let mut total_rows = 0usize;
        let mut scrollback_rows = 0usize;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_TOTAL_ROWS,
                &mut total_rows as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS,
                &mut scrollback_rows as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert!(total_rows > 3);
        assert_eq!(scrollback_rows, total_rows - 3);

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_get_abi_deferred_fields_return_no_value() {
        let terminal = new_terminal(5, 3);
        let mut value = 0usize;

        for data in [
            ROASTTY_TERMINAL_DATA_SCROLLBAR,
            ROASTTY_TERMINAL_DATA_CURSOR_STYLE,
            ROASTTY_TERMINAL_DATA_TITLE,
            ROASTTY_TERMINAL_DATA_PWD,
            ROASTTY_TERMINAL_DATA_WIDTH_PX,
            ROASTTY_TERMINAL_DATA_HEIGHT_PX,
            ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT,
            ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE,
            ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE,
            ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM,
            ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS,
            ROASTTY_TERMINAL_DATA_SELECTION,
            ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE,
        ] {
            assert_eq!(
                roastty_terminal_get(terminal, data, &mut value as *mut _ as *mut c_void),
                ROASTTY_NO_VALUE
            );
        }

        assert!(terminal_string(terminal, roastty_terminal_title).is_empty());
        assert!(terminal_string(terminal, roastty_terminal_pwd).is_empty());

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_get_abi_multi_reports_success_and_partial_counts() {
        let terminal = new_terminal(8, 4);
        write_terminal(terminal, b"abc");

        let mut cols = 0u16;
        let mut rows = 0u16;
        let mut cursor_x = 0u16;
        let keys = [
            ROASTTY_TERMINAL_DATA_COLS,
            ROASTTY_TERMINAL_DATA_ROWS,
            ROASTTY_TERMINAL_DATA_CURSOR_X,
        ];
        let mut values = [
            &mut cols as *mut _ as *mut c_void,
            &mut rows as *mut _ as *mut c_void,
            &mut cursor_x as *mut _ as *mut c_void,
        ];
        let mut written = 999usize;
        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 3);
        assert_eq!((cols, rows, cursor_x), (8, 4, 3));

        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                0,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);

        let deferred_keys = [
            ROASTTY_TERMINAL_DATA_COLS,
            ROASTTY_TERMINAL_DATA_TITLE,
            ROASTTY_TERMINAL_DATA_ROWS,
        ];
        written = 999;
        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                deferred_keys.len(),
                deferred_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(written, 1);

        let invalid_keys = [ROASTTY_TERMINAL_DATA_COLS, 33];
        written = 999;
        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                invalid_keys.len(),
                invalid_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);

        values[1] = ptr::null_mut();
        written = 999;
        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);

        assert_eq!(
            roastty_terminal_get_multi(
                ptr::null_mut(),
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                keys.len(),
                ptr::null(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_get_multi(
                terminal,
                keys.len(),
                keys.as_ptr(),
                ptr::null_mut(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_mode_control_abi_tag_constants_match_packed_layout() {
        assert_eq!(ROASTTY_MODE_TAG_VALUE_MASK, 0x7fff);
        assert_eq!(ROASTTY_MODE_TAG_ANSI_BIT, 0x8000);
        assert_eq!(dec_mode_tag(1), 0x0001);
        assert_eq!(ansi_mode_tag(4), 0x8004);
        assert_eq!(dec_mode_tag(7), 0x0007);
        assert_eq!(dec_mode_tag(2004), 0x07d4);
        assert_eq!(ansi_mode_tag(20), 0x8014);
        assert_eq!(mode_tag_parts(ansi_mode_tag(4)), (4, true));
        assert_eq!(mode_tag_parts(dec_mode_tag(1000)), (1000, false));
    }

    #[test]
    fn terminal_mode_control_abi_validates_mode_get_inputs() {
        let terminal = new_terminal(5, 3);
        let mut out = false;

        assert_eq!(
            roastty_terminal_mode_get(ptr::null_mut(), ansi_mode_tag(4), &mut out),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_mode_get(terminal, ansi_mode_tag(4), ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_mode_get(terminal, ansi_mode_tag(9), &mut out),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_mode_get(terminal, dec_mode_tag(9999), &mut out),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_mode_set(ptr::null_mut(), ansi_mode_tag(4), true),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_mode_set(terminal, ansi_mode_tag(9), true),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_mode_set(terminal, dec_mode_tag(9999), true),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_mode_control_abi_gets_defaults_and_round_trips_mode_set() {
        let terminal = new_terminal(5, 3);

        assert!(!terminal_mode_get(terminal, ansi_mode_tag(4)));
        assert!(terminal_mode_get(terminal, ansi_mode_tag(12)));
        assert!(terminal_mode_get(terminal, dec_mode_tag(7)));
        assert!(!terminal_mode_get(terminal, dec_mode_tag(2004)));

        for (tag, enabled) in [
            (ansi_mode_tag(4), true),
            (ansi_mode_tag(20), true),
            (dec_mode_tag(7), false),
            (dec_mode_tag(2004), true),
        ] {
            assert_eq!(
                roastty_terminal_mode_set(terminal, tag, enabled),
                ROASTTY_SUCCESS
            );
            assert_eq!(terminal_mode_get(terminal, tag), enabled);
            assert_eq!(
                roastty_terminal_mode_set(terminal, tag, !enabled),
                ROASTTY_SUCCESS
            );
            assert_eq!(terminal_mode_get(terminal, tag), !enabled);
        }

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_mode_control_abi_side_effect_modes_update_table_only() {
        let terminal = new_terminal(5, 3);

        assert_eq!(
            roastty_terminal_mode_set(terminal, dec_mode_tag(1049), true),
            ROASTTY_SUCCESS
        );
        assert!(terminal_mode_get(terminal, dec_mode_tag(1049)));
        assert_eq!(
            terminal_get_screen(terminal),
            ROASTTY_TERMINAL_SCREEN_PRIMARY
        );

        assert_eq!(
            roastty_terminal_mode_set(terminal, dec_mode_tag(1049), false),
            ROASTTY_SUCCESS
        );
        assert!(!terminal_mode_get(terminal, dec_mode_tag(1049)));
        assert_eq!(
            terminal_get_screen(terminal),
            ROASTTY_TERMINAL_SCREEN_PRIMARY
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_mode_control_abi_reset_restores_terminal_state() {
        let terminal = new_terminal(10, 4);

        write_terminal(terminal, b"abcde");
        write_terminal(terminal, b"\x1b[?1049hALT");
        write_terminal(terminal, b"\x1b[?25l\x1b[?1000h\x1b[>4u");
        write_terminal(terminal, b"\x1b[2;3r\x1b[3g\x1b]0;title\x07");
        write_terminal(terminal, b"\x1b]1337;CurrentDir=file://host/tmp\x07");

        assert_eq!(terminal_get_u16(terminal, ROASTTY_TERMINAL_DATA_COLS), 10);
        assert_eq!(terminal_get_u16(terminal, ROASTTY_TERMINAL_DATA_ROWS), 4);
        assert_eq!(
            terminal_get_screen(terminal),
            ROASTTY_TERMINAL_SCREEN_ALTERNATE
        );
        assert!(!terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE
        ));
        assert!(terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
        ));
        assert_eq!(
            terminal_string(terminal, roastty_terminal_title),
            b"title".to_vec()
        );
        assert_eq!(
            terminal_string(terminal, roastty_terminal_pwd),
            b"file://host/tmp".to_vec()
        );

        roastty_terminal_reset(ptr::null_mut());
        roastty_terminal_reset(terminal);

        assert_eq!(terminal_get_u16(terminal, ROASTTY_TERMINAL_DATA_COLS), 10);
        assert_eq!(terminal_get_u16(terminal, ROASTTY_TERMINAL_DATA_ROWS), 4);
        assert_eq!(
            terminal_get_screen(terminal),
            ROASTTY_TERMINAL_SCREEN_PRIMARY
        );
        assert!(terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE
        ));
        assert!(!terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP
        ));
        assert!(!terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
        ));
        let mut key_flags = 99u8;
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS,
                &mut key_flags as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(key_flags, 0);
        assert!(terminal_string(terminal, roastty_terminal_title).is_empty());
        assert!(terminal_string(terminal, roastty_terminal_pwd).is_empty());

        write_terminal(terminal, b"\x1b[b");
        assert!(terminal_plain_string(terminal).is_empty());

        write_terminal(terminal, b"\tX");
        assert_eq!(terminal_plain_string(terminal), b"        X".to_vec());

        roastty_terminal_reset(terminal);
        write_terminal(terminal, b"A\r\nB\r\nC");
        assert_eq!(terminal_plain_string(terminal), b"A\nB\nC".to_vec());

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_mode_control_abi_mouse_tracking_getter_reads_mode_table() {
        let terminal = new_terminal(5, 3);

        for tag in [
            dec_mode_tag(9),
            dec_mode_tag(1000),
            dec_mode_tag(1002),
            dec_mode_tag(1003),
        ] {
            roastty_terminal_reset(terminal);
            assert_eq!(
                roastty_terminal_mode_set(terminal, tag, true),
                ROASTTY_SUCCESS
            );
            assert!(terminal_get_bool(
                terminal,
                ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
            ));
        }

        for tag in [
            dec_mode_tag(9),
            dec_mode_tag(1000),
            dec_mode_tag(1002),
            dec_mode_tag(1003),
        ] {
            assert_eq!(
                roastty_terminal_mode_set(terminal, tag, true),
                ROASTTY_SUCCESS
            );
        }
        assert!(terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
        ));
        for tag in [
            dec_mode_tag(9),
            dec_mode_tag(1000),
            dec_mode_tag(1002),
            dec_mode_tag(1003),
        ] {
            assert_eq!(
                roastty_terminal_mode_set(terminal, tag, false),
                ROASTTY_SUCCESS
            );
        }
        assert!(!terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
        ));

        write_terminal(terminal, b"\x1b[?1000h");
        assert!(terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
        ));
        write_terminal(terminal, b"\x1b[?1000l");
        assert!(!terminal_get_bool(
            terminal,
            ROASTTY_TERMINAL_DATA_MOUSE_TRACKING
        ));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn key_event_abi_sets_and_gets_fields() {
        roastty_key_event_free(ptr::null_mut());
        assert_eq!(
            roastty_key_event_new(ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        let event = new_key_event();
        assert_eq!(roastty_key_event_set_action(event, 2), ROASTTY_SUCCESS);
        assert_eq!(roastty_key_event_get_action(event), 2);
        assert_eq!(roastty_key_event_set_key(event, 78), ROASTTY_SUCCESS);
        assert_eq!(roastty_key_event_get_key(event), 78);

        let mut mods = key_mods();
        mods.shift = true;
        mods.ctrl = true;
        mods.shift_side = 1;
        mods.ctrl_side = 1;
        assert_eq!(roastty_key_event_set_mods(event, mods), ROASTTY_SUCCESS);
        let got_mods = roastty_key_event_get_mods(event);
        assert!(got_mods.shift);
        assert!(got_mods.ctrl);
        assert_eq!(got_mods.shift_side, 1);
        assert_eq!(got_mods.ctrl_side, 1);

        let mut consumed = key_mods();
        consumed.alt = true;
        consumed.alt_side = 1;
        assert_eq!(
            roastty_key_event_set_consumed_mods(event, consumed),
            ROASTTY_SUCCESS
        );
        let got_consumed = roastty_key_event_get_consumed_mods(event);
        assert!(got_consumed.alt);
        assert_eq!(got_consumed.alt_side, 1);

        assert_eq!(
            roastty_key_event_set_composing(event, true),
            ROASTTY_SUCCESS
        );
        assert!(roastty_key_event_get_composing(event));
        assert_eq!(
            roastty_key_event_set_unshifted_codepoint(event, 'A' as u32),
            ROASTTY_SUCCESS
        );
        assert_eq!(roastty_key_event_get_unshifted_codepoint(event), 'A' as u32);

        roastty_key_event_free(event);
    }

    #[test]
    fn key_event_abi_rejects_invalid_values() {
        let event = new_key_event();
        assert_eq!(
            roastty_key_event_set_action(event, 9999),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_key_event_set_key(event, key::KEY_COUNT as c_int),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(roastty_key_event_set_key(event, -1), ROASTTY_INVALID_VALUE);

        let mut mods = key_mods();
        mods.shift_side = 2;
        assert_eq!(
            roastty_key_event_set_mods(event, mods),
            ROASTTY_INVALID_VALUE
        );
        mods = key_mods();
        mods.super_side = -1;
        assert_eq!(
            roastty_key_event_set_consumed_mods(event, mods),
            ROASTTY_INVALID_VALUE
        );

        roastty_key_event_free(event);
    }

    #[test]
    fn key_event_utf8_is_owned_and_validated() {
        let event = new_key_event();
        let mut bytes = b"ok".to_vec();
        assert_eq!(
            roastty_key_event_set_utf8(event, bytes.as_ptr(), bytes.len()),
            ROASTTY_SUCCESS
        );
        bytes[0] = b'n';

        let mut len = 0usize;
        let ptr = roastty_key_event_get_utf8(event, &mut len);
        assert_eq!(len, 2);
        assert!(!ptr.is_null());
        let got = unsafe { slice::from_raw_parts(ptr, len) };
        assert_eq!(got, b"ok");

        let invalid = [0xffu8];
        assert_eq!(
            roastty_key_event_set_utf8(event, invalid.as_ptr(), invalid.len()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_key_event_set_utf8(event, ptr::null(), 1),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_key_event_set_utf8(event, ptr::null(), 0),
            ROASTTY_SUCCESS
        );
        assert!(roastty_key_event_get_utf8(event, &mut len).is_null());
        assert_eq!(len, 0);
        assert!(roastty_key_event_get_utf8(ptr::null_mut(), &mut len).is_null());
        assert_eq!(len, 0);

        roastty_key_event_free(event);
    }

    #[test]
    fn key_encoder_abi_options_and_encode() {
        roastty_key_encoder_free(ptr::null_mut());
        assert_eq!(
            roastty_key_encoder_new(ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        let event = new_key_event();
        let encoder = new_key_encoder();
        assert_eq!(
            roastty_key_event_set_key(event, key::Key::KeyC as c_int),
            ROASTTY_SUCCESS
        );
        let mut mods = key_mods();
        mods.ctrl = true;
        assert_eq!(roastty_key_event_set_mods(event, mods), ROASTTY_SUCCESS);

        let mut written = 0usize;
        assert_eq!(
            roastty_key_encoder_encode(encoder, event, ptr::null_mut(), 0, &mut written),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, 1);
        let mut out = [0u8; 8];
        assert_eq!(
            roastty_key_encoder_encode(encoder, event, out.as_mut_ptr(), out.len(), &mut written),
            ROASTTY_SUCCESS
        );
        assert_eq!(&out[..written], b"\x03");

        let kitty_flags = KeyFlags::TRUE.int();
        assert_eq!(
            roastty_key_encoder_setopt(encoder, 5, (&kitty_flags as *const u8).cast::<c_void>()),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_key_event_set_key(event, key::Key::ControlLeft as c_int),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_key_event_set_action(event, key::KeyAction::Release as c_int),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_key_encoder_encode(encoder, event, out.as_mut_ptr(), out.len(), &mut written),
            ROASTTY_OUT_OF_SPACE
        );
        assert!(written > out.len());

        roastty_key_encoder_free(encoder);
        roastty_key_event_free(event);
    }

    #[test]
    fn key_encoder_abi_rejects_invalid_options() {
        let encoder = new_key_encoder();
        let yes = true;
        assert_eq!(
            roastty_key_encoder_setopt(encoder, 0, ptr::null()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_key_encoder_setopt(encoder, 99, (&yes as *const bool).cast()),
            ROASTTY_INVALID_VALUE
        );
        let bad_flags = 0b100000u8;
        assert_eq!(
            roastty_key_encoder_setopt(encoder, 5, (&bad_flags as *const u8).cast()),
            ROASTTY_INVALID_VALUE
        );
        let bad_option_as_alt = 4i32;
        assert_eq!(
            roastty_key_encoder_setopt(encoder, 6, (&bad_option_as_alt as *const i32).cast()),
            ROASTTY_INVALID_VALUE
        );

        for option in [0, 1, 2, 3, 4, 7] {
            assert_eq!(
                roastty_key_encoder_setopt(encoder, option, (&yes as *const bool).cast()),
                ROASTTY_SUCCESS
            );
        }
        let option_as_alt = 3i32;
        assert_eq!(
            roastty_key_encoder_setopt(encoder, 6, (&option_as_alt as *const i32).cast()),
            ROASTTY_SUCCESS
        );

        roastty_key_encoder_free(encoder);
    }

    #[test]
    fn key_abi_discriminants_match_internal_key_values() {
        assert_eq!(key::KEY_COUNT, 176);
        assert_eq!(key::KeyAction::Release as c_int, 0);
        assert_eq!(key::KeyAction::Press as c_int, 1);
        assert_eq!(key::KeyAction::Repeat as c_int, 2);

        assert_eq!(key_to_int(key::Key::Unidentified), 0);
        assert_eq!(key_to_int(key::Key::KeyA), 20);
        assert_eq!(key_to_int(key::Key::AltLeft), 51);
        assert_eq!(key_to_int(key::Key::ArrowUp), 78);
        assert_eq!(key_to_int(key::Key::Numpad0), 80);
        assert_eq!(key_to_int(key::Key::F1), 121);
        assert_eq!(key_to_int(key::Key::BrowserBack), 151);
        assert_eq!(key_to_int(key::Key::Paste), 175);
        assert_eq!(key_from_int(0), Some(key::Key::Unidentified));
        assert_eq!(key_from_int(175), Some(key::Key::Paste));
        assert_eq!(key_from_int(176), None);
    }

    #[test]
    fn osc_parser_abi_allocates_parses_title_and_extracts_data() {
        roastty_osc_free(ptr::null_mut());
        roastty_osc_reset(ptr::null_mut());
        roastty_osc_next(ptr::null_mut(), b'x');
        assert_eq!(roastty_osc_new(ptr::null_mut()), ROASTTY_INVALID_VALUE);
        assert!(roastty_osc_end(ptr::null_mut(), 0).is_null());
        assert_eq!(roastty_osc_command_type(ptr::null_mut()), 0);
        assert!(!roastty_osc_command_data(
            ptr::null_mut(),
            1,
            ptr::null_mut()
        ));

        let parser = new_osc_parser();
        let command = parse_osc(parser, b"0;hello", 0);
        assert!(!command.is_null());
        assert_eq!(roastty_osc_command_type(command), 1);

        let mut title: *const c_char = ptr::null();
        assert!(roastty_osc_command_data(
            command,
            1,
            (&mut title as *mut *const c_char).cast()
        ));
        assert!(!title.is_null());
        let title = unsafe { std::ffi::CStr::from_ptr(title) };
        assert_eq!(title.to_bytes(), b"hello");

        let mut unchanged: *const c_char = ptr::dangling();
        assert!(!roastty_osc_command_data(
            command,
            0,
            (&mut unchanged as *mut *const c_char).cast()
        ));
        assert_eq!(unchanged, ptr::dangling());
        assert!(!roastty_osc_command_data(command, 1, ptr::null_mut()));

        let nul_title = parse_osc(parser, b"0;a\0b", 0);
        assert!(nul_title.is_null());

        roastty_osc_free(parser);
    }

    #[test]
    fn osc_parser_abi_end_resets_input_for_sequential_commands() {
        let parser = new_osc_parser();
        let first = parse_osc(parser, b"0;first", 0);
        assert_eq!(roastty_osc_command_type(first), 1);
        let first_addr = first as usize;

        let second = parse_osc(parser, b"0;second", 0);
        assert_eq!(roastty_osc_command_type(second), 1);
        assert_ne!(second as usize, 0);
        assert_eq!(first_addr, second as usize);

        let mut title: *const c_char = ptr::null();
        assert!(roastty_osc_command_data(
            second,
            1,
            (&mut title as *mut *const c_char).cast()
        ));
        let title_cstr = unsafe { std::ffi::CStr::from_ptr(title) };
        assert_eq!(title_cstr.to_bytes(), b"second");

        roastty_osc_reset(parser);
        let third = parse_osc(parser, b"7;file://host/path", 0);
        assert_eq!(roastty_osc_command_type(third), 5);
        assert!(!roastty_osc_command_data(
            third,
            1,
            (&mut title as *mut *const c_char).cast()
        ));

        roastty_osc_next(parser, b'x');
        assert_eq!(roastty_osc_command_type(ptr::null_mut()), 0);
        let after_partial = roastty_osc_end(parser, 0);
        assert!(after_partial.is_null());

        roastty_osc_free(parser);
    }

    #[test]
    fn osc_parser_abi_validates_terminators_and_preserves_sensitive_state() {
        let parser = new_osc_parser();
        assert!(parse_osc(parser, b"0;title", 9999).is_null());

        let color_default = parse_osc(parser, b"4;2;?", 0);
        assert_eq!(roastty_osc_command_type(color_default), 7);
        assert_eq!(
            osc_command_from_handle(color_default).and_then(|command| command.terminator),
            Some(b'\\' as c_int)
        );

        let color_st = parse_osc(parser, b"4;2;?", b'\\' as c_int);
        assert_eq!(roastty_osc_command_type(color_st), 7);
        assert_eq!(
            osc_command_from_handle(color_st).and_then(|command| command.terminator),
            Some(b'\\' as c_int)
        );

        let color_bel = parse_osc(parser, b"4;2;?", 0x07);
        assert_eq!(roastty_osc_command_type(color_bel), 7);
        assert_eq!(
            osc_command_from_handle(color_bel).and_then(|command| command.terminator),
            Some(0x07)
        );

        let kitty_clipboard = parse_osc(parser, b"5522;type=read;payload", 0x07);
        assert_eq!(roastty_osc_command_type(kitty_clipboard), 23);
        assert_eq!(
            osc_command_from_handle(kitty_clipboard).and_then(|command| command.terminator),
            Some(0x07)
        );

        roastty_osc_free(parser);
    }

    #[test]
    fn osc_parser_abi_maps_current_command_types_and_reserves_unsupported_slots() {
        let parser = new_osc_parser();
        let cases: &[(&[u8], c_int)] = &[
            (b"7;file://host/path", 5),
            (b"8;;https://example.com", 10),
            (b"8;;", 11),
            (b"777;notify;title;body", 9),
            (b"22;pointer", 6),
            (b"4;2;?", 7),
            (b"66;;hello", 22),
            (b"5522;type=read;payload", 23),
            (b"3008;start=abc123", 24),
            (b"133;A", 3),
        ];

        for (input, expected) in cases {
            let command = parse_osc(parser, input, 0);
            assert_eq!(roastty_osc_command_type(command), *expected, "{input:?}");
        }

        for unsupported in [b"1;icon".as_slice(), b"9;1;10", b"9;2;message"] {
            let command = parse_osc(parser, unsupported, 0);
            assert!(
                command.is_null(),
                "reserved command unexpectedly returned for {unsupported:?}"
            );
        }

        roastty_osc_free(parser);
    }
}
