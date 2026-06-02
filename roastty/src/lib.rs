use std::alloc::{self, Layout};
use std::ffi::CString;
use std::io::Write;
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::ptr::NonNull;
use std::slice;
use std::sync::Mutex;

use input::{key, key_encode, key_mods, paste};
use terminal::color;
use terminal::cursor;
use terminal::focus;
use terminal::kitty::graphics_command::{TransmissionCompression, TransmissionFormat};
use terminal::kitty::graphics_image::Image as KittyImage;
use terminal::kitty::graphics_storage::{
    ImageStorage as KittyImageStorage, Placement as KittyPlacement,
    PlacementId as KittyPlacementId, PlacementKey as KittyPlacementKey,
    PlacementLocation as KittyPlacementLocation,
};
use terminal::kitty::KeyFlags;
use terminal::modes;
use terminal::selection_gesture::{
    SelectionGesture, SelectionGestureAutoscroll, SelectionGestureAutoscrollTick,
    SelectionGestureBehavior, SelectionGestureDeepPress, SelectionGestureDrag,
    SelectionGestureGeometry, SelectionGesturePress, SelectionGestureRelease, DEFAULT_BEHAVIORS,
};
use terminal::terminal::{
    Terminal as InnerTerminal, TerminalBellCallback, TerminalColorKind,
    TerminalColorSchemeCallback, TerminalDeviceAttributesCallback, TerminalEnquiryCallback,
    TerminalFormatterExtra, TerminalGridRef, TerminalGridRefPointError, TerminalPointTag,
    TerminalScreen, TerminalSelection, TerminalSelectionAdjustment, TerminalSelectionFormat,
    TerminalSelectionOrder, TerminalSizeCallback, TerminalStreamError,
    TerminalTitleChangedCallback, TerminalTrackedGridRef, TerminalWritePtyCallback,
    TerminalXtversionCallback,
};
use terminal::{mouse, mouse_encode, osc, point, sgr, size_report, style};

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
pub type RoasttyFormatter = *mut c_void;
pub type RoasttyKeyEncoder = *mut c_void;
pub type RoasttyKeyEvent = *mut c_void;
pub type RoasttyMouseEncoder = *mut c_void;
pub type RoasttyMouseEvent = *mut c_void;
pub type RoasttyOscCommand = *mut c_void;
pub type RoasttyOscParser = *mut c_void;
pub type RoasttySelectionGesture = *mut c_void;
pub type RoasttySelectionGestureEvent = *mut c_void;
pub type RoasttySurface = *mut c_void;
pub type RoasttyTerminal = *mut c_void;
pub type RoasttyTrackedGridRef = *mut c_void;
pub type RoasttyRenderStateHandle = *mut c_void;
pub type RoasttyRenderStateRowIterator = *mut c_void;
pub type RoasttyRenderStateRowCells = *mut c_void;
pub type RoasttyKittyGraphics = *mut c_void;
pub type RoasttyKittyGraphicsImage = *mut c_void;
pub type RoasttyKittyGraphicsPlacementIterator = *mut c_void;
type RoasttyCell = u64;
type RoasttyRow = u64;

const ROASTTY_MODE_TAG_VALUE_MASK: u16 = 0x7fff;
const ROASTTY_MODE_TAG_ANSI_BIT: u16 = 0x8000;

const ROASTTY_SUCCESS: c_int = 0;
#[allow(dead_code)]
const ROASTTY_OUT_OF_MEMORY: c_int = 1;
const ROASTTY_INVALID_VALUE: c_int = 2;
const ROASTTY_OUT_OF_SPACE: c_int = 3;
const ROASTTY_NO_VALUE: c_int = 4;
const ROASTTY_BUILD_MODE_DEBUG: c_int = 0;

const ROASTTY_OPTIMIZE_DEBUG: c_int = 0;
#[allow(dead_code)]
const ROASTTY_OPTIMIZE_RELEASE_SAFE: c_int = 1;
#[allow(dead_code)]
const ROASTTY_OPTIMIZE_RELEASE_SMALL: c_int = 2;
const ROASTTY_OPTIMIZE_RELEASE_FAST: c_int = 3;

const ROASTTY_BUILD_INFO_INVALID: c_int = 0;
const ROASTTY_BUILD_INFO_SIMD: c_int = 1;
const ROASTTY_BUILD_INFO_KITTY_GRAPHICS: c_int = 2;
const ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE: c_int = 3;
const ROASTTY_BUILD_INFO_OPTIMIZE: c_int = 4;
const ROASTTY_BUILD_INFO_VERSION_STRING: c_int = 5;
const ROASTTY_BUILD_INFO_VERSION_MAJOR: c_int = 6;
const ROASTTY_BUILD_INFO_VERSION_MINOR: c_int = 7;
const ROASTTY_BUILD_INFO_VERSION_PATCH: c_int = 8;
const ROASTTY_BUILD_INFO_VERSION_PRE: c_int = 9;
const ROASTTY_BUILD_INFO_VERSION_BUILD: c_int = 10;

const ROASTTY_SYS_LOG_LEVEL_ERROR: c_int = 0;
const ROASTTY_SYS_LOG_LEVEL_WARNING: c_int = 1;
const ROASTTY_SYS_LOG_LEVEL_INFO: c_int = 2;
const ROASTTY_SYS_LOG_LEVEL_DEBUG: c_int = 3;

const ROASTTY_SYS_OPT_USERDATA: c_int = 0;
const ROASTTY_SYS_OPT_DECODE_PNG: c_int = 1;
const ROASTTY_SYS_OPT_LOG: c_int = 2;

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

const ROASTTY_KITTY_GRAPHICS_DATA_INVALID: c_int = 0;
const ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR: c_int = 1;

const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID: c_int = 0;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID: c_int = 1;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID: c_int = 2;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL: c_int = 3;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET: c_int = 4;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET: c_int = 5;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_X: c_int = 6;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_Y: c_int = 7;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_WIDTH: c_int = 8;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_HEIGHT: c_int = 9;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS: c_int = 10;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS: c_int = 11;
const ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z: c_int = 12;

const ROASTTY_KITTY_PLACEMENT_LAYER_ALL: c_int = 0;
const ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG: c_int = 1;
const ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT: c_int = 2;
const ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT: c_int = 3;

const ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER: c_int = 0;

const ROASTTY_KITTY_IMAGE_FORMAT_RGB: c_int = 0;
const ROASTTY_KITTY_IMAGE_FORMAT_RGBA: c_int = 1;
const ROASTTY_KITTY_IMAGE_FORMAT_PNG: c_int = 2;
const ROASTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA: c_int = 3;
const ROASTTY_KITTY_IMAGE_FORMAT_GRAY: c_int = 4;

const ROASTTY_KITTY_IMAGE_COMPRESSION_NONE: c_int = 0;
const ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE: c_int = 1;

const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID: c_int = 0;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID: c_int = 1;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER: c_int = 2;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH: c_int = 3;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT: c_int = 4;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT: c_int = 5;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION: c_int = 6;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR: c_int = 7;
const ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN: c_int = 8;

const ROASTTY_TERMINAL_OPTION_USERDATA: c_int = 0;
const ROASTTY_TERMINAL_OPTION_WRITE_PTY: c_int = 1;
const ROASTTY_TERMINAL_OPTION_BELL: c_int = 2;
const ROASTTY_TERMINAL_OPTION_ENQUIRY: c_int = 3;
const ROASTTY_TERMINAL_OPTION_XTVERSION: c_int = 4;
const ROASTTY_TERMINAL_OPTION_TITLE_CHANGED: c_int = 5;
const ROASTTY_TERMINAL_OPTION_SIZE_CB: c_int = 6;
const ROASTTY_TERMINAL_OPTION_COLOR_SCHEME: c_int = 7;
const ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES: c_int = 8;
const ROASTTY_TERMINAL_OPTION_TITLE: c_int = 9;
const ROASTTY_TERMINAL_OPTION_PWD: c_int = 10;
const ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND: c_int = 11;
const ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND: c_int = 12;
const ROASTTY_TERMINAL_OPTION_COLOR_CURSOR: c_int = 13;
const ROASTTY_TERMINAL_OPTION_COLOR_PALETTE: c_int = 14;
const ROASTTY_TERMINAL_OPTION_SELECTION: c_int = 21;

const ROASTTY_FOCUS_EVENT_GAINED: c_int = 0;
const ROASTTY_FOCUS_EVENT_LOST: c_int = 1;

const ROASTTY_MODE_REPORT_NOT_RECOGNIZED: c_int = 0;
const ROASTTY_MODE_REPORT_SET: c_int = 1;
const ROASTTY_MODE_REPORT_RESET: c_int = 2;
const ROASTTY_MODE_REPORT_PERMANENTLY_SET: c_int = 3;
const ROASTTY_MODE_REPORT_PERMANENTLY_RESET: c_int = 4;

#[allow(dead_code)]
const ROASTTY_SELECTION_FORMAT_PLAIN: c_int = 0;
#[allow(dead_code)]
const ROASTTY_SELECTION_FORMAT_VT: c_int = 1;
#[allow(dead_code)]
const ROASTTY_SELECTION_FORMAT_HTML: c_int = 2;

#[allow(dead_code)]
const ROASTTY_FORMATTER_FORMAT_PLAIN: c_int = 0;
#[allow(dead_code)]
const ROASTTY_FORMATTER_FORMAT_VT: c_int = 1;
#[allow(dead_code)]
const ROASTTY_FORMATTER_FORMAT_HTML: c_int = 2;

#[allow(dead_code)]
const ROASTTY_SELECTION_ORDER_FORWARD: c_int = 0;
#[allow(dead_code)]
const ROASTTY_SELECTION_ORDER_REVERSE: c_int = 1;
#[allow(dead_code)]
const ROASTTY_SELECTION_ORDER_MIRRORED_FORWARD: c_int = 2;
#[allow(dead_code)]
const ROASTTY_SELECTION_ORDER_MIRRORED_REVERSE: c_int = 3;

#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_LEFT: c_int = 0;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_RIGHT: c_int = 1;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_UP: c_int = 2;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_DOWN: c_int = 3;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_HOME: c_int = 4;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_END: c_int = 5;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_PAGE_UP: c_int = 6;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_PAGE_DOWN: c_int = 7;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_BEGINNING_OF_LINE: c_int = 8;
#[allow(dead_code)]
const ROASTTY_SELECTION_ADJUST_END_OF_LINE: c_int = 9;

const ROASTTY_SELECTION_GESTURE_EVENT_PRESS: c_int = 0;
const ROASTTY_SELECTION_GESTURE_EVENT_RELEASE: c_int = 1;
const ROASTTY_SELECTION_GESTURE_EVENT_DRAG: c_int = 2;
const ROASTTY_SELECTION_GESTURE_EVENT_AUTOSCROLL_TICK: c_int = 3;
const ROASTTY_SELECTION_GESTURE_EVENT_DEEP_PRESS: c_int = 4;

const ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT: c_int = 0;
const ROASTTY_SELECTION_GESTURE_DATA_DRAGGED: c_int = 1;
const ROASTTY_SELECTION_GESTURE_DATA_AUTOSCROLL: c_int = 2;
const ROASTTY_SELECTION_GESTURE_DATA_BEHAVIOR: c_int = 3;
const ROASTTY_SELECTION_GESTURE_DATA_ANCHOR: c_int = 4;

const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF: c_int = 0;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION: c_int = 1;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_DISTANCE: c_int = 2;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS: c_int = 3;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_INTERVAL_NS: c_int = 4;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS: c_int = 5;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_BEHAVIORS: c_int = 6;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_RECTANGLE: c_int = 7;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_GEOMETRY: c_int = 8;
const ROASTTY_SELECTION_GESTURE_EVENT_OPTION_VIEWPORT: c_int = 9;

const ROASTTY_SELECTION_GESTURE_AUTOSCROLL_NONE: c_int = 0;
const ROASTTY_SELECTION_GESTURE_AUTOSCROLL_UP: c_int = 1;
const ROASTTY_SELECTION_GESTURE_AUTOSCROLL_DOWN: c_int = 2;

const ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL: c_int = 0;
const ROASTTY_SELECTION_GESTURE_BEHAVIOR_WORD: c_int = 1;
const ROASTTY_SELECTION_GESTURE_BEHAVIOR_LINE: c_int = 2;
const ROASTTY_SELECTION_GESTURE_BEHAVIOR_OUTPUT: c_int = 3;

#[allow(dead_code)]
const ROASTTY_COLOR_SCHEME_LIGHT: c_int = 0;
#[allow(dead_code)]
const ROASTTY_COLOR_SCHEME_DARK: c_int = 1;

#[allow(dead_code)]
const ROASTTY_SIZE_REPORT_MODE_2048: c_int = 0;
#[allow(dead_code)]
const ROASTTY_SIZE_REPORT_CSI_14_T: c_int = 1;
#[allow(dead_code)]
const ROASTTY_SIZE_REPORT_CSI_16_T: c_int = 2;
#[allow(dead_code)]
const ROASTTY_SIZE_REPORT_CSI_18_T: c_int = 3;

#[allow(dead_code)]
const ROASTTY_POINT_ACTIVE: c_int = 0;
#[allow(dead_code)]
const ROASTTY_POINT_VIEWPORT: c_int = 1;
#[allow(dead_code)]
const ROASTTY_POINT_SCREEN: c_int = 2;
#[allow(dead_code)]
const ROASTTY_POINT_HISTORY: c_int = 3;

const ROASTTY_RENDER_STATE_DIRTY_FALSE: c_int = 0;
const ROASTTY_RENDER_STATE_DIRTY_PARTIAL: c_int = 1;
const ROASTTY_RENDER_STATE_DIRTY_FULL: c_int = 2;

const ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR: c_int = 0;
const ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK: c_int = 1;
const ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE: c_int = 2;
const ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW: c_int = 3;

const ROASTTY_RENDER_STATE_DATA_INVALID: c_int = 0;
const ROASTTY_RENDER_STATE_DATA_COLS: c_int = 1;
const ROASTTY_RENDER_STATE_DATA_ROWS: c_int = 2;
const ROASTTY_RENDER_STATE_DATA_DIRTY: c_int = 3;
const ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR: c_int = 4;
const ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND: c_int = 5;
const ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND: c_int = 6;
const ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR: c_int = 7;
const ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE: c_int = 8;
const ROASTTY_RENDER_STATE_DATA_COLOR_PALETTE: c_int = 9;
const ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE: c_int = 10;
const ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE: c_int = 11;
const ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING: c_int = 12;
const ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT: c_int = 13;
const ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE: c_int = 14;
const ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X: c_int = 15;
const ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y: c_int = 16;
const ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL: c_int = 17;

const ROASTTY_RENDER_STATE_OPTION_DIRTY: c_int = 0;

const ROASTTY_RENDER_STATE_ROW_DATA_INVALID: c_int = 0;
const ROASTTY_RENDER_STATE_ROW_DATA_DIRTY: c_int = 1;
const ROASTTY_RENDER_STATE_ROW_DATA_RAW: c_int = 2;
const ROASTTY_RENDER_STATE_ROW_DATA_CELLS: c_int = 3;
const ROASTTY_RENDER_STATE_ROW_DATA_SELECTION: c_int = 4;

const ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY: c_int = 0;

const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID: c_int = 0;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW: c_int = 1;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE: c_int = 2;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN: c_int = 3;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF: c_int = 4;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR: c_int = 5;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR: c_int = 6;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED: c_int = 7;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING: c_int = 8;
const ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8: c_int = 9;

const ROASTTY_STYLE_COLOR_NONE: c_int = 0;
const ROASTTY_STYLE_COLOR_PALETTE: c_int = 1;
const ROASTTY_STYLE_COLOR_RGB: c_int = 2;

const ROASTTY_CELL_CONTENT_CODEPOINT: c_int = 0;
const ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME: c_int = 1;
const ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE: c_int = 2;
const ROASTTY_CELL_CONTENT_BG_COLOR_RGB: c_int = 3;

#[allow(dead_code)]
const ROASTTY_CELL_WIDE_NARROW: c_int = 0;
#[allow(dead_code)]
const ROASTTY_CELL_WIDE_WIDE: c_int = 1;
#[allow(dead_code)]
const ROASTTY_CELL_WIDE_SPACER_TAIL: c_int = 2;
#[allow(dead_code)]
const ROASTTY_CELL_WIDE_SPACER_HEAD: c_int = 3;

#[allow(dead_code)]
const ROASTTY_CELL_SEMANTIC_OUTPUT: c_int = 0;
#[allow(dead_code)]
const ROASTTY_CELL_SEMANTIC_INPUT: c_int = 1;
#[allow(dead_code)]
const ROASTTY_CELL_SEMANTIC_PROMPT: c_int = 2;

const ROASTTY_CELL_DATA_INVALID: c_int = 0;
const ROASTTY_CELL_DATA_CODEPOINT: c_int = 1;
const ROASTTY_CELL_DATA_CONTENT_TAG: c_int = 2;
const ROASTTY_CELL_DATA_WIDE: c_int = 3;
const ROASTTY_CELL_DATA_HAS_TEXT: c_int = 4;
const ROASTTY_CELL_DATA_HAS_STYLING: c_int = 5;
const ROASTTY_CELL_DATA_STYLE_ID: c_int = 6;
const ROASTTY_CELL_DATA_HAS_HYPERLINK: c_int = 7;
const ROASTTY_CELL_DATA_PROTECTED: c_int = 8;
const ROASTTY_CELL_DATA_SEMANTIC: c_int = 9;
const ROASTTY_CELL_DATA_COLOR_PALETTE: c_int = 10;
const ROASTTY_CELL_DATA_COLOR_RGB: c_int = 11;

#[allow(dead_code)]
const ROASTTY_ROW_SEMANTIC_NONE: c_int = 0;
#[allow(dead_code)]
const ROASTTY_ROW_SEMANTIC_PROMPT: c_int = 1;
#[allow(dead_code)]
const ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION: c_int = 2;

const ROASTTY_ROW_DATA_INVALID: c_int = 0;
const ROASTTY_ROW_DATA_WRAP: c_int = 1;
const ROASTTY_ROW_DATA_WRAP_CONTINUATION: c_int = 2;
const ROASTTY_ROW_DATA_GRAPHEME: c_int = 3;
const ROASTTY_ROW_DATA_STYLED: c_int = 4;
const ROASTTY_ROW_DATA_HYPERLINK: c_int = 5;
const ROASTTY_ROW_DATA_SEMANTIC_PROMPT: c_int = 6;
const ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER: c_int = 7;
const ROASTTY_ROW_DATA_DIRTY: c_int = 8;

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

type AllocCallback = Option<unsafe extern "C" fn(*mut c_void, usize, u8, usize) -> *mut u8>;
type ResizeCallback =
    Option<unsafe extern "C" fn(*mut c_void, *mut c_void, usize, u8, usize, usize) -> bool>;
type RemapCallback =
    Option<unsafe extern "C" fn(*mut c_void, *mut c_void, usize, u8, usize, usize) -> *mut c_void>;
type FreeCallback = Option<unsafe extern "C" fn(*mut c_void, *mut c_void, usize, u8, usize)>;

#[repr(C)]
pub struct RoasttyAllocatorVtable {
    alloc: AllocCallback,
    resize: ResizeCallback,
    remap: RemapCallback,
    free: FreeCallback,
}

#[repr(C)]
pub struct RoasttyAllocator {
    ctx: *mut c_void,
    vtable: *const RoasttyAllocatorVtable,
}

#[repr(C)]
pub struct RoasttySysImage {
    width: u32,
    height: u32,
    data: *mut u8,
    data_len: usize,
}

type SysLogCallback = unsafe extern "C" fn(*mut c_void, c_int, *const u8, usize, *const u8, usize);
type SysDecodePngCallback = unsafe extern "C" fn(
    *mut c_void,
    *const RoasttyAllocator,
    *const u8,
    usize,
    *mut RoasttySysImage,
) -> bool;

#[derive(Clone, Copy, Debug, Default)]
struct SysState {
    userdata: usize,
    decode_png: Option<SysDecodePngCallback>,
    log: Option<SysLogCallback>,
}

static SYS_STATE: Mutex<SysState> = Mutex::new(SysState {
    userdata: 0,
    decode_png: None,
    log: None,
});

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyRgb {
    r: u8,
    g: u8,
    b: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoasttyBuffer {
    ptr: *mut u8,
    cap: usize,
    len: usize,
}

type RoasttyPalette = [RoasttyRgb; 256];

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyRenderStateColors {
    size: usize,
    background: RoasttyRgb,
    foreground: RoasttyRgb,
    cursor: RoasttyRgb,
    cursor_has_value: bool,
    palette: RoasttyPalette,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoasttyRenderStateRowSelection {
    size: usize,
    start_x: u16,
    end_x: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderStateScalar {
    cols: u16,
    rows: u16,
    background: RoasttyRgb,
    foreground: RoasttyRgb,
    cursor: Option<RoasttyRgb>,
    palette: RoasttyPalette,
    dirty: c_int,
    cursor_visual_style: c_int,
    cursor_visible: bool,
    cursor_blinking: bool,
    cursor_password_input: bool,
    cursor_viewport: Option<RenderStateCursorViewport>,
    rows_snapshot: Vec<RenderStateRowSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RenderStateCursorViewport {
    x: u16,
    y: u16,
    wide_tail: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderStateRowSnapshot {
    raw: RoasttyRow,
    dirty: bool,
    selection: Option<RenderStateRowSelectionRange>,
    cells: Vec<RenderStateCellSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RenderStateRowSelectionRange {
    start_x: u16,
    end_x: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderStateCellSnapshot {
    raw: RoasttyCell,
    style: Option<style::Style>,
    graphemes: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KittyGraphicsImageSnapshot {
    id: u32,
    number: u32,
    width: u32,
    height: u32,
    format: c_int,
    compression: c_int,
    data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KittyGraphicsPlacementSnapshot {
    key: KittyPlacementKey,
    placement: KittyPlacement,
}

#[derive(Debug)]
struct KittyGraphicsPlacementIterator {
    entries: Vec<KittyGraphicsPlacementSnapshot>,
    selected: Option<usize>,
    layer_filter: c_int,
}

impl Default for KittyGraphicsPlacementIterator {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            selected: None,
            layer_filter: ROASTTY_KITTY_PLACEMENT_LAYER_ALL,
        }
    }
}

#[derive(Debug)]
struct RenderStateRowIterator {
    rows: Vec<RenderStateRowSnapshot>,
    palette: RoasttyPalette,
    selected: Option<usize>,
    bound: bool,
}

impl Default for RenderStateRowIterator {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            palette: palette_from_color(color::DEFAULT_PALETTE),
            selected: None,
            bound: false,
        }
    }
}

#[derive(Debug)]
struct RenderStateRowCells {
    cells: Vec<RenderStateCellSnapshot>,
    palette: RoasttyPalette,
    selected: Option<usize>,
    selection: Option<RenderStateRowSelectionRange>,
    bound: bool,
}

impl Default for RenderStateRowCells {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            palette: palette_from_color(color::DEFAULT_PALETTE),
            selected: None,
            selection: None,
            bound: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union RoasttyStyleColorValue {
    palette: u8,
    rgb: RoasttyRgb,
    _padding: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyStyleColor {
    tag: c_int,
    value: RoasttyStyleColorValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoasttyStyle {
    size: usize,
    fg_color: RoasttyStyleColor,
    bg_color: RoasttyStyleColor,
    underline_color: RoasttyStyleColor,
    bold: bool,
    italic: bool,
    faint: bool,
    blink: bool,
    inverse: bool,
    invisible: bool,
    strikethrough: bool,
    overline: bool,
    underline: c_int,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttySizeReportSize {
    rows: u16,
    columns: u16,
    cell_width: u32,
    cell_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyPointCoordinate {
    x: u16,
    y: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union RoasttyPointValue {
    active: RoasttyPointCoordinate,
    viewport: RoasttyPointCoordinate,
    screen: RoasttyPointCoordinate,
    history: RoasttyPointCoordinate,
    _padding: [u64; 2],
}

impl Default for RoasttyPointValue {
    fn default() -> Self {
        Self { _padding: [0; 2] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RoasttyPoint {
    tag: c_int,
    value: RoasttyPointValue,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyGridRef {
    size: usize,
    node: *mut c_void,
    x: u16,
    y: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttySelection {
    size: usize,
    start: RoasttyGridRef,
    end: RoasttyGridRef,
    rectangle: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyKittyGraphicsPlacementRenderInfo {
    size: usize,
    pixel_width: u32,
    pixel_height: u32,
    grid_cols: u32,
    grid_rows: u32,
    viewport_col: i32,
    viewport_row: i32,
    viewport_visible: bool,
    source_x: u32,
    source_y: u32,
    source_width: u32,
    source_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RoasttySurfacePosition {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyCodepoints {
    ptr: *const u32,
    len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttySelectionGestureBehaviors {
    single_click: c_int,
    double_click: c_int,
    triple_click: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttySelectionGestureGeometry {
    columns: u32,
    cell_width: u32,
    padding_left: u32,
    screen_height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyTerminalSelectWordOptions {
    size: usize,
    ref_: RoasttyGridRef,
    boundary_codepoints: *const u32,
    boundary_codepoints_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyTerminalSelectWordBetweenOptions {
    size: usize,
    start: RoasttyGridRef,
    end: RoasttyGridRef,
    boundary_codepoints: *const u32,
    boundary_codepoints_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyTerminalSelectLineOptions {
    size: usize,
    ref_: RoasttyGridRef,
    whitespace: *const u32,
    whitespace_len: usize,
    semantic_prompt_boundary: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyTerminalSelectionFormatOptions {
    size: usize,
    emit: c_int,
    unwrap: bool,
    trim: bool,
    selection: *const RoasttySelection,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyFormatterScreenExtra {
    size: usize,
    cursor: bool,
    style: bool,
    hyperlink: bool,
    protection: bool,
    kitty_keyboard: bool,
    charsets: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyFormatterTerminalExtra {
    size: usize,
    palette: bool,
    modes: bool,
    scrolling_region: bool,
    tabstops: bool,
    pwd: bool,
    keyboard: bool,
    screen: RoasttyFormatterScreenExtra,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoasttyFormatterTerminalOptions {
    size: usize,
    emit: c_int,
    unwrap: bool,
    trim: bool,
    extra: RoasttyFormatterTerminalExtra,
    selection: *const RoasttySelection,
}

struct SelectionGestureHandle {
    gesture: SelectionGesture,
}

struct SelectionGestureEventHandle {
    event: SelectionGestureEventKind,
    ref_: Option<TerminalGridRef>,
    position: RoasttySurfacePosition,
    repeat_distance: f64,
    time_ns: Option<u64>,
    repeat_interval_ns: u64,
    word_boundary_codepoints: Option<Vec<u32>>,
    behaviors: [SelectionGestureBehavior; 3],
    rectangle: bool,
    geometry: Option<SelectionGestureGeometry>,
    viewport: Option<point::Coordinate>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionGestureEventKind {
    Press,
    Release,
    Drag,
    AutoscrollTick,
    DeepPress,
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
    tracked_grid_refs: Vec<NonNull<TrackedGridRefHandle>>,
}

struct FormatterHandle {
    terminal: RoasttyTerminal,
    format: TerminalSelectionFormat,
    unwrap: bool,
    trim: bool,
    extra: TerminalFormatterExtra,
    selection: Option<TerminalSelection>,
}

struct TrackedGridRefHandle {
    terminal: Option<NonNull<Terminal>>,
    terminal_handle: RoasttyTerminal,
    tracked: Option<TerminalTrackedGridRef>,
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
static EMPTY_STRING: &[u8] = b"\0";
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

fn formatter_from_handle<'a>(handle: RoasttyFormatter) -> Option<&'a mut FormatterHandle> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<FormatterHandle>()) })
    }
}

fn tracked_grid_ref_from_handle<'a>(
    handle: RoasttyTrackedGridRef,
) -> Option<&'a mut TrackedGridRefHandle> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<TrackedGridRefHandle>()) })
    }
}

impl Terminal {
    fn detach_tracked_grid_refs(&mut self) {
        for mut tracked in self.tracked_grid_refs.drain(..) {
            let tracked = unsafe { tracked.as_mut() };
            tracked.terminal = None;
            tracked.tracked = None;
        }
    }

    fn unregister_tracked_grid_ref(&mut self, tracked: NonNull<TrackedGridRefHandle>) {
        if let Some(index) = self
            .tracked_grid_refs
            .iter()
            .position(|current| *current == tracked)
        {
            self.tracked_grid_refs.swap_remove(index);
        }
    }
}

fn selection_gesture_from_handle<'a>(
    handle: RoasttySelectionGesture,
) -> Option<&'a mut SelectionGestureHandle> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<SelectionGestureHandle>()) })
    }
}

fn selection_gesture_event_from_handle<'a>(
    handle: RoasttySelectionGestureEvent,
) -> Option<&'a mut SelectionGestureEventHandle> {
    if handle.is_null() {
        None
    } else {
        Some(unsafe { &mut *(handle.cast::<SelectionGestureEventHandle>()) })
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

fn roastty_rgb(rgb: (u8, u8, u8)) -> RoasttyRgb {
    RoasttyRgb {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
    }
}

fn palette_from_color(palette: color::Palette) -> RoasttyPalette {
    let mut result = [RoasttyRgb::default(); 256];
    for (index, rgb) in palette.into_iter().enumerate() {
        result[index] = RoasttyRgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        };
    }
    result
}

fn write_rgb(out: *mut c_void, rgb: (u8, u8, u8)) {
    unsafe {
        out.cast::<RoasttyRgb>().write(roastty_rgb(rgb));
    }
}

fn write_palette(out: *mut c_void, palette: [(u8, u8, u8); 256]) {
    unsafe {
        out.cast::<RoasttyPalette>()
            .write(palette_from_tuples(palette));
    }
}

fn render_state_default() -> RenderStateScalar {
    RenderStateScalar {
        cols: 0,
        rows: 0,
        background: RoasttyRgb { r: 0, g: 0, b: 0 },
        foreground: RoasttyRgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        },
        cursor: None,
        palette: palette_from_color(color::DEFAULT_PALETTE),
        dirty: ROASTTY_RENDER_STATE_DIRTY_FALSE,
        cursor_visual_style: ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK,
        cursor_visible: true,
        cursor_blinking: false,
        cursor_password_input: false,
        cursor_viewport: None,
        rows_snapshot: Vec::new(),
    }
}

fn render_cursor_visual_style(value: cursor::VisualStyle) -> c_int {
    match value {
        cursor::VisualStyle::Bar => ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR,
        cursor::VisualStyle::Block => ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK,
        cursor::VisualStyle::Underline => ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE,
        cursor::VisualStyle::BlockHollow => ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW,
    }
}

fn render_state_from_terminal(terminal: &InnerTerminal) -> RenderStateScalar {
    let background = terminal
        .color_effective(TerminalColorKind::Background)
        .map(roastty_rgb)
        .unwrap_or(RoasttyRgb { r: 0, g: 0, b: 0 });
    let foreground = terminal
        .color_effective(TerminalColorKind::Foreground)
        .map(roastty_rgb)
        .unwrap_or(RoasttyRgb {
            r: 0xff,
            g: 0xff,
            b: 0xff,
        });
    let cursor = terminal
        .color_effective(TerminalColorKind::Cursor)
        .map(roastty_rgb);
    let (cursor_x, cursor_y) = terminal.cursor_position();
    let cursor_viewport = (cursor_x < terminal.columns() && cursor_y < terminal.rows()).then_some(
        RenderStateCursorViewport {
            x: cursor_x,
            y: cursor_y,
            wide_tail: false,
        },
    );

    RenderStateScalar {
        cols: terminal.columns(),
        rows: terminal.rows(),
        background,
        foreground,
        cursor,
        palette: palette_from_tuples(terminal.palette_current()),
        dirty: ROASTTY_RENDER_STATE_DIRTY_FULL,
        cursor_visual_style: render_cursor_visual_style(terminal.cursor_visual_style()),
        cursor_visible: terminal.cursor_visible(),
        cursor_blinking: terminal.cursor_blinking(),
        cursor_password_input: false,
        cursor_viewport,
        rows_snapshot: terminal
            .render_rows_snapshot()
            .into_iter()
            .map(|row| RenderStateRowSnapshot {
                raw: row.raw,
                dirty: row.dirty,
                selection: row.selection.map(|selection| RenderStateRowSelectionRange {
                    start_x: selection.start_x,
                    end_x: selection.end_x,
                }),
                cells: row
                    .cells
                    .into_iter()
                    .map(|cell| RenderStateCellSnapshot {
                        raw: cell.raw,
                        style: cell.style,
                        graphemes: cell.graphemes,
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn render_state_from_handle(
    handle: RoasttyRenderStateHandle,
) -> Option<&'static RenderStateScalar> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &*handle.cast::<RenderStateScalar>() })
}

fn render_state_mut_from_handle(
    handle: RoasttyRenderStateHandle,
) -> Option<&'static mut RenderStateScalar> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &mut *handle.cast::<RenderStateScalar>() })
}

fn render_state_row_iterator_mut_from_handle(
    handle: RoasttyRenderStateRowIterator,
) -> Option<&'static mut RenderStateRowIterator> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &mut *handle.cast::<RenderStateRowIterator>() })
}

fn render_state_row_cells_mut_from_handle(
    handle: RoasttyRenderStateRowCells,
) -> Option<&'static mut RenderStateRowCells> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &mut *handle.cast::<RenderStateRowCells>() })
}

fn kitty_graphics_from_handle(handle: RoasttyKittyGraphics) -> Option<&'static KittyImageStorage> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &*handle.cast::<KittyImageStorage>() })
}

fn kitty_graphics_image_from_handle(
    handle: RoasttyKittyGraphicsImage,
) -> Option<&'static KittyGraphicsImageSnapshot> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &*handle.cast::<KittyGraphicsImageSnapshot>() })
}

fn kitty_graphics_placement_iterator_mut_from_handle(
    handle: RoasttyKittyGraphicsPlacementIterator,
) -> Option<&'static mut KittyGraphicsPlacementIterator> {
    if handle.is_null() {
        return None;
    }
    Some(unsafe { &mut *handle.cast::<KittyGraphicsPlacementIterator>() })
}

fn kitty_image_format_value(format: TransmissionFormat) -> c_int {
    match format {
        TransmissionFormat::Rgb => ROASTTY_KITTY_IMAGE_FORMAT_RGB,
        TransmissionFormat::Rgba => ROASTTY_KITTY_IMAGE_FORMAT_RGBA,
        TransmissionFormat::Png => ROASTTY_KITTY_IMAGE_FORMAT_PNG,
        TransmissionFormat::GrayAlpha => ROASTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA,
        TransmissionFormat::Gray => ROASTTY_KITTY_IMAGE_FORMAT_GRAY,
    }
}

fn kitty_image_compression_value(compression: TransmissionCompression) -> c_int {
    match compression {
        TransmissionCompression::None => ROASTTY_KITTY_IMAGE_COMPRESSION_NONE,
        TransmissionCompression::ZlibDeflate => ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE,
    }
}

fn kitty_image_format_from_value(format: c_int) -> Option<TransmissionFormat> {
    match format {
        ROASTTY_KITTY_IMAGE_FORMAT_RGB => Some(TransmissionFormat::Rgb),
        ROASTTY_KITTY_IMAGE_FORMAT_RGBA => Some(TransmissionFormat::Rgba),
        ROASTTY_KITTY_IMAGE_FORMAT_PNG => Some(TransmissionFormat::Png),
        ROASTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA => Some(TransmissionFormat::GrayAlpha),
        ROASTTY_KITTY_IMAGE_FORMAT_GRAY => Some(TransmissionFormat::Gray),
        _ => None,
    }
}

fn kitty_image_compression_from_value(compression: c_int) -> Option<TransmissionCompression> {
    match compression {
        ROASTTY_KITTY_IMAGE_COMPRESSION_NONE => Some(TransmissionCompression::None),
        ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE => Some(TransmissionCompression::ZlibDeflate),
        _ => None,
    }
}

fn kitty_image_snapshot(image: &KittyImage) -> Result<KittyGraphicsImageSnapshot, c_int> {
    let mut data = Vec::new();
    data.try_reserve_exact(image.data.len())
        .map_err(|_| ROASTTY_OUT_OF_MEMORY)?;
    data.extend_from_slice(&image.data);
    Ok(KittyGraphicsImageSnapshot {
        id: image.id,
        number: image.number,
        width: image.width,
        height: image.height,
        format: kitty_image_format_value(image.format),
        compression: kitty_image_compression_value(image.compression),
        data,
    })
}

fn kitty_image_from_snapshot(image: &KittyGraphicsImageSnapshot) -> Result<KittyImage, c_int> {
    Ok(KittyImage {
        id: image.id,
        number: image.number,
        width: image.width,
        height: image.height,
        format: kitty_image_format_from_value(image.format).ok_or(ROASTTY_INVALID_VALUE)?,
        compression: kitty_image_compression_from_value(image.compression)
            .ok_or(ROASTTY_INVALID_VALUE)?,
        data: Vec::new(),
        transmit_time: None,
        implicit_id: false,
    })
}

fn kitty_placement_public_id(id: KittyPlacementId) -> u32 {
    match id {
        KittyPlacementId::External(id) => id,
        KittyPlacementId::Internal(_) => 0,
    }
}

fn kitty_placement_layer_matches(layer: c_int, z: i32) -> bool {
    match layer {
        ROASTTY_KITTY_PLACEMENT_LAYER_ALL => true,
        ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG => z < i32::MIN / 2,
        ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT => z >= i32::MIN / 2 && z < 0,
        ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT => z >= 0,
        _ => false,
    }
}

fn kitty_selected_placement_key(
    iterator: &KittyGraphicsPlacementIterator,
) -> Result<KittyPlacementKey, c_int> {
    let selected = iterator.selected.ok_or(ROASTTY_NO_VALUE)?;
    let item = iterator.entries.get(selected).ok_or(ROASTTY_NO_VALUE)?;
    Ok(item.key)
}

fn kitty_source_rect(
    placement: KittyPlacement,
    image: &KittyGraphicsImageSnapshot,
) -> (u32, u32, u32, u32) {
    let x = placement.source_x.min(image.width);
    let y = placement.source_y.min(image.height);
    let width = if placement.source_width > 0 {
        placement.source_width
    } else {
        image.width
    };
    let height = if placement.source_height > 0 {
        placement.source_height
    } else {
        image.height
    };
    (
        x,
        y,
        width.min(image.width.saturating_sub(x)),
        height.min(image.height.saturating_sub(y)),
    )
}

fn kitty_live_placement_for_geometry(
    iterator: &KittyGraphicsPlacementIterator,
    terminal: &InnerTerminal,
) -> Result<KittyPlacement, c_int> {
    let key = kitty_selected_placement_key(iterator)?;
    terminal.kitty_placement(key).ok_or(ROASTTY_NO_VALUE)
}

fn render_dirty_from_raw(value: c_int) -> Option<c_int> {
    match value {
        ROASTTY_RENDER_STATE_DIRTY_FALSE
        | ROASTTY_RENDER_STATE_DIRTY_PARTIAL
        | ROASTTY_RENDER_STATE_DIRTY_FULL => Some(value),
        _ => None,
    }
}

fn valid_render_state_data(data: c_int) -> bool {
    matches!(
        data,
        ROASTTY_RENDER_STATE_DATA_INVALID..=ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL
    )
}

fn valid_render_state_row_data(data: c_int) -> bool {
    matches!(
        data,
        ROASTTY_RENDER_STATE_ROW_DATA_INVALID..=ROASTTY_RENDER_STATE_ROW_DATA_SELECTION
    )
}

fn valid_render_state_row_option(option: c_int) -> bool {
    option == ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY
}

fn valid_kitty_graphics_data(data: c_int) -> bool {
    matches!(
        data,
        ROASTTY_KITTY_GRAPHICS_DATA_INVALID..=ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR
    )
}

fn valid_kitty_graphics_image_data(data: c_int) -> bool {
    matches!(
        data,
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID..=ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN
    )
}

fn valid_kitty_graphics_placement_data(data: c_int) -> bool {
    matches!(
        data,
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID..=ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z
    )
}

fn valid_kitty_graphics_placement_layer(layer: c_int) -> bool {
    matches!(
        layer,
        ROASTTY_KITTY_PLACEMENT_LAYER_ALL
            | ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG
            | ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT
            | ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT
    )
}

fn valid_kitty_graphics_placement_iterator_option(option: c_int) -> bool {
    option == ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER
}

fn valid_render_state_row_cells_data(data: c_int) -> bool {
    matches!(
        data,
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID
            ..=ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8
    )
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
        ROASTTY_TERMINAL_DATA_SELECTION => {
            let Some(selection) = terminal.active_selection() else {
                return ROASTTY_NO_VALUE;
            };
            write_selection(out.cast::<RoasttySelection>(), selection);
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
        | ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE => return ROASTTY_NO_VALUE,
        ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS => out
            .cast::<RoasttyKittyGraphics>()
            .write(terminal.kitty_images() as *const KittyImageStorage as RoasttyKittyGraphics),
        _ => return ROASTTY_INVALID_VALUE,
    }

    ROASTTY_SUCCESS
}

unsafe fn render_state_get_write(
    state: &RenderStateScalar,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    match data {
        ROASTTY_RENDER_STATE_DATA_COLS => out.cast::<u16>().write(state.cols),
        ROASTTY_RENDER_STATE_DATA_ROWS => out.cast::<u16>().write(state.rows),
        ROASTTY_RENDER_STATE_DATA_DIRTY => out.cast::<c_int>().write(state.dirty),
        ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR => {
            let iterator_handle = out.cast::<RoasttyRenderStateRowIterator>().read();
            let Some(iterator) = render_state_row_iterator_mut_from_handle(iterator_handle) else {
                return ROASTTY_INVALID_VALUE;
            };
            iterator.rows.clone_from(&state.rows_snapshot);
            iterator.palette = state.palette;
            iterator.selected = None;
            iterator.bound = true;
        }
        ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND => {
            out.cast::<RoasttyRgb>().write(state.background)
        }
        ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND => {
            out.cast::<RoasttyRgb>().write(state.foreground)
        }
        ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR => {
            let Some(cursor) = state.cursor else {
                return ROASTTY_NO_VALUE;
            };
            out.cast::<RoasttyRgb>().write(cursor);
        }
        ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE => {
            out.cast::<bool>().write(state.cursor.is_some())
        }
        ROASTTY_RENDER_STATE_DATA_COLOR_PALETTE => {
            out.cast::<RoasttyPalette>().write(state.palette)
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE => {
            out.cast::<c_int>().write(state.cursor_visual_style)
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE => out.cast::<bool>().write(state.cursor_visible),
        ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING => {
            out.cast::<bool>().write(state.cursor_blinking)
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT => {
            out.cast::<bool>().write(state.cursor_password_input)
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE => {
            out.cast::<bool>().write(state.cursor_viewport.is_some())
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X => {
            let Some(viewport) = state.cursor_viewport else {
                return ROASTTY_NO_VALUE;
            };
            out.cast::<u16>().write(viewport.x);
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y => {
            let Some(viewport) = state.cursor_viewport else {
                return ROASTTY_NO_VALUE;
            };
            out.cast::<u16>().write(viewport.y);
        }
        ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL => {
            let Some(viewport) = state.cursor_viewport else {
                return ROASTTY_NO_VALUE;
            };
            out.cast::<bool>().write(viewport.wide_tail);
        }
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

fn point_tag_from_raw(value: c_int) -> Option<TerminalPointTag> {
    TerminalPointTag::from_raw(value)
}

fn point_coordinate(point: RoasttyPoint, tag: TerminalPointTag) -> RoasttyPointCoordinate {
    unsafe {
        match tag {
            TerminalPointTag::Active => point.value.active,
            TerminalPointTag::Viewport => point.value.viewport,
            TerminalPointTag::Screen => point.value.screen,
            TerminalPointTag::History => point.value.history,
        }
    }
}

fn write_grid_ref(out: *mut RoasttyGridRef, grid_ref: TerminalGridRef) {
    unsafe {
        out.write(RoasttyGridRef {
            size: std::mem::size_of::<RoasttyGridRef>(),
            node: grid_ref.node.cast_mut().cast(),
            x: grid_ref.x,
            y: grid_ref.y,
        });
    }
}

fn read_grid_ref_value(value: RoasttyGridRef) -> Result<TerminalGridRef, c_int> {
    if value.size < std::mem::size_of::<RoasttyGridRef>() {
        return Err(ROASTTY_INVALID_VALUE);
    }
    Ok(TerminalGridRef {
        node: value.node.cast_const().cast(),
        x: value.x,
        y: value.y,
    })
}

fn read_grid_ref_ptr(grid_ref: *const RoasttyGridRef) -> Result<TerminalGridRef, c_int> {
    if grid_ref.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    let grid_ref_size = unsafe { (*grid_ref).size };
    if grid_ref_size < std::mem::size_of::<RoasttyGridRef>() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    read_grid_ref_value(unsafe { grid_ref.read() })
}

fn write_selection(out: *mut RoasttySelection, selection: TerminalSelection) {
    unsafe {
        out.write(RoasttySelection {
            size: std::mem::size_of::<RoasttySelection>(),
            start: RoasttyGridRef {
                size: std::mem::size_of::<RoasttyGridRef>(),
                node: selection.start.node.cast_mut().cast(),
                x: selection.start.x,
                y: selection.start.y,
            },
            end: RoasttyGridRef {
                size: std::mem::size_of::<RoasttyGridRef>(),
                node: selection.end.node.cast_mut().cast(),
                x: selection.end.x,
                y: selection.end.y,
            },
            rectangle: selection.rectangle,
        });
    }
}

fn read_selection(selection: *const RoasttySelection) -> Result<TerminalSelection, c_int> {
    if selection.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    let selection_size = unsafe { (*selection).size };
    if selection_size < std::mem::size_of::<RoasttySelection>() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    Ok(TerminalSelection {
        start: read_grid_ref_ptr(unsafe { ptr::addr_of!((*selection).start) })?,
        end: read_grid_ref_ptr(unsafe { ptr::addr_of!((*selection).end) })?,
        rectangle: unsafe { ptr::addr_of!((*selection).rectangle).read() },
    })
}

fn selection_gesture_event_kind_from_raw(value: c_int) -> Option<SelectionGestureEventKind> {
    match value {
        ROASTTY_SELECTION_GESTURE_EVENT_PRESS => Some(SelectionGestureEventKind::Press),
        ROASTTY_SELECTION_GESTURE_EVENT_RELEASE => Some(SelectionGestureEventKind::Release),
        ROASTTY_SELECTION_GESTURE_EVENT_DRAG => Some(SelectionGestureEventKind::Drag),
        ROASTTY_SELECTION_GESTURE_EVENT_AUTOSCROLL_TICK => {
            Some(SelectionGestureEventKind::AutoscrollTick)
        }
        ROASTTY_SELECTION_GESTURE_EVENT_DEEP_PRESS => Some(SelectionGestureEventKind::DeepPress),
        _ => None,
    }
}

fn selection_gesture_behavior_from_raw(value: c_int) -> Option<SelectionGestureBehavior> {
    match value {
        ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL => Some(SelectionGestureBehavior::Cell),
        ROASTTY_SELECTION_GESTURE_BEHAVIOR_WORD => Some(SelectionGestureBehavior::Word),
        ROASTTY_SELECTION_GESTURE_BEHAVIOR_LINE => Some(SelectionGestureBehavior::Line),
        ROASTTY_SELECTION_GESTURE_BEHAVIOR_OUTPUT => Some(SelectionGestureBehavior::Output),
        _ => None,
    }
}

fn selection_gesture_behavior_to_raw(value: SelectionGestureBehavior) -> c_int {
    match value {
        SelectionGestureBehavior::Cell => ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL,
        SelectionGestureBehavior::Word => ROASTTY_SELECTION_GESTURE_BEHAVIOR_WORD,
        SelectionGestureBehavior::Line => ROASTTY_SELECTION_GESTURE_BEHAVIOR_LINE,
        SelectionGestureBehavior::Output => ROASTTY_SELECTION_GESTURE_BEHAVIOR_OUTPUT,
    }
}

fn selection_gesture_autoscroll_to_raw(value: SelectionGestureAutoscroll) -> c_int {
    match value {
        SelectionGestureAutoscroll::None => ROASTTY_SELECTION_GESTURE_AUTOSCROLL_NONE,
        SelectionGestureAutoscroll::Up => ROASTTY_SELECTION_GESTURE_AUTOSCROLL_UP,
        SelectionGestureAutoscroll::Down => ROASTTY_SELECTION_GESTURE_AUTOSCROLL_DOWN,
    }
}

fn read_selection_gesture_behaviors(
    behaviors: RoasttySelectionGestureBehaviors,
) -> Option<[SelectionGestureBehavior; 3]> {
    Some([
        selection_gesture_behavior_from_raw(behaviors.single_click)?,
        selection_gesture_behavior_from_raw(behaviors.double_click)?,
        selection_gesture_behavior_from_raw(behaviors.triple_click)?,
    ])
}

fn read_selection_gesture_geometry(
    geometry: RoasttySelectionGestureGeometry,
) -> Option<SelectionGestureGeometry> {
    if geometry.columns == 0 || geometry.cell_width == 0 || geometry.screen_height == 0 {
        return None;
    }
    Some(SelectionGestureGeometry {
        columns: geometry.columns,
        cell_width: geometry.cell_width,
        padding_left: geometry.padding_left,
        screen_height: geometry.screen_height,
    })
}

fn read_selection_gesture_codepoints(value: *const RoasttyCodepoints) -> Result<Vec<u32>, c_int> {
    if value.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }
    let value = unsafe { value.read() };
    if value.len > 0 && value.ptr.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }
    if value.len == 0 {
        return Ok(Vec::new());
    }
    let codepoints = unsafe { slice::from_raw_parts(value.ptr, value.len) };
    if codepoints
        .iter()
        .any(|codepoint| char::from_u32(*codepoint).is_none())
    {
        return Err(ROASTTY_INVALID_VALUE);
    }
    Ok(codepoints.to_vec())
}

fn read_sized_abi<T: Copy>(value: *const T) -> Result<T, c_int> {
    if value.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    let size = unsafe { value.cast::<usize>().read() };
    if size < std::mem::size_of::<T>() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    Ok(unsafe { value.read() })
}

fn validate_sized_abi<T>(value: *const T) -> Result<(), c_int> {
    if value.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    let size = unsafe { value.cast::<usize>().read() };
    if size < std::mem::size_of::<T>() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    Ok(())
}

fn read_select_word_options(
    options: *const RoasttyTerminalSelectWordOptions,
) -> Result<(TerminalGridRef, Option<Vec<u32>>), c_int> {
    validate_sized_abi(options)?;
    let ref_ = read_grid_ref_ptr(unsafe { ptr::addr_of!((*options).ref_) })?;
    let boundary_codepoints = unsafe { ptr::addr_of!((*options).boundary_codepoints).read() };
    let boundary_codepoints_len =
        unsafe { ptr::addr_of!((*options).boundary_codepoints_len).read() };
    Ok((
        ref_,
        read_codepoints(boundary_codepoints, boundary_codepoints_len)?,
    ))
}

fn read_select_word_between_options(
    options: *const RoasttyTerminalSelectWordBetweenOptions,
) -> Result<(TerminalGridRef, TerminalGridRef, Option<Vec<u32>>), c_int> {
    validate_sized_abi(options)?;
    let start = read_grid_ref_ptr(unsafe { ptr::addr_of!((*options).start) })?;
    let end = read_grid_ref_ptr(unsafe { ptr::addr_of!((*options).end) })?;
    let boundary_codepoints = unsafe { ptr::addr_of!((*options).boundary_codepoints).read() };
    let boundary_codepoints_len =
        unsafe { ptr::addr_of!((*options).boundary_codepoints_len).read() };
    Ok((
        start,
        end,
        read_codepoints(boundary_codepoints, boundary_codepoints_len)?,
    ))
}

fn read_select_line_options(
    options: *const RoasttyTerminalSelectLineOptions,
) -> Result<(TerminalGridRef, Option<Vec<u32>>, bool), c_int> {
    validate_sized_abi(options)?;
    let ref_ = read_grid_ref_ptr(unsafe { ptr::addr_of!((*options).ref_) })?;
    let whitespace = unsafe { ptr::addr_of!((*options).whitespace).read() };
    let whitespace_len = unsafe { ptr::addr_of!((*options).whitespace_len).read() };
    let semantic_prompt_boundary =
        unsafe { ptr::addr_of!((*options).semantic_prompt_boundary).read() };
    Ok((
        ref_,
        read_codepoints(whitespace, whitespace_len)?,
        semantic_prompt_boundary,
    ))
}

fn read_codepoints(ptr: *const u32, len: usize) -> Result<Option<Vec<u32>>, c_int> {
    if len == 0 {
        return if ptr.is_null() {
            Ok(None)
        } else {
            Ok(Some(Vec::new()))
        };
    }
    if ptr.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }

    let values = unsafe { slice::from_raw_parts(ptr, len) };
    if values
        .iter()
        .any(|codepoint| char::from_u32(*codepoint).is_none())
    {
        return Err(ROASTTY_INVALID_VALUE);
    }
    Ok(Some(values.to_vec()))
}

fn selection_format_from_raw(value: c_int) -> Result<TerminalSelectionFormat, c_int> {
    TerminalSelectionFormat::from_raw(value).ok_or(ROASTTY_INVALID_VALUE)
}

fn formatter_format_from_raw(value: c_int) -> Result<TerminalSelectionFormat, c_int> {
    selection_format_from_raw(value)
}

fn sized_abi_size<T>(value: *const T) -> Result<usize, c_int> {
    if value.is_null() {
        return Err(ROASTTY_INVALID_VALUE);
    }
    let size = unsafe { value.cast::<usize>().read() };
    if size < std::mem::size_of::<usize>() {
        return Err(ROASTTY_INVALID_VALUE);
    }
    Ok(size)
}

const fn field_end<T, F>(field_offset: usize) -> usize {
    field_offset + std::mem::size_of::<F>()
}

fn formatter_field_present<T, F>(provided_size: usize, field_offset: usize) -> bool {
    provided_size >= field_end::<T, F>(field_offset)
}

fn formatter_nested_size_is_contained(
    parent_size: usize,
    field_offset: usize,
    nested_size: usize,
) -> bool {
    field_offset
        .checked_add(nested_size)
        .is_some_and(|end| parent_size >= end)
}

fn read_formatter_screen_extra(
    extra: *const RoasttyFormatterScreenExtra,
) -> Result<(bool, bool, bool, bool, bool, bool), c_int> {
    let size = sized_abi_size(extra)?;
    let mut cursor = false;
    let mut style = false;
    let mut hyperlink = false;
    let mut protection = false;
    let mut kitty_keyboard = false;
    let mut charsets = false;

    unsafe {
        if formatter_field_present::<RoasttyFormatterScreenExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterScreenExtra, cursor),
        ) {
            cursor = ptr::addr_of!((*extra).cursor).read();
        }
        if formatter_field_present::<RoasttyFormatterScreenExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterScreenExtra, style),
        ) {
            style = ptr::addr_of!((*extra).style).read();
        }
        if formatter_field_present::<RoasttyFormatterScreenExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterScreenExtra, hyperlink),
        ) {
            hyperlink = ptr::addr_of!((*extra).hyperlink).read();
        }
        if formatter_field_present::<RoasttyFormatterScreenExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterScreenExtra, protection),
        ) {
            protection = ptr::addr_of!((*extra).protection).read();
        }
        if formatter_field_present::<RoasttyFormatterScreenExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterScreenExtra, kitty_keyboard),
        ) {
            kitty_keyboard = ptr::addr_of!((*extra).kitty_keyboard).read();
        }
        if formatter_field_present::<RoasttyFormatterScreenExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterScreenExtra, charsets),
        ) {
            charsets = ptr::addr_of!((*extra).charsets).read();
        }
    }

    Ok((
        cursor,
        style,
        hyperlink,
        protection,
        kitty_keyboard,
        charsets,
    ))
}

fn read_formatter_terminal_extra(
    extra: *const RoasttyFormatterTerminalExtra,
) -> Result<TerminalFormatterExtra, c_int> {
    let size = sized_abi_size(extra)?;
    let mut result = TerminalFormatterExtra::none();

    unsafe {
        if formatter_field_present::<RoasttyFormatterTerminalExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, palette),
        ) {
            result = result.palette(ptr::addr_of!((*extra).palette).read());
        }
        if formatter_field_present::<RoasttyFormatterTerminalExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, modes),
        ) {
            result = result.modes(ptr::addr_of!((*extra).modes).read());
        }
        if formatter_field_present::<RoasttyFormatterTerminalExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, scrolling_region),
        ) {
            result = result.scrolling_region(ptr::addr_of!((*extra).scrolling_region).read());
        }
        if formatter_field_present::<RoasttyFormatterTerminalExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, tabstops),
        ) {
            result = result.tabstops(ptr::addr_of!((*extra).tabstops).read());
        }
        if formatter_field_present::<RoasttyFormatterTerminalExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, pwd),
        ) {
            result = result.pwd(ptr::addr_of!((*extra).pwd).read());
        }
        if formatter_field_present::<RoasttyFormatterTerminalExtra, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, keyboard),
        ) {
            result = result.keyboard(ptr::addr_of!((*extra).keyboard).read());
        }
        if formatter_field_present::<RoasttyFormatterTerminalExtra, usize>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, screen),
        ) {
            let screen_size = ptr::addr_of!((*extra).screen).cast::<usize>().read();
            if screen_size < std::mem::size_of::<usize>()
                || !formatter_nested_size_is_contained(
                    size,
                    std::mem::offset_of!(RoasttyFormatterTerminalExtra, screen),
                    screen_size,
                )
            {
                return Err(ROASTTY_INVALID_VALUE);
            }
            let (cursor, style, hyperlink, protection, kitty_keyboard, charsets) =
                read_formatter_screen_extra(ptr::addr_of!((*extra).screen))?;
            result = result.screen_extra(
                cursor,
                style,
                hyperlink,
                protection,
                kitty_keyboard,
                charsets,
            );
        }
    }

    Ok(result)
}

fn read_formatter_terminal_options(
    options: *const RoasttyFormatterTerminalOptions,
) -> Result<FormatterHandle, c_int> {
    let size = sized_abi_size(options)?;
    let mut emit = ROASTTY_FORMATTER_FORMAT_PLAIN;
    let mut unwrap = false;
    let mut trim = false;
    let mut extra = TerminalFormatterExtra::none();
    let mut selection_ptr = ptr::null();

    unsafe {
        if formatter_field_present::<RoasttyFormatterTerminalOptions, c_int>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, emit),
        ) {
            emit = ptr::addr_of!((*options).emit).read();
        }
        if formatter_field_present::<RoasttyFormatterTerminalOptions, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, unwrap),
        ) {
            unwrap = ptr::addr_of!((*options).unwrap).read();
        }
        if formatter_field_present::<RoasttyFormatterTerminalOptions, bool>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, trim),
        ) {
            trim = ptr::addr_of!((*options).trim).read();
        }
        if formatter_field_present::<RoasttyFormatterTerminalOptions, usize>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, extra),
        ) {
            let extra_size = ptr::addr_of!((*options).extra).cast::<usize>().read();
            if extra_size < std::mem::size_of::<usize>()
                || !formatter_nested_size_is_contained(
                    size,
                    std::mem::offset_of!(RoasttyFormatterTerminalOptions, extra),
                    extra_size,
                )
            {
                return Err(ROASTTY_INVALID_VALUE);
            }
            extra = read_formatter_terminal_extra(ptr::addr_of!((*options).extra))?;
        }
        if formatter_field_present::<RoasttyFormatterTerminalOptions, *const RoasttySelection>(
            size,
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, selection),
        ) {
            selection_ptr = ptr::addr_of!((*options).selection).read();
        }
    }

    let selection = if selection_ptr.is_null() {
        None
    } else {
        Some(read_selection(selection_ptr)?)
    };

    Ok(FormatterHandle {
        terminal: ptr::null_mut(),
        format: formatter_format_from_raw(emit)?,
        unwrap,
        trim,
        extra,
        selection,
    })
}

fn selection_order_from_raw(value: c_int) -> Result<TerminalSelectionOrder, c_int> {
    TerminalSelectionOrder::from_raw(value).ok_or(ROASTTY_INVALID_VALUE)
}

fn selection_adjustment_from_raw(value: c_int) -> Result<TerminalSelectionAdjustment, c_int> {
    TerminalSelectionAdjustment::from_raw(value).ok_or(ROASTTY_INVALID_VALUE)
}

fn grid_ref_error_result(error: TerminalGridRefPointError) -> c_int {
    match error {
        TerminalGridRefPointError::InvalidValue => ROASTTY_INVALID_VALUE,
        TerminalGridRefPointError::NoValue => ROASTTY_NO_VALUE,
    }
}

fn underline_to_int(value: sgr::Underline) -> c_int {
    match value {
        sgr::Underline::None => 0,
        sgr::Underline::Single => 1,
        sgr::Underline::Double => 2,
        sgr::Underline::Curly => 3,
        sgr::Underline::Dotted => 4,
        sgr::Underline::Dashed => 5,
    }
}

fn style_color_to_c(value: style::Color) -> RoasttyStyleColor {
    match value {
        style::Color::None => RoasttyStyleColor {
            tag: ROASTTY_STYLE_COLOR_NONE,
            value: RoasttyStyleColorValue { _padding: 0 },
        },
        style::Color::Palette(palette) => RoasttyStyleColor {
            tag: ROASTTY_STYLE_COLOR_PALETTE,
            value: RoasttyStyleColorValue { palette },
        },
        style::Color::Rgb(rgb) => RoasttyStyleColor {
            tag: ROASTTY_STYLE_COLOR_RGB,
            value: RoasttyStyleColorValue {
                rgb: RoasttyRgb {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                },
            },
        },
    }
}

fn style_to_c(value: style::Style) -> RoasttyStyle {
    RoasttyStyle {
        size: std::mem::size_of::<RoasttyStyle>(),
        fg_color: style_color_to_c(value.fg_color),
        bg_color: style_color_to_c(value.bg_color),
        underline_color: style_color_to_c(value.underline_color),
        bold: value.flags.bold,
        italic: value.flags.italic,
        faint: value.flags.faint,
        blink: value.flags.blink,
        inverse: value.flags.inverse,
        invisible: value.flags.invisible,
        strikethrough: value.flags.strikethrough,
        overline: value.flags.overline,
        underline: underline_to_int(value.flags.underline),
    }
}

fn style_color_is_none(value: RoasttyStyleColor) -> bool {
    value.tag == ROASTTY_STYLE_COLOR_NONE
}

fn style_is_default(value: RoasttyStyle) -> bool {
    value.size == std::mem::size_of::<RoasttyStyle>()
        && style_color_is_none(value.fg_color)
        && style_color_is_none(value.bg_color)
        && style_color_is_none(value.underline_color)
        && !value.bold
        && !value.italic
        && !value.faint
        && !value.blink
        && !value.inverse
        && !value.invisible
        && !value.strikethrough
        && !value.overline
        && value.underline == 0
}

fn packed_bits(value: u64, shift: u32, mask: u64) -> u64 {
    (value >> shift) & mask
}

fn packed_bit(value: u64, shift: u32) -> bool {
    packed_bits(value, shift, 1) == 1
}

fn cell_content_tag(cell: RoasttyCell) -> c_int {
    packed_bits(cell, 0, 0b11) as c_int
}

fn cell_codepoint(cell: RoasttyCell) -> u32 {
    match cell_content_tag(cell) {
        ROASTTY_CELL_CONTENT_CODEPOINT | ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME => {
            packed_bits(cell, 2, 0x001f_ffff) as u32
        }
        ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE | ROASTTY_CELL_CONTENT_BG_COLOR_RGB => 0,
        _ => 0,
    }
}

fn cell_has_text(cell: RoasttyCell) -> bool {
    matches!(
        cell_content_tag(cell),
        ROASTTY_CELL_CONTENT_CODEPOINT | ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME
    ) && cell_codepoint(cell) != 0
}

fn cell_rgb(cell: RoasttyCell) -> RoasttyRgb {
    let bits = packed_bits(cell, 2, 0x00ff_ffff);
    RoasttyRgb {
        r: ((bits >> 16) & 0xff) as u8,
        g: ((bits >> 8) & 0xff) as u8,
        b: (bits & 0xff) as u8,
    }
}

fn cell_palette_index(cell: RoasttyCell) -> u8 {
    packed_bits(cell, 2, 0xff) as u8
}

fn row_semantic_prompt(row: RoasttyRow) -> c_int {
    packed_bits(row, 37, 0b11) as c_int
}

fn resolve_style_color(color: style::Color, palette: RoasttyPalette) -> Option<RoasttyRgb> {
    match color {
        style::Color::None => None,
        style::Color::Palette(index) => Some(palette[index as usize]),
        style::Color::Rgb(rgb) => Some(RoasttyRgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }),
    }
}

fn write_graphemes_utf8(cell: &RenderStateCellSnapshot, out: *mut RoasttyBuffer) -> c_int {
    unsafe {
        ptr::addr_of_mut!((*out).len).write(0);
    }

    if !cell_has_text(cell.raw) {
        return ROASTTY_SUCCESS;
    }

    let mut encoded = String::new();
    let Some(base) = char::from_u32(cell_codepoint(cell.raw)) else {
        return ROASTTY_INVALID_VALUE;
    };
    encoded.push(base);
    for codepoint in &cell.graphemes {
        let Some(ch) = char::from_u32(*codepoint) else {
            return ROASTTY_INVALID_VALUE;
        };
        encoded.push(ch);
    }

    let bytes = encoded.as_bytes();
    unsafe {
        ptr::addr_of_mut!((*out).len).write(bytes.len());
        let dst = ptr::addr_of!((*out).ptr).read();
        let cap = ptr::addr_of!((*out).cap).read();
        if dst.is_null() || cap < bytes.len() {
            return ROASTTY_OUT_OF_SPACE;
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
    }
    ROASTTY_SUCCESS
}

unsafe fn cell_get_write(cell: RoasttyCell, data: c_int, out: *mut c_void) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match data {
        ROASTTY_CELL_DATA_INVALID => ROASTTY_INVALID_VALUE,
        ROASTTY_CELL_DATA_CODEPOINT => {
            out.cast::<u32>().write(cell_codepoint(cell));
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_CONTENT_TAG => {
            out.cast::<c_int>().write(cell_content_tag(cell));
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_WIDE => {
            out.cast::<c_int>()
                .write(packed_bits(cell, 42, 0b11) as c_int);
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_HAS_TEXT => {
            out.cast::<bool>().write(cell_has_text(cell));
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_HAS_STYLING => {
            out.cast::<bool>()
                .write(packed_bits(cell, 26, u16::MAX as u64) != 0);
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_STYLE_ID => {
            out.cast::<u16>()
                .write(packed_bits(cell, 26, u16::MAX as u64) as u16);
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_HAS_HYPERLINK => {
            out.cast::<bool>().write(packed_bit(cell, 45));
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_PROTECTED => {
            out.cast::<bool>().write(packed_bit(cell, 44));
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_SEMANTIC => {
            out.cast::<c_int>()
                .write(packed_bits(cell, 46, 0b11) as c_int);
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_COLOR_PALETTE => {
            out.cast::<u8>().write(packed_bits(cell, 2, 0xff) as u8);
            ROASTTY_SUCCESS
        }
        ROASTTY_CELL_DATA_COLOR_RGB => {
            out.cast::<RoasttyRgb>().write(cell_rgb(cell));
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

unsafe fn row_get_write(row: RoasttyRow, data: c_int, out: *mut c_void) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match data {
        ROASTTY_ROW_DATA_INVALID => ROASTTY_INVALID_VALUE,
        ROASTTY_ROW_DATA_WRAP => {
            out.cast::<bool>().write(packed_bit(row, 32));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_WRAP_CONTINUATION => {
            out.cast::<bool>().write(packed_bit(row, 33));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_GRAPHEME => {
            out.cast::<bool>().write(packed_bit(row, 34));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_STYLED => {
            out.cast::<bool>().write(packed_bit(row, 35));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_HYPERLINK => {
            out.cast::<bool>().write(packed_bit(row, 36));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_SEMANTIC_PROMPT => {
            out.cast::<c_int>().write(row_semantic_prompt(row));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER => {
            out.cast::<bool>().write(packed_bit(row, 39));
            ROASTTY_SUCCESS
        }
        ROASTTY_ROW_DATA_DIRTY => {
            out.cast::<bool>().write(packed_bit(row, 40));
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

fn render_state_row_iterator_selected_mut(
    iterator: RoasttyRenderStateRowIterator,
) -> Result<&'static mut RenderStateRowSnapshot, c_int> {
    let Some(iterator) = render_state_row_iterator_mut_from_handle(iterator) else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    if !iterator.bound {
        return Err(ROASTTY_INVALID_VALUE);
    }
    let Some(index) = iterator.selected else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    iterator.rows.get_mut(index).ok_or(ROASTTY_INVALID_VALUE)
}

fn render_state_row_iterator_selected_mut_with_palette(
    iterator: RoasttyRenderStateRowIterator,
) -> Result<(&'static mut RenderStateRowSnapshot, RoasttyPalette), c_int> {
    let Some(iterator) = render_state_row_iterator_mut_from_handle(iterator) else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    if !iterator.bound {
        return Err(ROASTTY_INVALID_VALUE);
    }
    let Some(index) = iterator.selected else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    let palette = iterator.palette;
    let row = iterator.rows.get_mut(index).ok_or(ROASTTY_INVALID_VALUE)?;
    Ok((row, palette))
}

unsafe fn render_state_row_get_write(
    row: &RenderStateRowSnapshot,
    palette: RoasttyPalette,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match data {
        ROASTTY_RENDER_STATE_ROW_DATA_INVALID => ROASTTY_INVALID_VALUE,
        ROASTTY_RENDER_STATE_ROW_DATA_DIRTY => {
            out.cast::<bool>().write(row.dirty);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_DATA_RAW => {
            out.cast::<RoasttyRow>().write(row.raw);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_DATA_CELLS => {
            let cells_handle = out.cast::<RoasttyRenderStateRowCells>().read();
            let Some(cells) = render_state_row_cells_mut_from_handle(cells_handle) else {
                return ROASTTY_INVALID_VALUE;
            };
            cells.cells.clone_from(&row.cells);
            cells.palette = palette;
            cells.selected = None;
            cells.selection = row.selection;
            cells.bound = true;
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_DATA_SELECTION => {
            let out = out.cast::<RoasttyRenderStateRowSelection>();
            if ptr::addr_of!((*out).size).read()
                < std::mem::size_of::<RoasttyRenderStateRowSelection>()
            {
                return ROASTTY_INVALID_VALUE;
            }
            let Some(selection) = row.selection else {
                return ROASTTY_NO_VALUE;
            };
            ptr::addr_of_mut!((*out).start_x).write(selection.start_x);
            ptr::addr_of_mut!((*out).end_x).write(selection.end_x);
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

unsafe fn render_state_row_cells_get_write(
    cells: RoasttyRenderStateRowCells,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if !valid_render_state_row_cells_data(data)
        || data == ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID
    {
        return ROASTTY_INVALID_VALUE;
    }

    let Some(cells) = render_state_row_cells_mut_from_handle(cells) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !cells.bound {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(index) = cells.selected else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(cell) = cells.cells.get(index) else {
        return ROASTTY_INVALID_VALUE;
    };

    match data {
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW => {
            out.cast::<RoasttyCell>().write(cell.raw);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE => {
            let out = out.cast::<RoasttyStyle>();
            if ptr::addr_of!((*out).size).read() < std::mem::size_of::<RoasttyStyle>() {
                return ROASTTY_INVALID_VALUE;
            }
            out.write(style_to_c(cell.style.unwrap_or_default()));
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN => {
            let len = if cell_has_text(cell.raw) {
                1 + cell.graphemes.len() as u32
            } else {
                0
            };
            out.cast::<u32>().write(len);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF => {
            if cell_has_text(cell.raw) {
                let buf = out.cast::<u32>();
                buf.write(cell_codepoint(cell.raw));
                for (offset, codepoint) in cell.graphemes.iter().enumerate() {
                    buf.add(offset + 1).write(*codepoint);
                }
            }
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR => {
            let rgb = match cell_content_tag(cell.raw) {
                ROASTTY_CELL_CONTENT_BG_COLOR_RGB => Some(cell_rgb(cell.raw)),
                ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE => {
                    Some(cells.palette[cell_palette_index(cell.raw) as usize])
                }
                _ => cell
                    .style
                    .and_then(|style| resolve_style_color(style.bg_color, cells.palette)),
            };
            let Some(rgb) = rgb else {
                return ROASTTY_INVALID_VALUE;
            };
            out.cast::<RoasttyRgb>().write(rgb);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR => {
            let Some(rgb) = cell
                .style
                .and_then(|style| resolve_style_color(style.fg_color, cells.palette))
            else {
                return ROASTTY_INVALID_VALUE;
            };
            out.cast::<RoasttyRgb>().write(rgb);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED => {
            let selected = cells
                .selection
                .map(|selection| {
                    index >= selection.start_x as usize && index <= selection.end_x as usize
                })
                .unwrap_or(false);
            out.cast::<bool>().write(selected);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING => {
            out.cast::<bool>()
                .write(packed_bits(cell.raw, 26, u16::MAX as u64) != 0);
            ROASTTY_SUCCESS
        }
        ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8 => {
            write_graphemes_utf8(cell, out.cast::<RoasttyBuffer>())
        }
        _ => ROASTTY_INVALID_VALUE,
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
pub extern "C" fn roastty_cell_get(cell: RoasttyCell, data: c_int, out: *mut c_void) -> c_int {
    unsafe { cell_get_write(cell, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_cell_get_multi(
    cell: RoasttyCell,
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
    if keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if count == 0 {
        return ROASTTY_SUCCESS;
    }

    for index in 0..count {
        let key = unsafe { *keys.add(index) };
        let value = unsafe { *values.add(index) };
        let result = roastty_cell_get(cell, key, value);
        if result != ROASTTY_SUCCESS {
            if !out_written.is_null() {
                unsafe {
                    out_written.write(index);
                }
            }
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
pub extern "C" fn roastty_row_get(row: RoasttyRow, data: c_int, out: *mut c_void) -> c_int {
    unsafe { row_get_write(row, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_row_get_multi(
    row: RoasttyRow,
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
    if keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if count == 0 {
        return ROASTTY_SUCCESS;
    }

    for index in 0..count {
        let key = unsafe { *keys.add(index) };
        let value = unsafe { *values.add(index) };
        let result = roastty_row_get(row, key, value);
        if result != ROASTTY_SUCCESS {
            if !out_written.is_null() {
                unsafe {
                    out_written.write(index);
                }
            }
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
pub extern "C" fn roastty_style_default(out: *mut RoasttyStyle) {
    if out.is_null() {
        return;
    }

    unsafe {
        out.write(style_to_c(style::Style::default()));
    }
}

#[no_mangle]
pub extern "C" fn roastty_style_is_default(value: *const RoasttyStyle) -> bool {
    if value.is_null() {
        return false;
    }

    unsafe {
        if ptr::addr_of!((*value).size).read() != std::mem::size_of::<RoasttyStyle>() {
            return false;
        }

        style_is_default(value.read())
    }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_new(out: *mut RoasttyRenderStateHandle) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let state = Box::new(render_state_default());
    unsafe {
        out.write(Box::into_raw(state).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_free(state: RoasttyRenderStateHandle) {
    if state.is_null() {
        return;
    }

    unsafe {
        drop(Box::from_raw(state.cast::<RenderStateScalar>()));
    }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_iterator_new(
    out: *mut RoasttyRenderStateRowIterator,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let iterator = Box::new(RenderStateRowIterator::default());
    unsafe {
        out.write(Box::into_raw(iterator).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_iterator_free(iterator: RoasttyRenderStateRowIterator) {
    if iterator.is_null() {
        return;
    }

    unsafe {
        drop(Box::from_raw(iterator.cast::<RenderStateRowIterator>()));
    }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_iterator_next(
    iterator: RoasttyRenderStateRowIterator,
) -> bool {
    let Some(iterator) = render_state_row_iterator_mut_from_handle(iterator) else {
        return false;
    };
    if !iterator.bound {
        return false;
    }

    let next = iterator.selected.map_or(0, |index| index + 1);
    if next >= iterator.rows.len() {
        return false;
    }
    iterator.selected = Some(next);
    true
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_cells_new(
    out: *mut RoasttyRenderStateRowCells,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let cells = Box::new(RenderStateRowCells::default());
    unsafe {
        out.write(Box::into_raw(cells).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_cells_free(cells: RoasttyRenderStateRowCells) {
    if cells.is_null() {
        return;
    }

    unsafe {
        drop(Box::from_raw(cells.cast::<RenderStateRowCells>()));
    }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_cells_next(cells: RoasttyRenderStateRowCells) -> bool {
    let Some(cells) = render_state_row_cells_mut_from_handle(cells) else {
        return false;
    };
    if !cells.bound {
        return false;
    }

    let next = cells.selected.map_or(0, |index| index + 1);
    if next >= cells.cells.len() {
        return false;
    }
    cells.selected = Some(next);
    true
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_cells_select(
    cells: RoasttyRenderStateRowCells,
    x: u16,
) -> c_int {
    let Some(cells) = render_state_row_cells_mut_from_handle(cells) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !cells.bound || x as usize >= cells.cells.len() {
        return ROASTTY_INVALID_VALUE;
    }
    cells.selected = Some(x as usize);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_update(
    state: RoasttyRenderStateHandle,
    terminal: RoasttyTerminal,
) -> c_int {
    let Some(state) = render_state_mut_from_handle(state) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };

    *state = render_state_from_terminal(&terminal.terminal);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_get(
    state: RoasttyRenderStateHandle,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    if !valid_render_state_data(data) || data == ROASTTY_RENDER_STATE_DATA_INVALID {
        return ROASTTY_INVALID_VALUE;
    }
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(state) = render_state_from_handle(state) else {
        return ROASTTY_INVALID_VALUE;
    };

    unsafe { render_state_get_write(state, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_get_multi(
    state: RoasttyRenderStateHandle,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> c_int {
    if keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    for index in 0..count {
        let key = unsafe { keys.add(index).read() };
        let value = unsafe { values.add(index).read() };
        let result = roastty_render_state_get(state, key, value);
        if result != ROASTTY_SUCCESS {
            if !out_written.is_null() {
                unsafe {
                    out_written.write(index);
                }
            }
            return result;
        }
    }

    if !out_written.is_null() {
        unsafe {
            out_written.write(count);
        }
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_set(
    state: RoasttyRenderStateHandle,
    option: c_int,
    value: *const c_void,
) -> c_int {
    let Some(state) = render_state_mut_from_handle(state) else {
        return ROASTTY_INVALID_VALUE;
    };
    if value.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match option {
        ROASTTY_RENDER_STATE_OPTION_DIRTY => {
            let raw = unsafe { value.cast::<c_int>().read() };
            let Some(dirty) = render_dirty_from_raw(raw) else {
                return ROASTTY_INVALID_VALUE;
            };
            state.dirty = dirty;
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_get(
    iterator: RoasttyRenderStateRowIterator,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    if !valid_render_state_row_data(data) || data == ROASTTY_RENDER_STATE_ROW_DATA_INVALID {
        return ROASTTY_INVALID_VALUE;
    }
    let Ok((row, palette)) = render_state_row_iterator_selected_mut_with_palette(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };

    unsafe { render_state_row_get_write(row, palette, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_get_multi(
    iterator: RoasttyRenderStateRowIterator,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> c_int {
    if count == 0 {
        if !out_written.is_null() {
            unsafe {
                out_written.write(0);
            }
        }
        return ROASTTY_SUCCESS;
    }
    if keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    for index in 0..count {
        let key = unsafe { keys.add(index).read() };
        let value = unsafe { values.add(index).read() };
        let result = roastty_render_state_row_get(iterator, key, value);
        if result != ROASTTY_SUCCESS {
            if !out_written.is_null() {
                unsafe {
                    out_written.write(index);
                }
            }
            return result;
        }
    }

    if !out_written.is_null() {
        unsafe {
            out_written.write(count);
        }
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_set(
    iterator: RoasttyRenderStateRowIterator,
    option: c_int,
    value: *const c_void,
) -> c_int {
    if !valid_render_state_row_option(option) {
        return ROASTTY_INVALID_VALUE;
    }
    if value.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Ok(row) = render_state_row_iterator_selected_mut(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };

    match option {
        ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY => {
            row.dirty = unsafe { value.cast::<bool>().read() };
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_cells_get(
    cells: RoasttyRenderStateRowCells,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    if !valid_render_state_row_cells_data(data)
        || data == ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID
    {
        return ROASTTY_INVALID_VALUE;
    }

    unsafe { render_state_row_cells_get_write(cells, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_render_state_row_cells_get_multi(
    cells: RoasttyRenderStateRowCells,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> c_int {
    if count == 0 {
        if !out_written.is_null() {
            unsafe {
                out_written.write(0);
            }
        }
        return ROASTTY_SUCCESS;
    }
    if keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    for index in 0..count {
        let key = unsafe { keys.add(index).read() };
        let value = unsafe { values.add(index).read() };
        let result = roastty_render_state_row_cells_get(cells, key, value);
        if result != ROASTTY_SUCCESS {
            if !out_written.is_null() {
                unsafe {
                    out_written.write(index);
                }
            }
            return result;
        }
    }

    if !out_written.is_null() {
        unsafe {
            out_written.write(count);
        }
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_render_state_colors_get(
    state: RoasttyRenderStateHandle,
    out: *mut RoasttyRenderStateColors,
) -> c_int {
    let Some(state) = render_state_from_handle(state) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let out_size = unsafe { ptr::addr_of!((*out).size).read() };
    if out_size < std::mem::size_of::<usize>() {
        return ROASTTY_INVALID_VALUE;
    }

    unsafe {
        if out_size
            >= std::mem::offset_of!(RoasttyRenderStateColors, background)
                + std::mem::size_of::<RoasttyRgb>()
        {
            ptr::addr_of_mut!((*out).background).write(state.background);
        }
        if out_size
            >= std::mem::offset_of!(RoasttyRenderStateColors, foreground)
                + std::mem::size_of::<RoasttyRgb>()
        {
            ptr::addr_of_mut!((*out).foreground).write(state.foreground);
        }
        if let Some(cursor) = state.cursor {
            if out_size
                >= std::mem::offset_of!(RoasttyRenderStateColors, cursor)
                    + std::mem::size_of::<RoasttyRgb>()
            {
                ptr::addr_of_mut!((*out).cursor).write(cursor);
            }
        }
        if out_size
            >= std::mem::offset_of!(RoasttyRenderStateColors, cursor_has_value)
                + std::mem::size_of::<bool>()
        {
            ptr::addr_of_mut!((*out).cursor_has_value).write(state.cursor.is_some());
        }

        let palette_offset = std::mem::offset_of!(RoasttyRenderStateColors, palette);
        if out_size > palette_offset {
            let available = out_size - palette_offset;
            let entries = std::cmp::min(
                state.palette.len(),
                available / std::mem::size_of::<RoasttyRgb>(),
            );
            for index in 0..entries {
                ptr::addr_of_mut!((*out).palette)
                    .cast::<RoasttyRgb>()
                    .add(index)
                    .write(state.palette[index]);
            }
        }
    }

    ROASTTY_SUCCESS
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

fn version_component(index: usize) -> usize {
    let version = std::str::from_utf8(&VERSION[..VERSION.len() - 1]).unwrap_or("");
    let core = version.split(['-', '+']).next().unwrap_or("");
    core.split('.')
        .nth(index)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

fn borrowed_string(bytes: &'static [u8]) -> RoasttyString {
    RoasttyString {
        ptr: bytes.as_ptr().cast::<c_char>(),
        len: bytes.len().saturating_sub(1),
        sentinel: false,
    }
}

fn current_optimize_mode() -> c_int {
    if cfg!(debug_assertions) {
        ROASTTY_OPTIMIZE_DEBUG
    } else {
        ROASTTY_OPTIMIZE_RELEASE_FAST
    }
}

#[no_mangle]
pub extern "C" fn roastty_build_info(data: c_int, out: *mut c_void) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    unsafe {
        match data {
            ROASTTY_BUILD_INFO_INVALID => ROASTTY_INVALID_VALUE,
            ROASTTY_BUILD_INFO_SIMD => {
                out.cast::<bool>().write(false);
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_KITTY_GRAPHICS => {
                out.cast::<bool>().write(false);
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE => {
                out.cast::<bool>().write(false);
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_OPTIMIZE => {
                out.cast::<c_int>().write(current_optimize_mode());
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_VERSION_STRING => {
                out.cast::<RoasttyString>().write(borrowed_string(VERSION));
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_VERSION_MAJOR => {
                out.cast::<usize>().write(version_component(0));
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_VERSION_MINOR => {
                out.cast::<usize>().write(version_component(1));
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_VERSION_PATCH => {
                out.cast::<usize>().write(version_component(2));
                ROASTTY_SUCCESS
            }
            ROASTTY_BUILD_INFO_VERSION_PRE | ROASTTY_BUILD_INFO_VERSION_BUILD => {
                out.cast::<RoasttyString>()
                    .write(borrowed_string(EMPTY_STRING));
                ROASTTY_SUCCESS
            }
            _ => ROASTTY_INVALID_VALUE,
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_alloc(allocator: *const RoasttyAllocator, len: usize) -> *mut u8 {
    if len == 0 {
        return ptr::null_mut();
    }

    if allocator.is_null() {
        let Ok(layout) = Layout::array::<u8>(len) else {
            return ptr::null_mut();
        };
        return unsafe { alloc::alloc(layout) };
    }

    let allocator = unsafe { &*allocator };
    if allocator.vtable.is_null() {
        return ptr::null_mut();
    }
    let vtable = unsafe { &*allocator.vtable };
    let Some(alloc_fn) = vtable.alloc else {
        return ptr::null_mut();
    };
    unsafe { alloc_fn(allocator.ctx, len, 1, 0) }
}

#[no_mangle]
pub extern "C" fn roastty_free(allocator: *const RoasttyAllocator, ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    if allocator.is_null() {
        let Ok(layout) = Layout::array::<u8>(len) else {
            return;
        };
        unsafe {
            alloc::dealloc(ptr, layout);
        }
        return;
    }

    let allocator = unsafe { &*allocator };
    if allocator.vtable.is_null() {
        return;
    }
    let vtable = unsafe { &*allocator.vtable };
    let Some(free_fn) = vtable.free else {
        return;
    };
    unsafe {
        free_fn(allocator.ctx, ptr.cast::<c_void>(), len, 1, 0);
    }
}

#[no_mangle]
pub extern "C" fn roastty_sys_set(option: c_int, value: *const c_void) -> c_int {
    let Ok(mut state) = SYS_STATE.lock() else {
        return ROASTTY_INVALID_VALUE;
    };
    match option {
        ROASTTY_SYS_OPT_USERDATA => {
            state.userdata = value as usize;
            ROASTTY_SUCCESS
        }
        ROASTTY_SYS_OPT_DECODE_PNG => {
            state.decode_png = if value.is_null() {
                None
            } else {
                Some(unsafe { std::mem::transmute::<*const c_void, SysDecodePngCallback>(value) })
            };
            ROASTTY_SUCCESS
        }
        ROASTTY_SYS_OPT_LOG => {
            state.log = if value.is_null() {
                None
            } else {
                Some(unsafe { std::mem::transmute::<*const c_void, SysLogCallback>(value) })
            };
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

fn log_level_name(level: c_int) -> &'static str {
    match level {
        ROASTTY_SYS_LOG_LEVEL_ERROR => "error",
        ROASTTY_SYS_LOG_LEVEL_WARNING => "warning",
        ROASTTY_SYS_LOG_LEVEL_INFO => "info",
        ROASTTY_SYS_LOG_LEVEL_DEBUG => "debug",
        _ => "unknown",
    }
}

unsafe fn nullable_bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        slice::from_raw_parts(ptr, len)
    }
}

#[no_mangle]
pub extern "C" fn roastty_sys_log_stderr(
    _userdata: *mut c_void,
    level: c_int,
    scope: *const u8,
    scope_len: usize,
    message: *const u8,
    message_len: usize,
) {
    let scope = unsafe { nullable_bytes(scope, scope_len) };
    let message = unsafe { nullable_bytes(message, message_len) };
    let level = log_level_name(level);
    let mut stderr = std::io::stderr().lock();
    if scope.is_empty() {
        let _ = writeln!(stderr, "[{level}]: {}", String::from_utf8_lossy(message));
    } else {
        let _ = writeln!(
            stderr,
            "[{level}]({}): {}",
            String::from_utf8_lossy(scope),
            String::from_utf8_lossy(message)
        );
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
pub extern "C" fn roastty_size_report_encode(
    style: c_int,
    size: RoasttySizeReportSize,
    buf: *mut c_char,
    buf_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let Some(style) = size_report::style_from_int(style) else {
        unsafe {
            out_written.write(0);
        }
        return ROASTTY_INVALID_VALUE;
    };

    let encoded = size_report::encode(
        style,
        size_report::Size {
            rows: size.rows,
            columns: size.columns,
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        },
    );

    unsafe {
        out_written.write(encoded.len());
    }

    if buf.is_null() || buf_len < encoded.len() {
        return ROASTTY_OUT_OF_SPACE;
    }

    unsafe {
        ptr::copy_nonoverlapping(encoded.as_ptr(), buf.cast::<u8>(), encoded.len());
    }
    ROASTTY_SUCCESS
}

fn write_byte_segments(
    segments: &[&[u8]],
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    let required = segments.iter().map(|segment| segment.len()).sum::<usize>();
    unsafe {
        out_written.write(required);
    }

    if (out.is_null() && required > 0) || out_len < required {
        return ROASTTY_OUT_OF_SPACE;
    }

    let mut offset = 0;
    for segment in segments {
        if !segment.is_empty() {
            unsafe {
                ptr::copy_nonoverlapping(segment.as_ptr(), out.add(offset), segment.len());
            }
        }
        offset += segment.len();
    }

    ROASTTY_SUCCESS
}

fn focus_event_from_raw(event: c_int) -> Option<focus::FocusEvent> {
    match event {
        ROASTTY_FOCUS_EVENT_GAINED => Some(focus::FocusEvent::Gained),
        ROASTTY_FOCUS_EVENT_LOST => Some(focus::FocusEvent::Lost),
        _ => None,
    }
}

#[no_mangle]
pub extern "C" fn roastty_focus_encode(
    event: c_int,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(event) = focus_event_from_raw(event) else {
        unsafe {
            out_written.write(0);
        }
        return ROASTTY_INVALID_VALUE;
    };
    write_byte_segments(&[focus::encode(event)], out, out_len, out_written)
}

#[no_mangle]
pub extern "C" fn roastty_paste_is_safe(data: *const u8, len: usize) -> bool {
    if data.is_null() {
        return true;
    }
    paste::is_safe(unsafe { slice::from_raw_parts(data, len) })
}

#[no_mangle]
pub extern "C" fn roastty_paste_encode(
    data: *mut u8,
    data_len: usize,
    bracketed: bool,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let data = if data.is_null() {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(data, data_len) }
    };
    let segments = paste::encode(data, paste::PasteOptions { bracketed });
    write_byte_segments(&segments, out, out_len, out_written)
}

fn mode_report_state_from_raw(state: c_int) -> Option<modes::ReportState> {
    match state {
        ROASTTY_MODE_REPORT_NOT_RECOGNIZED => Some(modes::ReportState::NotRecognized),
        ROASTTY_MODE_REPORT_SET => Some(modes::ReportState::Set),
        ROASTTY_MODE_REPORT_RESET => Some(modes::ReportState::Reset),
        ROASTTY_MODE_REPORT_PERMANENTLY_SET => Some(modes::ReportState::PermanentlySet),
        ROASTTY_MODE_REPORT_PERMANENTLY_RESET => Some(modes::ReportState::PermanentlyReset),
        _ => None,
    }
}

#[no_mangle]
pub extern "C" fn roastty_mode_report_encode(
    tag: u16,
    state: c_int,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(state) = mode_report_state_from_raw(state) else {
        unsafe {
            out_written.write(0);
        }
        return ROASTTY_INVALID_VALUE;
    };
    let (value, ansi) = mode_tag_parts(tag);
    let output = modes::Report::new(modes::ModeTag::new(value, ansi), state).encode_vt();
    write_byte_segments(&[output.as_bytes()], out, out_len, out_written)
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

    let mut terminal = Box::new(Terminal {
        terminal,
        tracked_grid_refs: Vec::new(),
    });
    let handle = (&mut *terminal) as *mut Terminal as RoasttyTerminal;
    terminal.terminal.set_effect_handle(handle);
    unsafe {
        out.write(Box::into_raw(terminal).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_free(terminal: RoasttyTerminal) {
    if !terminal.is_null() {
        unsafe {
            let mut terminal = Box::from_raw(terminal.cast::<Terminal>());
            terminal.detach_tracked_grid_refs();
            drop(terminal);
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
        ROASTTY_TERMINAL_OPTION_USERDATA => {
            terminal.terminal.set_effect_userdata(value.cast_mut());
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_WRITE_PTY => {
            terminal
                .terminal
                .set_write_pty_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalWritePtyCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_BELL => {
            terminal
                .terminal
                .set_bell_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalBellCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_ENQUIRY => {
            terminal
                .terminal
                .set_enquiry_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalEnquiryCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_XTVERSION => {
            terminal
                .terminal
                .set_xtversion_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalXtversionCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_TITLE_CHANGED => {
            terminal
                .terminal
                .set_title_changed_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalTitleChangedCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_SIZE_CB => {
            terminal
                .terminal
                .set_size_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalSizeCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_COLOR_SCHEME => {
            terminal
                .terminal
                .set_color_scheme_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalColorSchemeCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
        ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES => {
            terminal
                .terminal
                .set_device_attributes_callback((!value.is_null()).then(|| unsafe {
                    std::mem::transmute::<*const c_void, TerminalDeviceAttributesCallback>(value)
                }));
            ROASTTY_SUCCESS
        }
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
        ROASTTY_TERMINAL_OPTION_SELECTION => {
            let selection = if value.is_null() {
                None
            } else {
                match read_selection(value.cast::<RoasttySelection>()) {
                    Ok(selection) => Some(selection),
                    Err(error) => return error,
                }
            };
            match terminal.terminal.set_selection(selection) {
                Ok(()) => ROASTTY_SUCCESS,
                Err(error) => grid_ref_error_result(error),
            }
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
pub extern "C" fn roastty_kitty_graphics_get(
    graphics: RoasttyKittyGraphics,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    let Some(storage) = kitty_graphics_from_handle(graphics) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !valid_kitty_graphics_data(data) || out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match data {
        ROASTTY_KITTY_GRAPHICS_DATA_INVALID => ROASTTY_INVALID_VALUE,
        ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR => {
            let iterator_handle =
                unsafe { out.cast::<RoasttyKittyGraphicsPlacementIterator>().read() };
            let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator_handle)
            else {
                return ROASTTY_INVALID_VALUE;
            };
            iterator.entries = storage
                .placement_snapshots()
                .into_iter()
                .map(|(key, placement)| KittyGraphicsPlacementSnapshot { key, placement })
                .collect();
            iterator.selected = None;
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_image(
    graphics: RoasttyKittyGraphics,
    image_id: u32,
) -> RoasttyKittyGraphicsImage {
    let Some(storage) = kitty_graphics_from_handle(graphics) else {
        return ptr::null_mut();
    };
    let Some(image) = storage.image_by_id(image_id) else {
        return ptr::null_mut();
    };
    let Ok(snapshot) = kitty_image_snapshot(image) else {
        return ptr::null_mut();
    };
    Box::into_raw(Box::new(snapshot)).cast()
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_image_free(image: RoasttyKittyGraphicsImage) {
    if image.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(image.cast::<KittyGraphicsImageSnapshot>()));
    }
}

unsafe fn kitty_graphics_image_get_write(
    image: &KittyGraphicsImageSnapshot,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    match data {
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID => out.cast::<u32>().write(image.id),
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER => out.cast::<u32>().write(image.number),
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH => out.cast::<u32>().write(image.width),
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT => out.cast::<u32>().write(image.height),
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT => out.cast::<c_int>().write(image.format),
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION => {
            out.cast::<c_int>().write(image.compression)
        }
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR => {
            out.cast::<*const u8>().write(image.data.as_ptr())
        }
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN => out.cast::<usize>().write(image.data.len()),
        ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID => return ROASTTY_INVALID_VALUE,
        _ => return ROASTTY_INVALID_VALUE,
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_image_get(
    image: RoasttyKittyGraphicsImage,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !valid_kitty_graphics_image_data(data) || out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    unsafe { kitty_graphics_image_get_write(image, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_image_get_multi(
    image: RoasttyKittyGraphicsImage,
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
    if kitty_graphics_image_from_handle(image).is_none() || keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if count == 0 {
        return ROASTTY_SUCCESS;
    }

    for index in 0..count {
        let key = unsafe { *keys.add(index) };
        if !valid_kitty_graphics_image_data(key) {
            return ROASTTY_INVALID_VALUE;
        }
        let value = unsafe { *values.add(index) };
        if value.is_null() {
            return ROASTTY_INVALID_VALUE;
        }
        let result = roastty_kitty_graphics_image_get(image, key, value);
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
pub extern "C" fn roastty_kitty_graphics_placement_iterator_new(
    out: *mut RoasttyKittyGraphicsPlacementIterator,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    unsafe {
        out.write(Box::into_raw(Box::new(KittyGraphicsPlacementIterator::default())).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_iterator_free(
    iterator: RoasttyKittyGraphicsPlacementIterator,
) {
    if iterator.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(
            iterator.cast::<KittyGraphicsPlacementIterator>(),
        ));
    }
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_iterator_set(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    option: c_int,
    value: *const c_void,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !valid_kitty_graphics_placement_iterator_option(option) || value.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match option {
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER => {
            let layer = unsafe { value.cast::<c_int>().read() };
            if !valid_kitty_graphics_placement_layer(layer) {
                return ROASTTY_INVALID_VALUE;
            }
            iterator.layer_filter = layer;
            iterator.selected = None;
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_next(
    iterator: RoasttyKittyGraphicsPlacementIterator,
) -> bool {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return false;
    };
    let start = iterator.selected.map_or(0, |index| index + 1);
    for index in start..iterator.entries.len() {
        if kitty_placement_layer_matches(iterator.layer_filter, iterator.entries[index].placement.z)
        {
            iterator.selected = Some(index);
            return true;
        }
    }
    iterator.selected = None;
    false
}

unsafe fn kitty_graphics_placement_get_write(
    item: &KittyGraphicsPlacementSnapshot,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    match data {
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID => {
            out.cast::<u32>().write(item.key.image_id)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID => out
            .cast::<u32>()
            .write(kitty_placement_public_id(item.key.placement_id)),
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL => out.cast::<bool>().write(matches!(
            item.placement.location,
            KittyPlacementLocation::Virtual
        )),
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET => {
            out.cast::<u32>().write(item.placement.x_offset)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET => {
            out.cast::<u32>().write(item.placement.y_offset)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_X => {
            out.cast::<u32>().write(item.placement.source_x)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_Y => {
            out.cast::<u32>().write(item.placement.source_y)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_WIDTH => {
            out.cast::<u32>().write(item.placement.source_width)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_HEIGHT => {
            out.cast::<u32>().write(item.placement.source_height)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS => {
            out.cast::<u32>().write(item.placement.columns)
        }
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS => out.cast::<u32>().write(item.placement.rows),
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z => out.cast::<i32>().write(item.placement.z),
        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID => return ROASTTY_INVALID_VALUE,
        _ => return ROASTTY_INVALID_VALUE,
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_get(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    if !valid_kitty_graphics_placement_data(data) || out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(selected) = iterator.selected else {
        return ROASTTY_NO_VALUE;
    };
    let Some(item) = iterator.entries.get(selected) else {
        return ROASTTY_NO_VALUE;
    };

    unsafe { kitty_graphics_placement_get_write(item, data, out) }
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_get_multi(
    iterator: RoasttyKittyGraphicsPlacementIterator,
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
    if kitty_graphics_placement_iterator_mut_from_handle(iterator).is_none()
        || keys.is_null()
        || values.is_null()
    {
        return ROASTTY_INVALID_VALUE;
    }
    if count == 0 {
        return ROASTTY_SUCCESS;
    }

    for index in 0..count {
        let key = unsafe { *keys.add(index) };
        if !valid_kitty_graphics_placement_data(key) {
            return ROASTTY_INVALID_VALUE;
        }
        let value = unsafe { *values.add(index) };
        if value.is_null() {
            return ROASTTY_INVALID_VALUE;
        }
        let result = roastty_kitty_graphics_placement_get(iterator, key, value);
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
pub extern "C" fn roastty_kitty_graphics_placement_rect(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    image: RoasttyKittyGraphicsImage,
    terminal: RoasttyTerminal,
    out: *mut RoasttySelection,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let selection_size = unsafe { (*out).size };
    if selection_size < std::mem::size_of::<RoasttySelection>() {
        return ROASTTY_INVALID_VALUE;
    }
    let key = match kitty_selected_placement_key(iterator) {
        Ok(key) => key,
        Err(error) => return error,
    };
    if terminal.terminal.kitty_placement(key).is_none() {
        return ROASTTY_NO_VALUE;
    }
    let image = match kitty_image_from_snapshot(image) {
        Ok(image) => image,
        Err(error) => return error,
    };
    let Some(selection) = terminal.terminal.kitty_placement_selection(key, &image) else {
        return ROASTTY_NO_VALUE;
    };
    write_selection(out, selection);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_pixel_size(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    image: RoasttyKittyGraphicsImage,
    terminal: RoasttyTerminal,
    out_width: *mut u32,
    out_height: *mut u32,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_width.is_null() || out_height.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let placement = match kitty_live_placement_for_geometry(iterator, &terminal.terminal) {
        Ok(placement) => placement,
        Err(error) => return error,
    };
    let image = match kitty_image_from_snapshot(image) {
        Ok(image) => image,
        Err(error) => return error,
    };
    let pixel_size = placement.pixel_size(&image, terminal.terminal.kitty_cell_metrics());
    unsafe {
        out_width.write(pixel_size.width);
        out_height.write(pixel_size.height);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_grid_size(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    image: RoasttyKittyGraphicsImage,
    terminal: RoasttyTerminal,
    out_cols: *mut u32,
    out_rows: *mut u32,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_cols.is_null() || out_rows.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let placement = match kitty_live_placement_for_geometry(iterator, &terminal.terminal) {
        Ok(placement) => placement,
        Err(error) => return error,
    };
    let image = match kitty_image_from_snapshot(image) {
        Ok(image) => image,
        Err(error) => return error,
    };
    let grid_size = placement.grid_size(&image, terminal.terminal.kitty_cell_metrics());
    unsafe {
        out_cols.write(grid_size.columns);
        out_rows.write(grid_size.rows);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_viewport_pos(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    image: RoasttyKittyGraphicsImage,
    terminal: RoasttyTerminal,
    out_col: *mut i32,
    out_row: *mut i32,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_col.is_null() || out_row.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let key = match kitty_selected_placement_key(iterator) {
        Ok(key) => key,
        Err(error) => return error,
    };
    if terminal.terminal.kitty_placement(key).is_none() {
        return ROASTTY_NO_VALUE;
    }
    let image = match kitty_image_from_snapshot(image) {
        Ok(image) => image,
        Err(error) => return error,
    };
    let Some((col, row, visible)) = terminal.terminal.kitty_placement_viewport_pos(key, &image)
    else {
        return ROASTTY_NO_VALUE;
    };
    if !visible {
        return ROASTTY_NO_VALUE;
    }
    unsafe {
        out_col.write(col);
        out_row.write(row);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_source_rect(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    image: RoasttyKittyGraphicsImage,
    out_x: *mut u32,
    out_y: *mut u32,
    out_width: *mut u32,
    out_height: *mut u32,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_x.is_null() || out_y.is_null() || out_width.is_null() || out_height.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let selected = match iterator.selected {
        Some(selected) => selected,
        None => return ROASTTY_NO_VALUE,
    };
    let Some(item) = iterator.entries.get(selected) else {
        return ROASTTY_NO_VALUE;
    };
    let (x, y, width, height) = kitty_source_rect(item.placement, image);
    unsafe {
        out_x.write(x);
        out_y.write(y);
        out_width.write(width);
        out_height.write(height);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_kitty_graphics_placement_render_info(
    iterator: RoasttyKittyGraphicsPlacementIterator,
    image: RoasttyKittyGraphicsImage,
    terminal: RoasttyTerminal,
    out: *mut RoasttyKittyGraphicsPlacementRenderInfo,
) -> c_int {
    let Some(iterator) = kitty_graphics_placement_iterator_mut_from_handle(iterator) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(image) = kitty_graphics_image_from_handle(image) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let out_size = unsafe { (*out).size };
    if out_size < std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>() {
        return ROASTTY_INVALID_VALUE;
    }
    let selected = match iterator.selected {
        Some(selected) => selected,
        None => return ROASTTY_NO_VALUE,
    };
    let Some(item) = iterator.entries.get(selected) else {
        return ROASTTY_NO_VALUE;
    };
    let placement = match terminal.terminal.kitty_placement(item.key) {
        Some(placement) => placement,
        None => return ROASTTY_NO_VALUE,
    };
    let image_meta = match kitty_image_from_snapshot(image) {
        Ok(image) => image,
        Err(error) => return error,
    };
    let metrics = terminal.terminal.kitty_cell_metrics();
    let pixel_size = placement.pixel_size(&image_meta, metrics);
    let grid_size = placement.grid_size(&image_meta, metrics);
    let (viewport_col, viewport_row, viewport_visible) = terminal
        .terminal
        .kitty_placement_viewport_pos(item.key, &image_meta)
        .unwrap_or((0, 0, false));
    let (source_x, source_y, source_width, source_height) =
        kitty_source_rect(item.placement, image);
    unsafe {
        out.write(RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            pixel_width: pixel_size.width,
            pixel_height: pixel_size.height,
            grid_cols: grid_size.columns,
            grid_rows: grid_size.rows,
            viewport_col,
            viewport_row,
            viewport_visible,
            source_x,
            source_y,
            source_width,
            source_height,
        });
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
pub extern "C" fn roastty_terminal_grid_ref(
    terminal: RoasttyTerminal,
    point: RoasttyPoint,
    out_ref: *mut RoasttyGridRef,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_ref.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(tag) = point_tag_from_raw(point.tag) else {
        return ROASTTY_INVALID_VALUE;
    };

    let coord = point_coordinate(point, tag);
    let Some(grid_ref) = terminal
        .terminal
        .grid_ref(tag, point::Coordinate::new(coord.x, coord.y))
    else {
        return ROASTTY_INVALID_VALUE;
    };

    write_grid_ref(out_ref, grid_ref);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_grid_ref_cell(
    ref_: *const RoasttyGridRef,
    out: *mut RoasttyCell,
) -> c_int {
    let Ok(ref_) = read_grid_ref_ptr(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };

    match ref_.cell_raw() {
        Ok(cell) => {
            if !out.is_null() {
                unsafe {
                    out.write(cell);
                }
            }
            ROASTTY_SUCCESS
        }
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_grid_ref_row(ref_: *const RoasttyGridRef, out: *mut RoasttyRow) -> c_int {
    let Ok(ref_) = read_grid_ref_ptr(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };

    match ref_.row_raw() {
        Ok(row) => {
            if !out.is_null() {
                unsafe {
                    out.write(row);
                }
            }
            ROASTTY_SUCCESS
        }
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_grid_ref_graphemes(
    ref_: *const RoasttyGridRef,
    out_buf: *mut u32,
    buf_len: usize,
    out_len: *mut usize,
) -> c_int {
    if out_len.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Ok(ref_) = read_grid_ref_ptr(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };

    match ref_.graphemes() {
        Ok(graphemes) => unsafe {
            out_len.write(graphemes.len());
            if graphemes.is_empty() {
                return ROASTTY_SUCCESS;
            }
            if out_buf.is_null() || buf_len < graphemes.len() {
                return ROASTTY_OUT_OF_SPACE;
            }
            ptr::copy_nonoverlapping(graphemes.as_ptr(), out_buf, graphemes.len());
            ROASTTY_SUCCESS
        },
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_grid_ref_hyperlink_uri(
    ref_: *const RoasttyGridRef,
    out_buf: *mut u8,
    buf_len: usize,
    out_len: *mut usize,
) -> c_int {
    if out_len.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Ok(ref_) = read_grid_ref_ptr(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };

    match ref_.hyperlink_uri() {
        Ok(uri) => unsafe {
            out_len.write(uri.len());
            if uri.is_empty() {
                return ROASTTY_SUCCESS;
            }
            if out_buf.is_null() || buf_len < uri.len() {
                return ROASTTY_OUT_OF_SPACE;
            }
            ptr::copy_nonoverlapping(uri.as_ptr(), out_buf, uri.len());
            ROASTTY_SUCCESS
        },
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_grid_ref_style(
    ref_: *const RoasttyGridRef,
    out: *mut RoasttyStyle,
) -> c_int {
    let Ok(ref_) = read_grid_ref_ptr(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };

    match ref_.style() {
        Ok(style) => {
            if !out.is_null() {
                unsafe {
                    if ptr::addr_of!((*out).size).read() < std::mem::size_of::<RoasttyStyle>() {
                        return ROASTTY_INVALID_VALUE;
                    }
                    out.write(style_to_c(style));
                }
            }
            ROASTTY_SUCCESS
        }
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_point_from_grid_ref(
    terminal: RoasttyTerminal,
    grid_ref: *const RoasttyGridRef,
    tag: c_int,
    out_coordinate: *mut RoasttyPointCoordinate,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if grid_ref.is_null() || out_coordinate.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(tag) = point_tag_from_raw(tag) else {
        return ROASTTY_INVALID_VALUE;
    };

    let Ok(grid_ref) = read_grid_ref_ptr(grid_ref) else {
        return ROASTTY_INVALID_VALUE;
    };
    match terminal.terminal.point_from_grid_ref(grid_ref, tag) {
        Ok(coord) => {
            unsafe {
                out_coordinate.write(RoasttyPointCoordinate {
                    x: coord.x,
                    y: coord.y,
                });
            }
            ROASTTY_SUCCESS
        }
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_grid_ref_track(
    terminal: RoasttyTerminal,
    point: RoasttyPoint,
    out_ref: *mut RoasttyTrackedGridRef,
) -> c_int {
    if out_ref.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    unsafe {
        out_ref.write(ptr::null_mut());
    }

    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(tag) = point_tag_from_raw(point.tag) else {
        return ROASTTY_INVALID_VALUE;
    };

    let coord = point_coordinate(point, tag);
    let Some(tracked) = terminal
        .terminal
        .track_grid_ref(tag, point::Coordinate::new(coord.x, coord.y))
    else {
        return ROASTTY_INVALID_VALUE;
    };

    let terminal_ptr = NonNull::from(&mut *terminal);
    let terminal_handle = terminal_ptr.as_ptr().cast::<c_void>();
    let mut handle = Box::new(TrackedGridRefHandle {
        terminal: Some(terminal_ptr),
        terminal_handle,
        tracked: Some(tracked),
    });
    let handle_ptr = NonNull::from(handle.as_mut());
    terminal.tracked_grid_refs.push(handle_ptr);

    unsafe {
        out_ref.write(Box::into_raw(handle).cast());
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_tracked_grid_ref_free(ref_: RoasttyTrackedGridRef) {
    if ref_.is_null() {
        return;
    }

    let mut ref_ = unsafe { Box::from_raw(ref_.cast::<TrackedGridRefHandle>()) };
    let ref_ptr = NonNull::from(ref_.as_mut());
    if let Some(mut terminal_ptr) = ref_.terminal.take() {
        let terminal = unsafe { terminal_ptr.as_mut() };
        terminal.unregister_tracked_grid_ref(ref_ptr);
        if let Some(tracked) = ref_.tracked.take() {
            terminal.terminal.untrack_grid_ref(tracked);
        }
    }
}

fn tracked_grid_ref_snapshot(ref_: &TrackedGridRefHandle) -> Option<TerminalGridRef> {
    let terminal_ptr = ref_.terminal?;
    let tracked = ref_.tracked.as_ref()?;
    let terminal = unsafe { terminal_ptr.as_ref() };
    terminal.terminal.tracked_grid_ref_snapshot(tracked)
}

#[no_mangle]
pub extern "C" fn roastty_tracked_grid_ref_has_value(ref_: RoasttyTrackedGridRef) -> bool {
    let Some(ref_) = tracked_grid_ref_from_handle(ref_) else {
        return false;
    };
    tracked_grid_ref_snapshot(ref_).is_some()
}

#[no_mangle]
pub extern "C" fn roastty_tracked_grid_ref_snapshot(
    ref_: RoasttyTrackedGridRef,
    out_ref: *mut RoasttyGridRef,
) -> c_int {
    let Some(ref_) = tracked_grid_ref_from_handle(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(snapshot) = tracked_grid_ref_snapshot(ref_) else {
        return ROASTTY_NO_VALUE;
    };
    if !out_ref.is_null() {
        write_grid_ref(out_ref, snapshot);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_tracked_grid_ref_point(
    ref_: RoasttyTrackedGridRef,
    tag: c_int,
    out_coordinate: *mut RoasttyPointCoordinate,
) -> c_int {
    let Some(ref_) = tracked_grid_ref_from_handle(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(tag) = point_tag_from_raw(tag) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal_ptr) = ref_.terminal else {
        return ROASTTY_NO_VALUE;
    };
    let Some(tracked) = ref_.tracked.as_ref() else {
        return ROASTTY_NO_VALUE;
    };

    let terminal = unsafe { terminal_ptr.as_ref() };
    match terminal.terminal.tracked_grid_ref_point(tracked, tag) {
        Ok(coord) => {
            if !out_coordinate.is_null() {
                unsafe {
                    out_coordinate.write(RoasttyPointCoordinate {
                        x: coord.x,
                        y: coord.y,
                    });
                }
            }
            ROASTTY_SUCCESS
        }
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_tracked_grid_ref_set(
    ref_: RoasttyTrackedGridRef,
    terminal: RoasttyTerminal,
    point: RoasttyPoint,
) -> c_int {
    let Some(ref_) = tracked_grid_ref_from_handle(ref_) else {
        return ROASTTY_INVALID_VALUE;
    };
    if terminal.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if ref_.terminal.is_none() {
        return ROASTTY_INVALID_VALUE;
    }
    if terminal != ref_.terminal_handle {
        return ROASTTY_INVALID_VALUE;
    }

    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(tag) = point_tag_from_raw(point.tag) else {
        return ROASTTY_INVALID_VALUE;
    };

    let coord = point_coordinate(point, tag);
    let Some(new_tracked) = terminal
        .terminal
        .track_grid_ref(tag, point::Coordinate::new(coord.x, coord.y))
    else {
        return ROASTTY_INVALID_VALUE;
    };

    if let Some(old_tracked) = ref_.tracked.replace(new_tracked) {
        terminal.terminal.untrack_grid_ref(old_tracked);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_select_word(
    terminal: RoasttyTerminal,
    options: *const RoasttyTerminalSelectWordOptions,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_selection.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let (ref_, boundary_codepoints) = match read_select_word_options(options) {
        Ok(options) => options,
        Err(error) => return error,
    };

    match terminal
        .terminal
        .select_word(ref_, boundary_codepoints.as_deref())
    {
        Ok(Some(selection)) => {
            write_selection(out_selection, selection);
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_select_word_between(
    terminal: RoasttyTerminal,
    options: *const RoasttyTerminalSelectWordBetweenOptions,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_selection.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let (start, end, boundary_codepoints) = match read_select_word_between_options(options) {
        Ok(options) => options,
        Err(error) => return error,
    };

    match terminal
        .terminal
        .select_word_between(start, end, boundary_codepoints.as_deref())
    {
        Ok(Some(selection)) => {
            write_selection(out_selection, selection);
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_select_line(
    terminal: RoasttyTerminal,
    options: *const RoasttyTerminalSelectLineOptions,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_selection.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let (ref_, whitespace, semantic_prompt_boundary) = match read_select_line_options(options) {
        Ok(options) => options,
        Err(error) => return error,
    };

    match terminal
        .terminal
        .select_line(ref_, whitespace.as_deref(), semantic_prompt_boundary)
    {
        Ok(Some(selection)) => {
            write_selection(out_selection, selection);
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_select_all(
    terminal: RoasttyTerminal,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_selection.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let Some(selection) = terminal.terminal.select_all() else {
        return ROASTTY_NO_VALUE;
    };
    write_selection(out_selection, selection);
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_select_output(
    terminal: RoasttyTerminal,
    ref_: *const RoasttyGridRef,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_selection.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let ref_ = match read_grid_ref_ptr(ref_) {
        Ok(ref_) => ref_,
        Err(error) => return error,
    };
    match terminal.terminal.select_output(ref_) {
        Ok(Some(selection)) => {
            write_selection(out_selection, selection);
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_adjust(
    terminal: RoasttyTerminal,
    selection: *mut RoasttySelection,
    adjustment: c_int,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    let adjustment = match selection_adjustment_from_raw(adjustment) {
        Ok(adjustment) => adjustment,
        Err(error) => return error,
    };
    let input = match read_selection(selection.cast_const()) {
        Ok(selection) => selection,
        Err(error) => return error,
    };
    match terminal.terminal.selection_adjust(input, adjustment) {
        Ok(Some(output)) => {
            write_selection(selection, output);
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_order(
    terminal: RoasttyTerminal,
    selection: *const RoasttySelection,
    out_order: *mut c_int,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_order.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let selection = match read_selection(selection) {
        Ok(selection) => selection,
        Err(error) => return error,
    };
    match terminal.terminal.selection_order(selection) {
        Ok(Some(order)) => {
            unsafe {
                out_order.write(order.raw());
            }
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_ordered(
    terminal: RoasttyTerminal,
    selection: *const RoasttySelection,
    desired_order: c_int,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_selection.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let desired_order = match selection_order_from_raw(desired_order) {
        Ok(desired_order) => desired_order,
        Err(error) => return error,
    };
    let selection = match read_selection(selection) {
        Ok(selection) => selection,
        Err(error) => return error,
    };
    match terminal
        .terminal
        .selection_ordered(selection, desired_order)
    {
        Ok(Some(selection)) => {
            write_selection(out_selection, selection);
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_contains(
    terminal: RoasttyTerminal,
    selection: *const RoasttySelection,
    point: RoasttyPoint,
    out_contains: *mut bool,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_contains.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let selection = match read_selection(selection) {
        Ok(selection) => selection,
        Err(error) => return error,
    };
    let Some(tag) = point_tag_from_raw(point.tag) else {
        return ROASTTY_INVALID_VALUE;
    };
    let coord = point_coordinate(point, tag);
    match terminal.terminal.selection_contains(
        selection,
        tag,
        point::Coordinate::new(coord.x, coord.y),
    ) {
        Ok(Some(contains)) => {
            unsafe {
                out_contains.write(contains);
            }
            ROASTTY_SUCCESS
        }
        Ok(None) => ROASTTY_NO_VALUE,
        Err(error) => grid_ref_error_result(error),
    }
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_equal(
    terminal: RoasttyTerminal,
    a: *const RoasttySelection,
    b: *const RoasttySelection,
    out_equal: *mut bool,
) -> c_int {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out_equal.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let a = match read_selection(a) {
        Ok(selection) => selection,
        Err(error) => return error,
    };
    let b = match read_selection(b) {
        Ok(selection) => selection,
        Err(error) => return error,
    };
    match terminal.terminal.selection_equal(a, b) {
        Ok(equal) => {
            unsafe {
                out_equal.write(equal);
            }
            ROASTTY_SUCCESS
        }
        Err(error) => grid_ref_error_result(error),
    }
}

fn terminal_selection_format_text(
    terminal: RoasttyTerminal,
    options: *const RoasttyTerminalSelectionFormatOptions,
) -> Result<String, c_int> {
    let Some(terminal) = terminal_from_handle(terminal) else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    let options: RoasttyTerminalSelectionFormatOptions = read_sized_abi(options)?;
    let format = selection_format_from_raw(options.emit)?;
    let selection = if options.selection.is_null() {
        None
    } else {
        Some(read_selection(options.selection)?)
    };
    terminal
        .terminal
        .selection_format(format, options.unwrap, options.trim, selection)
        .map_err(grid_ref_error_result)
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_format_buf(
    terminal: RoasttyTerminal,
    options: *const RoasttyTerminalSelectionFormatOptions,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if out.is_null() && out_len > 0 {
        return ROASTTY_INVALID_VALUE;
    }
    let text = match terminal_selection_format_text(terminal, options) {
        Ok(text) => text,
        Err(error) => return error,
    };
    let bytes = text.as_bytes();
    unsafe {
        out_written.write(bytes.len());
    }
    if out.is_null() || out_len < bytes.len() {
        return ROASTTY_OUT_OF_SPACE;
    }
    if !bytes.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        }
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_terminal_selection_format(
    terminal: RoasttyTerminal,
    options: *const RoasttyTerminalSelectionFormatOptions,
    out: *mut RoasttyString,
) -> c_int {
    write_empty_string(out);
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let text = match terminal_selection_format_text(terminal, options) {
        Ok(text) => text,
        Err(error) => return error,
    };
    write_copied_string(out, text.as_bytes())
}

#[no_mangle]
pub extern "C" fn roastty_formatter_terminal_new(
    out: *mut RoasttyFormatter,
    terminal: RoasttyTerminal,
    options: RoasttyFormatterTerminalOptions,
) -> c_int {
    if !out.is_null() {
        unsafe {
            out.write(ptr::null_mut());
        }
    }
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    if terminal_from_handle(terminal).is_none() {
        return ROASTTY_INVALID_VALUE;
    }

    let mut formatter = match read_formatter_terminal_options(&options) {
        Ok(formatter) => formatter,
        Err(error) => return error,
    };
    formatter.terminal = terminal;

    let handle = Box::into_raw(Box::new(formatter)).cast::<c_void>();
    unsafe {
        out.write(handle);
    }
    ROASTTY_SUCCESS
}

fn formatter_format_text(formatter: RoasttyFormatter) -> Result<String, c_int> {
    let Some(formatter) = formatter_from_handle(formatter) else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    let Some(terminal) = terminal_from_handle(formatter.terminal) else {
        return Err(ROASTTY_INVALID_VALUE);
    };
    terminal
        .terminal
        .formatter_format(
            formatter.format,
            formatter.unwrap,
            formatter.trim,
            formatter.extra,
            formatter.selection,
        )
        .map_err(grid_ref_error_result)
}

#[no_mangle]
pub extern "C" fn roastty_formatter_format_buf(
    formatter: RoasttyFormatter,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    if out_written.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let text = match formatter_format_text(formatter) {
        Ok(text) => text,
        Err(error) => return error,
    };
    let bytes = text.as_bytes();
    unsafe {
        out_written.write(bytes.len());
    }
    if (out.is_null() && !bytes.is_empty()) || out_len < bytes.len() {
        return ROASTTY_OUT_OF_SPACE;
    }
    if !bytes.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        }
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_formatter_format(
    formatter: RoasttyFormatter,
    out: *mut RoasttyString,
) -> c_int {
    write_empty_string(out);
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    let text = match formatter_format_text(formatter) {
        Ok(text) => text,
        Err(error) => return error,
    };
    write_copied_string(out, text.as_bytes())
}

#[no_mangle]
pub extern "C" fn roastty_formatter_free(formatter: RoasttyFormatter) {
    if !formatter.is_null() {
        unsafe {
            drop(Box::from_raw(formatter.cast::<FormatterHandle>()));
        }
    }
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_new(out: *mut RoasttySelectionGesture) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    unsafe {
        out.write(ptr::null_mut());
    }
    let mut handle = Box::new(SelectionGestureHandle {
        gesture: SelectionGesture::default(),
    });
    let ptr = (&mut *handle) as *mut SelectionGestureHandle as RoasttySelectionGesture;
    std::mem::forget(handle);
    unsafe {
        out.write(ptr);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_free(
    gesture: RoasttySelectionGesture,
    terminal: RoasttyTerminal,
) {
    let Some(mut gesture) = (if gesture.is_null() {
        None
    } else {
        Some(unsafe { Box::from_raw(gesture.cast::<SelectionGestureHandle>()) })
    }) else {
        return;
    };
    let terminal = terminal_from_handle(terminal).map(|terminal| &mut terminal.terminal);
    gesture.gesture.free(terminal);
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_reset(
    gesture: RoasttySelectionGesture,
    terminal: RoasttyTerminal,
) {
    let Some(gesture) = selection_gesture_from_handle(gesture) else {
        return;
    };
    let terminal = terminal_from_handle(terminal).map(|terminal| &mut terminal.terminal);
    gesture.gesture.reset(terminal);
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_event_new(
    out: *mut RoasttySelectionGestureEvent,
    event_type: c_int,
) -> c_int {
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    unsafe {
        out.write(ptr::null_mut());
    }
    let Some(kind) = selection_gesture_event_kind_from_raw(event_type) else {
        return ROASTTY_INVALID_VALUE;
    };
    let mut handle = Box::new(SelectionGestureEventHandle {
        event: kind,
        ref_: None,
        position: RoasttySurfacePosition::default(),
        repeat_distance: 0.0,
        time_ns: None,
        repeat_interval_ns: 0,
        word_boundary_codepoints: None,
        behaviors: DEFAULT_BEHAVIORS,
        rectangle: false,
        geometry: None,
        viewport: None,
    });
    let ptr = (&mut *handle) as *mut SelectionGestureEventHandle as RoasttySelectionGestureEvent;
    std::mem::forget(handle);
    unsafe {
        out.write(ptr);
    }
    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_event_free(event: RoasttySelectionGestureEvent) {
    if event.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(event.cast::<SelectionGestureEventHandle>()) });
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_get(
    gesture: RoasttySelectionGesture,
    terminal: RoasttyTerminal,
    data: c_int,
    out: *mut c_void,
) -> c_int {
    let Some(gesture) = selection_gesture_from_handle(gesture) else {
        return ROASTTY_INVALID_VALUE;
    };
    if out.is_null() {
        return ROASTTY_INVALID_VALUE;
    }

    match data {
        ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT => unsafe {
            out.cast::<u8>().write(gesture.gesture.click_count());
            ROASTTY_SUCCESS
        },
        ROASTTY_SELECTION_GESTURE_DATA_DRAGGED => unsafe {
            out.cast::<bool>().write(gesture.gesture.dragged());
            ROASTTY_SUCCESS
        },
        ROASTTY_SELECTION_GESTURE_DATA_AUTOSCROLL => unsafe {
            out.cast::<c_int>()
                .write(selection_gesture_autoscroll_to_raw(
                    gesture.gesture.autoscroll(),
                ));
            ROASTTY_SUCCESS
        },
        ROASTTY_SELECTION_GESTURE_DATA_BEHAVIOR => unsafe {
            out.cast::<c_int>().write(selection_gesture_behavior_to_raw(
                gesture.gesture.behavior(),
            ));
            ROASTTY_SUCCESS
        },
        ROASTTY_SELECTION_GESTURE_DATA_ANCHOR => {
            let Some(terminal) = terminal_from_handle(terminal) else {
                return ROASTTY_INVALID_VALUE;
            };
            let Some(anchor) = gesture.gesture.anchor_ref(&terminal.terminal) else {
                return ROASTTY_NO_VALUE;
            };
            write_grid_ref(out.cast::<RoasttyGridRef>(), anchor);
            ROASTTY_SUCCESS
        }
        _ => ROASTTY_INVALID_VALUE,
    }
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_get_multi(
    gesture: RoasttySelectionGesture,
    terminal: RoasttyTerminal,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> c_int {
    if keys.is_null() || values.is_null() {
        return ROASTTY_INVALID_VALUE;
    }
    for index in 0..count {
        let key = unsafe { keys.add(index).read() };
        let value = unsafe { values.add(index).read() };
        let result = roastty_selection_gesture_get(gesture, terminal, key, value);
        if result != ROASTTY_SUCCESS {
            if !out_written.is_null() {
                unsafe {
                    out_written.write(index);
                }
            }
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
pub extern "C" fn roastty_selection_gesture_event_set(
    event: RoasttySelectionGestureEvent,
    option: c_int,
    value: *const c_void,
) -> c_int {
    let Some(event) = selection_gesture_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };

    match option {
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF => match event.event {
            SelectionGestureEventKind::Press
            | SelectionGestureEventKind::Release
            | SelectionGestureEventKind::Drag => {
                if value.is_null() {
                    event.ref_ = None;
                } else {
                    match read_grid_ref_ptr(value.cast::<RoasttyGridRef>()) {
                        Ok(ref_) => event.ref_ = Some(ref_),
                        Err(result) => return result,
                    }
                }
            }
            _ => return ROASTTY_INVALID_VALUE,
        },
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION => match event.event {
            SelectionGestureEventKind::Press
            | SelectionGestureEventKind::Drag
            | SelectionGestureEventKind::AutoscrollTick => {
                event.position = if value.is_null() {
                    RoasttySurfacePosition::default()
                } else {
                    unsafe { value.cast::<RoasttySurfacePosition>().read() }
                };
            }
            _ => return ROASTTY_INVALID_VALUE,
        },
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_DISTANCE => {
            if event.event != SelectionGestureEventKind::Press || value.is_null() {
                return ROASTTY_INVALID_VALUE;
            }
            event.repeat_distance = unsafe { value.cast::<f64>().read() };
        }
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS => {
            if event.event != SelectionGestureEventKind::Press {
                return ROASTTY_INVALID_VALUE;
            }
            event.time_ns = if value.is_null() {
                None
            } else {
                Some(unsafe { value.cast::<u64>().read() })
            };
        }
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_INTERVAL_NS => {
            if event.event != SelectionGestureEventKind::Press || value.is_null() {
                return ROASTTY_INVALID_VALUE;
            }
            event.repeat_interval_ns = unsafe { value.cast::<u64>().read() };
        }
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS => match event.event {
            SelectionGestureEventKind::Press
            | SelectionGestureEventKind::Drag
            | SelectionGestureEventKind::AutoscrollTick
            | SelectionGestureEventKind::DeepPress => {
                event.word_boundary_codepoints = if value.is_null() {
                    None
                } else {
                    match read_selection_gesture_codepoints(value.cast::<RoasttyCodepoints>()) {
                        Ok(codepoints) => Some(codepoints),
                        Err(result) => return result,
                    }
                };
            }
            _ => return ROASTTY_INVALID_VALUE,
        },
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_BEHAVIORS => {
            if event.event != SelectionGestureEventKind::Press || value.is_null() {
                return ROASTTY_INVALID_VALUE;
            }
            let behaviors = unsafe { value.cast::<RoasttySelectionGestureBehaviors>().read() };
            let Some(behaviors) = read_selection_gesture_behaviors(behaviors) else {
                return ROASTTY_INVALID_VALUE;
            };
            event.behaviors = behaviors;
        }
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_RECTANGLE => match event.event {
            SelectionGestureEventKind::Drag | SelectionGestureEventKind::AutoscrollTick => {
                event.rectangle = if value.is_null() {
                    false
                } else {
                    unsafe { value.cast::<bool>().read() }
                };
            }
            _ => return ROASTTY_INVALID_VALUE,
        },
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_GEOMETRY => match event.event {
            SelectionGestureEventKind::Drag | SelectionGestureEventKind::AutoscrollTick => {
                if value.is_null() {
                    event.geometry = None;
                } else {
                    let geometry =
                        unsafe { value.cast::<RoasttySelectionGestureGeometry>().read() };
                    let Some(geometry) = read_selection_gesture_geometry(geometry) else {
                        return ROASTTY_INVALID_VALUE;
                    };
                    event.geometry = Some(geometry);
                }
            }
            _ => return ROASTTY_INVALID_VALUE,
        },
        ROASTTY_SELECTION_GESTURE_EVENT_OPTION_VIEWPORT => {
            if event.event != SelectionGestureEventKind::AutoscrollTick {
                return ROASTTY_INVALID_VALUE;
            }
            if value.is_null() {
                event.viewport = None;
            } else {
                let coordinate = unsafe { value.cast::<RoasttyPointCoordinate>().read() };
                event.viewport = Some(point::Coordinate::new(coordinate.x, coordinate.y));
            }
        }
        _ => return ROASTTY_INVALID_VALUE,
    }

    ROASTTY_SUCCESS
}

#[no_mangle]
pub extern "C" fn roastty_selection_gesture_handle_event(
    gesture: RoasttySelectionGesture,
    terminal: RoasttyTerminal,
    event: RoasttySelectionGestureEvent,
    out_selection: *mut RoasttySelection,
) -> c_int {
    let Some(gesture) = selection_gesture_from_handle(gesture) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(terminal) = terminal_from_handle(terminal) else {
        return ROASTTY_INVALID_VALUE;
    };
    let Some(event) = selection_gesture_event_from_handle(event) else {
        return ROASTTY_INVALID_VALUE;
    };

    let boundary = event.word_boundary_codepoints.as_deref();
    let selection = match event.event {
        SelectionGestureEventKind::Press => {
            let Some(ref_) = event.ref_ else {
                return ROASTTY_INVALID_VALUE;
            };
            let Ok(coord) = terminal
                .terminal
                .point_from_grid_ref(ref_, TerminalPointTag::Active)
            else {
                return ROASTTY_INVALID_VALUE;
            };
            let Some(pin) = terminal.terminal.active_pin(coord) else {
                return ROASTTY_INVALID_VALUE;
            };
            gesture.gesture.press(
                &mut terminal.terminal,
                SelectionGesturePress {
                    time_ns: event.time_ns,
                    pin,
                    x: event.position.x,
                    y: event.position.y,
                    max_distance: event.repeat_distance,
                    repeat_interval_ns: event.repeat_interval_ns,
                    word_boundary_codepoints: boundary,
                    behaviors: event.behaviors,
                },
            )
        }
        SelectionGestureEventKind::Release => {
            let pin = match event.ref_ {
                Some(ref_) => {
                    let Ok(coord) = terminal
                        .terminal
                        .point_from_grid_ref(ref_, TerminalPointTag::Active)
                    else {
                        return ROASTTY_INVALID_VALUE;
                    };
                    terminal.terminal.active_pin(coord)
                }
                None => None,
            };
            gesture
                .gesture
                .release(&terminal.terminal, SelectionGestureRelease { pin });
            None
        }
        SelectionGestureEventKind::Drag => {
            let Some(ref_) = event.ref_ else {
                return ROASTTY_INVALID_VALUE;
            };
            let Some(geometry) = event.geometry else {
                return ROASTTY_INVALID_VALUE;
            };
            let Ok(coord) = terminal
                .terminal
                .point_from_grid_ref(ref_, TerminalPointTag::Active)
            else {
                return ROASTTY_INVALID_VALUE;
            };
            let Some(pin) = terminal.terminal.active_pin(coord) else {
                return ROASTTY_INVALID_VALUE;
            };
            gesture.gesture.drag(
                &mut terminal.terminal,
                SelectionGestureDrag {
                    pin,
                    x: event.position.x,
                    y: event.position.y,
                    rectangle: event.rectangle,
                    word_boundary_codepoints: boundary,
                    geometry,
                },
            )
        }
        SelectionGestureEventKind::AutoscrollTick => {
            let Some(geometry) = event.geometry else {
                return ROASTTY_INVALID_VALUE;
            };
            let Some(viewport) = event.viewport else {
                return ROASTTY_INVALID_VALUE;
            };
            gesture.gesture.autoscroll_tick(
                &mut terminal.terminal,
                SelectionGestureAutoscrollTick {
                    viewport,
                    x: event.position.x,
                    y: event.position.y,
                    rectangle: event.rectangle,
                    word_boundary_codepoints: boundary,
                    geometry,
                },
            )
        }
        SelectionGestureEventKind::DeepPress => gesture.gesture.deep_press(
            &mut terminal.terminal,
            SelectionGestureDeepPress {
                word_boundary_codepoints: boundary,
            },
        ),
    };

    let Some(selection) = selection else {
        return ROASTTY_NO_VALUE;
    };
    if !out_selection.is_null() {
        write_selection(out_selection, selection);
    }
    ROASTTY_SUCCESS
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
    use crate::terminal::terminal::{
        TerminalDeviceAttributes, TerminalDeviceAttributesPrimary,
        TerminalDeviceAttributesSecondary, TerminalDeviceAttributesTertiary,
    };
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

    #[derive(Default)]
    struct EffectState {
        write_calls: Vec<Vec<u8>>,
        bell_count: usize,
        title_changed_count: usize,
        last_terminal: RoasttyTerminal,
        last_userdata: *mut c_void,
        enquiry_ptr: *const c_char,
        enquiry_len: usize,
        xtversion_ptr: *const c_char,
        xtversion_len: usize,
        size_response: size_report::Size,
        size_result: bool,
        size_count: usize,
        color_scheme_response: c_int,
        color_scheme_result: bool,
        color_scheme_count: usize,
        device_attributes_response: TerminalDeviceAttributes,
        device_attributes_result: bool,
        device_attributes_count: usize,
    }

    thread_local! {
        static EFFECT_STATE: RefCell<EffectState> = RefCell::new(EffectState::default());
    }

    fn reset_effect_state() {
        EFFECT_STATE.with(|state| *state.borrow_mut() = EffectState::default());
    }

    fn with_effect_state<R>(f: impl FnOnce(&mut EffectState) -> R) -> R {
        EFFECT_STATE.with(|state| f(&mut state.borrow_mut()))
    }

    unsafe extern "C" fn write_pty_cb(
        terminal: RoasttyTerminal,
        userdata: *mut c_void,
        ptr: *const u8,
        len: usize,
    ) {
        let bytes = if ptr.is_null() || len == 0 {
            Vec::new()
        } else {
            slice::from_raw_parts(ptr, len).to_vec()
        };
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            state.write_calls.push(bytes);
        });
    }

    unsafe extern "C" fn bell_cb(terminal: RoasttyTerminal, userdata: *mut c_void) {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            state.bell_count += 1;
        });
    }

    unsafe extern "C" fn enquiry_cb(
        terminal: RoasttyTerminal,
        userdata: *mut c_void,
    ) -> RoasttyString {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            RoasttyString {
                ptr: state.enquiry_ptr,
                len: state.enquiry_len,
                sentinel: false,
            }
        })
    }

    unsafe extern "C" fn xtversion_cb(
        terminal: RoasttyTerminal,
        userdata: *mut c_void,
    ) -> RoasttyString {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            RoasttyString {
                ptr: state.xtversion_ptr,
                len: state.xtversion_len,
                sentinel: false,
            }
        })
    }

    unsafe extern "C" fn title_changed_cb(terminal: RoasttyTerminal, userdata: *mut c_void) {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            state.title_changed_count += 1;
        });
    }

    unsafe extern "C" fn size_cb(
        terminal: RoasttyTerminal,
        userdata: *mut c_void,
        out_size: *mut size_report::Size,
    ) -> bool {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            state.size_count += 1;
            if state.size_result && !out_size.is_null() {
                out_size.write(state.size_response);
            }
            state.size_result
        })
    }

    unsafe extern "C" fn color_scheme_cb(
        terminal: RoasttyTerminal,
        userdata: *mut c_void,
        out_scheme: *mut c_int,
    ) -> bool {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            state.color_scheme_count += 1;
            if state.color_scheme_result && !out_scheme.is_null() {
                out_scheme.write(state.color_scheme_response);
            }
            state.color_scheme_result
        })
    }

    unsafe extern "C" fn device_attributes_cb(
        terminal: RoasttyTerminal,
        userdata: *mut c_void,
        out_attrs: *mut TerminalDeviceAttributes,
    ) -> bool {
        with_effect_state(|state| {
            state.last_terminal = terminal;
            state.last_userdata = userdata;
            state.device_attributes_count += 1;
            if state.device_attributes_result && !out_attrs.is_null() {
                out_attrs.write(state.device_attributes_response);
            }
            state.device_attributes_result
        })
    }

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

    fn kitty_graphics_handle(terminal: RoasttyTerminal) -> RoasttyKittyGraphics {
        let mut graphics: RoasttyKittyGraphics = ptr::null_mut();
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS,
                (&mut graphics as *mut RoasttyKittyGraphics).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert!(!graphics.is_null());
        graphics
    }

    fn write_kitty_transmit_display(
        terminal: RoasttyTerminal,
        image_id: u32,
        placement_id: u32,
        z: i32,
    ) {
        write_terminal(
            terminal,
            format!(
                "\x1b_Ga=T,f=32,s=1,v=1,i={image_id},p={placement_id},x=2,y=3,w=4,h=5,X=6,Y=7,c=3,r=2,z={z},C=1;AQIDBA==\x1b\\"
            )
            .as_bytes(),
        );
        let _ = terminal_string(terminal, roastty_terminal_take_pty_response);
    }

    #[derive(Clone, Copy)]
    struct KittyDisplayFixture {
        image_id: u32,
        placement_id: u32,
        image_width: u32,
        image_height: u32,
        source_x: u32,
        source_y: u32,
        source_width: u32,
        source_height: u32,
        x_offset: u32,
        y_offset: u32,
        columns: u32,
        rows: u32,
        z: i32,
    }

    fn kitty_rgba_payload(width: u32, height: u32) -> &'static str {
        match width.saturating_mul(height).saturating_mul(4) {
            4 => "AQIDBA==",
            32 => "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA=",
            64 => {
                "AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8wMTIzNDU2Nzg5Ojs8PT4/QA=="
            }
            other => panic!("missing Kitty RGBA payload for {other} bytes"),
        }
    }

    fn write_kitty_fixture(terminal: RoasttyTerminal, fixture: KittyDisplayFixture) {
        write_terminal(
            terminal,
            format!(
                "\x1b_Ga=T,f=32,s={},v={},i={},p={},x={},y={},w={},h={},X={},Y={},c={},r={},z={},C=1;{}\x1b\\",
                fixture.image_width,
                fixture.image_height,
                fixture.image_id,
                fixture.placement_id,
                fixture.source_x,
                fixture.source_y,
                fixture.source_width,
                fixture.source_height,
                fixture.x_offset,
                fixture.y_offset,
                fixture.columns,
                fixture.rows,
                fixture.z,
                kitty_rgba_payload(fixture.image_width, fixture.image_height)
            )
            .as_bytes(),
        );
        let _ = terminal_string(terminal, roastty_terminal_take_pty_response);
    }

    fn write_kitty_delete(terminal: RoasttyTerminal, args: &str) {
        write_terminal(terminal, format!("\x1b_Ga=d,{args}\x1b\\").as_bytes());
        let _ = terminal_string(terminal, roastty_terminal_take_pty_response);
    }

    fn set_kitty_metrics(terminal: RoasttyTerminal, width_px: u32, height_px: u32) {
        terminal_from_handle(terminal)
            .unwrap()
            .terminal
            .set_kitty_cell_metrics_for_tests(width_px, height_px);
    }

    fn selected_kitty_placement(
        terminal: RoasttyTerminal,
        image_id: u32,
    ) -> (
        RoasttyKittyGraphicsImage,
        RoasttyKittyGraphicsPlacementIterator,
    ) {
        let image = roastty_kitty_graphics_image(kitty_graphics_handle(terminal), image_id);
        assert!(!image.is_null());
        let mut iterator = ptr::null_mut();
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_new(&mut iterator),
            ROASTTY_SUCCESS
        );
        assert!(!iterator.is_null());
        assert_eq!(
            roastty_kitty_graphics_get(
                kitty_graphics_handle(terminal),
                ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
                (&mut iterator as *mut RoasttyKittyGraphicsPlacementIterator).cast()
            ),
            ROASTTY_SUCCESS
        );
        let mut selected_image_id = 0u32;
        while roastty_kitty_graphics_placement_next(iterator) {
            assert_eq!(
                roastty_kitty_graphics_placement_get(
                    iterator,
                    ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
                    (&mut selected_image_id as *mut u32).cast()
                ),
                ROASTTY_SUCCESS
            );
            if selected_image_id == image_id {
                return (image, iterator);
            }
        }
        panic!("missing Kitty graphics placement for image {image_id}");
    }

    fn c_point(tag: c_int, x: u16, y: u32) -> RoasttyPoint {
        let coordinate = RoasttyPointCoordinate { x, y };
        let value = match tag {
            ROASTTY_POINT_ACTIVE => RoasttyPointValue { active: coordinate },
            ROASTTY_POINT_VIEWPORT => RoasttyPointValue {
                viewport: coordinate,
            },
            ROASTTY_POINT_SCREEN => RoasttyPointValue { screen: coordinate },
            ROASTTY_POINT_HISTORY => RoasttyPointValue {
                history: coordinate,
            },
            _ => RoasttyPointValue { active: coordinate },
        };
        RoasttyPoint { tag, value }
    }

    fn terminal_grid_ref_at(terminal: RoasttyTerminal, x: u16, y: u32) -> RoasttyGridRef {
        let mut grid_ref = RoasttyGridRef::default();
        assert_eq!(
            roastty_terminal_grid_ref(terminal, c_point(ROASTTY_POINT_ACTIVE, x, y), &mut grid_ref),
            ROASTTY_SUCCESS
        );
        grid_ref
    }

    fn terminal_tracked_grid_ref_at(
        terminal: RoasttyTerminal,
        x: u16,
        y: u32,
    ) -> RoasttyTrackedGridRef {
        let mut tracked = ptr::null_mut();
        assert_eq!(
            roastty_terminal_grid_ref_track(
                terminal,
                c_point(ROASTTY_POINT_ACTIVE, x, y),
                &mut tracked
            ),
            ROASTTY_SUCCESS
        );
        assert!(!tracked.is_null());
        tracked
    }

    fn terminal_selection(
        terminal: RoasttyTerminal,
        start: (u16, u32),
        end: (u16, u32),
        rectangle: bool,
    ) -> RoasttySelection {
        RoasttySelection {
            size: std::mem::size_of::<RoasttySelection>(),
            start: terminal_grid_ref_at(terminal, start.0, start.1),
            end: terminal_grid_ref_at(terminal, end.0, end.1),
            rectangle,
        }
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

    fn formatter_options(emit: c_int) -> RoasttyFormatterTerminalOptions {
        RoasttyFormatterTerminalOptions {
            size: std::mem::size_of::<RoasttyFormatterTerminalOptions>(),
            emit,
            unwrap: true,
            trim: true,
            extra: RoasttyFormatterTerminalExtra {
                size: std::mem::size_of::<RoasttyFormatterTerminalExtra>(),
                screen: RoasttyFormatterScreenExtra {
                    size: std::mem::size_of::<RoasttyFormatterScreenExtra>(),
                    ..RoasttyFormatterScreenExtra::default()
                },
                ..RoasttyFormatterTerminalExtra::default()
            },
            selection: ptr::null(),
        }
    }

    fn new_formatter(
        terminal: RoasttyTerminal,
        options: RoasttyFormatterTerminalOptions,
    ) -> RoasttyFormatter {
        let mut formatter = ptr::null_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, options),
            ROASTTY_SUCCESS
        );
        assert!(!formatter.is_null());
        formatter
    }

    fn formatter_string(formatter: RoasttyFormatter) -> Vec<u8> {
        let mut out = empty_string();
        assert_eq!(
            roastty_formatter_format(formatter, &mut out),
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

    fn new_render_state() -> RoasttyRenderStateHandle {
        let mut state = ptr::null_mut();
        assert_eq!(roastty_render_state_new(&mut state), ROASTTY_SUCCESS);
        assert!(!state.is_null());
        state
    }

    fn new_render_state_row_iterator() -> RoasttyRenderStateRowIterator {
        let mut iterator: RoasttyRenderStateRowIterator = ptr::null_mut();
        assert_eq!(
            roastty_render_state_row_iterator_new(&mut iterator),
            ROASTTY_SUCCESS
        );
        assert!(!iterator.is_null());
        iterator
    }

    fn new_render_state_row_cells() -> RoasttyRenderStateRowCells {
        let mut cells: RoasttyRenderStateRowCells = ptr::null_mut();
        assert_eq!(
            roastty_render_state_row_cells_new(&mut cells),
            ROASTTY_SUCCESS
        );
        assert!(!cells.is_null());
        cells
    }

    fn bind_render_state_rows(
        state: RoasttyRenderStateHandle,
        iterator: RoasttyRenderStateRowIterator,
    ) {
        let mut iterator_handle = iterator;
        assert_eq!(
            roastty_render_state_get(
                state,
                ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                &mut iterator_handle as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(iterator_handle, iterator);
    }

    fn bind_render_state_row_cells(
        iterator: RoasttyRenderStateRowIterator,
        cells: RoasttyRenderStateRowCells,
    ) {
        let mut cells_handle = cells;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                &mut cells_handle as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(cells_handle, cells);
    }

    fn render_state_get_u16(state: RoasttyRenderStateHandle, data: c_int) -> u16 {
        let mut out = u16::MAX;
        assert_eq!(
            roastty_render_state_get(state, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn render_state_get_i32(state: RoasttyRenderStateHandle, data: c_int) -> c_int {
        let mut out = -1;
        assert_eq!(
            roastty_render_state_get(state, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn render_state_get_bool(state: RoasttyRenderStateHandle, data: c_int) -> bool {
        let mut out = false;
        assert_eq!(
            roastty_render_state_get(state, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn render_state_get_rgb(state: RoasttyRenderStateHandle, data: c_int) -> RoasttyRgb {
        let mut out = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_get(state, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn render_state_get_palette(state: RoasttyRenderStateHandle) -> RoasttyPalette {
        let mut out = [RoasttyRgb::default(); 256];
        assert_eq!(
            roastty_render_state_get(
                state,
                ROASTTY_RENDER_STATE_DATA_COLOR_PALETTE,
                &mut out as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        out
    }

    fn packed_cell(tag: c_int, content: u64) -> RoasttyCell {
        (tag as u64) | (content << 2)
    }

    fn packed_cell_with_flags(
        mut cell: RoasttyCell,
        style_id: u16,
        wide: c_int,
        protected: bool,
        hyperlink: bool,
        semantic: c_int,
    ) -> RoasttyCell {
        cell |= (style_id as u64) << 26;
        cell |= (wide as u64) << 42;
        if protected {
            cell |= 1_u64 << 44;
        }
        if hyperlink {
            cell |= 1_u64 << 45;
        }
        cell | ((semantic as u64) << 46)
    }

    #[allow(clippy::too_many_arguments)]
    fn packed_row(
        wrap: bool,
        wrap_continuation: bool,
        grapheme: bool,
        styled: bool,
        hyperlink: bool,
        semantic_prompt: c_int,
        kitty_virtual_placeholder: bool,
        dirty: bool,
    ) -> RoasttyRow {
        let mut row = 0_u64;
        if wrap {
            row |= 1_u64 << 32;
        }
        if wrap_continuation {
            row |= 1_u64 << 33;
        }
        if grapheme {
            row |= 1_u64 << 34;
        }
        if styled {
            row |= 1_u64 << 35;
        }
        if hyperlink {
            row |= 1_u64 << 36;
        }
        row |= (semantic_prompt as u64) << 37;
        if kitty_virtual_placeholder {
            row |= 1_u64 << 39;
        }
        if dirty {
            row |= 1_u64 << 40;
        }
        row
    }

    fn cell_get_u32(cell: RoasttyCell, data: c_int) -> u32 {
        let mut out = 999_u32;
        assert_eq!(
            roastty_cell_get(cell, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn cell_get_i32(cell: RoasttyCell, data: c_int) -> c_int {
        let mut out = -1;
        assert_eq!(
            roastty_cell_get(cell, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn cell_get_bool(cell: RoasttyCell, data: c_int) -> bool {
        let mut out = false;
        assert_eq!(
            roastty_cell_get(cell, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn row_get_i32(row: RoasttyRow, data: c_int) -> c_int {
        let mut out = -1;
        assert_eq!(
            roastty_row_get(row, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn row_get_bool(row: RoasttyRow, data: c_int) -> bool {
        let mut out = false;
        assert_eq!(
            roastty_row_get(row, data, &mut out as *mut _ as *mut c_void),
            ROASTTY_SUCCESS
        );
        out
    }

    fn style_color_palette(color: RoasttyStyleColor) -> u8 {
        unsafe { color.value.palette }
    }

    fn style_color_rgb(color: RoasttyStyleColor) -> RoasttyRgb {
        unsafe { color.value.rgb }
    }

    fn default_c_style() -> RoasttyStyle {
        let mut style = RoasttyStyle {
            size: 0,
            fg_color: RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_RGB,
                value: RoasttyStyleColorValue { _padding: u64::MAX },
            },
            bg_color: RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_RGB,
                value: RoasttyStyleColorValue { _padding: u64::MAX },
            },
            underline_color: RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_RGB,
                value: RoasttyStyleColorValue { _padding: u64::MAX },
            },
            bold: true,
            italic: true,
            faint: true,
            blink: true,
            inverse: true,
            invisible: true,
            strikethrough: true,
            overline: true,
            underline: 5,
        };
        roastty_style_default(&mut style);
        style
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
        assert_eq!(ROASTTY_TERMINAL_OPTION_USERDATA, 0);
        assert_eq!(ROASTTY_TERMINAL_OPTION_WRITE_PTY, 1);
        assert_eq!(ROASTTY_TERMINAL_OPTION_BELL, 2);
        assert_eq!(ROASTTY_TERMINAL_OPTION_ENQUIRY, 3);
        assert_eq!(ROASTTY_TERMINAL_OPTION_XTVERSION, 4);
        assert_eq!(ROASTTY_TERMINAL_OPTION_TITLE_CHANGED, 5);
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
        for option in [
            ROASTTY_TERMINAL_OPTION_SIZE_CB,
            ROASTTY_TERMINAL_OPTION_COLOR_SCHEME,
            ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES,
        ] {
            assert_eq!(
                roastty_terminal_set(terminal, option, ptr::null()),
                ROASTTY_SUCCESS
            );
        }
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
    fn terminal_basic_effects_abi_write_pty_and_bell_callbacks() {
        reset_effect_state();
        let terminal = new_terminal(5, 3);
        let userdata = 0x1234usize as *const c_void;

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_USERDATA, userdata),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_WRITE_PTY,
                write_pty_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );

        write_terminal(terminal, b"\x1b[6n");
        with_effect_state(|state| {
            assert_eq!(state.last_terminal, terminal);
            assert_eq!(state.last_userdata, userdata.cast_mut());
            assert_eq!(state.write_calls, vec![b"\x1b[1;1R".to_vec()]);
        });
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[1;1R"
        );

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_WRITE_PTY, ptr::null()),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[6n");
        with_effect_state(|state| assert_eq!(state.write_calls.len(), 1));
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[1;1R"
        );

        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_BELL,
                bell_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        let before_bell_plain = terminal_plain_string(terminal);
        write_terminal(terminal, b"\x07");
        with_effect_state(|state| assert_eq!(state.bell_count, 1));
        assert_eq!(terminal_plain_string(terminal), before_bell_plain);
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_BELL, ptr::null()),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x07");
        with_effect_state(|state| assert_eq!(state.bell_count, 1));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_basic_effects_abi_enquiry_and_xtversion_callbacks() {
        reset_effect_state();
        let terminal = new_terminal(5, 3);
        let userdata = 0x5678usize as *const c_void;
        let enquiry = b"ENQ";
        let xtversion = b"roastty-test";
        let long_enquiry = [b'x'; 256];
        let long_xtversion = [b'y'; 257];

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_USERDATA, userdata),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_WRITE_PTY,
                write_pty_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_ENQUIRY,
                enquiry_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        with_effect_state(|state| {
            state.enquiry_ptr = enquiry.as_ptr().cast::<c_char>();
            state.enquiry_len = enquiry.len();
        });
        write_terminal(terminal, b"\x05");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            enquiry
        );
        with_effect_state(|state| {
            assert_eq!(state.last_terminal, terminal);
            assert_eq!(state.last_userdata, userdata.cast_mut());
            assert_eq!(state.write_calls.last().unwrap(), enquiry);
        });

        for (ptr, len) in [
            (ptr::null(), 1),
            (enquiry.as_ptr().cast::<c_char>(), 0),
            (long_enquiry.as_ptr().cast::<c_char>(), long_enquiry.len()),
        ] {
            with_effect_state(|state| {
                state.enquiry_ptr = ptr;
                state.enquiry_len = len;
            });
            write_terminal(terminal, b"\x05");
            assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());
        }

        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_XTVERSION,
                xtversion_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        with_effect_state(|state| {
            state.xtversion_ptr = xtversion.as_ptr().cast::<c_char>();
            state.xtversion_len = xtversion.len();
        });
        write_terminal(terminal, b"\x1b[>0q");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1bP>|roastty-test\x1b\\"
        );

        for (ptr, len) in [
            (ptr::null(), 1),
            (xtversion.as_ptr().cast::<c_char>(), 0),
            (
                long_xtversion.as_ptr().cast::<c_char>(),
                long_xtversion.len(),
            ),
        ] {
            with_effect_state(|state| {
                state.xtversion_ptr = ptr;
                state.xtversion_len = len;
            });
            write_terminal(terminal, b"\x1b[>0q");
            assert_eq!(
                terminal_string(terminal, roastty_terminal_take_pty_response),
                b"\x1bP>|libroastty\x1b\\"
            );
        }

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_basic_effects_abi_title_changed_only_fires_for_stream_title() {
        reset_effect_state();
        let terminal = new_terminal(8, 3);
        let userdata = 0xabcdusize as *const c_void;
        let direct_title = borrowed_roastty_string(b"direct");

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_USERDATA, userdata),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE_CHANGED,
                title_changed_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &direct_title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        with_effect_state(|state| assert_eq!(state.title_changed_count, 0));

        write_terminal(terminal, b"\x1b]0;stream\x07");
        assert_eq!(terminal_string(terminal, roastty_terminal_title), b"stream");
        with_effect_state(|state| {
            assert_eq!(state.title_changed_count, 1);
            assert_eq!(state.last_terminal, terminal);
            assert_eq!(state.last_userdata, userdata.cast_mut());
        });

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_TITLE_CHANGED, ptr::null(),),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b]2;stream 2\x07");
        with_effect_state(|state| assert_eq!(state.title_changed_count, 1));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn build_info_c_abi_returns_static_values() {
        assert_eq!(ROASTTY_OPTIMIZE_DEBUG, 0);
        assert_eq!(ROASTTY_OPTIMIZE_RELEASE_SAFE, 1);
        assert_eq!(ROASTTY_OPTIMIZE_RELEASE_SMALL, 2);
        assert_eq!(ROASTTY_OPTIMIZE_RELEASE_FAST, 3);
        assert_eq!(ROASTTY_BUILD_INFO_INVALID, 0);
        assert_eq!(ROASTTY_BUILD_INFO_SIMD, 1);
        assert_eq!(ROASTTY_BUILD_INFO_KITTY_GRAPHICS, 2);
        assert_eq!(ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE, 3);
        assert_eq!(ROASTTY_BUILD_INFO_OPTIMIZE, 4);
        assert_eq!(ROASTTY_BUILD_INFO_VERSION_STRING, 5);
        assert_eq!(ROASTTY_BUILD_INFO_VERSION_MAJOR, 6);
        assert_eq!(ROASTTY_BUILD_INFO_VERSION_MINOR, 7);
        assert_eq!(ROASTTY_BUILD_INFO_VERSION_PATCH, 8);
        assert_eq!(ROASTTY_BUILD_INFO_VERSION_PRE, 9);
        assert_eq!(ROASTTY_BUILD_INFO_VERSION_BUILD, 10);

        let mut simd = true;
        assert_eq!(
            roastty_build_info(ROASTTY_BUILD_INFO_SIMD, (&mut simd as *mut bool).cast()),
            ROASTTY_SUCCESS
        );
        assert!(!simd);

        let mut kitty_graphics = true;
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_KITTY_GRAPHICS,
                (&mut kitty_graphics as *mut bool).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert!(!kitty_graphics);
        let mut tmux_control_mode = true;
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE,
                (&mut tmux_control_mode as *mut bool).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert!(!tmux_control_mode);

        let mut optimize = -1;
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_OPTIMIZE,
                (&mut optimize as *mut c_int).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(optimize, current_optimize_mode());

        let mut version = RoasttyString {
            ptr: ptr::null(),
            len: 0,
            sentinel: true,
        };
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_VERSION_STRING,
                (&mut version as *mut RoasttyString).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(version.ptr, VERSION.as_ptr().cast::<c_char>());
        assert_eq!(version.len, VERSION.len() - 1);
        assert!(!version.sentinel);

        let mut major = usize::MAX;
        let mut minor = usize::MAX;
        let mut patch = usize::MAX;
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_VERSION_MAJOR,
                (&mut major as *mut usize).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_VERSION_MINOR,
                (&mut minor as *mut usize).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_VERSION_PATCH,
                (&mut patch as *mut usize).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((major, minor, patch), (0, 1, 0));

        let mut pre = RoasttyString {
            ptr: ptr::null(),
            len: 0,
            sentinel: true,
        };
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_VERSION_PRE,
                (&mut pre as *mut RoasttyString).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(pre.len, 0);
        assert!(!pre.ptr.is_null());
        let mut build = RoasttyString {
            ptr: ptr::null(),
            len: 0,
            sentinel: true,
        };
        assert_eq!(
            roastty_build_info(
                ROASTTY_BUILD_INFO_VERSION_BUILD,
                (&mut build as *mut RoasttyString).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(build.len, 0);
        assert!(!build.ptr.is_null());

        assert_eq!(
            roastty_build_info(ROASTTY_BUILD_INFO_INVALID, (&mut simd as *mut bool).cast()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_build_info(99, (&mut simd as *mut bool).cast()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_build_info(ROASTTY_BUILD_INFO_SIMD, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
    }

    static CUSTOM_ALLOC_CALLED: AtomicUsize = AtomicUsize::new(0);
    static CUSTOM_FREE_CALLED: AtomicUsize = AtomicUsize::new(0);
    static CUSTOM_LAST_CTX: AtomicUsize = AtomicUsize::new(0);
    static CUSTOM_LAST_LEN: AtomicUsize = AtomicUsize::new(0);
    static CUSTOM_LAST_ALIGNMENT: AtomicU8 = AtomicU8::new(0);

    unsafe extern "C" fn custom_alloc(
        ctx: *mut c_void,
        len: usize,
        alignment: u8,
        ret_addr: usize,
    ) -> *mut u8 {
        assert_eq!(ret_addr, 0);
        CUSTOM_ALLOC_CALLED.fetch_add(1, Ordering::SeqCst);
        CUSTOM_LAST_CTX.store(ctx as usize, Ordering::SeqCst);
        CUSTOM_LAST_LEN.store(len, Ordering::SeqCst);
        CUSTOM_LAST_ALIGNMENT.store(alignment, Ordering::SeqCst);
        roastty_alloc(ptr::null(), len)
    }

    unsafe extern "C" fn custom_free(
        ctx: *mut c_void,
        memory: *mut c_void,
        memory_len: usize,
        alignment: u8,
        ret_addr: usize,
    ) {
        assert_eq!(ret_addr, 0);
        CUSTOM_FREE_CALLED.fetch_add(1, Ordering::SeqCst);
        CUSTOM_LAST_CTX.store(ctx as usize, Ordering::SeqCst);
        CUSTOM_LAST_LEN.store(memory_len, Ordering::SeqCst);
        CUSTOM_LAST_ALIGNMENT.store(alignment, Ordering::SeqCst);
        roastty_free(ptr::null(), memory.cast::<u8>(), memory_len);
    }

    #[test]
    fn support_allocator_c_abi_allocates_and_frees() {
        let ptr = roastty_alloc(ptr::null(), 8);
        assert!(!ptr.is_null());
        unsafe {
            ptr.write_bytes(0xab, 8);
        }
        roastty_free(ptr::null(), ptr, 8);
        assert!(roastty_alloc(ptr::null(), 0).is_null());
        roastty_free(ptr::null(), ptr::null_mut(), 8);

        CUSTOM_ALLOC_CALLED.store(0, Ordering::SeqCst);
        CUSTOM_FREE_CALLED.store(0, Ordering::SeqCst);
        CUSTOM_LAST_CTX.store(0, Ordering::SeqCst);
        CUSTOM_LAST_LEN.store(0, Ordering::SeqCst);
        CUSTOM_LAST_ALIGNMENT.store(0, Ordering::SeqCst);

        let vtable = RoasttyAllocatorVtable {
            alloc: Some(custom_alloc),
            resize: None,
            remap: None,
            free: Some(custom_free),
        };
        let allocator = RoasttyAllocator {
            ctx: 0xfeedusize as *mut c_void,
            vtable: &vtable,
        };
        let ptr = roastty_alloc(&allocator, 11);
        assert!(!ptr.is_null());
        assert_eq!(CUSTOM_ALLOC_CALLED.load(Ordering::SeqCst), 1);
        assert_eq!(CUSTOM_LAST_CTX.load(Ordering::SeqCst), 0xfeed);
        assert_eq!(CUSTOM_LAST_LEN.load(Ordering::SeqCst), 11);
        assert_eq!(CUSTOM_LAST_ALIGNMENT.load(Ordering::SeqCst), 1);
        roastty_free(&allocator, ptr, 11);
        assert_eq!(CUSTOM_FREE_CALLED.load(Ordering::SeqCst), 1);

        assert!(roastty_alloc(&allocator, 0).is_null());
        assert_eq!(CUSTOM_ALLOC_CALLED.load(Ordering::SeqCst), 1);
        roastty_free(&allocator, 0x1usize as *mut u8, 0);
        assert_eq!(CUSTOM_FREE_CALLED.load(Ordering::SeqCst), 1);

        let malformed = RoasttyAllocator {
            ctx: ptr::null_mut(),
            vtable: ptr::null(),
        };
        assert!(roastty_alloc(&malformed, 1).is_null());
        roastty_free(&malformed, 0x1usize as *mut u8, 1);

        let no_alloc = RoasttyAllocatorVtable {
            alloc: None,
            resize: None,
            remap: None,
            free: None,
        };
        let malformed = RoasttyAllocator {
            ctx: ptr::null_mut(),
            vtable: &no_alloc,
        };
        assert!(roastty_alloc(&malformed, 1).is_null());

        let no_free = RoasttyAllocatorVtable {
            alloc: Some(custom_alloc),
            resize: None,
            remap: None,
            free: None,
        };
        let malformed = RoasttyAllocator {
            ctx: ptr::null_mut(),
            vtable: &no_free,
        };
        roastty_free(&malformed, 0x1usize as *mut u8, 1);
    }

    static SYS_LOG_CALLED: AtomicBool = AtomicBool::new(false);
    static SYS_LOG_USERDATA: AtomicUsize = AtomicUsize::new(0);
    static SYS_DECODE_CALLED: AtomicBool = AtomicBool::new(false);
    static SYS_DECODE_USERDATA: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn sys_log_cb(
        userdata: *mut c_void,
        _level: c_int,
        _scope: *const u8,
        _scope_len: usize,
        _message: *const u8,
        _message_len: usize,
    ) {
        SYS_LOG_CALLED.store(true, Ordering::SeqCst);
        SYS_LOG_USERDATA.store(userdata as usize, Ordering::SeqCst);
    }

    unsafe extern "C" fn sys_decode_cb(
        userdata: *mut c_void,
        _allocator: *const RoasttyAllocator,
        _data: *const u8,
        _data_len: usize,
        out: *mut RoasttySysImage,
    ) -> bool {
        SYS_DECODE_CALLED.store(true, Ordering::SeqCst);
        SYS_DECODE_USERDATA.store(userdata as usize, Ordering::SeqCst);
        if !out.is_null() {
            out.write(RoasttySysImage {
                width: 0,
                height: 0,
                data: ptr::null_mut(),
                data_len: 0,
            });
        }
        true
    }

    #[test]
    fn sys_c_abi_sets_callbacks_and_userdata() {
        assert_eq!(ROASTTY_SYS_LOG_LEVEL_ERROR, 0);
        assert_eq!(ROASTTY_SYS_LOG_LEVEL_WARNING, 1);
        assert_eq!(ROASTTY_SYS_LOG_LEVEL_INFO, 2);
        assert_eq!(ROASTTY_SYS_LOG_LEVEL_DEBUG, 3);
        assert_eq!(ROASTTY_SYS_OPT_USERDATA, 0);
        assert_eq!(ROASTTY_SYS_OPT_DECODE_PNG, 1);
        assert_eq!(ROASTTY_SYS_OPT_LOG, 2);

        SYS_LOG_CALLED.store(false, Ordering::SeqCst);
        SYS_DECODE_CALLED.store(false, Ordering::SeqCst);
        assert_eq!(
            roastty_sys_set(ROASTTY_SYS_OPT_USERDATA, 0xbeefusize as *const c_void),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_sys_set(ROASTTY_SYS_OPT_LOG, sys_log_cb as *const c_void),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_sys_set(ROASTTY_SYS_OPT_DECODE_PNG, sys_decode_cb as *const c_void),
            ROASTTY_SUCCESS
        );

        {
            let state = SYS_STATE.lock().unwrap();
            assert_eq!(state.userdata, 0xbeef);
            assert!(state.log.is_some());
            assert!(state.decode_png.is_some());
            unsafe {
                (state.log.unwrap())(
                    state.userdata as *mut c_void,
                    ROASTTY_SYS_LOG_LEVEL_INFO,
                    ptr::null(),
                    0,
                    ptr::null(),
                    0,
                );
                let mut image = RoasttySysImage {
                    width: 1,
                    height: 1,
                    data: 0x1usize as *mut u8,
                    data_len: 1,
                };
                assert!((state.decode_png.unwrap())(
                    state.userdata as *mut c_void,
                    ptr::null(),
                    ptr::null(),
                    0,
                    &mut image,
                ));
                assert_eq!(image.width, 0);
                assert_eq!(image.data_len, 0);
            }
        }
        assert!(SYS_LOG_CALLED.load(Ordering::SeqCst));
        assert_eq!(SYS_LOG_USERDATA.load(Ordering::SeqCst), 0xbeef);
        assert!(SYS_DECODE_CALLED.load(Ordering::SeqCst));
        assert_eq!(SYS_DECODE_USERDATA.load(Ordering::SeqCst), 0xbeef);

        assert_eq!(
            roastty_sys_set(ROASTTY_SYS_OPT_LOG, ptr::null()),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_sys_set(ROASTTY_SYS_OPT_DECODE_PNG, ptr::null()),
            ROASTTY_SUCCESS
        );
        assert_eq!(roastty_sys_set(99, ptr::null()), ROASTTY_INVALID_VALUE);
        {
            let state = SYS_STATE.lock().unwrap();
            assert!(state.log.is_none());
            assert!(state.decode_png.is_none());
        }

        roastty_sys_log_stderr(
            ptr::null_mut(),
            99,
            b"scope".as_ptr(),
            5,
            b"message".as_ptr(),
            7,
        );
    }

    #[test]
    fn size_report_encoder_abi_matches_upstream_layout() {
        let size = RoasttySizeReportSize {
            rows: 24,
            columns: 80,
            cell_width: 9,
            cell_height: 18,
        };
        let mut buf = [0i8; 64];
        let mut written = usize::MAX;

        assert_eq!(
            roastty_size_report_encode(
                ROASTTY_SIZE_REPORT_MODE_2048,
                size,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, b"\x1b[48;24;80;432;720t".len());
        assert_eq!(
            unsafe { slice::from_raw_parts(buf.as_ptr().cast::<u8>(), written) },
            b"\x1b[48;24;80;432;720t"
        );

        for (style, expected) in [
            (ROASTTY_SIZE_REPORT_CSI_14_T, b"\x1b[4;432;720t".as_slice()),
            (ROASTTY_SIZE_REPORT_CSI_16_T, b"\x1b[6;18;9t".as_slice()),
            (ROASTTY_SIZE_REPORT_CSI_18_T, b"\x1b[8;24;80t".as_slice()),
        ] {
            written = usize::MAX;
            assert_eq!(
                roastty_size_report_encode(style, size, buf.as_mut_ptr(), buf.len(), &mut written),
                ROASTTY_SUCCESS
            );
            assert_eq!(
                unsafe { slice::from_raw_parts(buf.as_ptr().cast::<u8>(), written) },
                expected
            );
        }

        written = usize::MAX;
        assert_eq!(
            roastty_size_report_encode(
                ROASTTY_SIZE_REPORT_CSI_18_T,
                size,
                ptr::null_mut(),
                0,
                &mut written,
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, b"\x1b[8;24;80t".len());

        written = usize::MAX;
        assert_eq!(
            roastty_size_report_encode(
                ROASTTY_SIZE_REPORT_CSI_18_T,
                size,
                buf.as_mut_ptr(),
                1,
                &mut written,
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, b"\x1b[8;24;80t".len());

        assert_eq!(
            roastty_size_report_encode(
                ROASTTY_SIZE_REPORT_CSI_18_T,
                size,
                buf.as_mut_ptr(),
                buf.len(),
                ptr::null_mut(),
            ),
            ROASTTY_INVALID_VALUE
        );

        written = usize::MAX;
        assert_eq!(
            roastty_size_report_encode(99, size, buf.as_mut_ptr(), buf.len(), &mut written),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 0);
    }

    #[test]
    fn focus_encoder_abi_matches_upstream_layout() {
        let mut buf = [0u8; 8];
        let mut written = usize::MAX;

        assert_eq!(
            roastty_focus_encode(
                ROASTTY_FOCUS_EVENT_GAINED,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"\x1b[I");

        assert_eq!(
            roastty_focus_encode(
                ROASTTY_FOCUS_EVENT_LOST,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"\x1b[O");

        assert_eq!(
            roastty_focus_encode(
                ROASTTY_FOCUS_EVENT_GAINED,
                ptr::null_mut(),
                99,
                &mut written
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, focus::MAX_ENCODE_SIZE);

        assert_eq!(
            roastty_focus_encode(99, buf.as_mut_ptr(), buf.len(), &mut written),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_focus_encode(
                ROASTTY_FOCUS_EVENT_GAINED,
                buf.as_mut_ptr(),
                buf.len(),
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
    }

    #[test]
    fn paste_c_abi_checks_safety_and_encodes_with_mutation() {
        assert!(roastty_paste_is_safe(b"hello".as_ptr(), 5));
        assert!(roastty_paste_is_safe(ptr::null(), 0));
        assert!(roastty_paste_is_safe(ptr::null(), 99));
        assert!(!roastty_paste_is_safe(b"hello\n".as_ptr(), 6));
        assert!(!roastty_paste_is_safe(b"he\x1b[201~llo".as_ptr(), 10));

        let mut buf = [0u8; 64];
        let mut written = usize::MAX;
        let mut data = b"hello".to_vec();
        assert_eq!(
            roastty_paste_encode(
                data.as_mut_ptr(),
                data.len(),
                true,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"\x1b[200~hello\x1b[201~");
        assert_eq!(data, b"hello");

        let mut data = b"hello\nworld".to_vec();
        assert_eq!(
            roastty_paste_encode(
                data.as_mut_ptr(),
                data.len(),
                false,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"hello\rworld");
        assert_eq!(data, b"hello\rworld");

        let mut data = b"hel\x1blo\x00wor\x03ld\x7f".to_vec();
        assert_eq!(
            roastty_paste_encode(
                data.as_mut_ptr(),
                data.len(),
                true,
                ptr::null_mut(),
                99,
                &mut written
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(data, b"hel lo wor ld ");
        assert_eq!(written, b"\x1b[200~hel lo wor ld \x1b[201~".len());

        let mut data = b"hello\nworld".to_vec();
        assert_eq!(
            roastty_paste_encode(
                data.as_mut_ptr(),
                data.len(),
                false,
                buf.as_mut_ptr(),
                1,
                &mut written
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(data, b"hello\rworld");
        assert_eq!(written, b"hello\rworld".len());

        assert_eq!(
            roastty_paste_encode(ptr::null_mut(), 99, false, ptr::null_mut(), 0, &mut written),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_paste_encode(ptr::null_mut(), 99, true, ptr::null_mut(), 0, &mut written),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, b"\x1b[200~\x1b[201~".len());
        assert_eq!(
            roastty_paste_encode(
                ptr::null_mut(),
                0,
                false,
                ptr::null_mut(),
                0,
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
    }

    #[test]
    fn mode_report_encoder_abi_decodes_packed_tags() {
        let mut buf = [0u8; 32];
        let mut written = usize::MAX;

        assert_eq!(
            roastty_mode_report_encode(
                dec_mode_tag(1),
                ROASTTY_MODE_REPORT_SET,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"\x1b[?1;1$y");

        assert_eq!(
            roastty_mode_report_encode(
                ansi_mode_tag(4),
                ROASTTY_MODE_REPORT_RESET,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"\x1b[4;2$y");

        assert_eq!(
            roastty_mode_report_encode(
                dec_mode_tag(9999),
                ROASTTY_MODE_REPORT_NOT_RECOGNIZED,
                ptr::null_mut(),
                42,
                &mut written
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, b"\x1b[?9999;0$y".len());

        assert_eq!(
            roastty_mode_report_encode(
                dec_mode_tag(ROASTTY_MODE_TAG_VALUE_MASK),
                ROASTTY_MODE_REPORT_PERMANENTLY_RESET,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"\x1b[?32767;4$y");

        assert_eq!(
            roastty_mode_report_encode(
                dec_mode_tag(1),
                99,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_mode_report_encode(
                dec_mode_tag(1),
                ROASTTY_MODE_REPORT_SET,
                buf.as_mut_ptr(),
                buf.len(),
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
    }

    #[test]
    fn terminal_query_callbacks_abi_option_values_and_size_reports() {
        reset_effect_state();
        let terminal = new_terminal(8, 4);
        let userdata = 0x9876usize as *const c_void;

        assert_eq!(ROASTTY_TERMINAL_OPTION_SIZE_CB, 6);
        assert_eq!(ROASTTY_TERMINAL_OPTION_COLOR_SCHEME, 7);
        assert_eq!(ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES, 8);
        assert_eq!(ROASTTY_COLOR_SCHEME_LIGHT, 0);
        assert_eq!(ROASTTY_COLOR_SCHEME_DARK, 1);

        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());
        write_terminal(terminal, b"\x1b[14t\x1b[16t\x1b[18t");
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());
        with_effect_state(|state| assert_eq!(state.size_count, 0));

        with_effect_state(|state| {
            state.size_response = size_report::Size {
                rows: 24,
                columns: 80,
                cell_width: 9,
                cell_height: 18,
            };
            state.size_result = true;
        });
        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_USERDATA, userdata),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_WRITE_PTY,
                write_pty_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SIZE_CB,
                size_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[14t\x1b[16t\x1b[18t");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[4;432;720t\x1b[6;18;9t\x1b[8;24;80t"
        );
        with_effect_state(|state| {
            assert_eq!(state.size_count, 3);
            assert_eq!(state.last_terminal, terminal);
            assert_eq!(state.last_userdata, userdata.cast_mut());
            assert_eq!(state.write_calls.len(), 3);
            assert_eq!(state.write_calls[0], b"\x1b[4;432;720t");
            assert_eq!(state.write_calls[1], b"\x1b[6;18;9t");
            assert_eq!(state.write_calls[2], b"\x1b[8;24;80t");
            state.size_result = false;
        });
        write_terminal(terminal, b"\x1b[14t");
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());
        with_effect_state(|state| assert_eq!(state.size_count, 4));

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_SIZE_CB, ptr::null()),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[14t");
        with_effect_state(|state| assert_eq!(state.size_count, 4));

        let title = borrowed_roastty_string(b"query title");
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_TITLE,
                &title as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[21t");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b]lquery title\x1b\\"
        );
        with_effect_state(|state| assert_eq!(state.size_count, 4));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_query_callbacks_abi_color_scheme() {
        reset_effect_state();
        let terminal = new_terminal(5, 3);
        let userdata = 0x2468usize as *const c_void;

        write_terminal(terminal, b"\x1b[?996n");
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_USERDATA, userdata),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_WRITE_PTY,
                write_pty_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_SCHEME,
                color_scheme_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );

        with_effect_state(|state| {
            state.color_scheme_response = ROASTTY_COLOR_SCHEME_DARK;
            state.color_scheme_result = true;
        });
        write_terminal(terminal, b"\x1b[?996n");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[?997;1n"
        );
        with_effect_state(|state| {
            assert_eq!(state.color_scheme_count, 1);
            assert_eq!(state.last_terminal, terminal);
            assert_eq!(state.last_userdata, userdata.cast_mut());
            assert_eq!(state.write_calls.last().unwrap(), b"\x1b[?997;1n");
            state.color_scheme_response = ROASTTY_COLOR_SCHEME_LIGHT;
        });
        write_terminal(terminal, b"\x1b[?996n");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[?997;2n"
        );

        with_effect_state(|state| {
            state.color_scheme_response = 99;
            state.color_scheme_result = true;
        });
        write_terminal(terminal, b"\x1b[?996n");
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());

        with_effect_state(|state| {
            state.color_scheme_response = ROASTTY_COLOR_SCHEME_DARK;
            state.color_scheme_result = false;
        });
        write_terminal(terminal, b"\x1b[?996n");
        assert!(terminal_string(terminal, roastty_terminal_take_pty_response).is_empty());

        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_COLOR_SCHEME, ptr::null(),),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[?996n");
        with_effect_state(|state| assert_eq!(state.color_scheme_count, 4));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_query_callbacks_abi_device_attributes() {
        reset_effect_state();
        let terminal = new_terminal(5, 3);

        write_terminal(terminal, b"\x1b[c\x1b[>c\x1b[=c");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[?62;22c\x1b[>1;0;0c\x1bP!|00000000\x1b\\"
        );

        let mut features = [0u16; 64];
        features[0] = 444;
        features[1] = 555;
        features[2] = 666;
        with_effect_state(|state| {
            state.device_attributes_response = TerminalDeviceAttributes {
                primary: TerminalDeviceAttributesPrimary {
                    conformance_level: 777,
                    features,
                    num_features: 3,
                },
                secondary: TerminalDeviceAttributesSecondary {
                    device_type: 888,
                    firmware_version: 99,
                    rom_cartridge: 7,
                },
                tertiary: TerminalDeviceAttributesTertiary {
                    unit_id: 0xAABBCCDD,
                },
            };
            state.device_attributes_result = true;
        });
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES,
                device_attributes_cb as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[c\x1b[>c\x1b[=c");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[?777;444;555;666c\x1b[>888;99;7c\x1bP!|AABBCCDD\x1b\\"
        );
        with_effect_state(|state| assert_eq!(state.device_attributes_count, 3));

        let all_features = [42u16; 64];
        with_effect_state(|state| {
            state.device_attributes_response.primary.features = all_features;
            state.device_attributes_response.primary.num_features = 1000;
        });
        write_terminal(terminal, b"\x1b[c");
        let response = terminal_string(terminal, roastty_terminal_take_pty_response);
        assert!(response.starts_with(b"\x1b[?777;42;42"));
        assert!(response.ends_with(b"c"));
        assert_eq!(response.iter().filter(|byte| **byte == b';').count(), 64);

        with_effect_state(|state| state.device_attributes_result = false);
        write_terminal(terminal, b"\x1b[c\x1b[>c\x1b[=c");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[?62;22c\x1b[>1;0;0c\x1bP!|00000000\x1b\\"
        );

        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES,
                ptr::null(),
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[c");
        assert_eq!(
            terminal_string(terminal, roastty_terminal_take_pty_response),
            b"\x1b[?62;22c"
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn style_c_abi_layout_and_values_are_stable() {
        assert_eq!(ROASTTY_STYLE_COLOR_NONE, 0);
        assert_eq!(ROASTTY_STYLE_COLOR_PALETTE, 1);
        assert_eq!(ROASTTY_STYLE_COLOR_RGB, 2);

        assert_eq!(std::mem::size_of::<RoasttyStyleColorValue>(), 8);
        assert_eq!(std::mem::align_of::<RoasttyStyleColorValue>(), 8);
        assert_eq!(std::mem::size_of::<RoasttyStyleColor>(), 16);
        assert_eq!(std::mem::align_of::<RoasttyStyleColor>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyStyleColor, tag), 0);
        assert_eq!(std::mem::offset_of!(RoasttyStyleColor, value), 8);
        assert_eq!(std::mem::size_of::<RoasttyStyle>(), 72);
        assert_eq!(std::mem::align_of::<RoasttyStyle>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, size), 0);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, fg_color), 8);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, bg_color), 24);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, underline_color), 40);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, bold), 56);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, italic), 57);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, faint), 58);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, blink), 59);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, inverse), 60);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, invisible), 61);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, strikethrough), 62);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, overline), 63);
        assert_eq!(std::mem::offset_of!(RoasttyStyle, underline), 64);

        assert_eq!(std::mem::size_of::<RoasttyBuffer>(), 24);
        assert_eq!(std::mem::align_of::<RoasttyBuffer>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyBuffer, ptr), 0);
        assert_eq!(std::mem::offset_of!(RoasttyBuffer, cap), 8);
        assert_eq!(std::mem::offset_of!(RoasttyBuffer, len), 16);

        assert_eq!(underline_to_int(sgr::Underline::None), 0);
        assert_eq!(underline_to_int(sgr::Underline::Single), 1);
        assert_eq!(underline_to_int(sgr::Underline::Double), 2);
        assert_eq!(underline_to_int(sgr::Underline::Curly), 3);
        assert_eq!(underline_to_int(sgr::Underline::Dotted), 4);
        assert_eq!(underline_to_int(sgr::Underline::Dashed), 5);
    }

    #[test]
    fn style_c_abi_default_and_non_default_conversion() {
        let mut out = RoasttyStyle {
            size: 0,
            fg_color: RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_RGB,
                value: RoasttyStyleColorValue { _padding: u64::MAX },
            },
            bg_color: RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_RGB,
                value: RoasttyStyleColorValue { _padding: u64::MAX },
            },
            underline_color: RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_RGB,
                value: RoasttyStyleColorValue { _padding: u64::MAX },
            },
            bold: true,
            italic: true,
            faint: true,
            blink: true,
            inverse: true,
            invisible: true,
            strikethrough: true,
            overline: true,
            underline: 5,
        };
        roastty_style_default(&mut out);
        assert_eq!(out.size, std::mem::size_of::<RoasttyStyle>());
        assert_eq!(out.fg_color.tag, ROASTTY_STYLE_COLOR_NONE);
        assert_eq!(out.bg_color.tag, ROASTTY_STYLE_COLOR_NONE);
        assert_eq!(out.underline_color.tag, ROASTTY_STYLE_COLOR_NONE);
        assert!(!out.bold);
        assert!(!out.italic);
        assert!(!out.faint);
        assert!(!out.blink);
        assert!(!out.inverse);
        assert!(!out.invisible);
        assert!(!out.strikethrough);
        assert!(!out.overline);
        assert_eq!(out.underline, 0);
        assert!(roastty_style_is_default(&out));

        let converted = style_to_c(style::Style {
            fg_color: style::Color::Palette(42),
            bg_color: style::Color::Rgb(color::Rgb::new(1, 2, 3)),
            underline_color: style::Color::Rgb(color::Rgb::new(4, 5, 6)),
            flags: style::Flags {
                bold: true,
                italic: true,
                faint: true,
                blink: true,
                inverse: true,
                invisible: true,
                strikethrough: true,
                overline: true,
                underline: sgr::Underline::Curly,
            },
        });
        assert_eq!(converted.size, std::mem::size_of::<RoasttyStyle>());
        assert_eq!(converted.fg_color.tag, ROASTTY_STYLE_COLOR_PALETTE);
        assert_eq!(style_color_palette(converted.fg_color), 42);
        assert_eq!(converted.bg_color.tag, ROASTTY_STYLE_COLOR_RGB);
        assert_eq!(
            style_color_rgb(converted.bg_color),
            RoasttyRgb { r: 1, g: 2, b: 3 }
        );
        assert_eq!(converted.underline_color.tag, ROASTTY_STYLE_COLOR_RGB);
        assert_eq!(
            style_color_rgb(converted.underline_color),
            RoasttyRgb { r: 4, g: 5, b: 6 }
        );
        assert!(converted.bold);
        assert!(converted.italic);
        assert!(converted.faint);
        assert!(converted.blink);
        assert!(converted.inverse);
        assert!(converted.invisible);
        assert!(converted.strikethrough);
        assert!(converted.overline);
        assert_eq!(converted.underline, 3);
        assert!(!roastty_style_is_default(&converted));
    }

    #[test]
    fn style_c_abi_default_checks_validate_null_size_and_fields() {
        roastty_style_default(ptr::null_mut());
        assert!(!roastty_style_is_default(ptr::null()));

        let mut style = style_to_c(style::Style::default());
        assert!(roastty_style_is_default(&style));
        style.size -= 1;
        assert!(!roastty_style_is_default(&style));

        let mut cases = Vec::new();
        macro_rules! push_case {
            ($field:ident, $value:expr) => {{
                let mut value = style_to_c(style::Style::default());
                value.$field = $value;
                cases.push(value);
            }};
        }
        push_case!(
            fg_color,
            RoasttyStyleColor {
                tag: ROASTTY_STYLE_COLOR_PALETTE,
                value: RoasttyStyleColorValue { palette: 1 }
            }
        );
        push_case!(bold, true);
        push_case!(italic, true);
        push_case!(faint, true);
        push_case!(blink, true);
        push_case!(inverse, true);
        push_case!(invisible, true);
        push_case!(strikethrough, true);
        push_case!(overline, true);
        push_case!(underline, 1);

        for case in cases {
            assert!(!roastty_style_is_default(&case));
        }
    }

    #[test]
    fn render_state_c_abi_layout_values_and_pre_update_defaults() {
        assert_eq!(ROASTTY_RENDER_STATE_DIRTY_FALSE, 0);
        assert_eq!(ROASTTY_RENDER_STATE_DIRTY_PARTIAL, 1);
        assert_eq!(ROASTTY_RENDER_STATE_DIRTY_FULL, 2);
        assert_eq!(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR, 0);
        assert_eq!(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK, 1);
        assert_eq!(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE, 2);
        assert_eq!(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW, 3);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_INVALID, 0);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_COLS, 1);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_ROWS, 2);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_DIRTY, 3);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR, 4);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND, 5);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND, 6);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR, 7);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE, 8);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_COLOR_PALETTE, 9);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE, 10);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE, 11);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING, 12);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT, 13);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE, 14);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X, 15);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y, 16);
        assert_eq!(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL, 17);
        assert_eq!(ROASTTY_RENDER_STATE_OPTION_DIRTY, 0);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_DATA_INVALID, 0);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_DATA_DIRTY, 1);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_DATA_RAW, 2);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_DATA_CELLS, 3);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_DATA_SELECTION, 4);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY, 0);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID, 0);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW, 1);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, 2);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN, 3);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF, 4);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR, 5);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR, 6);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED, 7);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING, 8);
        assert_eq!(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8, 9);

        assert_eq!(std::mem::size_of::<RoasttyRenderStateColors>(), 792);
        assert_eq!(std::mem::align_of::<RoasttyRenderStateColors>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyRenderStateColors, size), 0);
        assert_eq!(
            std::mem::offset_of!(RoasttyRenderStateColors, background),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyRenderStateColors, foreground),
            11
        );
        assert_eq!(std::mem::offset_of!(RoasttyRenderStateColors, cursor), 14);
        assert_eq!(
            std::mem::offset_of!(RoasttyRenderStateColors, cursor_has_value),
            17
        );
        assert_eq!(std::mem::offset_of!(RoasttyRenderStateColors, palette), 18);
        assert_eq!(std::mem::size_of::<RoasttyRenderStateRowSelection>(), 16);
        assert_eq!(std::mem::align_of::<RoasttyRenderStateRowSelection>(), 8);
        assert_eq!(
            std::mem::offset_of!(RoasttyRenderStateRowSelection, size),
            0
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyRenderStateRowSelection, start_x),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyRenderStateRowSelection, end_x),
            10
        );

        assert_eq!(
            roastty_render_state_new(ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        let state = new_render_state();
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(!roastty_render_state_row_iterator_next(iterator));
        assert_eq!(
            render_state_get_u16(state, ROASTTY_RENDER_STATE_DATA_COLS),
            0
        );
        assert_eq!(
            render_state_get_u16(state, ROASTTY_RENDER_STATE_DATA_ROWS),
            0
        );
        assert_eq!(
            render_state_get_i32(state, ROASTTY_RENDER_STATE_DATA_DIRTY),
            ROASTTY_RENDER_STATE_DIRTY_FALSE
        );
        assert_eq!(
            render_state_get_rgb(state, ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND),
            RoasttyRgb { r: 0, g: 0, b: 0 }
        );
        assert_eq!(
            render_state_get_rgb(state, ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND),
            RoasttyRgb {
                r: 0xff,
                g: 0xff,
                b: 0xff
            }
        );
        let mut rgb = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_get(
                state,
                ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR,
                &mut rgb as *mut _ as *mut c_void,
            ),
            ROASTTY_NO_VALUE
        );
        assert!(!render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE
        ));
        assert_eq!(
            render_state_get_i32(state, ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE),
            ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK
        );
        assert!(render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE
        ));
        assert!(!render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING
        ));
        assert!(!render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT
        ));
        assert!(!render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE
        ));
        assert_eq!(
            roastty_render_state_get(
                state,
                ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
                &mut rgb as *mut _ as *mut c_void,
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            render_state_get_palette(state),
            palette_from_color(color::DEFAULT_PALETTE)
        );

        roastty_render_state_free(state);
        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(ptr::null_mut());
        roastty_render_state_row_iterator_free(ptr::null_mut());
    }

    #[test]
    fn render_state_c_abi_update_snapshots_terminal_scalars() {
        let terminal = new_terminal(80, 24);
        let foreground = RoasttyRgb { r: 1, g: 2, b: 3 };
        let background = RoasttyRgb { r: 4, g: 5, b: 6 };
        let cursor = RoasttyRgb { r: 7, g: 8, b: 9 };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND,
                &foreground as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND,
                &background as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_CURSOR,
                &cursor as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[3 q");

        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(ptr::null_mut(), terminal),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_update(state, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );

        assert_eq!(
            render_state_get_u16(state, ROASTTY_RENDER_STATE_DATA_COLS),
            80
        );
        assert_eq!(
            render_state_get_u16(state, ROASTTY_RENDER_STATE_DATA_ROWS),
            24
        );
        assert_eq!(
            render_state_get_i32(state, ROASTTY_RENDER_STATE_DATA_DIRTY),
            ROASTTY_RENDER_STATE_DIRTY_FULL
        );
        assert_eq!(
            render_state_get_rgb(state, ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND),
            foreground
        );
        assert_eq!(
            render_state_get_rgb(state, ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND),
            background
        );
        assert_eq!(
            render_state_get_rgb(state, ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR),
            cursor
        );
        assert!(render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE
        ));
        assert_eq!(
            render_state_get_i32(state, ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE),
            ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE
        );
        assert!(render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING
        ));
        assert!(render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE
        ));
        assert_eq!(
            render_state_get_u16(state, ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X),
            0
        );
        assert_eq!(
            render_state_get_u16(state, ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y),
            0
        );
        assert!(!render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL
        ));
        assert!(!render_state_get_bool(
            state,
            ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT
        ));

        let mut colors = RoasttyRenderStateColors {
            size: std::mem::size_of::<RoasttyRenderStateColors>(),
            background: RoasttyRgb::default(),
            foreground: RoasttyRgb::default(),
            cursor: RoasttyRgb::default(),
            cursor_has_value: false,
            palette: [RoasttyRgb::default(); 256],
        };
        assert_eq!(
            roastty_render_state_colors_get(state, &mut colors),
            ROASTTY_SUCCESS
        );
        assert_eq!(colors.background, background);
        assert_eq!(colors.foreground, foreground);
        assert_eq!(colors.cursor, cursor);
        assert!(colors.cursor_has_value);
        assert_eq!(
            colors.palette,
            terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE)
        );

        roastty_render_state_free(state);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_c_abi_iterates_bound_snapshot_rows() {
        let terminal = new_terminal(5, 3);
        write_terminal(terminal, b"abc\nx");
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );

        let iterator = new_render_state_row_iterator();
        let mut dirty = false;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                &mut dirty as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );

        bind_render_state_rows(state, iterator);
        let mut rows = Vec::new();
        while roastty_render_state_row_iterator_next(iterator) {
            let mut raw: RoasttyRow = 0;
            assert_eq!(
                roastty_render_state_row_get(
                    iterator,
                    ROASTTY_RENDER_STATE_ROW_DATA_RAW,
                    &mut raw as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            assert_eq!(
                roastty_render_state_row_get(
                    iterator,
                    ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                    &mut dirty as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            let mut raw_dirty = false;
            assert_eq!(
                roastty_row_get(
                    raw,
                    ROASTTY_ROW_DATA_DIRTY,
                    &mut raw_dirty as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            assert_eq!(dirty, raw_dirty);
            assert_eq!(
                roastty_render_state_row_get(
                    iterator,
                    ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                    ptr::null_mut(),
                ),
                ROASTTY_INVALID_VALUE
            );
            let cells = new_render_state_row_cells();
            let mut cells_handle = cells;
            assert_eq!(
                roastty_render_state_row_get(
                    iterator,
                    ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                    &mut cells_handle as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            assert_eq!(cells_handle, cells);
            assert!(roastty_render_state_row_cells_next(cells));
            roastty_render_state_row_cells_free(cells);
            rows.push(raw);
        }
        assert_eq!(rows.len(), 3);
        assert!(!roastty_render_state_row_iterator_next(iterator));

        let value = false;
        assert_eq!(
            roastty_render_state_row_set(
                iterator,
                ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY,
                &value as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        dirty = true;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                &mut dirty as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert!(!dirty);

        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_c_abi_snapshot_survives_terminal_and_state_mutation() {
        let terminal = new_terminal(5, 2);
        write_terminal(terminal, b"abc");
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );

        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));
        let mut before: RoasttyRow = 0;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_RAW,
                &mut before as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );

        write_terminal(terminal, b"\rXYZ");
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        roastty_render_state_free(state);

        let mut after: RoasttyRow = 0;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_RAW,
                &mut after as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(after, before);

        roastty_render_state_row_iterator_free(iterator);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_c_abi_reports_selection_ranges() {
        let terminal = new_terminal(6, 3);
        let selection = terminal_selection(terminal, (2, 0), (4, 1), false);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SELECTION,
                &selection as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);

        assert!(roastty_render_state_row_iterator_next(iterator));
        let mut row_selection = RoasttyRenderStateRowSelection {
            size: std::mem::size_of::<RoasttyRenderStateRowSelection>(),
            start_x: 99,
            end_x: 99,
        };
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_SELECTION,
                &mut row_selection as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            row_selection.size,
            std::mem::size_of::<RoasttyRenderStateRowSelection>()
        );
        assert_eq!(row_selection.start_x, 2);
        assert_eq!(row_selection.end_x, 5);

        assert!(roastty_render_state_row_iterator_next(iterator));
        row_selection.start_x = 99;
        row_selection.end_x = 99;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_SELECTION,
                &mut row_selection as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(row_selection.start_x, 0);
        assert_eq!(row_selection.end_x, 4);

        assert!(roastty_render_state_row_iterator_next(iterator));
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_SELECTION,
                &mut row_selection as *mut _ as *mut c_void,
            ),
            ROASTTY_NO_VALUE
        );
        let mut undersized = RoasttyRenderStateRowSelection {
            size: std::mem::size_of::<usize>() - 1,
            start_x: 0,
            end_x: 0,
        };
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_SELECTION,
                &mut undersized as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_c_abi_validates_inputs_and_multi_get() {
        assert_eq!(
            roastty_render_state_row_iterator_new(ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert!(!roastty_render_state_row_iterator_next(ptr::null_mut()));
        assert_eq!(
            roastty_render_state_row_get(
                ptr::null_mut(),
                ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                ptr::null_mut(),
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_set(
                ptr::null_mut(),
                ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY,
                ptr::null(),
            ),
            ROASTTY_INVALID_VALUE
        );

        let terminal = new_terminal(4, 1);
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));

        let mut dirty = false;
        assert_eq!(
            roastty_render_state_row_get(iterator, 99, &mut dirty as *mut _ as *mut c_void),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                ptr::null_mut(),
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_set(iterator, 99, &dirty as *const _ as *const c_void,),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_set(
                iterator,
                ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY,
                ptr::null(),
            ),
            ROASTTY_INVALID_VALUE
        );

        let keys = [
            ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
            ROASTTY_RENDER_STATE_ROW_DATA_RAW,
        ];
        let mut raw: RoasttyRow = 0;
        let mut values = [
            &mut dirty as *mut _ as *mut c_void,
            &mut raw as *mut _ as *mut c_void,
        ];
        let mut written = 99;
        assert_eq!(
            roastty_render_state_row_get_multi(
                iterator,
                2,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 2);
        assert_eq!(
            roastty_render_state_row_get_multi(
                iterator,
                0,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        written = 99;
        assert_eq!(
            roastty_render_state_row_get_multi(
                iterator,
                0,
                ptr::null(),
                ptr::null_mut(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_render_state_row_get_multi(
                iterator,
                1,
                ptr::null(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_get_multi(
                iterator,
                1,
                keys.as_ptr(),
                ptr::null_mut(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        let partial_keys = [
            ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
            ROASTTY_RENDER_STATE_ROW_DATA_SELECTION,
        ];
        let mut row_selection = RoasttyRenderStateRowSelection {
            size: std::mem::size_of::<RoasttyRenderStateRowSelection>(),
            start_x: 0,
            end_x: 0,
        };
        values[1] = &mut row_selection as *mut _ as *mut c_void;
        assert_eq!(
            roastty_render_state_row_get_multi(
                iterator,
                2,
                partial_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(written, 1);

        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_cells_c_abi_iterates_raw_selected_and_styling() {
        let terminal = new_terminal(6, 2);
        write_terminal(terminal, b"A\x1b[31mB\x1b[0mC");
        let selection = terminal_selection(terminal, (1, 0), (2, 0), false);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SELECTION,
                &selection as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );

        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));
        let cells = new_render_state_row_cells();
        bind_render_state_row_cells(iterator, cells);

        let mut raw_cells = Vec::new();
        let mut selected = Vec::new();
        let mut styled = Vec::new();
        while roastty_render_state_row_cells_next(cells) {
            let mut raw: RoasttyCell = 0;
            let mut is_selected = false;
            let mut has_styling = false;
            assert_eq!(
                roastty_render_state_row_cells_get(
                    cells,
                    ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                    &mut raw as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            assert_eq!(
                roastty_render_state_row_cells_get(
                    cells,
                    ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED,
                    &mut is_selected as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            assert_eq!(
                roastty_render_state_row_cells_get(
                    cells,
                    ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING,
                    &mut has_styling as *mut _ as *mut c_void,
                ),
                ROASTTY_SUCCESS
            );
            raw_cells.push(raw);
            selected.push(is_selected);
            styled.push(has_styling);
        }

        assert_eq!(raw_cells.len(), 6);
        assert_eq!(cell_codepoint(raw_cells[0]), 'A' as u32);
        assert_eq!(cell_codepoint(raw_cells[1]), 'B' as u32);
        assert_eq!(cell_codepoint(raw_cells[2]), 'C' as u32);
        assert_eq!(selected, vec![false, true, true, false, false, false]);
        assert_eq!(styled, vec![false, true, false, false, false, false]);

        roastty_render_state_row_cells_free(cells);
        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_cells_c_abi_select_get_multi_and_deferred_values() {
        let terminal = new_terminal(4, 1);
        write_terminal(terminal, b"WXYZ");
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));
        let cells = new_render_state_row_cells();

        let mut raw: RoasttyCell = 0;
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                ptr::null_mut(),
            ),
            ROASTTY_INVALID_VALUE
        );
        let mut null_cells: RoasttyRenderStateRowCells = ptr::null_mut();
        assert_eq!(
            roastty_render_state_row_get(
                iterator,
                ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                &mut null_cells as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_cells_select(cells, 0),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                &mut raw as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_cells_get(
                ptr::null_mut(),
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut raw as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut raw as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );

        bind_render_state_row_cells(iterator, cells);
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut raw as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_cells_select(cells, 2),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                &mut raw as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(cell_codepoint(raw), 'Y' as u32);
        assert_eq!(
            roastty_render_state_row_cells_select(cells, 4),
            ROASTTY_INVALID_VALUE
        );

        let mut style_out = style_to_c(style::Style::default());
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut style_out as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert!(style_is_default(style_out));

        let mut graphemes_len = u32::MAX;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
                &mut graphemes_len as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(graphemes_len, 1);
        let mut graphemes = [0u32; 1];
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
                graphemes.as_mut_ptr().cast(),
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(graphemes, ['Y' as u32]);
        let mut utf8_bytes = [0u8; 1];
        let mut utf8 = RoasttyBuffer {
            ptr: utf8_bytes.as_mut_ptr(),
            cap: utf8_bytes.len(),
            len: 0,
        };
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                &mut utf8 as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(utf8.len, 1);
        assert_eq!(&utf8_bytes, b"Y");
        let mut rgb = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
                &mut rgb as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
                &mut rgb as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );

        let keys = [
            ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
            ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED,
        ];
        let mut selected = true;
        let mut values = [
            &mut raw as *mut _ as *mut c_void,
            &mut selected as *mut _ as *mut c_void,
        ];
        let mut written = 99;
        assert_eq!(
            roastty_render_state_row_cells_get_multi(
                cells,
                2,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 2);
        written = 99;
        assert_eq!(
            roastty_render_state_row_cells_get_multi(
                cells,
                0,
                ptr::null(),
                ptr::null_mut(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_render_state_row_cells_get_multi(
                cells,
                1,
                ptr::null(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        let partial_keys = [
            ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
            ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
        ];
        assert_eq!(
            roastty_render_state_row_cells_get_multi(
                cells,
                2,
                partial_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);

        roastty_render_state_row_cells_free(cells);
        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_cells_c_abi_styles_and_colors() {
        let terminal = new_terminal(6, 1);
        let mut palette = terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE);
        palette[1] = RoasttyRgb {
            r: 0x11,
            g: 0x22,
            b: 0x33,
        };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                &palette as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[38;5;1m\x1b[48;2;10;20;30m\x1b[1;3;4mA");

        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));
        let cells = new_render_state_row_cells();
        bind_render_state_row_cells(iterator, cells);
        assert!(roastty_render_state_row_cells_next(cells));

        let mut small = style_to_c(style::Style::default());
        small.size = std::mem::size_of::<RoasttyStyle>() - 1;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut small as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(small.size, std::mem::size_of::<RoasttyStyle>() - 1);

        let mut style_out = style_to_c(style::Style::default());
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut style_out as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(style_out.size, std::mem::size_of::<RoasttyStyle>());
        assert_eq!(style_out.fg_color.tag, ROASTTY_STYLE_COLOR_PALETTE);
        assert_eq!(style_color_palette(style_out.fg_color), 1);
        assert_eq!(style_out.bg_color.tag, ROASTTY_STYLE_COLOR_RGB);
        assert_eq!(
            style_color_rgb(style_out.bg_color),
            RoasttyRgb {
                r: 10,
                g: 20,
                b: 30
            }
        );
        assert!(style_out.bold);
        assert!(style_out.italic);
        assert_eq!(style_out.underline, 1);

        let mut fg = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
                &mut fg as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            fg,
            RoasttyRgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            }
        );
        let mut bg = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
                &mut bg as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            bg,
            RoasttyRgb {
                r: 10,
                g: 20,
                b: 30
            }
        );

        roastty_render_state_row_cells_free(cells);
        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);
        roastty_terminal_free(terminal);

        let mut palette = palette_from_color(color::DEFAULT_PALETTE);
        palette[2] = RoasttyRgb { r: 4, g: 5, b: 6 };
        let cells: RoasttyRenderStateRowCells = Box::into_raw(Box::new(RenderStateRowCells {
            cells: vec![RenderStateCellSnapshot {
                raw: packed_cell(ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE, 2),
                style: None,
                graphemes: Vec::new(),
            }],
            palette,
            selected: Some(0),
            selection: None,
            bound: true,
        }))
        .cast();

        let mut inline_style = style_to_c(style::Style::default());
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                &mut inline_style as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert!(style_is_default(inline_style));
        let mut inline_bg = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
                &mut inline_bg as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(inline_bg, RoasttyRgb { r: 4, g: 5, b: 6 });

        roastty_render_state_row_cells_free(cells);
    }

    #[test]
    fn render_state_row_cells_c_abi_graphemes_and_utf8() {
        let cells: RoasttyRenderStateRowCells = Box::into_raw(Box::new(RenderStateRowCells {
            cells: vec![
                RenderStateCellSnapshot {
                    raw: packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME, 'e' as u64),
                    style: None,
                    graphemes: vec![0x0301],
                },
                RenderStateCellSnapshot {
                    raw: packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT, 0),
                    style: None,
                    graphemes: Vec::new(),
                },
            ],
            palette: palette_from_color(color::DEFAULT_PALETTE),
            selected: Some(0),
            selection: None,
            bound: true,
        }))
        .cast();

        let mut graphemes_len = 0;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
                &mut graphemes_len as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(graphemes_len, 2);
        let mut graphemes = [0u32; 2];
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
                graphemes.as_mut_ptr().cast(),
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(graphemes, ['e' as u32, 0x0301]);

        let mut utf8 = RoasttyBuffer {
            ptr: ptr::null_mut(),
            cap: 0,
            len: 0,
        };
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                &mut utf8 as *mut _ as *mut c_void,
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(utf8.len, "e\u{0301}".len());
        let mut utf8_bytes = [0u8; 3];
        utf8.ptr = utf8_bytes.as_mut_ptr();
        utf8.cap = utf8_bytes.len();
        utf8.len = 0;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                &mut utf8 as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(utf8.len, 3);
        assert_eq!(&utf8_bytes, "e\u{0301}".as_bytes());

        assert_eq!(
            roastty_render_state_row_cells_select(cells, 1),
            ROASTTY_SUCCESS
        );
        graphemes_len = u32::MAX;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
                &mut graphemes_len as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(graphemes_len, 0);
        utf8 = RoasttyBuffer {
            ptr: ptr::null_mut(),
            cap: 0,
            len: usize::MAX,
        };
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                &mut utf8 as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(utf8.len, 0);

        roastty_render_state_row_cells_free(cells);
    }

    #[test]
    fn render_state_row_cells_c_abi_palette_snapshot_survives_state_changes() {
        let terminal = new_terminal(3, 1);
        let mut palette = terminal_get_palette(terminal, ROASTTY_TERMINAL_DATA_COLOR_PALETTE);
        palette[1] = RoasttyRgb {
            r: 0xaa,
            g: 0xbb,
            b: 0xcc,
        };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                &palette as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        write_terminal(terminal, b"\x1b[38;5;1mA");
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));
        let cells = new_render_state_row_cells();
        bind_render_state_row_cells(iterator, cells);
        assert!(roastty_render_state_row_cells_next(cells));

        palette[1] = RoasttyRgb { r: 1, g: 2, b: 3 };
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                &palette as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);

        let mut fg = RoasttyRgb::default();
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
                &mut fg as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            fg,
            RoasttyRgb {
                r: 0xaa,
                g: 0xbb,
                b: 0xcc
            }
        );

        roastty_render_state_row_cells_free(cells);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_row_cells_c_abi_snapshot_survives_rebind_and_frees() {
        let terminal = new_terminal(3, 1);
        write_terminal(terminal, b"abc");
        let state = new_render_state();
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        let iterator = new_render_state_row_iterator();
        bind_render_state_rows(state, iterator);
        assert!(roastty_render_state_row_iterator_next(iterator));
        let cells = new_render_state_row_cells();
        bind_render_state_row_cells(iterator, cells);
        assert!(roastty_render_state_row_cells_next(cells));
        let mut before: RoasttyCell = 0;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                &mut before as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );

        write_terminal(terminal, b"\rXYZ");
        assert_eq!(
            roastty_render_state_update(state, terminal),
            ROASTTY_SUCCESS
        );
        roastty_render_state_row_iterator_free(iterator);
        roastty_render_state_free(state);

        let mut after: RoasttyCell = 0;
        assert_eq!(
            roastty_render_state_row_cells_get(
                cells,
                ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                &mut after as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(after, before);

        roastty_render_state_row_cells_free(cells);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn render_state_c_abi_validation_multi_and_colors_capacity() {
        let state = new_render_state();
        let mut out_u16 = 77u16;
        assert_eq!(
            roastty_render_state_get(
                ptr::null_mut(),
                ROASTTY_RENDER_STATE_DATA_COLS,
                &mut out_u16 as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_COLS, ptr::null_mut(),),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_get(
                state,
                ROASTTY_RENDER_STATE_DATA_INVALID,
                &mut out_u16 as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_get(state, 99, &mut out_u16 as *mut _ as *mut c_void),
            ROASTTY_INVALID_VALUE
        );
        let mut iterator: RoasttyRenderStateRowIterator = ptr::null_mut();
        assert_eq!(
            roastty_render_state_get(
                state,
                ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                &mut iterator as *mut _ as *mut c_void,
            ),
            ROASTTY_INVALID_VALUE
        );

        let partial = ROASTTY_RENDER_STATE_DIRTY_PARTIAL;
        assert_eq!(
            roastty_render_state_set(
                ptr::null_mut(),
                ROASTTY_RENDER_STATE_OPTION_DIRTY,
                &partial as *const _ as *const c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_set(state, ROASTTY_RENDER_STATE_OPTION_DIRTY, ptr::null(),),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_set(state, 99, &partial as *const _ as *const c_void),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_set(
                state,
                ROASTTY_RENDER_STATE_OPTION_DIRTY,
                &partial as *const _ as *const c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            render_state_get_i32(state, ROASTTY_RENDER_STATE_DATA_DIRTY),
            ROASTTY_RENDER_STATE_DIRTY_PARTIAL
        );
        let invalid_dirty = 99;
        assert_eq!(
            roastty_render_state_set(
                state,
                ROASTTY_RENDER_STATE_OPTION_DIRTY,
                &invalid_dirty as *const _ as *const c_void,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            render_state_get_i32(state, ROASTTY_RENDER_STATE_DATA_DIRTY),
            ROASTTY_RENDER_STATE_DIRTY_PARTIAL
        );

        let keys = [
            ROASTTY_RENDER_STATE_DATA_COLS,
            ROASTTY_RENDER_STATE_DATA_ROWS,
            ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
        ];
        let mut cols = 1u16;
        let mut rows = 2u16;
        let mut viewport_x = 3u16;
        let mut values = [
            &mut cols as *mut _ as *mut c_void,
            &mut rows as *mut _ as *mut c_void,
            &mut viewport_x as *mut _ as *mut c_void,
        ];
        let mut written = usize::MAX;
        assert_eq!(
            roastty_render_state_get_multi(
                state,
                2,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 2);
        assert_eq!(cols, 0);
        assert_eq!(rows, 0);
        written = usize::MAX;
        assert_eq!(
            roastty_render_state_get_multi(
                state,
                3,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(written, 2);
        written = usize::MAX;
        assert_eq!(
            roastty_render_state_get_multi(
                state,
                0,
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_render_state_get_multi(
                state,
                1,
                ptr::null(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_get_multi(state, 1, keys.as_ptr(), ptr::null_mut(), &mut written,),
            ROASTTY_INVALID_VALUE
        );

        let mut colors = RoasttyRenderStateColors {
            size: std::mem::size_of::<usize>() - 1,
            background: RoasttyRgb { r: 9, g: 9, b: 9 },
            foreground: RoasttyRgb { r: 9, g: 9, b: 9 },
            cursor: RoasttyRgb { r: 9, g: 9, b: 9 },
            cursor_has_value: true,
            palette: [RoasttyRgb { r: 9, g: 9, b: 9 }; 256],
        };
        assert_eq!(
            roastty_render_state_colors_get(ptr::null_mut(), &mut colors),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_colors_get(state, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_render_state_colors_get(state, &mut colors),
            ROASTTY_INVALID_VALUE
        );
        colors.size = std::mem::offset_of!(RoasttyRenderStateColors, cursor_has_value)
            + std::mem::size_of::<bool>();
        assert_eq!(
            roastty_render_state_colors_get(state, &mut colors),
            ROASTTY_SUCCESS
        );
        assert_eq!(colors.background, RoasttyRgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            colors.foreground,
            RoasttyRgb {
                r: 0xff,
                g: 0xff,
                b: 0xff
            }
        );
        assert_eq!(colors.cursor, RoasttyRgb { r: 9, g: 9, b: 9 });
        assert!(!colors.cursor_has_value);
        assert_eq!(colors.palette[0], RoasttyRgb { r: 9, g: 9, b: 9 });

        roastty_render_state_free(state);
    }

    #[test]
    fn cell_c_abi_layout_and_values_are_stable() {
        assert_eq!(std::mem::size_of::<RoasttyCell>(), 8);
        assert_eq!(std::mem::align_of::<RoasttyCell>(), 8);
        assert_eq!(ROASTTY_CELL_CONTENT_CODEPOINT, 0);
        assert_eq!(ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME, 1);
        assert_eq!(ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE, 2);
        assert_eq!(ROASTTY_CELL_CONTENT_BG_COLOR_RGB, 3);
        assert_eq!(ROASTTY_CELL_WIDE_NARROW, 0);
        assert_eq!(ROASTTY_CELL_WIDE_WIDE, 1);
        assert_eq!(ROASTTY_CELL_WIDE_SPACER_TAIL, 2);
        assert_eq!(ROASTTY_CELL_WIDE_SPACER_HEAD, 3);
        assert_eq!(ROASTTY_CELL_SEMANTIC_OUTPUT, 0);
        assert_eq!(ROASTTY_CELL_SEMANTIC_INPUT, 1);
        assert_eq!(ROASTTY_CELL_SEMANTIC_PROMPT, 2);
        assert_eq!(ROASTTY_CELL_DATA_INVALID, 0);
        assert_eq!(ROASTTY_CELL_DATA_CODEPOINT, 1);
        assert_eq!(ROASTTY_CELL_DATA_CONTENT_TAG, 2);
        assert_eq!(ROASTTY_CELL_DATA_WIDE, 3);
        assert_eq!(ROASTTY_CELL_DATA_HAS_TEXT, 4);
        assert_eq!(ROASTTY_CELL_DATA_HAS_STYLING, 5);
        assert_eq!(ROASTTY_CELL_DATA_STYLE_ID, 6);
        assert_eq!(ROASTTY_CELL_DATA_HAS_HYPERLINK, 7);
        assert_eq!(ROASTTY_CELL_DATA_PROTECTED, 8);
        assert_eq!(ROASTTY_CELL_DATA_SEMANTIC, 9);
        assert_eq!(ROASTTY_CELL_DATA_COLOR_PALETTE, 10);
        assert_eq!(ROASTTY_CELL_DATA_COLOR_RGB, 11);
    }

    #[test]
    fn cell_c_abi_get_reads_packed_fields() {
        let cell = packed_cell_with_flags(
            packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT, 'A' as u64),
            7,
            ROASTTY_CELL_WIDE_WIDE,
            true,
            true,
            ROASTTY_CELL_SEMANTIC_PROMPT,
        );

        assert_eq!(cell_get_u32(cell, ROASTTY_CELL_DATA_CODEPOINT), 'A' as u32);
        assert_eq!(cell_get_i32(cell, ROASTTY_CELL_DATA_CONTENT_TAG), 0);
        assert_eq!(
            cell_get_i32(cell, ROASTTY_CELL_DATA_WIDE),
            ROASTTY_CELL_WIDE_WIDE
        );
        assert!(cell_get_bool(cell, ROASTTY_CELL_DATA_HAS_TEXT));
        assert!(cell_get_bool(cell, ROASTTY_CELL_DATA_HAS_STYLING));
        let mut style_id = 0u16;
        assert_eq!(
            roastty_cell_get(
                cell,
                ROASTTY_CELL_DATA_STYLE_ID,
                &mut style_id as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(style_id, 7);
        assert!(cell_get_bool(cell, ROASTTY_CELL_DATA_HAS_HYPERLINK));
        assert!(cell_get_bool(cell, ROASTTY_CELL_DATA_PROTECTED));
        assert_eq!(
            cell_get_i32(cell, ROASTTY_CELL_DATA_SEMANTIC),
            ROASTTY_CELL_SEMANTIC_PROMPT
        );

        let empty = packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT, 0);
        assert!(!cell_get_bool(empty, ROASTTY_CELL_DATA_HAS_TEXT));
        let grapheme = packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME, 'B' as u64);
        assert!(cell_get_bool(grapheme, ROASTTY_CELL_DATA_HAS_TEXT));

        let palette = packed_cell(ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE, 0xab);
        assert_eq!(cell_get_u32(palette, ROASTTY_CELL_DATA_CODEPOINT), 0);
        let mut palette_out = 0u8;
        assert_eq!(
            roastty_cell_get(
                palette,
                ROASTTY_CELL_DATA_COLOR_PALETTE,
                &mut palette_out as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(palette_out, 0xab);

        let rgb = packed_cell(ROASTTY_CELL_CONTENT_BG_COLOR_RGB, 0x11_22_33);
        let mut rgb_out = RoasttyRgb::default();
        assert_eq!(
            roastty_cell_get(
                rgb,
                ROASTTY_CELL_DATA_COLOR_RGB,
                &mut rgb_out as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            rgb_out,
            RoasttyRgb {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            }
        );
        assert_eq!(cell_get_u32(rgb, ROASTTY_CELL_DATA_CODEPOINT), 0);
        assert!(!cell_get_bool(rgb, ROASTTY_CELL_DATA_HAS_TEXT));
    }

    #[test]
    fn cell_c_abi_validates_inputs_and_multi_reports_progress() {
        let cell = packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT, 'C' as u64);
        let mut codepoint = 0u32;
        assert_eq!(
            roastty_cell_get(
                cell,
                ROASTTY_CELL_DATA_INVALID,
                &mut codepoint as *mut _ as *mut c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_cell_get(cell, ROASTTY_CELL_DATA_CODEPOINT, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_cell_get(cell, 99, &mut codepoint as *mut _ as *mut c_void),
            ROASTTY_INVALID_VALUE
        );

        let mut has_text = false;
        let keys = [ROASTTY_CELL_DATA_CODEPOINT, ROASTTY_CELL_DATA_HAS_TEXT];
        let mut values = [
            &mut codepoint as *mut _ as *mut c_void,
            &mut has_text as *mut _ as *mut c_void,
        ];
        let mut written = 99usize;
        assert_eq!(
            roastty_cell_get_multi(
                cell,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 2);
        assert_eq!(codepoint, 'C' as u32);
        assert!(has_text);
        assert_eq!(
            roastty_cell_get_multi(cell, 0, keys.as_ptr(), values.as_mut_ptr(), &mut written),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_cell_get_multi(
                cell,
                keys.len(),
                ptr::null(),
                values.as_mut_ptr(),
                &mut written
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 0);

        let partial_keys = [ROASTTY_CELL_DATA_CODEPOINT, ROASTTY_CELL_DATA_INVALID];
        written = 99;
        assert_eq!(
            roastty_cell_get_multi(
                cell,
                partial_keys.len(),
                partial_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);
        assert_eq!(
            roastty_cell_get_multi(
                cell,
                partial_keys.len(),
                partial_keys.as_ptr(),
                values.as_mut_ptr(),
                ptr::null_mut(),
            ),
            ROASTTY_INVALID_VALUE
        );
    }

    #[test]
    fn row_c_abi_layout_and_values_are_stable() {
        assert_eq!(std::mem::size_of::<RoasttyRow>(), 8);
        assert_eq!(std::mem::align_of::<RoasttyRow>(), 8);
        assert_eq!(ROASTTY_ROW_SEMANTIC_NONE, 0);
        assert_eq!(ROASTTY_ROW_SEMANTIC_PROMPT, 1);
        assert_eq!(ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION, 2);
        assert_eq!(ROASTTY_ROW_DATA_INVALID, 0);
        assert_eq!(ROASTTY_ROW_DATA_WRAP, 1);
        assert_eq!(ROASTTY_ROW_DATA_WRAP_CONTINUATION, 2);
        assert_eq!(ROASTTY_ROW_DATA_GRAPHEME, 3);
        assert_eq!(ROASTTY_ROW_DATA_STYLED, 4);
        assert_eq!(ROASTTY_ROW_DATA_HYPERLINK, 5);
        assert_eq!(ROASTTY_ROW_DATA_SEMANTIC_PROMPT, 6);
        assert_eq!(ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER, 7);
        assert_eq!(ROASTTY_ROW_DATA_DIRTY, 8);
    }

    #[test]
    fn row_c_abi_get_reads_packed_fields() {
        let row = packed_row(
            true,
            true,
            true,
            true,
            true,
            ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION,
            true,
            true,
        );

        assert!(row_get_bool(row, ROASTTY_ROW_DATA_WRAP));
        assert!(row_get_bool(row, ROASTTY_ROW_DATA_WRAP_CONTINUATION));
        assert!(row_get_bool(row, ROASTTY_ROW_DATA_GRAPHEME));
        assert!(row_get_bool(row, ROASTTY_ROW_DATA_STYLED));
        assert!(row_get_bool(row, ROASTTY_ROW_DATA_HYPERLINK));
        assert_eq!(
            row_get_i32(row, ROASTTY_ROW_DATA_SEMANTIC_PROMPT),
            ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION
        );
        assert!(row_get_bool(
            row,
            ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER
        ));
        assert!(row_get_bool(row, ROASTTY_ROW_DATA_DIRTY));

        let empty = packed_row(false, false, false, false, false, 0, false, false);
        assert!(!row_get_bool(empty, ROASTTY_ROW_DATA_WRAP));
        assert_eq!(
            row_get_i32(empty, ROASTTY_ROW_DATA_SEMANTIC_PROMPT),
            ROASTTY_ROW_SEMANTIC_NONE
        );
    }

    #[test]
    fn row_c_abi_validates_inputs_and_multi_reports_progress() {
        let row = packed_row(
            true,
            false,
            false,
            false,
            false,
            ROASTTY_ROW_SEMANTIC_PROMPT,
            false,
            true,
        );
        let mut wrap = false;
        assert_eq!(
            roastty_row_get(
                row,
                ROASTTY_ROW_DATA_INVALID,
                &mut wrap as *mut _ as *mut c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_row_get(row, ROASTTY_ROW_DATA_WRAP, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_row_get(row, 99, &mut wrap as *mut _ as *mut c_void),
            ROASTTY_INVALID_VALUE
        );

        let mut dirty = false;
        let keys = [ROASTTY_ROW_DATA_WRAP, ROASTTY_ROW_DATA_DIRTY];
        let mut values = [
            &mut wrap as *mut _ as *mut c_void,
            &mut dirty as *mut _ as *mut c_void,
        ];
        let mut written = 99usize;
        assert_eq!(
            roastty_row_get_multi(
                row,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 2);
        assert!(wrap);
        assert!(dirty);
        assert_eq!(
            roastty_row_get_multi(row, 0, keys.as_ptr(), values.as_mut_ptr(), &mut written),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 0);
        assert_eq!(
            roastty_row_get_multi(
                row,
                keys.len(),
                ptr::null(),
                values.as_mut_ptr(),
                &mut written
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 0);

        let partial_keys = [ROASTTY_ROW_DATA_WRAP, ROASTTY_ROW_DATA_INVALID];
        written = 99;
        assert_eq!(
            roastty_row_get_multi(
                row,
                partial_keys.len(),
                partial_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);
        assert_eq!(
            roastty_row_get_multi(
                row,
                partial_keys.len(),
                partial_keys.as_ptr(),
                values.as_mut_ptr(),
                ptr::null_mut(),
            ),
            ROASTTY_INVALID_VALUE
        );
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
        assert_eq!(ROASTTY_KITTY_GRAPHICS_DATA_INVALID, 0);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR, 1);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID, 0);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID, 1);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID, 2);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL, 3);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET, 4);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET, 5);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_X, 6);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_Y, 7);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_WIDTH, 8);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_HEIGHT, 9);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS, 10);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS, 11);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z, 12);
        assert_eq!(ROASTTY_KITTY_PLACEMENT_LAYER_ALL, 0);
        assert_eq!(ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG, 1);
        assert_eq!(ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT, 2);
        assert_eq!(ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT, 3);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER, 0);
        assert_eq!(ROASTTY_KITTY_IMAGE_FORMAT_RGB, 0);
        assert_eq!(ROASTTY_KITTY_IMAGE_FORMAT_RGBA, 1);
        assert_eq!(ROASTTY_KITTY_IMAGE_FORMAT_PNG, 2);
        assert_eq!(ROASTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA, 3);
        assert_eq!(ROASTTY_KITTY_IMAGE_FORMAT_GRAY, 4);
        assert_eq!(ROASTTY_KITTY_IMAGE_COMPRESSION_NONE, 0);
        assert_eq!(ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE, 1);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID, 0);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID, 1);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER, 2);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH, 3);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT, 4);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT, 5);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION, 6);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR, 7);
        assert_eq!(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN, 8);
        assert_eq!(ROASTTY_POINT_ACTIVE, 0);
        assert_eq!(ROASTTY_POINT_VIEWPORT, 1);
        assert_eq!(ROASTTY_POINT_SCREEN, 2);
        assert_eq!(ROASTTY_POINT_HISTORY, 3);
    }

    #[test]
    fn terminal_grid_ref_abi_layout_is_stable() {
        assert_eq!(std::mem::size_of::<RoasttyPointCoordinate>(), 8);
        assert_eq!(std::mem::align_of::<RoasttyPointCoordinate>(), 4);
        assert_eq!(std::mem::offset_of!(RoasttyPointCoordinate, x), 0);
        assert_eq!(std::mem::offset_of!(RoasttyPointCoordinate, y), 4);
        assert_eq!(std::mem::size_of::<RoasttyPointValue>(), 16);
        assert_eq!(std::mem::align_of::<RoasttyPointValue>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyPointValue, active), 0);
        assert_eq!(std::mem::offset_of!(RoasttyPointValue, viewport), 0);
        assert_eq!(std::mem::offset_of!(RoasttyPointValue, screen), 0);
        assert_eq!(std::mem::offset_of!(RoasttyPointValue, history), 0);
        assert_eq!(std::mem::offset_of!(RoasttyPointValue, _padding), 0);
        assert_eq!(std::mem::size_of::<RoasttyPoint>(), 24);
        assert_eq!(std::mem::align_of::<RoasttyPoint>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyPoint, tag), 0);
        assert_eq!(std::mem::offset_of!(RoasttyPoint, value), 8);
        assert_eq!(std::mem::size_of::<RoasttyGridRef>(), 24);
        assert_eq!(std::mem::align_of::<RoasttyGridRef>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyGridRef, size), 0);
        assert_eq!(std::mem::offset_of!(RoasttyGridRef, node), 8);
        assert_eq!(std::mem::offset_of!(RoasttyGridRef, x), 16);
        assert_eq!(std::mem::offset_of!(RoasttyGridRef, y), 18);
    }

    #[test]
    fn terminal_grid_ref_abi_round_trips_active_and_viewport_points() {
        let terminal = new_terminal(10, 4);
        write_terminal(terminal, b"hello");

        let mut grid_ref = RoasttyGridRef::default();
        assert_eq!(
            roastty_terminal_grid_ref(terminal, c_point(ROASTTY_POINT_ACTIVE, 1, 0), &mut grid_ref),
            ROASTTY_SUCCESS
        );
        assert_eq!(grid_ref.size, std::mem::size_of::<RoasttyGridRef>());
        assert!(!grid_ref.node.is_null());
        assert_eq!((grid_ref.x, grid_ref.y), (1, 0));

        let mut coord = RoasttyPointCoordinate::default();
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &grid_ref,
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(coord, RoasttyPointCoordinate { x: 1, y: 0 });

        let mut viewport_ref = RoasttyGridRef::default();
        assert_eq!(
            roastty_terminal_grid_ref(
                terminal,
                c_point(ROASTTY_POINT_VIEWPORT, 2, 0),
                &mut viewport_ref
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &viewport_ref,
                ROASTTY_POINT_VIEWPORT,
                &mut coord
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(coord, RoasttyPointCoordinate { x: 2, y: 0 });

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_grid_ref_abi_round_trips_screen_history_and_rejects_active_miss() {
        let terminal = new_terminal(5, 3);
        for _ in 0..8 {
            write_terminal(terminal, b"line\n");
        }

        let mut screen_ref = RoasttyGridRef::default();
        assert_eq!(
            roastty_terminal_grid_ref(
                terminal,
                c_point(ROASTTY_POINT_SCREEN, 0, 0),
                &mut screen_ref
            ),
            ROASTTY_SUCCESS
        );
        let mut coord = RoasttyPointCoordinate::default();
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &screen_ref,
                ROASTTY_POINT_SCREEN,
                &mut coord
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(coord, RoasttyPointCoordinate { x: 0, y: 0 });
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &screen_ref,
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_NO_VALUE
        );

        let mut history_ref = RoasttyGridRef::default();
        assert_eq!(
            roastty_terminal_grid_ref(
                terminal,
                c_point(ROASTTY_POINT_HISTORY, 0, 0),
                &mut history_ref
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &history_ref,
                ROASTTY_POINT_HISTORY,
                &mut coord
            ),
            ROASTTY_SUCCESS
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn tracked_grid_ref_abi_validates_nulls_and_invalid_points() {
        let terminal = new_terminal(4, 2);
        let tracked = terminal_tracked_grid_ref_at(terminal, 1, 0);
        assert!(roastty_tracked_grid_ref_has_value(tracked));

        let mut invalid_out = tracked;
        assert_eq!(
            roastty_terminal_grid_ref_track(
                ptr::null_mut(),
                c_point(ROASTTY_POINT_ACTIVE, 0, 0),
                &mut invalid_out
            ),
            ROASTTY_INVALID_VALUE
        );
        assert!(invalid_out.is_null());

        assert_eq!(
            roastty_terminal_grid_ref_track(
                terminal,
                c_point(ROASTTY_POINT_ACTIVE, 0, 0),
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );

        assert_eq!(
            roastty_tracked_grid_ref_snapshot(ptr::null_mut(), ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_point(tracked, 999, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_set(
                ptr::null_mut(),
                terminal,
                c_point(ROASTTY_POINT_ACTIVE, 0, 0),
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_set(
                tracked,
                ptr::null_mut(),
                c_point(ROASTTY_POINT_ACTIVE, 0, 0)
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_tracked_grid_ref_free(tracked);
        roastty_tracked_grid_ref_free(ptr::null_mut());
        roastty_terminal_free(terminal);
    }

    #[test]
    fn tracked_grid_ref_snapshot_follows_scroll_and_allows_null_outputs() {
        let terminal = new_terminal(5, 3);
        write_terminal(terminal, b"alpha");
        let tracked = terminal_tracked_grid_ref_at(terminal, 1, 0);

        for _ in 0..6 {
            write_terminal(terminal, b"\nline");
        }

        assert!(roastty_tracked_grid_ref_has_value(tracked));
        assert_eq!(
            roastty_tracked_grid_ref_snapshot(tracked, ptr::null_mut()),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_tracked_grid_ref_point(tracked, ROASTTY_POINT_HISTORY, ptr::null_mut()),
            ROASTTY_SUCCESS
        );

        let mut snapshot = RoasttyGridRef::default();
        assert_eq!(
            roastty_tracked_grid_ref_snapshot(tracked, &mut snapshot),
            ROASTTY_SUCCESS
        );
        assert_eq!(snapshot.size, std::mem::size_of::<RoasttyGridRef>());
        assert_eq!(snapshot.x, 1);
        assert!(!snapshot.node.is_null());

        let mut coord = RoasttyPointCoordinate::default();
        assert_eq!(
            roastty_tracked_grid_ref_point(tracked, ROASTTY_POINT_HISTORY, &mut coord),
            ROASTTY_SUCCESS
        );
        assert_eq!(coord.x, 1);

        roastty_tracked_grid_ref_free(tracked);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn tracked_grid_ref_returns_no_value_after_reset_and_alternate_recreate() {
        let terminal = new_terminal(5, 3);
        let primary = terminal_tracked_grid_ref_at(terminal, 0, 0);
        roastty_terminal_reset(terminal);
        assert!(!roastty_tracked_grid_ref_has_value(primary));
        assert_eq!(
            roastty_tracked_grid_ref_snapshot(primary, ptr::null_mut()),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_point(primary, ROASTTY_POINT_ACTIVE, ptr::null_mut()),
            ROASTTY_NO_VALUE
        );
        roastty_tracked_grid_ref_free(primary);

        write_terminal(terminal, b"\x1b[?1049hALT");
        let alternate = terminal_tracked_grid_ref_at(terminal, 0, 0);
        roastty_terminal_reset(terminal);
        write_terminal(terminal, b"\x1b[?1049hNEW");
        assert!(!roastty_tracked_grid_ref_has_value(alternate));
        assert_eq!(
            roastty_tracked_grid_ref_snapshot(alternate, ptr::null_mut()),
            ROASTTY_NO_VALUE
        );
        roastty_tracked_grid_ref_free(alternate);

        roastty_terminal_free(terminal);
    }

    #[test]
    fn tracked_grid_ref_terminal_free_detaches_refs() {
        let terminal = new_terminal(5, 3);
        let tracked = terminal_tracked_grid_ref_at(terminal, 0, 0);

        roastty_terminal_free(terminal);

        assert!(!roastty_tracked_grid_ref_has_value(tracked));
        assert_eq!(
            roastty_tracked_grid_ref_snapshot(tracked, ptr::null_mut()),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_point(tracked, ROASTTY_POINT_ACTIVE, ptr::null_mut()),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_set(tracked, terminal, c_point(ROASTTY_POINT_ACTIVE, 0, 0)),
            ROASTTY_INVALID_VALUE
        );

        roastty_tracked_grid_ref_free(tracked);
    }

    #[test]
    fn tracked_grid_ref_set_updates_attached_ref_and_rejects_wrong_terminal() {
        let terminal = new_terminal(5, 3);
        write_terminal(terminal, b"abcde");
        let other = new_terminal(5, 3);
        let tracked = terminal_tracked_grid_ref_at(terminal, 0, 0);

        assert_eq!(
            roastty_tracked_grid_ref_set(tracked, other, c_point(ROASTTY_POINT_ACTIVE, 1, 0)),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_set(tracked, terminal, c_point(ROASTTY_POINT_ACTIVE, 9, 0)),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_tracked_grid_ref_set(tracked, terminal, c_point(ROASTTY_POINT_ACTIVE, 3, 0)),
            ROASTTY_SUCCESS
        );

        let mut coord = RoasttyPointCoordinate::default();
        assert_eq!(
            roastty_tracked_grid_ref_point(tracked, ROASTTY_POINT_ACTIVE, &mut coord),
            ROASTTY_SUCCESS
        );
        assert_eq!(coord, RoasttyPointCoordinate { x: 3, y: 0 });

        roastty_tracked_grid_ref_free(tracked);
        roastty_terminal_free(other);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_grid_ref_abi_validates_inputs() {
        let terminal = new_terminal(4, 2);
        let mut grid_ref = RoasttyGridRef::default();
        let mut coord = RoasttyPointCoordinate::default();

        assert_eq!(
            roastty_terminal_grid_ref(
                ptr::null_mut(),
                c_point(ROASTTY_POINT_ACTIVE, 0, 0),
                &mut grid_ref
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_grid_ref(
                terminal,
                c_point(ROASTTY_POINT_ACTIVE, 0, 0),
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_grid_ref(terminal, c_point(99, 0, 0), &mut grid_ref),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_grid_ref(terminal, c_point(ROASTTY_POINT_ACTIVE, 4, 0), &mut grid_ref),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_grid_ref(
                terminal,
                c_point(ROASTTY_POINT_VIEWPORT, 0, 99),
                &mut grid_ref
            ),
            ROASTTY_INVALID_VALUE
        );

        assert_eq!(
            roastty_terminal_grid_ref(terminal, c_point(ROASTTY_POINT_ACTIVE, 1, 0), &mut grid_ref),
            ROASTTY_SUCCESS
        );
        let mut undersized = grid_ref;
        undersized.size = std::mem::size_of::<RoasttyGridRef>() - 1;
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &undersized,
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                ptr::null_mut(),
                &grid_ref,
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                ptr::null(),
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &grid_ref,
                ROASTTY_POINT_ACTIVE,
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_point_from_grid_ref(terminal, &grid_ref, 99, &mut coord),
            ROASTTY_INVALID_VALUE
        );

        let mut forged_x = grid_ref;
        forged_x.x = 4;
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &forged_x,
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_INVALID_VALUE
        );
        let mut forged_y = grid_ref;
        forged_y.y = 99;
        assert_eq!(
            roastty_terminal_point_from_grid_ref(
                terminal,
                &forged_y,
                ROASTTY_POINT_ACTIVE,
                &mut coord
            ),
            ROASTTY_INVALID_VALUE
        );

        let other = new_terminal(4, 2);
        let foreign_result = roastty_terminal_point_from_grid_ref(
            other,
            &grid_ref,
            ROASTTY_POINT_ACTIVE,
            &mut coord,
        );
        assert!(foreign_result == ROASTTY_NO_VALUE || foreign_result == ROASTTY_INVALID_VALUE);

        roastty_terminal_free(other);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn grid_ref_accessor_c_abi_reads_cell_row_and_style() {
        let terminal = new_terminal(10, 3);
        write_terminal(terminal, b"A\x1b[1;31mB");

        let default_ref = terminal_grid_ref_at(terminal, 0, 0);
        let styled_ref = terminal_grid_ref_at(terminal, 1, 0);

        let mut cell = 0;
        assert_eq!(
            roastty_grid_ref_cell(&default_ref, &mut cell),
            ROASTTY_SUCCESS
        );
        let mut codepoint = 0;
        assert_eq!(
            roastty_cell_get(
                cell,
                ROASTTY_CELL_DATA_CODEPOINT,
                &mut codepoint as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(codepoint, 'A' as u32);
        assert_eq!(
            roastty_grid_ref_cell(&default_ref, ptr::null_mut()),
            ROASTTY_SUCCESS
        );

        let mut row = 0;
        assert_eq!(
            roastty_grid_ref_row(&default_ref, &mut row),
            ROASTTY_SUCCESS
        );
        let mut styled = false;
        assert_eq!(
            roastty_row_get(
                row,
                ROASTTY_ROW_DATA_STYLED,
                &mut styled as *mut _ as *mut c_void,
            ),
            ROASTTY_SUCCESS
        );
        assert!(styled);
        assert_eq!(
            roastty_grid_ref_row(&default_ref, ptr::null_mut()),
            ROASTTY_SUCCESS
        );

        let mut style = default_c_style();
        assert_eq!(
            roastty_grid_ref_style(&default_ref, &mut style),
            ROASTTY_SUCCESS
        );
        assert!(style_is_default(style));
        assert_eq!(
            roastty_grid_ref_style(&default_ref, ptr::null_mut()),
            ROASTTY_SUCCESS
        );

        let mut style = default_c_style();
        assert_eq!(
            roastty_grid_ref_style(&styled_ref, &mut style),
            ROASTTY_SUCCESS
        );
        assert!(style.bold);
        assert_eq!(style.fg_color.tag, ROASTTY_STYLE_COLOR_PALETTE);
        assert_eq!(style_color_palette(style.fg_color), 1);

        let mut undersized = default_c_style();
        undersized.size = std::mem::size_of::<RoasttyStyle>() - 1;
        assert_eq!(
            roastty_grid_ref_style(&styled_ref, &mut undersized),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn grid_ref_accessor_c_abi_reads_graphemes_and_hyperlinks() {
        let terminal = new_terminal(10, 3);
        write_terminal(
            terminal,
            b"e \x1b]8;id=id;https://example.test\x1b\\L\x1b]8;;\x1b\\",
        );
        terminal_from_handle(terminal)
            .expect("test terminal handle must be valid")
            .terminal
            .append_grapheme_for_tests(0, 0, 0x0301);

        let grapheme_ref = terminal_grid_ref_at(terminal, 0, 0);
        let empty_ref = terminal_grid_ref_at(terminal, 9, 0);
        let link_ref = terminal_grid_ref_at(terminal, 2, 0);

        let mut len = 99;
        assert_eq!(
            roastty_grid_ref_graphemes(&empty_ref, ptr::null_mut(), 0, &mut len),
            ROASTTY_SUCCESS
        );
        assert_eq!(len, 0);
        assert_eq!(
            roastty_grid_ref_graphemes(&grapheme_ref, ptr::null_mut(), 0, &mut len),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(len, 2);

        let mut codepoints = [0; 2];
        assert_eq!(
            roastty_grid_ref_graphemes(
                &grapheme_ref,
                codepoints.as_mut_ptr(),
                codepoints.len(),
                &mut len,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(len, 2);
        assert_eq!(codepoints, ['e' as u32, 0x0301]);
        assert_eq!(
            roastty_grid_ref_graphemes(&grapheme_ref, codepoints.as_mut_ptr(), 1, &mut len),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(
            roastty_grid_ref_graphemes(&link_ref, ptr::null_mut(), 0, &mut len),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(len, 1);
        let mut one_codepoint = [0; 1];
        assert_eq!(
            roastty_grid_ref_graphemes(
                &link_ref,
                one_codepoint.as_mut_ptr(),
                one_codepoint.len(),
                &mut len,
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(len, 1);
        assert_eq!(one_codepoint, ['L' as u32]);
        assert_eq!(
            roastty_grid_ref_graphemes(&grapheme_ref, codepoints.as_mut_ptr(), 2, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        assert_eq!(
            roastty_grid_ref_hyperlink_uri(&empty_ref, ptr::null_mut(), 0, &mut len),
            ROASTTY_SUCCESS
        );
        assert_eq!(len, 0);
        assert_eq!(
            roastty_grid_ref_hyperlink_uri(&link_ref, ptr::null_mut(), 0, &mut len),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(len, "https://example.test".len());

        let mut uri = [0_u8; 32];
        assert_eq!(
            roastty_grid_ref_hyperlink_uri(&link_ref, uri.as_mut_ptr(), 5, &mut len),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(
            roastty_grid_ref_hyperlink_uri(&link_ref, uri.as_mut_ptr(), uri.len(), &mut len),
            ROASTTY_SUCCESS
        );
        assert_eq!(&uri[..len], b"https://example.test");
        assert_eq!(
            roastty_grid_ref_hyperlink_uri(&link_ref, uri.as_mut_ptr(), uri.len(), ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn grid_ref_accessor_c_abi_validates_refs() {
        let terminal = new_terminal(10, 3);
        write_terminal(terminal, b"A");

        let valid_ref = terminal_grid_ref_at(terminal, 0, 0);
        let mut cell = 0;
        assert_eq!(
            roastty_grid_ref_cell(ptr::null(), &mut cell),
            ROASTTY_INVALID_VALUE
        );

        let mut undersized = valid_ref;
        undersized.size = std::mem::size_of::<RoasttyGridRef>() - 1;
        assert_eq!(
            roastty_grid_ref_cell(&undersized, &mut cell),
            ROASTTY_INVALID_VALUE
        );

        let mut null_node = valid_ref;
        null_node.node = ptr::null_mut();
        assert_eq!(
            roastty_grid_ref_cell(&null_node, &mut cell),
            ROASTTY_INVALID_VALUE
        );

        let mut out_of_range = valid_ref;
        out_of_range.x = 99;
        assert_eq!(
            roastty_grid_ref_cell(&out_of_range, &mut cell),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_selection_c_abi_layout_and_values_are_stable() {
        assert_eq!(ROASTTY_TERMINAL_OPTION_SELECTION, 21);
        assert_eq!(ROASTTY_SELECTION_FORMAT_PLAIN, 0);
        assert_eq!(ROASTTY_SELECTION_FORMAT_VT, 1);
        assert_eq!(ROASTTY_SELECTION_FORMAT_HTML, 2);
        assert_eq!(ROASTTY_SELECTION_ORDER_FORWARD, 0);
        assert_eq!(ROASTTY_SELECTION_ORDER_REVERSE, 1);
        assert_eq!(ROASTTY_SELECTION_ORDER_MIRRORED_FORWARD, 2);
        assert_eq!(ROASTTY_SELECTION_ORDER_MIRRORED_REVERSE, 3);
        assert_eq!(ROASTTY_SELECTION_ADJUST_LEFT, 0);
        assert_eq!(ROASTTY_SELECTION_ADJUST_END_OF_LINE, 9);

        assert_eq!(std::mem::size_of::<RoasttySelection>(), 64);
        assert_eq!(std::mem::align_of::<RoasttySelection>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttySelection, size), 0);
        assert_eq!(std::mem::offset_of!(RoasttySelection, start), 8);
        assert_eq!(std::mem::offset_of!(RoasttySelection, end), 32);
        assert_eq!(std::mem::offset_of!(RoasttySelection, rectangle), 56);

        assert_eq!(std::mem::size_of::<RoasttyTerminalSelectWordOptions>(), 48);
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectWordOptions, ref_),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectWordOptions, boundary_codepoints),
            32
        );
        assert_eq!(
            std::mem::size_of::<RoasttyTerminalSelectWordBetweenOptions>(),
            72
        );
        assert_eq!(
            std::mem::align_of::<RoasttyTerminalSelectWordBetweenOptions>(),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectWordBetweenOptions, size),
            0
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectWordBetweenOptions, start),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectWordBetweenOptions, end),
            32
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectWordBetweenOptions, boundary_codepoints),
            56
        );
        assert_eq!(std::mem::size_of::<RoasttyTerminalSelectLineOptions>(), 56);
        assert_eq!(std::mem::align_of::<RoasttyTerminalSelectLineOptions>(), 8);
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectLineOptions, size),
            0
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectLineOptions, ref_),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectLineOptions, whitespace),
            32
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectLineOptions, semantic_prompt_boundary),
            48
        );
        assert_eq!(
            std::mem::size_of::<RoasttyTerminalSelectionFormatOptions>(),
            24
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectionFormatOptions, emit),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyTerminalSelectionFormatOptions, selection),
            16
        );

        assert_eq!(ROASTTY_FORMATTER_FORMAT_PLAIN, 0);
        assert_eq!(ROASTTY_FORMATTER_FORMAT_VT, 1);
        assert_eq!(ROASTTY_FORMATTER_FORMAT_HTML, 2);
        assert_eq!(std::mem::size_of::<RoasttyFormatterScreenExtra>(), 16);
        assert_eq!(std::mem::align_of::<RoasttyFormatterScreenExtra>(), 8);
        assert_eq!(std::mem::offset_of!(RoasttyFormatterScreenExtra, cursor), 8);
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterScreenExtra, charsets),
            13
        );
        assert_eq!(std::mem::size_of::<RoasttyFormatterTerminalExtra>(), 32);
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, palette),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, keyboard),
            13
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterTerminalExtra, screen),
            16
        );
        assert_eq!(std::mem::size_of::<RoasttyFormatterTerminalOptions>(), 56);
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, emit),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, extra),
            16
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyFormatterTerminalOptions, selection),
            48
        );
    }

    #[test]
    fn terminal_selection_c_abi_set_get_clear_and_format_active_selection() {
        let terminal = new_terminal(20, 2);
        write_terminal(terminal, b"Hello World");
        let selection = terminal_selection(terminal, (6, 0), (10, 0), false);

        let mut out = RoasttySelection::default();
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_SELECTION,
                &mut out as *mut _ as *mut c_void
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SELECTION,
                &selection as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_SELECTION,
                &mut out as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(out.size, std::mem::size_of::<RoasttySelection>());
        assert_eq!(out.start.size, std::mem::size_of::<RoasttyGridRef>());
        assert_eq!(out.end.size, std::mem::size_of::<RoasttyGridRef>());
        assert_eq!((out.start.x, out.start.y), (6, 0));
        assert_eq!((out.end.x, out.end.y), (10, 0));

        let options = RoasttyTerminalSelectionFormatOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectionFormatOptions>(),
            emit: ROASTTY_SELECTION_FORMAT_PLAIN,
            unwrap: true,
            trim: true,
            selection: ptr::null(),
        };
        let mut written = 0usize;
        assert_eq!(
            roastty_terminal_selection_format_buf(
                terminal,
                &options,
                ptr::null_mut(),
                0,
                &mut written
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, 5);

        let mut small = [0u8; 2];
        assert_eq!(
            roastty_terminal_selection_format_buf(
                terminal,
                &options,
                small.as_mut_ptr(),
                small.len(),
                &mut written
            ),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, 5);

        let mut buf = [0u8; 16];
        assert_eq!(
            roastty_terminal_selection_format_buf(
                terminal,
                &options,
                buf.as_mut_ptr(),
                buf.len(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"World");

        let explicit = terminal_selection(terminal, (0, 0), (4, 0), false);
        let explicit_options = RoasttyTerminalSelectionFormatOptions {
            selection: &explicit,
            ..options
        };
        let mut formatted = empty_string();
        assert_eq!(
            roastty_terminal_selection_format(terminal, &explicit_options, &mut formatted),
            ROASTTY_SUCCESS
        );
        assert_eq!(take_roastty_string(formatted), b"Hello");

        formatted = empty_string();
        assert_eq!(
            roastty_terminal_set(terminal, ROASTTY_TERMINAL_OPTION_SELECTION, ptr::null()),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_SELECTION,
                &mut out as *mut _ as *mut c_void
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            roastty_terminal_selection_format(terminal, &options, &mut formatted),
            ROASTTY_NO_VALUE
        );

        roastty_terminal_free(terminal);
    }

    #[test]
    fn formatter_c_abi_validates_creation_inputs_and_sizes() {
        let terminal = new_terminal(20, 2);
        let options = formatter_options(ROASTTY_FORMATTER_FORMAT_PLAIN);
        let mut formatter = ptr::dangling_mut();

        assert_eq!(
            roastty_formatter_terminal_new(ptr::null_mut(), terminal, options),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, ptr::null_mut(), options),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut invalid_emit = options;
        invalid_emit.emit = 99;
        formatter = ptr::dangling_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, invalid_emit),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut undersized_options = options;
        undersized_options.size = std::mem::size_of::<usize>() - 1;
        formatter = ptr::dangling_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, undersized_options),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut undersized_extra = options;
        undersized_extra.extra.size = std::mem::size_of::<usize>() - 1;
        formatter = ptr::dangling_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, undersized_extra),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut undersized_screen = options;
        undersized_screen.extra.screen.size = std::mem::size_of::<usize>() - 1;
        formatter = ptr::dangling_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, undersized_screen),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut uncontained_extra = options;
        uncontained_extra.size = std::mem::offset_of!(RoasttyFormatterTerminalOptions, extra)
            + std::mem::size_of::<usize>();
        uncontained_extra.extra.size = std::mem::size_of::<RoasttyFormatterTerminalExtra>();
        formatter = ptr::dangling_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, uncontained_extra),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut uncontained_screen = options;
        uncontained_screen.extra.size = std::mem::offset_of!(RoasttyFormatterTerminalExtra, screen)
            + std::mem::size_of::<usize>();
        uncontained_screen.extra.screen.size = std::mem::size_of::<RoasttyFormatterScreenExtra>();
        formatter = ptr::dangling_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, uncontained_screen),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatter.is_null());

        let mut size_only = options;
        size_only.size = std::mem::size_of::<usize>();
        formatter = ptr::null_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, size_only),
            ROASTTY_SUCCESS
        );
        assert!(!formatter.is_null());
        roastty_formatter_free(formatter);

        let mut partial_screen = options;
        partial_screen.emit = ROASTTY_FORMATTER_FORMAT_VT;
        partial_screen.extra.size = std::mem::offset_of!(RoasttyFormatterTerminalExtra, screen)
            + std::mem::offset_of!(RoasttyFormatterScreenExtra, cursor)
            + std::mem::size_of::<bool>();
        partial_screen.extra.screen.size =
            std::mem::offset_of!(RoasttyFormatterScreenExtra, cursor) + std::mem::size_of::<bool>();
        partial_screen.extra.screen.cursor = true;
        partial_screen.size = std::mem::offset_of!(RoasttyFormatterTerminalOptions, extra)
            + partial_screen.extra.size;
        formatter = ptr::null_mut();
        assert_eq!(
            roastty_formatter_terminal_new(&mut formatter, terminal, partial_screen),
            ROASTTY_SUCCESS
        );
        assert!(!formatter.is_null());
        let output = formatter_string(formatter);
        let output = std::str::from_utf8(&output).unwrap();
        assert!(output.ends_with('H'));
        roastty_formatter_free(formatter);

        roastty_formatter_free(ptr::null_mut());
        roastty_terminal_free(terminal);
    }

    #[test]
    fn formatter_c_abi_formats_plain_to_caller_buffer() {
        let terminal = new_terminal(20, 2);
        write_terminal(terminal, b"Hello");
        let formatter = new_formatter(terminal, formatter_options(ROASTTY_FORMATTER_FORMAT_PLAIN));

        let mut written = 999usize;
        assert_eq!(
            roastty_formatter_format_buf(ptr::null_mut(), ptr::null_mut(), 0, &mut written),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_formatter_format_buf(formatter, ptr::null_mut(), 0, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );

        assert_eq!(
            roastty_formatter_format_buf(formatter, ptr::null_mut(), 0, &mut written),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, 5);
        assert_eq!(
            roastty_formatter_format_buf(formatter, ptr::null_mut(), 99, &mut written),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, 5);

        let mut small = [0u8; 2];
        assert_eq!(
            roastty_formatter_format_buf(formatter, small.as_mut_ptr(), small.len(), &mut written),
            ROASTTY_OUT_OF_SPACE
        );
        assert_eq!(written, 5);

        let mut buf = [0u8; 16];
        assert_eq!(
            roastty_formatter_format_buf(formatter, buf.as_mut_ptr(), buf.len(), &mut written),
            ROASTTY_SUCCESS
        );
        assert_eq!(&buf[..written], b"Hello");

        roastty_formatter_free(formatter);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn formatter_c_abi_formats_allocated_string_and_explicit_selection() {
        let terminal = new_terminal(20, 2);
        write_terminal(terminal, b"Hello World");
        let selection = terminal_selection(terminal, (6, 0), (10, 0), false);
        let mut options = formatter_options(ROASTTY_FORMATTER_FORMAT_PLAIN);
        options.selection = &selection;
        let formatter = new_formatter(terminal, options);

        let mut out = empty_string();
        assert_eq!(
            roastty_formatter_format(ptr::null_mut(), &mut out),
            ROASTTY_INVALID_VALUE
        );
        assert!(out.ptr.is_null());
        assert_eq!(out.len, 0);
        assert!(!out.sentinel);
        assert_eq!(
            roastty_formatter_format(formatter, ptr::null_mut()),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(formatter_string(formatter), b"World");

        roastty_formatter_free(formatter);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn formatter_c_abi_null_selection_formats_full_screen_not_active_selection() {
        let terminal = new_terminal(20, 2);
        write_terminal(terminal, b"Hello World");
        let active = terminal_selection(terminal, (6, 0), (10, 0), false);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SELECTION,
                &active as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );

        let formatter = new_formatter(terminal, formatter_options(ROASTTY_FORMATTER_FORMAT_PLAIN));
        assert_eq!(formatter_string(formatter), b"Hello World");

        roastty_formatter_free(formatter);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn formatter_c_abi_formats_vt_html_and_representative_extras() {
        let terminal = new_terminal(20, 3);
        write_terminal(terminal, b"<hi");

        let html = new_formatter(terminal, formatter_options(ROASTTY_FORMATTER_FORMAT_HTML));
        let html_output = formatter_string(html);
        assert!(std::str::from_utf8(&html_output)
            .unwrap()
            .contains("&lt;hi"));
        roastty_formatter_free(html);

        let mut options = formatter_options(ROASTTY_FORMATTER_FORMAT_VT);
        options.extra.palette = true;
        options.extra.screen.cursor = true;
        let vt = new_formatter(terminal, options);
        let vt_output = formatter_string(vt);
        let vt_output = std::str::from_utf8(&vt_output).unwrap();
        assert!(vt_output.starts_with("\x1b]4;0;"));
        assert!(vt_output.contains("<hi"));
        assert!(vt_output.ends_with('H'));

        roastty_formatter_free(vt);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_selection_c_abi_select_helpers_and_relations() {
        let terminal = new_terminal(20, 3);
        write_terminal(terminal, b"Hello World\r\nsecond line");
        let ref_ = terminal_grid_ref_at(terminal, 7, 0);
        let options = RoasttyTerminalSelectWordOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectWordOptions>(),
            ref_,
            boundary_codepoints: ptr::null(),
            boundary_codepoints_len: 0,
        };
        let mut selection = RoasttySelection::default();
        assert_eq!(
            roastty_terminal_select_word(terminal, &options, &mut selection),
            ROASTTY_SUCCESS
        );
        assert_eq!((selection.start.x, selection.start.y), (6, 0));
        assert_eq!((selection.end.x, selection.end.y), (10, 0));

        let mut order = -1;
        assert_eq!(
            roastty_terminal_selection_order(terminal, &selection, &mut order),
            ROASTTY_SUCCESS
        );
        assert_eq!(order, ROASTTY_SELECTION_ORDER_FORWARD);

        let mut contains = false;
        assert_eq!(
            roastty_terminal_selection_contains(
                terminal,
                &selection,
                c_point(ROASTTY_POINT_SCREEN, 8, 0),
                &mut contains
            ),
            ROASTTY_SUCCESS
        );
        assert!(contains);

        let mut equal = false;
        assert_eq!(
            roastty_terminal_selection_equal(terminal, &selection, &selection, &mut equal),
            ROASTTY_SUCCESS
        );
        assert!(equal);

        let mut adjusted = selection;
        assert_eq!(
            roastty_terminal_selection_adjust(
                terminal,
                &mut adjusted,
                ROASTTY_SELECTION_ADJUST_END_OF_LINE
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(adjusted.end.x, 19);

        let mut reversed = RoasttySelection::default();
        assert_eq!(
            roastty_terminal_selection_ordered(
                terminal,
                &selection,
                ROASTTY_SELECTION_ORDER_REVERSE,
                &mut reversed
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((reversed.start.x, reversed.end.x), (10, 6));

        let line_options = RoasttyTerminalSelectLineOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectLineOptions>(),
            ref_: terminal_grid_ref_at(terminal, 2, 1),
            whitespace: ptr::null(),
            whitespace_len: 0,
            semantic_prompt_boundary: false,
        };
        assert_eq!(
            roastty_terminal_select_line(terminal, &line_options, &mut selection),
            ROASTTY_SUCCESS
        );
        assert_eq!((selection.start.x, selection.start.y), (0, 1));

        assert_eq!(
            roastty_terminal_select_all(terminal, &mut selection),
            ROASTTY_SUCCESS
        );
        assert_eq!((selection.start.x, selection.start.y), (0, 0));

        roastty_terminal_free(terminal);
    }

    #[test]
    fn terminal_selection_c_abi_validates_inputs_atomically() {
        let terminal = new_terminal(20, 2);
        write_terminal(terminal, b"Hello World");
        let selection = terminal_selection(terminal, (6, 0), (10, 0), false);
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SELECTION,
                &selection as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );

        let mut invalid_replacement = selection;
        invalid_replacement.end.x = 99;
        assert_eq!(
            roastty_terminal_set(
                terminal,
                ROASTTY_TERMINAL_OPTION_SELECTION,
                &invalid_replacement as *const _ as *const c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        let mut out = RoasttySelection::default();
        assert_eq!(
            roastty_terminal_get(
                terminal,
                ROASTTY_TERMINAL_DATA_SELECTION,
                &mut out as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((out.start.x, out.end.x), (6, 10));

        let mut undersized = selection;
        undersized.size = std::mem::size_of::<RoasttySelection>() - 1;
        let mut result = RoasttySelection::default();
        let mut equal = false;
        assert_eq!(
            roastty_terminal_selection_equal(terminal, &undersized, &selection, &mut equal),
            ROASTTY_INVALID_VALUE
        );
        let mut undersized_nested = selection;
        undersized_nested.start.size = std::mem::size_of::<RoasttyGridRef>() - 1;
        assert_eq!(
            roastty_terminal_selection_equal(terminal, &undersized_nested, &selection, &mut equal),
            ROASTTY_INVALID_VALUE
        );
        let mut forged_y = selection;
        forged_y.end.y = 99;
        let mut order = 0;
        assert_eq!(
            roastty_terminal_selection_order(terminal, &forged_y, &mut order),
            ROASTTY_INVALID_VALUE
        );
        let other = new_terminal(20, 2);
        assert_eq!(
            roastty_terminal_selection_order(other, &selection, &mut order),
            ROASTTY_NO_VALUE
        );
        roastty_terminal_free(other);

        assert_eq!(
            roastty_terminal_select_word(terminal, ptr::null(), &mut result),
            ROASTTY_INVALID_VALUE
        );
        let mut word_options = RoasttyTerminalSelectWordOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectWordOptions>(),
            ref_: terminal_grid_ref_at(terminal, 7, 0),
            boundary_codepoints: ptr::null(),
            boundary_codepoints_len: 1,
        };
        let original_word_options = word_options;
        word_options.size = std::mem::size_of::<RoasttyTerminalSelectWordOptions>() - 1;
        assert_eq!(
            roastty_terminal_select_word(terminal, &word_options, &mut result),
            ROASTTY_INVALID_VALUE
        );
        word_options = original_word_options;
        word_options.ref_.size = std::mem::size_of::<RoasttyGridRef>() - 1;
        assert_eq!(
            roastty_terminal_select_word(terminal, &word_options, &mut result),
            ROASTTY_INVALID_VALUE
        );
        word_options = original_word_options;
        assert_eq!(
            roastty_terminal_select_word(terminal, &word_options, &mut result),
            ROASTTY_INVALID_VALUE
        );
        let invalid_scalar = [0xD800u32];
        word_options.boundary_codepoints = invalid_scalar.as_ptr();
        assert_eq!(
            roastty_terminal_select_word(terminal, &word_options, &mut result),
            ROASTTY_INVALID_VALUE
        );
        word_options.boundary_codepoints_len = 0;
        assert_eq!(
            roastty_terminal_select_word(terminal, &word_options, &mut result),
            ROASTTY_SUCCESS
        );

        let mut between_options = RoasttyTerminalSelectWordBetweenOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectWordBetweenOptions>(),
            start: terminal_grid_ref_at(terminal, 0, 0),
            end: terminal_grid_ref_at(terminal, 10, 0),
            boundary_codepoints: ptr::null(),
            boundary_codepoints_len: 0,
        };
        between_options.size = std::mem::size_of::<RoasttyTerminalSelectWordBetweenOptions>() - 1;
        assert_eq!(
            roastty_terminal_select_word_between(terminal, &between_options, &mut result),
            ROASTTY_INVALID_VALUE
        );
        between_options.size = std::mem::size_of::<RoasttyTerminalSelectWordBetweenOptions>();
        between_options.end.size = std::mem::size_of::<RoasttyGridRef>() - 1;
        assert_eq!(
            roastty_terminal_select_word_between(terminal, &between_options, &mut result),
            ROASTTY_INVALID_VALUE
        );

        let mut line_options = RoasttyTerminalSelectLineOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectLineOptions>(),
            ref_: terminal_grid_ref_at(terminal, 0, 0),
            whitespace: ptr::null(),
            whitespace_len: 0,
            semantic_prompt_boundary: false,
        };
        line_options.size = std::mem::size_of::<RoasttyTerminalSelectLineOptions>() - 1;
        assert_eq!(
            roastty_terminal_select_line(terminal, &line_options, &mut result),
            ROASTTY_INVALID_VALUE
        );
        line_options.size = std::mem::size_of::<RoasttyTerminalSelectLineOptions>();
        line_options.ref_.size = std::mem::size_of::<RoasttyGridRef>() - 1;
        assert_eq!(
            roastty_terminal_select_line(terminal, &line_options, &mut result),
            ROASTTY_INVALID_VALUE
        );

        let mut options = RoasttyTerminalSelectionFormatOptions {
            size: std::mem::size_of::<RoasttyTerminalSelectionFormatOptions>(),
            emit: 99,
            unwrap: true,
            trim: true,
            selection: ptr::null(),
        };
        options.size = std::mem::size_of::<RoasttyTerminalSelectionFormatOptions>() - 1;
        let mut formatted = empty_string();
        assert_eq!(
            roastty_terminal_selection_format(terminal, &options, &mut formatted),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatted.ptr.is_null());
        options.size = std::mem::size_of::<RoasttyTerminalSelectionFormatOptions>();
        let mut formatted = empty_string();
        assert_eq!(
            roastty_terminal_selection_format(terminal, &options, &mut formatted),
            ROASTTY_INVALID_VALUE
        );
        assert!(formatted.ptr.is_null());
        assert_eq!(
            roastty_terminal_selection_ordered(terminal, &selection, 99, &mut result),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_selection_adjust(terminal, &mut result, 99),
            ROASTTY_INVALID_VALUE
        );
        let mut contains = false;
        assert_eq!(
            roastty_terminal_selection_contains(
                terminal,
                &selection,
                c_point(99, 0, 0),
                &mut contains
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_terminal_selection_contains(
                terminal,
                &selection,
                c_point(ROASTTY_POINT_SCREEN, 99, 0),
                &mut contains
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_terminal_free(terminal);
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
    fn kitty_graphics_c_abi_returns_image_snapshot_handles() {
        let terminal = new_terminal(10, 3);
        write_kitty_transmit_display(terminal, 7, 4, 1);
        let graphics = kitty_graphics_handle(terminal);

        assert!(roastty_kitty_graphics_image(ptr::null_mut(), 7).is_null());
        assert!(roastty_kitty_graphics_image(graphics, 99).is_null());
        let image = roastty_kitty_graphics_image(graphics, 7);
        assert!(!image.is_null());

        let mut image_id = 0u32;
        let mut number = 99u32;
        let mut width = 0u32;
        let mut height = 0u32;
        let mut format = -1;
        let mut compression = -1;
        let mut data_ptr = ptr::null();
        let mut data_len = 0usize;
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID,
                (&mut image_id as *mut u32).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER,
                (&mut number as *mut u32).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH,
                (&mut width as *mut u32).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT,
                (&mut height as *mut u32).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT,
                (&mut format as *mut c_int).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION,
                (&mut compression as *mut c_int).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR,
                (&mut data_ptr as *mut *const u8).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_image_get(
                image,
                ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN,
                (&mut data_len as *mut usize).cast()
            ),
            ROASTTY_SUCCESS
        );

        assert_eq!((image_id, number, width, height), (7, 0, 1, 1));
        assert_eq!(format, ROASTTY_KITTY_IMAGE_FORMAT_RGBA);
        assert_eq!(compression, ROASTTY_KITTY_IMAGE_COMPRESSION_NONE);
        assert_eq!(data_len, 4);
        assert_eq!(
            unsafe { slice::from_raw_parts(data_ptr, data_len) },
            &[1u8, 2, 3, 4]
        );

        write_kitty_transmit_display(terminal, 7, 5, 1);
        assert_eq!(
            unsafe { slice::from_raw_parts(data_ptr, data_len) },
            &[1u8, 2, 3, 4]
        );
        roastty_kitty_graphics_image_free(image);
        roastty_kitty_graphics_image_free(ptr::null_mut());
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_c_abi_image_get_multi_reports_partial_counts() {
        let terminal = new_terminal(10, 3);
        write_kitty_transmit_display(terminal, 7, 4, 1);
        let image = roastty_kitty_graphics_image(kitty_graphics_handle(terminal), 7);
        assert!(!image.is_null());

        let mut image_id = 0u32;
        let mut width = 0u32;
        let mut height = 0u32;
        let keys = [
            ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID,
            ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH,
            ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT,
        ];
        let mut values = [
            (&mut image_id as *mut u32).cast(),
            (&mut width as *mut u32).cast(),
            (&mut height as *mut u32).cast(),
        ];
        let mut written = 99usize;
        assert_eq!(
            roastty_kitty_graphics_image_get_multi(
                image,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 3);
        assert_eq!((image_id, width, height), (7, 1, 1));

        let invalid_keys = [
            ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID,
            ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID,
        ];
        written = 99;
        assert_eq!(
            roastty_kitty_graphics_image_get_multi(
                image,
                invalid_keys.len(),
                invalid_keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);
        assert_eq!(
            roastty_kitty_graphics_image_get_multi(
                image,
                1,
                keys.as_ptr(),
                ptr::null_mut(),
                &mut written
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_kitty_graphics_image_free(image);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_c_abi_iterates_owned_placement_snapshots() {
        let terminal = new_terminal(10, 3);
        write_kitty_transmit_display(terminal, 7, 4, 1);
        write_kitty_transmit_display(terminal, 8, 5, -1);
        let graphics = kitty_graphics_handle(terminal);
        let mut iterator = ptr::null_mut();
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_new(&mut iterator),
            ROASTTY_SUCCESS
        );
        assert!(!iterator.is_null());
        assert_eq!(
            roastty_kitty_graphics_placement_get(
                iterator,
                ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
                (&mut 0u32 as *mut u32).cast()
            ),
            ROASTTY_NO_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_get(
                graphics,
                ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
                (&mut iterator as *mut RoasttyKittyGraphicsPlacementIterator).cast()
            ),
            ROASTTY_SUCCESS
        );

        let layer = ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT;
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_set(
                iterator,
                ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER,
                (&layer as *const c_int).cast()
            ),
            ROASTTY_SUCCESS
        );
        assert!(roastty_kitty_graphics_placement_next(iterator));

        let mut image_id = 0u32;
        let mut placement_id = 0u32;
        let mut virtual_placement = true;
        let mut x_offset = 0u32;
        let mut y_offset = 0u32;
        let mut source_x = 0u32;
        let mut source_y = 0u32;
        let mut source_width = 0u32;
        let mut source_height = 0u32;
        let mut columns = 0u32;
        let mut rows = 0u32;
        let mut z = i32::MIN;
        let keys = [
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_X,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_Y,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_WIDTH,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_HEIGHT,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z,
        ];
        let mut values = [
            (&mut image_id as *mut u32).cast(),
            (&mut placement_id as *mut u32).cast(),
            (&mut virtual_placement as *mut bool).cast(),
            (&mut x_offset as *mut u32).cast(),
            (&mut y_offset as *mut u32).cast(),
            (&mut source_x as *mut u32).cast(),
            (&mut source_y as *mut u32).cast(),
            (&mut source_width as *mut u32).cast(),
            (&mut source_height as *mut u32).cast(),
            (&mut columns as *mut u32).cast(),
            (&mut rows as *mut u32).cast(),
            (&mut z as *mut i32).cast(),
        ];
        let mut written = 99usize;
        assert_eq!(
            roastty_kitty_graphics_placement_get_multi(
                iterator,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(written, 12);
        assert_eq!((image_id, placement_id, virtual_placement), (7, 4, false));
        assert_eq!((x_offset, y_offset), (6, 7));
        assert_eq!((source_x, source_y), (2, 3));
        assert_eq!((source_width, source_height), (4, 5));
        assert_eq!((columns, rows, z), (3, 2, 1));

        let invalid_keys = [
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
            ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID,
        ];
        let mut invalid_values = [
            (&mut image_id as *mut u32).cast(),
            (&mut placement_id as *mut u32).cast(),
        ];
        written = 99;
        assert_eq!(
            roastty_kitty_graphics_placement_get_multi(
                iterator,
                invalid_keys.len(),
                invalid_keys.as_ptr(),
                invalid_values.as_mut_ptr(),
                &mut written
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(written, 1);

        write_kitty_transmit_display(terminal, 9, 6, 1);
        assert!(!roastty_kitty_graphics_placement_next(iterator));
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_placement_iterator_free(ptr::null_mut());
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_c_abi_placement_layers_and_internal_ids() {
        let terminal = new_terminal(10, 3);
        write_kitty_transmit_display(terminal, 7, 4, 1);
        write_kitty_transmit_display(terminal, 8, 5, -1);
        write_kitty_transmit_display(terminal, 9, 6, -1_500_000_000);
        write_kitty_transmit_display(terminal, 10, 0, 1);
        let graphics = kitty_graphics_handle(terminal);
        let mut iterator = ptr::null_mut();
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_new(&mut iterator),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_get(
                graphics,
                ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
                (&mut iterator as *mut RoasttyKittyGraphicsPlacementIterator).cast()
            ),
            ROASTTY_SUCCESS
        );

        for (layer, expected) in [
            (ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG, vec![9u32]),
            (ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT, vec![8u32]),
            (ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT, vec![7u32, 10]),
        ] {
            assert_eq!(
                roastty_kitty_graphics_placement_iterator_set(
                    iterator,
                    ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER,
                    (&layer as *const c_int).cast()
                ),
                ROASTTY_SUCCESS
            );
            let mut got = Vec::new();
            while roastty_kitty_graphics_placement_next(iterator) {
                let mut image_id = 0u32;
                assert_eq!(
                    roastty_kitty_graphics_placement_get(
                        iterator,
                        ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
                        (&mut image_id as *mut u32).cast()
                    ),
                    ROASTTY_SUCCESS
                );
                got.push(image_id);
            }
            got.sort_unstable();
            assert_eq!(got, expected);
        }

        let all = ROASTTY_KITTY_PLACEMENT_LAYER_ALL;
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_set(
                iterator,
                ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER,
                (&all as *const c_int).cast()
            ),
            ROASTTY_SUCCESS
        );
        let mut found_internal = false;
        while roastty_kitty_graphics_placement_next(iterator) {
            let mut image_id = 0u32;
            let mut placement_id = u32::MAX;
            assert_eq!(
                roastty_kitty_graphics_placement_get(
                    iterator,
                    ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
                    (&mut image_id as *mut u32).cast()
                ),
                ROASTTY_SUCCESS
            );
            assert_eq!(
                roastty_kitty_graphics_placement_get(
                    iterator,
                    ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID,
                    (&mut placement_id as *mut u32).cast()
                ),
                ROASTTY_SUCCESS
            );
            if image_id == 10 {
                assert_eq!(placement_id, 0);
                found_internal = true;
            }
        }
        assert!(found_internal);

        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_c_abi_validates_graphics_and_iterator_handles() {
        let terminal = new_terminal(10, 3);
        let mut iterator = ptr::null_mut();
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_new(&mut iterator),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_kitty_graphics_get(
                ptr::null_mut(),
                ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
                (&mut iterator as *mut RoasttyKittyGraphicsPlacementIterator).cast()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_get(
                kitty_graphics_handle(terminal),
                ROASTTY_KITTY_GRAPHICS_DATA_INVALID,
                (&mut iterator as *mut RoasttyKittyGraphicsPlacementIterator).cast()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_get(
                kitty_graphics_handle(terminal),
                ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
        let bad_layer = 99;
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_set(
                iterator,
                ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER,
                (&bad_layer as *const c_int).cast()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_iterator_set(iterator, 99, ptr::null()),
            ROASTTY_INVALID_VALUE
        );
        assert!(!roastty_kitty_graphics_placement_next(ptr::null_mut()));
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_layout_values_are_stable() {
        assert_eq!(
            std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            56
        );
        assert_eq!(
            std::mem::align_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, size),
            0
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, pixel_width),
            8
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, pixel_height),
            12
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, grid_cols),
            16
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, grid_rows),
            20
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, viewport_col),
            24
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, viewport_row),
            28
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, viewport_visible),
            32
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, source_x),
            36
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, source_y),
            40
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, source_width),
            44
        );
        assert_eq!(
            std::mem::offset_of!(RoasttyKittyGraphicsPlacementRenderInfo, source_height),
            48
        );
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_reports_geometry_helpers() {
        let terminal = new_terminal(10, 5);
        set_kitty_metrics(terminal, 100, 50);
        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 7,
                placement_id: 4,
                image_width: 4,
                image_height: 4,
                source_x: 1,
                source_y: 2,
                source_width: 2,
                source_height: 1,
                x_offset: 3,
                y_offset: 4,
                columns: 2,
                rows: 3,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 7);

        let mut width = 0u32;
        let mut height = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((width, height), (20, 30));

        let mut cols = 0u32;
        let mut rows = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_grid_size(
                iterator, image, terminal, &mut cols, &mut rows
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((cols, rows), (2, 3));

        let mut source_x = 0u32;
        let mut source_y = 0u32;
        let mut source_width = 0u32;
        let mut source_height = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_source_rect(
                iterator,
                image,
                &mut source_x,
                &mut source_y,
                &mut source_width,
                &mut source_height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            (source_x, source_y, source_width, source_height),
            (1, 2, 2, 1)
        );

        let mut rect = RoasttySelection {
            size: std::mem::size_of::<RoasttySelection>(),
            ..RoasttySelection::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_rect(iterator, image, terminal, &mut rect),
            ROASTTY_SUCCESS
        );
        assert!(rect.rectangle);
        assert_eq!((rect.start.x, rect.start.y), (0, 0));
        assert_eq!((rect.end.x, rect.end.y), (1, 2));

        let mut viewport_col = 99i32;
        let mut viewport_row = 99i32;
        assert_eq!(
            roastty_kitty_graphics_placement_viewport_pos(
                iterator,
                image,
                terminal,
                &mut viewport_col,
                &mut viewport_row
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((viewport_col, viewport_row), (0, 0));

        let mut info = RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            ..RoasttyKittyGraphicsPlacementRenderInfo::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(iterator, image, terminal, &mut info),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            info.size,
            std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>()
        );
        assert_eq!((info.pixel_width, info.pixel_height), (20, 30));
        assert_eq!((info.grid_cols, info.grid_rows), (2, 3));
        assert_eq!((info.viewport_col, info.viewport_row), (0, 0));
        assert!(info.viewport_visible);
        assert_eq!(
            (
                info.source_x,
                info.source_y,
                info.source_width,
                info.source_height
            ),
            (1, 2, 2, 1)
        );

        let mut undersized_rect = RoasttySelection {
            size: std::mem::size_of::<RoasttySelection>() - 1,
            rectangle: true,
            ..RoasttySelection::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_rect(iterator, image, terminal, &mut undersized_rect),
            ROASTTY_INVALID_VALUE
        );
        assert!(undersized_rect.rectangle);

        let mut undersized_info = RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>() - 1,
            pixel_width: 123,
            ..RoasttyKittyGraphicsPlacementRenderInfo::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(
                iterator,
                image,
                terminal,
                &mut undersized_info
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(undersized_info.pixel_width, 123);

        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_reports_aspect_ratio_and_source_clamps() {
        let terminal = new_terminal(10, 5);
        set_kitty_metrics(terminal, 100, 50);

        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 8,
                placement_id: 1,
                image_width: 2,
                image_height: 4,
                source_x: 0,
                source_y: 0,
                source_width: 0,
                source_height: 0,
                x_offset: 0,
                y_offset: 0,
                columns: 0,
                rows: 0,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 8);
        let mut width = 0u32;
        let mut height = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((width, height), (2, 4));
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);

        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 9,
                placement_id: 1,
                image_width: 2,
                image_height: 4,
                source_x: 0,
                source_y: 0,
                source_width: 0,
                source_height: 0,
                x_offset: 0,
                y_offset: 0,
                columns: 3,
                rows: 0,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 9);
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((width, height), (30, 60));
        let mut cols = 0u32;
        let mut rows = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_grid_size(
                iterator, image, terminal, &mut cols, &mut rows
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((cols, rows), (3, 6));
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);

        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 10,
                placement_id: 1,
                image_width: 2,
                image_height: 4,
                source_x: 0,
                source_y: 0,
                source_width: 0,
                source_height: 0,
                x_offset: 9,
                y_offset: 9,
                columns: 0,
                rows: 2,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 10);
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((width, height), (10, 20));
        assert_eq!(
            roastty_kitty_graphics_placement_grid_size(
                iterator, image, terminal, &mut cols, &mut rows
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((cols, rows), (2, 3));
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);

        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 12,
                placement_id: 1,
                image_width: 2,
                image_height: 4,
                source_x: 1,
                source_y: 3,
                source_width: 99,
                source_height: 0,
                x_offset: 0,
                y_offset: 0,
                columns: 1,
                rows: 1,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 12);
        let mut source_x = 0u32;
        let mut source_y = 0u32;
        let mut source_width = 0u32;
        let mut source_height = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_source_rect(
                iterator,
                image,
                &mut source_x,
                &mut source_y,
                &mut source_width,
                &mut source_height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            (source_x, source_y, source_width, source_height),
            (1, 3, 1, 1)
        );
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);

        set_kitty_metrics(terminal, 0, 0);
        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 11,
                placement_id: 1,
                image_width: 1,
                image_height: 1,
                source_x: 0,
                source_y: 0,
                source_width: 0,
                source_height: 0,
                x_offset: 0,
                y_offset: 0,
                columns: 2,
                rows: 3,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 11);
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((width, height), (0, 0));
        assert_eq!(
            roastty_kitty_graphics_placement_grid_size(
                iterator, image, terminal, &mut cols, &mut rows
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!((cols, rows), (2, 3));
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_revalidates_stale_iterator_selection() {
        let terminal = new_terminal(10, 5);
        set_kitty_metrics(terminal, 100, 50);
        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 7,
                placement_id: 4,
                image_width: 4,
                image_height: 4,
                source_x: 1,
                source_y: 0,
                source_width: 1,
                source_height: 1,
                x_offset: 0,
                y_offset: 0,
                columns: 2,
                rows: 2,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 7);

        write_kitty_delete(terminal, "d=i,i=7,p=4");
        let mut width = 0u32;
        let mut height = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_NO_VALUE
        );
        let mut info = RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            ..RoasttyKittyGraphicsPlacementRenderInfo::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(iterator, image, terminal, &mut info),
            ROASTTY_NO_VALUE
        );

        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_uses_live_replacement_geometry() {
        let terminal = new_terminal(10, 5);
        set_kitty_metrics(terminal, 100, 50);
        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 7,
                placement_id: 4,
                image_width: 4,
                image_height: 4,
                source_x: 1,
                source_y: 0,
                source_width: 1,
                source_height: 1,
                x_offset: 0,
                y_offset: 0,
                columns: 2,
                rows: 2,
                z: 1,
            },
        );
        let (image, iterator) = selected_kitty_placement(terminal, 7);

        write_terminal(terminal, b"\x1b[4;5H");
        write_kitty_fixture(
            terminal,
            KittyDisplayFixture {
                image_id: 7,
                placement_id: 4,
                image_width: 4,
                image_height: 4,
                source_x: 2,
                source_y: 0,
                source_width: 1,
                source_height: 1,
                x_offset: 0,
                y_offset: 0,
                columns: 3,
                rows: 1,
                z: 1,
            },
        );

        let mut rect = RoasttySelection {
            size: std::mem::size_of::<RoasttySelection>(),
            ..RoasttySelection::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_rect(iterator, image, terminal, &mut rect),
            ROASTTY_SUCCESS
        );
        assert_eq!((rect.start.x, rect.start.y), (4, 3));
        assert_eq!((rect.end.x, rect.end.y), (6, 3));

        let mut info = RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            ..RoasttyKittyGraphicsPlacementRenderInfo::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(iterator, image, terminal, &mut info),
            ROASTTY_SUCCESS
        );
        assert_eq!((info.grid_cols, info.grid_rows), (3, 1));
        assert_eq!((info.viewport_col, info.viewport_row), (4, 3));
        assert_eq!(
            (
                info.source_x,
                info.source_y,
                info.source_width,
                info.source_height
            ),
            (1, 0, 1, 1)
        );
        let mut source_x = 0u32;
        let mut source_y = 0u32;
        let mut source_width = 0u32;
        let mut source_height = 0u32;
        assert_eq!(
            roastty_kitty_graphics_placement_source_rect(
                iterator,
                image,
                &mut source_x,
                &mut source_y,
                &mut source_width,
                &mut source_height
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            (
                info.source_x,
                info.source_y,
                info.source_width,
                info.source_height
            ),
            (source_x, source_y, source_width, source_height)
        );

        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_reports_viewport_visibility() {
        let viewport_case = |cursor_row: u32,
                             image_id: u32,
                             rows: u32,
                             scroll_lines_before_place: u32,
                             scroll_lines_after_place: u32,
                             viewport_delta: isize|
         -> (i32, bool, c_int) {
            let terminal = new_terminal(10, 5);
            set_kitty_metrics(terminal, 100, 50);
            for _ in 0..scroll_lines_before_place {
                write_terminal(terminal, b"\x1b[5;1H\n");
            }
            write_terminal(terminal, format!("\x1b[{};1H", cursor_row + 1).as_bytes());
            write_kitty_fixture(
                terminal,
                KittyDisplayFixture {
                    image_id,
                    placement_id: 4,
                    image_width: 1,
                    image_height: 1,
                    source_x: 0,
                    source_y: 0,
                    source_width: 0,
                    source_height: 0,
                    x_offset: 0,
                    y_offset: 0,
                    columns: 1,
                    rows,
                    z: 1,
                },
            );
            for _ in 0..scroll_lines_after_place {
                write_terminal(terminal, b"\x1b[5;1H\n");
            }
            if viewport_delta != 0 {
                terminal_from_handle(terminal)
                    .unwrap()
                    .terminal
                    .scroll_selection_gesture_viewport(viewport_delta);
            }
            let (image, iterator) = selected_kitty_placement(terminal, image_id);
            let mut info = RoasttyKittyGraphicsPlacementRenderInfo {
                size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
                ..RoasttyKittyGraphicsPlacementRenderInfo::default()
            };
            assert_eq!(
                roastty_kitty_graphics_placement_render_info(iterator, image, terminal, &mut info),
                ROASTTY_SUCCESS
            );
            let mut col = 99i32;
            let mut row = 99i32;
            let pos_result = roastty_kitty_graphics_placement_viewport_pos(
                iterator, image, terminal, &mut col, &mut row,
            );
            roastty_kitty_graphics_placement_iterator_free(iterator);
            roastty_kitty_graphics_image_free(image);
            roastty_terminal_free(terminal);
            (info.viewport_row, info.viewport_visible, pos_result)
        };

        assert_eq!(viewport_case(0, 7, 1, 0, 0, 0), (0, true, ROASTTY_SUCCESS));
        assert_eq!(viewport_case(0, 8, 3, 0, 1, 0), (-1, true, ROASTTY_SUCCESS));
        assert_eq!(viewport_case(4, 9, 3, 0, 0, 0), (4, true, ROASTTY_SUCCESS));
        assert_eq!(
            viewport_case(0, 10, 10, 0, 3, 0),
            (-3, true, ROASTTY_SUCCESS)
        );
        assert_eq!(
            viewport_case(0, 11, 1, 0, 2, 0),
            (-2, false, ROASTTY_NO_VALUE)
        );
        assert_eq!(
            viewport_case(4, 12, 1, 8, 0, -8),
            (12, false, ROASTTY_NO_VALUE)
        );

        let terminal = new_terminal(10, 5);
        set_kitty_metrics(terminal, 100, 50);
        write_terminal(
            terminal,
            b"\x1b_Ga=T,f=32,s=1,v=1,i=13,p=4,U=1,C=1;AQIDBA==\x1b\\",
        );
        let _ = terminal_string(terminal, roastty_terminal_take_pty_response);
        let (image, iterator) = selected_kitty_placement(terminal, 13);
        let mut info = RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            ..RoasttyKittyGraphicsPlacementRenderInfo::default()
        };
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(iterator, image, terminal, &mut info),
            ROASTTY_SUCCESS
        );
        assert!(!info.viewport_visible);
        let mut col = 99i32;
        let mut row = 99i32;
        assert_eq!(
            roastty_kitty_graphics_placement_viewport_pos(
                iterator, image, terminal, &mut col, &mut row
            ),
            ROASTTY_NO_VALUE
        );
        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);
        roastty_terminal_free(terminal);
    }

    #[test]
    fn kitty_graphics_render_info_c_abi_validates_handles_and_outputs() {
        let terminal = new_terminal(10, 5);
        write_kitty_transmit_display(terminal, 7, 4, 1);
        let (image, iterator) = selected_kitty_placement(terminal, 7);
        let mut width = 0u32;
        let mut height = 0u32;
        let mut info = RoasttyKittyGraphicsPlacementRenderInfo {
            size: std::mem::size_of::<RoasttyKittyGraphicsPlacementRenderInfo>(),
            ..RoasttyKittyGraphicsPlacementRenderInfo::default()
        };

        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                ptr::null_mut(),
                image,
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                ptr::null_mut(),
                terminal,
                &mut width,
                &mut height
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                ptr::null_mut(),
                &mut width,
                &mut height
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_pixel_size(
                iterator,
                image,
                terminal,
                ptr::null_mut(),
                &mut height
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_grid_size(
                iterator,
                image,
                terminal,
                &mut width,
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_source_rect(
                iterator,
                image,
                &mut width,
                &mut height,
                ptr::null_mut(),
                &mut height
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(
                iterator,
                image,
                terminal,
                ptr::null_mut()
            ),
            ROASTTY_INVALID_VALUE
        );
        assert_eq!(
            roastty_kitty_graphics_placement_render_info(
                ptr::null_mut(),
                image,
                terminal,
                &mut info
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_kitty_graphics_placement_iterator_free(iterator);
        roastty_kitty_graphics_image_free(image);
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

    #[test]
    fn selection_gesture_c_abi_layout_validation_and_events() {
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_PRESS, 0);
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_RELEASE, 1);
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_DRAG, 2);
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_AUTOSCROLL_TICK, 3);
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_DEEP_PRESS, 4);
        assert_eq!(ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT, 0);
        assert_eq!(ROASTTY_SELECTION_GESTURE_DATA_ANCHOR, 4);
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF, 0);
        assert_eq!(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_VIEWPORT, 9);
        assert_eq!(ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL, 0);
        assert_eq!(ROASTTY_SELECTION_GESTURE_BEHAVIOR_OUTPUT, 3);
        assert_eq!(std::mem::size_of::<RoasttySurfacePosition>(), 16);
        assert_eq!(std::mem::offset_of!(RoasttySurfacePosition, y), 8);
        assert_eq!(std::mem::size_of::<RoasttyCodepoints>(), 16);
        assert_eq!(std::mem::offset_of!(RoasttyCodepoints, len), 8);
        assert_eq!(std::mem::size_of::<RoasttySelectionGestureBehaviors>(), 12);
        assert_eq!(
            std::mem::offset_of!(RoasttySelectionGestureBehaviors, triple_click),
            8
        );
        assert_eq!(std::mem::size_of::<RoasttySelectionGestureGeometry>(), 16);
        assert_eq!(
            std::mem::offset_of!(RoasttySelectionGestureGeometry, screen_height),
            12
        );

        let terminal = new_terminal(20, 3);
        write_terminal(terminal, b"abcde fghi");

        let mut gesture: RoasttySelectionGesture = ptr::null_mut();
        assert_eq!(roastty_selection_gesture_new(&mut gesture), ROASTTY_SUCCESS);
        assert!(!gesture.is_null());

        let mut press: RoasttySelectionGestureEvent = ptr::null_mut();
        assert_eq!(
            roastty_selection_gesture_event_new(&mut press, ROASTTY_SELECTION_GESTURE_EVENT_PRESS),
            ROASTTY_SUCCESS
        );
        let ref_ = terminal_grid_ref_at(terminal, 1, 0);
        let position = RoasttySurfacePosition { x: 10.0, y: 0.0 };
        let repeat_distance = 20.0;
        let time_ns = 1_u64;
        let repeat_interval_ns = 100_u64;
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF,
                &ref_ as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION,
                &position as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_DISTANCE,
                &repeat_distance as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS,
                &time_ns as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_INTERVAL_NS,
                &repeat_interval_ns as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );

        let mut custom_boundaries = [b'c' as u32];
        let codepoints = RoasttyCodepoints {
            ptr: custom_boundaries.as_ptr(),
            len: custom_boundaries.len(),
        };
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS,
                &codepoints as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        custom_boundaries[0] = b' ' as u32;
        assert_eq!(custom_boundaries[0], b' ' as u32);

        let mut selection = RoasttySelection::default();
        assert_eq!(
            roastty_selection_gesture_handle_event(gesture, terminal, press, &mut selection),
            ROASTTY_NO_VALUE
        );
        let mut click_count = 0_u8;
        assert_eq!(
            roastty_selection_gesture_get(
                gesture,
                terminal,
                ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT,
                &mut click_count as *mut _ as *mut c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(click_count, 1);

        let time_ns = 2_u64;
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS,
                &time_ns as *const _ as *const c_void
            ),
            ROASTTY_SUCCESS
        );
        assert_eq!(
            roastty_selection_gesture_handle_event(gesture, terminal, press, &mut selection),
            ROASTTY_SUCCESS
        );
        assert_eq!((selection.start.x, selection.end.x), (0, 1));

        let invalid_codepoints = RoasttyCodepoints {
            ptr: ptr::null(),
            len: 1,
        };
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS,
                &invalid_codepoints as *const _ as *const c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        let surrogate = [0xd800_u32];
        let invalid_codepoints = RoasttyCodepoints {
            ptr: surrogate.as_ptr(),
            len: surrogate.len(),
        };
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS,
                &invalid_codepoints as *const _ as *const c_void
            ),
            ROASTTY_INVALID_VALUE
        );
        let invalid_behaviors = RoasttySelectionGestureBehaviors {
            single_click: ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL,
            double_click: 99,
            triple_click: ROASTTY_SELECTION_GESTURE_BEHAVIOR_LINE,
        };
        assert_eq!(
            roastty_selection_gesture_event_set(
                press,
                ROASTTY_SELECTION_GESTURE_EVENT_OPTION_BEHAVIORS,
                &invalid_behaviors as *const _ as *const c_void
            ),
            ROASTTY_INVALID_VALUE
        );

        roastty_selection_gesture_event_free(press);
        roastty_selection_gesture_free(gesture, terminal);
        roastty_terminal_free(terminal);
    }
}
