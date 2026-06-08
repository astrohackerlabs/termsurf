#ifndef ROASTTY_H
#define ROASTTY_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

#ifdef _WIN32
#ifdef ROASTTY_STATIC
#define ROASTTY_API
#else
#define ROASTTY_API __declspec(dllimport)
#endif
#else
#define ROASTTY_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* Init/CLI success sentinel — an int macro (mirrors upstream `GHOSTTY_SUCCESS`),
 * so `roastty_init(...) != ROASTTY_SUCCESS` compares against `int`. The granular
 * `roastty_result_e` extension below uses `ROASTTY_RESULT_SUCCESS` to avoid the
 * macro collision. */
#define ROASTTY_SUCCESS 0

typedef void* roastty_app_t;
typedef void* roastty_config_t;
typedef void* roastty_formatter_t;
typedef void* roastty_inspector_t;
typedef void* roastty_key_encoder_t;
typedef void* roastty_key_event_t;
typedef void* roastty_mouse_encoder_t;
typedef void* roastty_mouse_event_t;
typedef void* roastty_osc_command_t;
typedef void* roastty_osc_parser_t;
typedef void* roastty_selection_gesture_t;
typedef void* roastty_selection_gesture_event_t;
typedef void* roastty_surface_t;
typedef void* roastty_terminal_t;
typedef void* roastty_tracked_grid_ref_t;
typedef void* roastty_render_state_t;
typedef void* roastty_render_state_row_iterator_t;
typedef void* roastty_render_state_row_cells_t;
typedef void* roastty_kitty_graphics_t;
typedef void* roastty_kitty_graphics_image_t;
typedef void* roastty_kitty_graphics_placement_iterator_t;
typedef void* roastty_kitty_graphics_render_placement_iterator_t;

typedef uint16_t roastty_mode_tag_t;

enum {
  ROASTTY_MODE_TAG_VALUE_MASK = 0x7fff,
  ROASTTY_MODE_TAG_ANSI_BIT = 0x8000,
};

typedef enum {
  ROASTTY_RESULT_SUCCESS = 0,
  ROASTTY_OUT_OF_MEMORY = 1,
  ROASTTY_INVALID_VALUE = 2,
  ROASTTY_OUT_OF_SPACE = 3,
  ROASTTY_NO_VALUE = 4,
} roastty_result_e;

typedef enum {
  ROASTTY_TERMINAL_DATA_INVALID = 0,
  ROASTTY_TERMINAL_DATA_COLS = 1,
  ROASTTY_TERMINAL_DATA_ROWS = 2,
  ROASTTY_TERMINAL_DATA_CURSOR_X = 3,
  ROASTTY_TERMINAL_DATA_CURSOR_Y = 4,
  ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP = 5,
  ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN = 6,
  ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE = 7,
  ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS = 8,
  ROASTTY_TERMINAL_DATA_SCROLLBAR = 9,
  ROASTTY_TERMINAL_DATA_CURSOR_STYLE = 10,
  ROASTTY_TERMINAL_DATA_MOUSE_TRACKING = 11,
  ROASTTY_TERMINAL_DATA_TITLE = 12,
  ROASTTY_TERMINAL_DATA_PWD = 13,
  ROASTTY_TERMINAL_DATA_TOTAL_ROWS = 14,
  ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS = 15,
  ROASTTY_TERMINAL_DATA_WIDTH_PX = 16,
  ROASTTY_TERMINAL_DATA_HEIGHT_PX = 17,
  ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND = 18,
  ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND = 19,
  ROASTTY_TERMINAL_DATA_COLOR_CURSOR = 20,
  ROASTTY_TERMINAL_DATA_COLOR_PALETTE = 21,
  ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT = 22,
  ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT = 23,
  ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT = 24,
  ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT = 25,
  ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT = 26,
  ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE = 27,
  ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE = 28,
  ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM = 29,
  ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS = 30,
  ROASTTY_TERMINAL_DATA_SELECTION = 31,
  ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE = 32,
} roastty_terminal_data_e;

typedef enum {
  ROASTTY_TERMINAL_SCREEN_PRIMARY = 0,
  ROASTTY_TERMINAL_SCREEN_ALTERNATE = 1,
} roastty_terminal_screen_e;

typedef enum {
  ROASTTY_KITTY_GRAPHICS_DATA_INVALID = 0,
  ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR = 1,
} roastty_kitty_graphics_data_e;

typedef enum {
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID = 0,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID = 1,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID = 2,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL = 3,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET = 4,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET = 5,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_X = 6,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_Y = 7,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_WIDTH = 8,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_HEIGHT = 9,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS = 10,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS = 11,
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z = 12,
} roastty_kitty_graphics_placement_data_e;

typedef enum {
  ROASTTY_KITTY_PLACEMENT_LAYER_ALL = 0,
  ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG = 1,
  ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT = 2,
  ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT = 3,
} roastty_kitty_placement_layer_e;

typedef enum {
  ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER = 0,
} roastty_kitty_graphics_placement_iterator_option_e;

typedef enum {
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_INVALID = 0,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_IMAGE_ID = 1,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_PLACEMENT_ID = 2,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_IS_VIRTUAL = 3,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_VIRTUAL_ROW = 4,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_VIRTUAL_COL = 5,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_SOURCE_X = 6,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_SOURCE_Y = 7,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_SOURCE_WIDTH = 8,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_SOURCE_HEIGHT = 9,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_GRID_COLS = 10,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_GRID_ROWS = 11,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_VIEWPORT_COL = 12,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_VIEWPORT_ROW = 13,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_X_OFFSET = 14,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_Y_OFFSET = 15,
  ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_Z = 16,
} roastty_kitty_graphics_render_placement_data_e;

typedef enum {
  ROASTTY_KITTY_IMAGE_FORMAT_RGB = 0,
  ROASTTY_KITTY_IMAGE_FORMAT_RGBA = 1,
  ROASTTY_KITTY_IMAGE_FORMAT_PNG = 2,
  ROASTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA = 3,
  ROASTTY_KITTY_IMAGE_FORMAT_GRAY = 4,
} roastty_kitty_image_format_e;

typedef enum {
  ROASTTY_KITTY_IMAGE_COMPRESSION_NONE = 0,
  ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE = 1,
} roastty_kitty_image_compression_e;

typedef enum {
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID = 0,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID = 1,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER = 2,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH = 3,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT = 4,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT = 5,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION = 6,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR = 7,
  ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN = 8,
} roastty_kitty_graphics_image_data_e;

typedef struct {
  size_t size;
  uint32_t pixel_width;
  uint32_t pixel_height;
  uint32_t grid_cols;
  uint32_t grid_rows;
  int32_t viewport_col;
  int32_t viewport_row;
  bool viewport_visible;
  uint32_t source_x;
  uint32_t source_y;
  uint32_t source_width;
  uint32_t source_height;
} roastty_kitty_graphics_placement_render_info_s;

typedef struct {
  size_t size;
  uint32_t image_id;
  uint32_t placement_id;
  bool is_virtual;
  uint32_t x_offset;
  uint32_t y_offset;
  uint32_t pixel_width;
  uint32_t pixel_height;
  uint32_t grid_cols;
  uint32_t grid_rows;
  int32_t viewport_col;
  int32_t viewport_row;
  bool viewport_visible;
  uint32_t source_x;
  uint32_t source_y;
  uint32_t source_width;
  uint32_t source_height;
  int32_t z;
} roastty_kitty_graphics_render_placement_info_s;

typedef enum {
  ROASTTY_FOCUS_EVENT_GAINED = 0,
  ROASTTY_FOCUS_EVENT_LOST = 1,
} roastty_focus_event_e;

typedef enum {
  ROASTTY_MODE_REPORT_NOT_RECOGNIZED = 0,
  ROASTTY_MODE_REPORT_SET = 1,
  ROASTTY_MODE_REPORT_RESET = 2,
  ROASTTY_MODE_REPORT_PERMANENTLY_SET = 3,
  ROASTTY_MODE_REPORT_PERMANENTLY_RESET = 4,
} roastty_mode_report_state_e;

typedef enum {
  ROASTTY_TERMINAL_OPTION_USERDATA = 0,
  ROASTTY_TERMINAL_OPTION_WRITE_PTY = 1,
  ROASTTY_TERMINAL_OPTION_BELL = 2,
  ROASTTY_TERMINAL_OPTION_ENQUIRY = 3,
  ROASTTY_TERMINAL_OPTION_XTVERSION = 4,
  ROASTTY_TERMINAL_OPTION_TITLE_CHANGED = 5,
  ROASTTY_TERMINAL_OPTION_SIZE_CB = 6,
  ROASTTY_TERMINAL_OPTION_COLOR_SCHEME = 7,
  ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES = 8,
  ROASTTY_TERMINAL_OPTION_TITLE = 9,
  ROASTTY_TERMINAL_OPTION_PWD = 10,
  ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND = 11,
  ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND = 12,
  ROASTTY_TERMINAL_OPTION_COLOR_CURSOR = 13,
  ROASTTY_TERMINAL_OPTION_COLOR_PALETTE = 14,
  ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_STORAGE_LIMIT = 15,
  ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_FILE = 16,
  ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_TEMP_FILE = 17,
  ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_SHARED_MEM = 18,
  ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES = 19,
  ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES_KITTY = 20,
  ROASTTY_TERMINAL_OPTION_SELECTION = 21,
} roastty_terminal_option_e;

typedef enum {
  ROASTTY_BUILD_MODE_DEBUG,
  ROASTTY_BUILD_MODE_RELEASE_SAFE,
  ROASTTY_BUILD_MODE_RELEASE_FAST,
  ROASTTY_BUILD_MODE_RELEASE_SMALL,
} roastty_build_mode_e;

typedef enum {
  ROASTTY_OPTIMIZE_DEBUG = 0,
  ROASTTY_OPTIMIZE_RELEASE_SAFE = 1,
  ROASTTY_OPTIMIZE_RELEASE_SMALL = 2,
  ROASTTY_OPTIMIZE_RELEASE_FAST = 3,
} roastty_optimize_mode_e;

typedef enum {
  ROASTTY_BUILD_INFO_INVALID = 0,
  ROASTTY_BUILD_INFO_SIMD = 1,
  ROASTTY_BUILD_INFO_KITTY_GRAPHICS = 2,
  ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE = 3,
  ROASTTY_BUILD_INFO_OPTIMIZE = 4,
  ROASTTY_BUILD_INFO_VERSION_STRING = 5,
  ROASTTY_BUILD_INFO_VERSION_MAJOR = 6,
  ROASTTY_BUILD_INFO_VERSION_MINOR = 7,
  ROASTTY_BUILD_INFO_VERSION_PATCH = 8,
  ROASTTY_BUILD_INFO_VERSION_PRE = 9,
  ROASTTY_BUILD_INFO_VERSION_BUILD = 10,
} roastty_build_info_e;

typedef struct roastty_allocator_vtable_s {
  void* (*alloc)(void* ctx, size_t len, uint8_t alignment, uintptr_t ret_addr);
  bool (*resize)(void* ctx,
                 void* memory,
                 size_t memory_len,
                 uint8_t alignment,
                 size_t new_len,
                 uintptr_t ret_addr);
  void* (*remap)(void* ctx,
                 void* memory,
                 size_t memory_len,
                 uint8_t alignment,
                 size_t new_len,
                 uintptr_t ret_addr);
  void (*free)(void* ctx,
               void* memory,
               size_t memory_len,
               uint8_t alignment,
               uintptr_t ret_addr);
} roastty_allocator_vtable_s;

typedef struct {
  void* ctx;
  const roastty_allocator_vtable_s* vtable;
} roastty_allocator_s;

typedef enum {
  ROASTTY_SYS_LOG_LEVEL_ERROR = 0,
  ROASTTY_SYS_LOG_LEVEL_WARNING = 1,
  ROASTTY_SYS_LOG_LEVEL_INFO = 2,
  ROASTTY_SYS_LOG_LEVEL_DEBUG = 3,
} roastty_sys_log_level_e;

typedef struct {
  uint32_t width;
  uint32_t height;
  uint8_t* data;
  size_t data_len;
} roastty_sys_image_s;

typedef void (*roastty_sys_log_fn)(void*,
                                   roastty_sys_log_level_e,
                                   const uint8_t*,
                                   size_t,
                                   const uint8_t*,
                                   size_t);

typedef bool (*roastty_sys_decode_png_fn)(void*,
                                          const roastty_allocator_s*,
                                          const uint8_t*,
                                          size_t,
                                          roastty_sys_image_s*);

typedef enum {
  ROASTTY_SYS_OPT_USERDATA = 0,
  ROASTTY_SYS_OPT_DECODE_PNG = 1,
  ROASTTY_SYS_OPT_LOG = 2,
} roastty_sys_option_e;

typedef struct {
  roastty_build_mode_e build_mode;
  const char* version;
  uintptr_t version_len;
} roastty_info_s;

typedef struct {
  const char* message;
} roastty_diagnostic_s;

typedef struct {
  const char* path;
  bool optional;
} roastty_config_path_s;

typedef struct {
  const char* ptr;
  uintptr_t len;
  bool sentinel;
} roastty_string_s;

typedef struct {
  double tl_px_x;
  double tl_px_y;
  uint32_t offset_start;
  uint32_t offset_len;
  const char* text;
  uintptr_t text_len;
} roastty_text_s;

typedef struct {
  uint8_t* ptr;
  size_t cap;
  size_t len;
} roastty_buffer_s;

typedef enum {
  ROASTTY_COLOR_SCHEME_LIGHT = 0,
  ROASTTY_COLOR_SCHEME_DARK = 1,
} roastty_color_scheme_e;

typedef enum {
  ROASTTY_SIZE_REPORT_MODE_2048 = 0,
  ROASTTY_SIZE_REPORT_CSI_14_T = 1,
  ROASTTY_SIZE_REPORT_CSI_16_T = 2,
  ROASTTY_SIZE_REPORT_CSI_18_T = 3,
} roastty_size_report_style_e;

typedef struct {
  uint16_t rows;
  uint16_t columns;
  uint32_t cell_width;
  uint32_t cell_height;
} roastty_size_report_size_s;

typedef enum {
  ROASTTY_POINT_ACTIVE = 0,
  ROASTTY_POINT_VIEWPORT = 1,
  ROASTTY_POINT_SCREEN = 2,
  ROASTTY_POINT_HISTORY = 3,
} roastty_point_tag_e;

typedef struct {
  uint16_t x;
  uint32_t y;
} roastty_grid_point_coordinate_s;

typedef union {
  roastty_grid_point_coordinate_s active;
  roastty_grid_point_coordinate_s viewport;
  roastty_grid_point_coordinate_s screen;
  roastty_grid_point_coordinate_s history;
  uint64_t _padding[2];
} roastty_grid_point_value_u;

typedef struct {
  roastty_point_tag_e tag;
  roastty_grid_point_value_u value;
} roastty_grid_point_s;

/* Embedded point/selection ABI (Issue 802 / Exp 11) — byte-faithful to upstream.
 * `point_tag_e` (above, with HISTORY at idx 3) is shared with the grid types. */
typedef enum {
  ROASTTY_POINT_COORD_EXACT = 0,
  ROASTTY_POINT_COORD_TOP_LEFT = 1,
  ROASTTY_POINT_COORD_BOTTOM_RIGHT = 2,
} roastty_point_coord_e;

typedef struct {
  roastty_point_tag_e tag;
  roastty_point_coord_e coord;
  uint32_t x;
  uint32_t y;
} roastty_point_s;

typedef struct {
  roastty_point_s top_left;
  roastty_point_s bottom_right;
  bool rectangle;
} roastty_selection_s;

_Static_assert(sizeof(roastty_point_s) == 16, "embedded point_s size");
_Static_assert(sizeof(roastty_selection_s) == 36, "embedded selection_s size");

/*
 * Borrowed snapshot reference into terminal page storage.
 *
 * The node field is opaque and must not be inspected, dereferenced, retained
 * as an owned value, or freed by C callers. A grid ref is valid only for
 * immediate calls back into the same terminal, before terminal mutation. Do not
 * use it after roastty_terminal_free, roastty_terminal_vt_write, reset, resize,
 * or future APIs that mutate scrollback, selection, or gestures. Long-lived
 * references belong to the future tracked-grid-ref ABI.
 */
typedef struct {
  size_t size;
  void* node;
  uint16_t x;
  uint16_t y;
} roastty_grid_ref_s;

typedef enum {
  ROASTTY_SELECTION_FORMAT_PLAIN = 0,
  ROASTTY_SELECTION_FORMAT_VT = 1,
  ROASTTY_SELECTION_FORMAT_HTML = 2,
} roastty_selection_format_e;

typedef enum {
  ROASTTY_FORMATTER_FORMAT_PLAIN = 0,
  ROASTTY_FORMATTER_FORMAT_VT = 1,
  ROASTTY_FORMATTER_FORMAT_HTML = 2,
} roastty_formatter_format_e;

typedef enum {
  ROASTTY_SELECTION_ORDER_FORWARD = 0,
  ROASTTY_SELECTION_ORDER_REVERSE = 1,
  ROASTTY_SELECTION_ORDER_MIRRORED_FORWARD = 2,
  ROASTTY_SELECTION_ORDER_MIRRORED_REVERSE = 3,
} roastty_selection_order_e;

typedef enum {
  ROASTTY_SELECTION_ADJUST_LEFT = 0,
  ROASTTY_SELECTION_ADJUST_RIGHT = 1,
  ROASTTY_SELECTION_ADJUST_UP = 2,
  ROASTTY_SELECTION_ADJUST_DOWN = 3,
  ROASTTY_SELECTION_ADJUST_HOME = 4,
  ROASTTY_SELECTION_ADJUST_END = 5,
  ROASTTY_SELECTION_ADJUST_PAGE_UP = 6,
  ROASTTY_SELECTION_ADJUST_PAGE_DOWN = 7,
  ROASTTY_SELECTION_ADJUST_BEGINNING_OF_LINE = 8,
  ROASTTY_SELECTION_ADJUST_END_OF_LINE = 9,
} roastty_selection_adjustment_e;

typedef enum {
  ROASTTY_SELECTION_GESTURE_EVENT_PRESS = 0,
  ROASTTY_SELECTION_GESTURE_EVENT_RELEASE = 1,
  ROASTTY_SELECTION_GESTURE_EVENT_DRAG = 2,
  ROASTTY_SELECTION_GESTURE_EVENT_AUTOSCROLL_TICK = 3,
  ROASTTY_SELECTION_GESTURE_EVENT_DEEP_PRESS = 4,
} roastty_selection_gesture_event_e;

typedef enum {
  ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT = 0,
  ROASTTY_SELECTION_GESTURE_DATA_DRAGGED = 1,
  ROASTTY_SELECTION_GESTURE_DATA_AUTOSCROLL = 2,
  ROASTTY_SELECTION_GESTURE_DATA_BEHAVIOR = 3,
  ROASTTY_SELECTION_GESTURE_DATA_ANCHOR = 4,
} roastty_selection_gesture_data_e;

typedef enum {
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF = 0,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION = 1,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_DISTANCE = 2,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS = 3,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_INTERVAL_NS = 4,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS = 5,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_BEHAVIORS = 6,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_RECTANGLE = 7,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_GEOMETRY = 8,
  ROASTTY_SELECTION_GESTURE_EVENT_OPTION_VIEWPORT = 9,
} roastty_selection_gesture_event_option_e;

typedef enum {
  ROASTTY_SELECTION_GESTURE_AUTOSCROLL_NONE = 0,
  ROASTTY_SELECTION_GESTURE_AUTOSCROLL_UP = 1,
  ROASTTY_SELECTION_GESTURE_AUTOSCROLL_DOWN = 2,
} roastty_selection_gesture_autoscroll_e;

typedef enum {
  ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL = 0,
  ROASTTY_SELECTION_GESTURE_BEHAVIOR_WORD = 1,
  ROASTTY_SELECTION_GESTURE_BEHAVIOR_LINE = 2,
  ROASTTY_SELECTION_GESTURE_BEHAVIOR_OUTPUT = 3,
} roastty_selection_gesture_behavior_e;

typedef struct {
  double x;
  double y;
} roastty_surface_position_s;

typedef struct {
  const uint32_t* ptr;
  size_t len;
} roastty_codepoints_s;

typedef struct {
  roastty_selection_gesture_behavior_e single_click;
  roastty_selection_gesture_behavior_e double_click;
  roastty_selection_gesture_behavior_e triple_click;
} roastty_selection_gesture_behaviors_s;

typedef struct {
  uint32_t columns;
  uint32_t cell_width;
  uint32_t padding_left;
  uint32_t screen_height;
} roastty_selection_gesture_geometry_s;

typedef struct {
  size_t size;
  roastty_grid_ref_s start;
  roastty_grid_ref_s end;
  bool rectangle;
} roastty_grid_selection_s;

typedef struct {
  size_t size;
  roastty_grid_ref_s ref;
  const uint32_t* boundary_codepoints;
  size_t boundary_codepoints_len;
} roastty_terminal_select_word_options_s;

typedef struct {
  size_t size;
  roastty_grid_ref_s start;
  roastty_grid_ref_s end;
  const uint32_t* boundary_codepoints;
  size_t boundary_codepoints_len;
} roastty_terminal_select_word_between_options_s;

typedef struct {
  size_t size;
  roastty_grid_ref_s ref;
  const uint32_t* whitespace;
  size_t whitespace_len;
  bool semantic_prompt_boundary;
} roastty_terminal_select_line_options_s;

typedef struct {
  size_t size;
  roastty_selection_format_e emit;
  bool unwrap;
  bool trim;
  const roastty_grid_selection_s* selection;
} roastty_terminal_selection_format_options_s;

typedef struct {
  size_t size;
  bool cursor;
  bool style;
  bool hyperlink;
  bool protection;
  bool kitty_keyboard;
  bool charsets;
} roastty_formatter_screen_extra_s;

typedef struct {
  size_t size;
  bool palette;
  bool modes;
  bool scrolling_region;
  bool tabstops;
  bool pwd;
  bool keyboard;
  roastty_formatter_screen_extra_s screen;
} roastty_formatter_terminal_extra_s;

typedef struct {
  size_t size;
  roastty_formatter_format_e emit;
  bool unwrap;
  bool trim;
  roastty_formatter_terminal_extra_s extra;
  const roastty_grid_selection_s* selection;
} roastty_formatter_terminal_options_s;

typedef struct {
  uint16_t conformance_level;
  uint16_t features[64];
  size_t num_features;
} roastty_device_attributes_primary_s;

typedef struct {
  uint16_t device_type;
  uint16_t firmware_version;
  uint16_t rom_cartridge;
} roastty_device_attributes_secondary_s;

typedef struct {
  uint32_t unit_id;
} roastty_device_attributes_tertiary_s;

typedef struct {
  roastty_device_attributes_primary_s primary;
  roastty_device_attributes_secondary_s secondary;
  roastty_device_attributes_tertiary_s tertiary;
} roastty_device_attributes_s;

typedef void (*roastty_terminal_write_pty_cb)(roastty_terminal_t terminal,
                                              void* userdata,
                                              const uint8_t* ptr,
                                              size_t len);
typedef void (*roastty_terminal_bell_cb)(roastty_terminal_t terminal,
                                         void* userdata);
typedef roastty_string_s (*roastty_terminal_enquiry_cb)(
    roastty_terminal_t terminal,
    void* userdata);
typedef roastty_string_s (*roastty_terminal_xtversion_cb)(
    roastty_terminal_t terminal,
    void* userdata);
typedef void (*roastty_terminal_title_changed_cb)(roastty_terminal_t terminal,
                                                  void* userdata);
typedef bool (*roastty_terminal_size_cb)(
    roastty_terminal_t terminal,
    void* userdata,
    roastty_size_report_size_s* out_size);
typedef bool (*roastty_terminal_color_scheme_cb)(
    roastty_terminal_t terminal,
    void* userdata,
    roastty_color_scheme_e* out_scheme);
typedef bool (*roastty_terminal_device_attributes_cb)(
    roastty_terminal_t terminal,
    void* userdata,
    roastty_device_attributes_s* out_attrs);

typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
} roastty_rgb_s;

typedef roastty_rgb_s roastty_palette_t[256];

typedef enum {
  ROASTTY_RENDER_STATE_DIRTY_FALSE = 0,
  ROASTTY_RENDER_STATE_DIRTY_PARTIAL = 1,
  ROASTTY_RENDER_STATE_DIRTY_FULL = 2,
} roastty_render_state_dirty_e;

typedef enum {
  ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR = 0,
  ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK = 1,
  ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE = 2,
  ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW = 3,
} roastty_render_state_cursor_visual_style_e;

typedef enum {
  ROASTTY_RENDER_STATE_DATA_INVALID = 0,
  ROASTTY_RENDER_STATE_DATA_COLS = 1,
  ROASTTY_RENDER_STATE_DATA_ROWS = 2,
  ROASTTY_RENDER_STATE_DATA_DIRTY = 3,
  ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR = 4,
  ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND = 5,
  ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND = 6,
  ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR = 7,
  ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE = 8,
  ROASTTY_RENDER_STATE_DATA_COLOR_PALETTE = 9,
  ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE = 10,
  ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE = 11,
  ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING = 12,
  ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT = 13,
  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE = 14,
  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X = 15,
  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y = 16,
  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL = 17,
  ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR = 18,
  ROASTTY_RENDER_STATE_DATA_DISPLAY_ID = 19,
} roastty_render_state_data_e;

typedef enum {
  ROASTTY_RENDER_STATE_OPTION_DIRTY = 0,
} roastty_render_state_option_e;

typedef struct {
  size_t size;
  roastty_rgb_s background;
  roastty_rgb_s foreground;
  roastty_rgb_s cursor;
  bool cursor_has_value;
  roastty_palette_t palette;
} roastty_render_state_colors_s;

typedef struct {
  size_t size;
  uint16_t start_x;
  uint16_t end_x;
} roastty_render_state_row_selection_s;

typedef enum {
  ROASTTY_RENDER_STATE_ROW_DATA_INVALID = 0,
  ROASTTY_RENDER_STATE_ROW_DATA_DIRTY = 1,
  ROASTTY_RENDER_STATE_ROW_DATA_RAW = 2,
  ROASTTY_RENDER_STATE_ROW_DATA_CELLS = 3,
  ROASTTY_RENDER_STATE_ROW_DATA_SELECTION = 4,
} roastty_render_state_row_data_e;

typedef enum {
  ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY = 0,
} roastty_render_state_row_option_e;

typedef enum {
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID = 0,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW = 1,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE = 2,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN = 3,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF = 4,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR = 5,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR = 6,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED = 7,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING = 8,
  ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8 = 9,
} roastty_render_state_row_cells_data_e;

typedef enum {
  ROASTTY_STYLE_COLOR_NONE = 0,
  ROASTTY_STYLE_COLOR_PALETTE = 1,
  ROASTTY_STYLE_COLOR_RGB = 2,
} roastty_style_color_tag_e;

typedef union {
  uint8_t palette;
  roastty_rgb_s rgb;
  uint64_t _padding;
} roastty_style_color_value_u;

typedef struct {
  roastty_style_color_tag_e tag;
  roastty_style_color_value_u value;
} roastty_style_color_s;

typedef struct {
  size_t size;
  roastty_style_color_s fg_color;
  roastty_style_color_s bg_color;
  roastty_style_color_s underline_color;
  bool bold;
  bool italic;
  bool faint;
  bool blink;
  bool inverse;
  bool invisible;
  bool strikethrough;
  bool overline;
  int underline;
} roastty_style_s;

typedef uint64_t roastty_cell_t;
typedef uint64_t roastty_row_t;

typedef enum {
  ROASTTY_CELL_CONTENT_CODEPOINT = 0,
  ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME = 1,
  ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE = 2,
  ROASTTY_CELL_CONTENT_BG_COLOR_RGB = 3,
} roastty_cell_content_tag_e;

typedef enum {
  ROASTTY_CELL_WIDE_NARROW = 0,
  ROASTTY_CELL_WIDE_WIDE = 1,
  ROASTTY_CELL_WIDE_SPACER_TAIL = 2,
  ROASTTY_CELL_WIDE_SPACER_HEAD = 3,
} roastty_cell_wide_e;

typedef enum {
  ROASTTY_CELL_SEMANTIC_OUTPUT = 0,
  ROASTTY_CELL_SEMANTIC_INPUT = 1,
  ROASTTY_CELL_SEMANTIC_PROMPT = 2,
} roastty_cell_semantic_content_e;

typedef enum {
  ROASTTY_CELL_DATA_INVALID = 0,
  ROASTTY_CELL_DATA_CODEPOINT = 1,
  ROASTTY_CELL_DATA_CONTENT_TAG = 2,
  ROASTTY_CELL_DATA_WIDE = 3,
  ROASTTY_CELL_DATA_HAS_TEXT = 4,
  ROASTTY_CELL_DATA_HAS_STYLING = 5,
  ROASTTY_CELL_DATA_STYLE_ID = 6,
  ROASTTY_CELL_DATA_HAS_HYPERLINK = 7,
  ROASTTY_CELL_DATA_PROTECTED = 8,
  ROASTTY_CELL_DATA_SEMANTIC = 9,
  ROASTTY_CELL_DATA_COLOR_PALETTE = 10,
  ROASTTY_CELL_DATA_COLOR_RGB = 11,
} roastty_cell_data_e;

typedef enum {
  ROASTTY_ROW_SEMANTIC_NONE = 0,
  ROASTTY_ROW_SEMANTIC_PROMPT = 1,
  ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION = 2,
} roastty_row_semantic_prompt_e;

typedef enum {
  ROASTTY_ROW_DATA_INVALID = 0,
  ROASTTY_ROW_DATA_WRAP = 1,
  ROASTTY_ROW_DATA_WRAP_CONTINUATION = 2,
  ROASTTY_ROW_DATA_GRAPHEME = 3,
  ROASTTY_ROW_DATA_STYLED = 4,
  ROASTTY_ROW_DATA_HYPERLINK = 5,
  ROASTTY_ROW_DATA_SEMANTIC_PROMPT = 6,
  ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER = 7,
  ROASTTY_ROW_DATA_DIRTY = 8,
} roastty_row_data_e;

typedef struct {
  const char* key;
  const char* value;
} roastty_env_var_s;

typedef enum {
  ROASTTY_PLATFORM_INVALID,
  ROASTTY_PLATFORM_MACOS,
  ROASTTY_PLATFORM_IOS,
} roastty_platform_e;

typedef struct {
  void* nsview;
} roastty_platform_macos_s;

typedef struct {
  void* uiview;
} roastty_platform_ios_s;

typedef union {
  roastty_platform_macos_s macos;
  roastty_platform_ios_s ios;
} roastty_platform_u;

typedef enum {
  ROASTTY_SURFACE_CONTEXT_WINDOW = 0,
  ROASTTY_SURFACE_CONTEXT_TAB = 1,
  ROASTTY_SURFACE_CONTEXT_SPLIT = 2,
} roastty_surface_context_e;

typedef struct {
  roastty_platform_e platform_tag;
  roastty_platform_u platform;
  void* userdata;
  double scale_factor;
  float font_size;
  const char* working_directory;
  const char* command;
  roastty_env_var_s* env_vars;
  size_t env_var_count;
  const char* initial_input;
  bool wait_after_command;
  roastty_surface_context_e context;
} roastty_surface_config_s;

typedef struct {
  uint16_t columns;
  uint16_t rows;
  uint32_t width_px;
  uint32_t height_px;
  uint32_t cell_width_px;
  uint32_t cell_height_px;
} roastty_surface_size_s;

typedef enum {
  ROASTTY_KEY_ACTION_RELEASE = 0,
  ROASTTY_KEY_ACTION_PRESS = 1,
  ROASTTY_KEY_ACTION_REPEAT = 2,
} roastty_key_action_e;

typedef uint8_t roastty_keybind_flags_t;

typedef enum {
  ROASTTY_MODS_NONE = 0,
  ROASTTY_MODS_SHIFT = 1 << 0,
  ROASTTY_MODS_CTRL = 1 << 1,
  ROASTTY_MODS_ALT = 1 << 2,
  ROASTTY_MODS_SUPER = 1 << 3,
  ROASTTY_MODS_CAPS = 1 << 4,
  ROASTTY_MODS_NUM = 1 << 5,
  ROASTTY_MODS_SHIFT_RIGHT = 1 << 6,
  ROASTTY_MODS_CTRL_RIGHT = 1 << 7,
  ROASTTY_MODS_ALT_RIGHT = 1 << 8,
  ROASTTY_MODS_SUPER_RIGHT = 1 << 9,
} roastty_input_mods_e;

typedef enum {
  ROASTTY_KEY_UNIDENTIFIED,

  // "Writing System Keys" § 3.1.1
  ROASTTY_KEY_BACKQUOTE,
  ROASTTY_KEY_BACKSLASH,
  ROASTTY_KEY_BRACKET_LEFT,
  ROASTTY_KEY_BRACKET_RIGHT,
  ROASTTY_KEY_COMMA,
  ROASTTY_KEY_DIGIT_0,
  ROASTTY_KEY_DIGIT_1,
  ROASTTY_KEY_DIGIT_2,
  ROASTTY_KEY_DIGIT_3,
  ROASTTY_KEY_DIGIT_4,
  ROASTTY_KEY_DIGIT_5,
  ROASTTY_KEY_DIGIT_6,
  ROASTTY_KEY_DIGIT_7,
  ROASTTY_KEY_DIGIT_8,
  ROASTTY_KEY_DIGIT_9,
  ROASTTY_KEY_EQUAL,
  ROASTTY_KEY_INTL_BACKSLASH,
  ROASTTY_KEY_INTL_RO,
  ROASTTY_KEY_INTL_YEN,
  ROASTTY_KEY_A,
  ROASTTY_KEY_B,
  ROASTTY_KEY_C,
  ROASTTY_KEY_D,
  ROASTTY_KEY_E,
  ROASTTY_KEY_F,
  ROASTTY_KEY_G,
  ROASTTY_KEY_H,
  ROASTTY_KEY_I,
  ROASTTY_KEY_J,
  ROASTTY_KEY_K,
  ROASTTY_KEY_L,
  ROASTTY_KEY_M,
  ROASTTY_KEY_N,
  ROASTTY_KEY_O,
  ROASTTY_KEY_P,
  ROASTTY_KEY_Q,
  ROASTTY_KEY_R,
  ROASTTY_KEY_S,
  ROASTTY_KEY_T,
  ROASTTY_KEY_U,
  ROASTTY_KEY_V,
  ROASTTY_KEY_W,
  ROASTTY_KEY_X,
  ROASTTY_KEY_Y,
  ROASTTY_KEY_Z,
  ROASTTY_KEY_MINUS,
  ROASTTY_KEY_PERIOD,
  ROASTTY_KEY_QUOTE,
  ROASTTY_KEY_SEMICOLON,
  ROASTTY_KEY_SLASH,

  // "Functional Keys" § 3.1.2
  ROASTTY_KEY_ALT_LEFT,
  ROASTTY_KEY_ALT_RIGHT,
  ROASTTY_KEY_BACKSPACE,
  ROASTTY_KEY_CAPS_LOCK,
  ROASTTY_KEY_CONTEXT_MENU,
  ROASTTY_KEY_CONTROL_LEFT,
  ROASTTY_KEY_CONTROL_RIGHT,
  ROASTTY_KEY_ENTER,
  ROASTTY_KEY_META_LEFT,
  ROASTTY_KEY_META_RIGHT,
  ROASTTY_KEY_SHIFT_LEFT,
  ROASTTY_KEY_SHIFT_RIGHT,
  ROASTTY_KEY_SPACE,
  ROASTTY_KEY_TAB,
  ROASTTY_KEY_CONVERT,
  ROASTTY_KEY_KANA_MODE,
  ROASTTY_KEY_NON_CONVERT,

  // "Control Pad Section" § 3.2
  ROASTTY_KEY_DELETE,
  ROASTTY_KEY_END,
  ROASTTY_KEY_HELP,
  ROASTTY_KEY_HOME,
  ROASTTY_KEY_INSERT,
  ROASTTY_KEY_PAGE_DOWN,
  ROASTTY_KEY_PAGE_UP,

  // "Arrow Pad Section" § 3.3
  ROASTTY_KEY_ARROW_DOWN,
  ROASTTY_KEY_ARROW_LEFT,
  ROASTTY_KEY_ARROW_RIGHT,
  ROASTTY_KEY_ARROW_UP,

  // "Numpad Section" § 3.4
  ROASTTY_KEY_NUM_LOCK,
  ROASTTY_KEY_NUMPAD_0,
  ROASTTY_KEY_NUMPAD_1,
  ROASTTY_KEY_NUMPAD_2,
  ROASTTY_KEY_NUMPAD_3,
  ROASTTY_KEY_NUMPAD_4,
  ROASTTY_KEY_NUMPAD_5,
  ROASTTY_KEY_NUMPAD_6,
  ROASTTY_KEY_NUMPAD_7,
  ROASTTY_KEY_NUMPAD_8,
  ROASTTY_KEY_NUMPAD_9,
  ROASTTY_KEY_NUMPAD_ADD,
  ROASTTY_KEY_NUMPAD_BACKSPACE,
  ROASTTY_KEY_NUMPAD_CLEAR,
  ROASTTY_KEY_NUMPAD_CLEAR_ENTRY,
  ROASTTY_KEY_NUMPAD_COMMA,
  ROASTTY_KEY_NUMPAD_DECIMAL,
  ROASTTY_KEY_NUMPAD_DIVIDE,
  ROASTTY_KEY_NUMPAD_ENTER,
  ROASTTY_KEY_NUMPAD_EQUAL,
  ROASTTY_KEY_NUMPAD_MEMORY_ADD,
  ROASTTY_KEY_NUMPAD_MEMORY_CLEAR,
  ROASTTY_KEY_NUMPAD_MEMORY_RECALL,
  ROASTTY_KEY_NUMPAD_MEMORY_STORE,
  ROASTTY_KEY_NUMPAD_MEMORY_SUBTRACT,
  ROASTTY_KEY_NUMPAD_MULTIPLY,
  ROASTTY_KEY_NUMPAD_PAREN_LEFT,
  ROASTTY_KEY_NUMPAD_PAREN_RIGHT,
  ROASTTY_KEY_NUMPAD_SUBTRACT,
  ROASTTY_KEY_NUMPAD_SEPARATOR,
  ROASTTY_KEY_NUMPAD_UP,
  ROASTTY_KEY_NUMPAD_DOWN,
  ROASTTY_KEY_NUMPAD_RIGHT,
  ROASTTY_KEY_NUMPAD_LEFT,
  ROASTTY_KEY_NUMPAD_BEGIN,
  ROASTTY_KEY_NUMPAD_HOME,
  ROASTTY_KEY_NUMPAD_END,
  ROASTTY_KEY_NUMPAD_INSERT,
  ROASTTY_KEY_NUMPAD_DELETE,
  ROASTTY_KEY_NUMPAD_PAGE_UP,
  ROASTTY_KEY_NUMPAD_PAGE_DOWN,

  // "Function Section" § 3.5
  ROASTTY_KEY_ESCAPE,
  ROASTTY_KEY_F1,
  ROASTTY_KEY_F2,
  ROASTTY_KEY_F3,
  ROASTTY_KEY_F4,
  ROASTTY_KEY_F5,
  ROASTTY_KEY_F6,
  ROASTTY_KEY_F7,
  ROASTTY_KEY_F8,
  ROASTTY_KEY_F9,
  ROASTTY_KEY_F10,
  ROASTTY_KEY_F11,
  ROASTTY_KEY_F12,
  ROASTTY_KEY_F13,
  ROASTTY_KEY_F14,
  ROASTTY_KEY_F15,
  ROASTTY_KEY_F16,
  ROASTTY_KEY_F17,
  ROASTTY_KEY_F18,
  ROASTTY_KEY_F19,
  ROASTTY_KEY_F20,
  ROASTTY_KEY_F21,
  ROASTTY_KEY_F22,
  ROASTTY_KEY_F23,
  ROASTTY_KEY_F24,
  ROASTTY_KEY_F25,
  ROASTTY_KEY_FN,
  ROASTTY_KEY_FN_LOCK,
  ROASTTY_KEY_PRINT_SCREEN,
  ROASTTY_KEY_SCROLL_LOCK,
  ROASTTY_KEY_PAUSE,

  // "Media Keys" § 3.6
  ROASTTY_KEY_BROWSER_BACK,
  ROASTTY_KEY_BROWSER_FAVORITES,
  ROASTTY_KEY_BROWSER_FORWARD,
  ROASTTY_KEY_BROWSER_HOME,
  ROASTTY_KEY_BROWSER_REFRESH,
  ROASTTY_KEY_BROWSER_SEARCH,
  ROASTTY_KEY_BROWSER_STOP,
  ROASTTY_KEY_EJECT,
  ROASTTY_KEY_LAUNCH_APP_1,
  ROASTTY_KEY_LAUNCH_APP_2,
  ROASTTY_KEY_LAUNCH_MAIL,
  ROASTTY_KEY_MEDIA_PLAY_PAUSE,
  ROASTTY_KEY_MEDIA_SELECT,
  ROASTTY_KEY_MEDIA_STOP,
  ROASTTY_KEY_MEDIA_TRACK_NEXT,
  ROASTTY_KEY_MEDIA_TRACK_PREVIOUS,
  ROASTTY_KEY_POWER,
  ROASTTY_KEY_SLEEP,
  ROASTTY_KEY_AUDIO_VOLUME_DOWN,
  ROASTTY_KEY_AUDIO_VOLUME_MUTE,
  ROASTTY_KEY_AUDIO_VOLUME_UP,
  ROASTTY_KEY_WAKE_UP,

  // "Legacy, Non-standard, and Special Keys" § 3.7
  ROASTTY_KEY_COPY,
  ROASTTY_KEY_CUT,
  ROASTTY_KEY_PASTE,
} roastty_key_e;

// --- Embedded input ABI (Issue 802 / Exp 8) ---
typedef enum {
  ROASTTY_ACTION_RELEASE,
  ROASTTY_ACTION_PRESS,
  ROASTTY_ACTION_REPEAT,
} roastty_input_action_e;
typedef roastty_key_e roastty_input_key_e;  // alias: same 176 members/values
typedef enum {
  ROASTTY_MOUSE_UNKNOWN,
  ROASTTY_MOUSE_LEFT,
  ROASTTY_MOUSE_RIGHT,
  ROASTTY_MOUSE_MIDDLE,
  ROASTTY_MOUSE_FOUR,
  ROASTTY_MOUSE_FIVE,
  ROASTTY_MOUSE_SIX,
  ROASTTY_MOUSE_SEVEN,
  ROASTTY_MOUSE_EIGHT,
  ROASTTY_MOUSE_NINE,
  ROASTTY_MOUSE_TEN,
  ROASTTY_MOUSE_ELEVEN,
} roastty_input_mouse_button_e;
typedef enum {
  ROASTTY_MOUSE_RELEASE,
  ROASTTY_MOUSE_PRESS,
} roastty_input_mouse_state_e;
typedef enum {
  ROASTTY_MOUSE_MOMENTUM_NONE,
  ROASTTY_MOUSE_MOMENTUM_BEGAN,
  ROASTTY_MOUSE_MOMENTUM_STATIONARY,
  ROASTTY_MOUSE_MOMENTUM_CHANGED,
  ROASTTY_MOUSE_MOMENTUM_ENDED,
  ROASTTY_MOUSE_MOMENTUM_CANCELLED,
  ROASTTY_MOUSE_MOMENTUM_MAY_BEGIN,
} roastty_input_mouse_momentum_e;
typedef struct {
  roastty_input_action_e action;
  roastty_input_mods_e mods;
  roastty_input_mods_e consumed_mods;
  uint32_t keycode;
  const char* text;
  uint32_t unshifted_codepoint;
  bool composing;
} roastty_input_key_s;
typedef enum {
  ROASTTY_BINDING_FLAGS_CONSUMED = 1 << 0,
  ROASTTY_BINDING_FLAGS_ALL = 1 << 1,
  ROASTTY_BINDING_FLAGS_GLOBAL = 1 << 2,
  ROASTTY_BINDING_FLAGS_PERFORMABLE = 1 << 3,
} roastty_binding_flags_e;
// --- end embedded input ABI ---


typedef enum {
  ROASTTY_TRIGGER_PHYSICAL = 0,
  ROASTTY_TRIGGER_UNICODE = 1,
  ROASTTY_TRIGGER_CATCH_ALL = 2,
} roastty_input_trigger_tag_e;

typedef union {
  roastty_key_e physical;
  uint32_t unicode;
} roastty_input_trigger_key_u;

typedef struct {
  roastty_input_trigger_tag_e tag;
  roastty_input_trigger_key_u key;
  roastty_input_mods_e mods;
} roastty_input_trigger_s;

typedef enum {
  ROASTTY_KEY_SIDE_LEFT = 0,
  ROASTTY_KEY_SIDE_RIGHT = 1,
} roastty_key_side_e;

typedef enum {
  ROASTTY_OPTION_AS_ALT_FALSE = 0,
  ROASTTY_OPTION_AS_ALT_TRUE = 1,
  ROASTTY_OPTION_AS_ALT_LEFT = 2,
  ROASTTY_OPTION_AS_ALT_RIGHT = 3,
} roastty_option_as_alt_e;

typedef enum {
  ROASTTY_KEY_ENCODER_OPTION_CURSOR_KEY_APPLICATION = 0,
  ROASTTY_KEY_ENCODER_OPTION_KEYPAD_KEY_APPLICATION = 1,
  ROASTTY_KEY_ENCODER_OPTION_IGNORE_KEYPAD_WITH_NUMLOCK = 2,
  ROASTTY_KEY_ENCODER_OPTION_ALT_ESC_PREFIX = 3,
  ROASTTY_KEY_ENCODER_OPTION_MODIFY_OTHER_KEYS_STATE_2 = 4,
  ROASTTY_KEY_ENCODER_OPTION_KITTY_FLAGS = 5,
  ROASTTY_KEY_ENCODER_OPTION_MACOS_OPTION_AS_ALT = 6,
  ROASTTY_KEY_ENCODER_OPTION_BACKARROW_KEY_MODE = 7,
} roastty_key_encoder_option_e;

typedef struct {
  bool shift;
  bool ctrl;
  bool alt;
  bool super;
  bool caps_lock;
  bool num_lock;
  int shift_side;
  int ctrl_side;
  int alt_side;
  int super_side;
} roastty_key_mods_s;

typedef enum {
  ROASTTY_OSC_COMMAND_INVALID = 0,
  ROASTTY_OSC_COMMAND_CHANGE_WINDOW_TITLE = 1,
  ROASTTY_OSC_COMMAND_CHANGE_WINDOW_ICON = 2,
  ROASTTY_OSC_COMMAND_SEMANTIC_PROMPT = 3,
  ROASTTY_OSC_COMMAND_CLIPBOARD_CONTENTS = 4,
  ROASTTY_OSC_COMMAND_REPORT_PWD = 5,
  ROASTTY_OSC_COMMAND_MOUSE_SHAPE = 6,
  ROASTTY_OSC_COMMAND_COLOR_OPERATION = 7,
  ROASTTY_OSC_COMMAND_KITTY_COLOR_PROTOCOL = 8,
  ROASTTY_OSC_COMMAND_SHOW_DESKTOP_NOTIFICATION = 9,
  ROASTTY_OSC_COMMAND_HYPERLINK_START = 10,
  ROASTTY_OSC_COMMAND_HYPERLINK_END = 11,
  ROASTTY_OSC_COMMAND_CONEMU_SLEEP = 12,
  ROASTTY_OSC_COMMAND_CONEMU_SHOW_MESSAGE_BOX = 13,
  ROASTTY_OSC_COMMAND_CONEMU_CHANGE_TAB_TITLE = 14,
  ROASTTY_OSC_COMMAND_CONEMU_PROGRESS_REPORT = 15,
  ROASTTY_OSC_COMMAND_CONEMU_WAIT_INPUT = 16,
  ROASTTY_OSC_COMMAND_CONEMU_GUIMACRO = 17,
  ROASTTY_OSC_COMMAND_CONEMU_RUN_PROCESS = 18,
  ROASTTY_OSC_COMMAND_CONEMU_OUTPUT_ENVIRONMENT_VARIABLE = 19,
  ROASTTY_OSC_COMMAND_CONEMU_XTERM_EMULATION = 20,
  ROASTTY_OSC_COMMAND_CONEMU_COMMENT = 21,
  ROASTTY_OSC_COMMAND_KITTY_TEXT_SIZING = 22,
  ROASTTY_OSC_COMMAND_KITTY_CLIPBOARD_PROTOCOL = 23,
  ROASTTY_OSC_COMMAND_CONTEXT_SIGNAL = 24,
} roastty_osc_command_e;

typedef enum {
  ROASTTY_OSC_COMMAND_DATA_INVALID = 0,
  /*
   * Expects out as const char**. On success, writes parser-owned
   * NUL-terminated memory valid until the owning parser receives another byte,
   * is reset, ends another command, or is freed.
   */
  ROASTTY_OSC_COMMAND_DATA_CHANGE_WINDOW_TITLE_STR = 1,
} roastty_osc_command_data_e;

typedef enum {
  ROASTTY_CLIPBOARD_STANDARD,
  ROASTTY_CLIPBOARD_SELECTION,
} roastty_clipboard_e;

typedef enum {
  ROASTTY_CLIPBOARD_REQUEST_PASTE = 0,
  ROASTTY_CLIPBOARD_REQUEST_OSC_52_READ = 1,
  ROASTTY_CLIPBOARD_REQUEST_OSC_52_WRITE = 2,
} roastty_clipboard_request_e;

typedef struct {
  const char* mime;
  const char* data;
} roastty_clipboard_content_s;

typedef enum {
  ROASTTY_TARGET_APP = 0,
  ROASTTY_TARGET_SURFACE = 1,
} roastty_target_tag_e;

typedef union {
  /* NULL when tag is ROASTTY_TARGET_APP. */
  roastty_surface_t surface;
} roastty_target_u;

typedef struct {
  roastty_target_tag_e tag;
  roastty_target_u target;
} roastty_target_s;

_Static_assert(sizeof(roastty_target_s) == 16, "roastty_target_s size");
_Static_assert(offsetof(roastty_target_s, target) == 8, "roastty_target_s.target offset");

typedef enum {
  ROASTTY_ACTION_QUIT = 0,
  ROASTTY_ACTION_NEW_WINDOW = 1,
  ROASTTY_ACTION_NEW_TAB = 2,
  ROASTTY_ACTION_CLOSE_TAB = 3,
  ROASTTY_ACTION_NEW_SPLIT = 4,
  ROASTTY_ACTION_CLOSE_ALL_WINDOWS = 5,
  ROASTTY_ACTION_TOGGLE_MAXIMIZE = 6,
  ROASTTY_ACTION_TOGGLE_FULLSCREEN = 7,
  ROASTTY_ACTION_TOGGLE_TAB_OVERVIEW = 8,
  ROASTTY_ACTION_TOGGLE_WINDOW_DECORATIONS = 9,
  ROASTTY_ACTION_TOGGLE_QUICK_TERMINAL = 10,
  ROASTTY_ACTION_TOGGLE_COMMAND_PALETTE = 11,
  ROASTTY_ACTION_TOGGLE_VISIBILITY = 12,
  ROASTTY_ACTION_TOGGLE_BACKGROUND_OPACITY = 13,
  ROASTTY_ACTION_MOVE_TAB = 14,
  ROASTTY_ACTION_GOTO_TAB = 15,
  ROASTTY_ACTION_GOTO_SPLIT = 16,
  ROASTTY_ACTION_GOTO_WINDOW = 17,
  ROASTTY_ACTION_RESIZE_SPLIT = 18,
  ROASTTY_ACTION_EQUALIZE_SPLITS = 19,
  ROASTTY_ACTION_TOGGLE_SPLIT_ZOOM = 20,
  ROASTTY_ACTION_RESET_WINDOW_SIZE = 23,
  ROASTTY_ACTION_INSPECTOR = 28,
  ROASTTY_ACTION_SHOW_GTK_INSPECTOR = 29,
  ROASTTY_ACTION_SET_TITLE = 32,
  ROASTTY_ACTION_SET_TAB_TITLE = 33,
  ROASTTY_ACTION_PROMPT_TITLE = 34,
  ROASTTY_ACTION_OPEN_CONFIG = 40,
  ROASTTY_ACTION_FLOAT_WINDOW = 42,
  ROASTTY_ACTION_SECURE_INPUT = 43,
  ROASTTY_ACTION_RELOAD_CONFIG = 47,
  ROASTTY_ACTION_CLOSE_WINDOW = 49,
  ROASTTY_ACTION_UNDO = 51,
  ROASTTY_ACTION_REDO = 52,
  ROASTTY_ACTION_CHECK_FOR_UPDATES = 53,
  ROASTTY_ACTION_OPEN_URL = 54,
  ROASTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD = 57,
  ROASTTY_ACTION_START_SEARCH = 59,
  ROASTTY_ACTION_END_SEARCH = 60,
  ROASTTY_ACTION_READONLY = 63,
  ROASTTY_ACTION_COPY_TITLE_TO_CLIPBOARD = 64,
  /*
   * Roastty-owned action extensions start at 1000. These actions do not map to
   * upstream Ghostty C action tags.
   */
  ROASTTY_ACTION_NAVIGATE_SEARCH = 1000,
  /* Upstream action tags filled in (Issue 802 / Exp 12). */
  ROASTTY_ACTION_PRESENT_TERMINAL = 21,
  ROASTTY_ACTION_SIZE_LIMIT = 22,
  ROASTTY_ACTION_INITIAL_SIZE = 24,
  ROASTTY_ACTION_CELL_SIZE = 25,
  ROASTTY_ACTION_SCROLLBAR = 26,
  ROASTTY_ACTION_RENDER = 27,
  ROASTTY_ACTION_RENDER_INSPECTOR = 30,
  ROASTTY_ACTION_DESKTOP_NOTIFICATION = 31,
  ROASTTY_ACTION_PWD = 35,
  ROASTTY_ACTION_MOUSE_SHAPE = 36,
  ROASTTY_ACTION_MOUSE_VISIBILITY = 37,
  ROASTTY_ACTION_MOUSE_OVER_LINK = 38,
  ROASTTY_ACTION_RENDERER_HEALTH = 39,
  ROASTTY_ACTION_QUIT_TIMER = 41,
  ROASTTY_ACTION_KEY_SEQUENCE = 44,
  ROASTTY_ACTION_KEY_TABLE = 45,
  ROASTTY_ACTION_COLOR_CHANGE = 46,
  ROASTTY_ACTION_CONFIG_CHANGE = 48,
  ROASTTY_ACTION_RING_BELL = 50,
  ROASTTY_ACTION_SHOW_CHILD_EXITED = 55,
  ROASTTY_ACTION_PROGRESS_REPORT = 56,
  ROASTTY_ACTION_COMMAND_FINISHED = 58,
  ROASTTY_ACTION_SEARCH_TOTAL = 61,
  ROASTTY_ACTION_SEARCH_SELECTED = 62,
} roastty_action_tag_e;

typedef enum {
  ROASTTY_INSPECTOR_TOGGLE = 0,
  ROASTTY_INSPECTOR_SHOW = 1,
  ROASTTY_INSPECTOR_HIDE = 2,
} roastty_inspector_mode_e;

typedef enum {
  ROASTTY_FLOAT_WINDOW_ON = 0,
  ROASTTY_FLOAT_WINDOW_OFF = 1,
  ROASTTY_FLOAT_WINDOW_TOGGLE = 2,
} roastty_float_window_e;

typedef enum {
  ROASTTY_SECURE_INPUT_ON = 0,
  ROASTTY_SECURE_INPUT_OFF = 1,
  ROASTTY_SECURE_INPUT_TOGGLE = 2,
} roastty_secure_input_e;

typedef enum {
  ROASTTY_CLOSE_TAB_THIS = 0,
  ROASTTY_CLOSE_TAB_OTHER = 1,
  ROASTTY_CLOSE_TAB_RIGHT = 2,
} roastty_close_tab_e;

typedef enum {
  ROASTTY_GOTO_WINDOW_PREVIOUS = 0,
  ROASTTY_GOTO_WINDOW_NEXT = 1,
} roastty_goto_window_e;

typedef enum {
  ROASTTY_GOTO_TAB_PREVIOUS = -1,
  ROASTTY_GOTO_TAB_NEXT = -2,
  ROASTTY_GOTO_TAB_LAST = -3,
} roastty_goto_tab_e;

typedef enum {
  ROASTTY_FULLSCREEN_NATIVE = 0,
  ROASTTY_FULLSCREEN_MACOS_NON_NATIVE = 1,
  ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU = 2,
  ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH = 3,
} roastty_fullscreen_e;

typedef enum {
  ROASTTY_PROMPT_TITLE_SURFACE = 0,
  ROASTTY_PROMPT_TITLE_TAB = 1,
} roastty_prompt_title_e;

typedef enum {
  ROASTTY_READONLY_OFF = 0,
  ROASTTY_READONLY_ON = 1,
} roastty_readonly_e;

typedef enum {
  ROASTTY_NAVIGATE_SEARCH_PREVIOUS = 0,
  ROASTTY_NAVIGATE_SEARCH_NEXT = 1,
} roastty_navigate_search_e;

typedef enum {
  ROASTTY_ACTION_OPEN_URL_KIND_UNKNOWN = 0,
  ROASTTY_ACTION_OPEN_URL_KIND_TEXT = 1,
  ROASTTY_ACTION_OPEN_URL_KIND_HTML = 2,
} roastty_action_open_url_kind_e;



typedef enum {
  ROASTTY_SPLIT_DIRECTION_RIGHT = 0,
  ROASTTY_SPLIT_DIRECTION_DOWN = 1,
  ROASTTY_SPLIT_DIRECTION_LEFT = 2,
  ROASTTY_SPLIT_DIRECTION_UP = 3,
} roastty_split_direction_e;

typedef enum {
  ROASTTY_GOTO_SPLIT_PREVIOUS = 0,
  ROASTTY_GOTO_SPLIT_NEXT = 1,
  ROASTTY_GOTO_SPLIT_UP = 2,
  ROASTTY_GOTO_SPLIT_LEFT = 3,
  ROASTTY_GOTO_SPLIT_DOWN = 4,
  ROASTTY_GOTO_SPLIT_RIGHT = 5,
} roastty_goto_split_e;

typedef enum {
  ROASTTY_RESIZE_SPLIT_UP = 0,
  ROASTTY_RESIZE_SPLIT_DOWN = 1,
  ROASTTY_RESIZE_SPLIT_LEFT = 2,
  ROASTTY_RESIZE_SPLIT_RIGHT = 3,
} roastty_resize_split_e;

// === Embedded action ABI (Issue 802 / Exp 9) ===

typedef roastty_float_window_e roastty_action_float_window_e;

typedef roastty_fullscreen_e roastty_action_fullscreen_e;

typedef roastty_goto_split_e roastty_action_goto_split_e;

typedef roastty_goto_tab_e roastty_action_goto_tab_e;

typedef roastty_goto_window_e roastty_action_goto_window_e;

typedef roastty_inspector_mode_e roastty_action_inspector_e;

typedef roastty_prompt_title_e roastty_action_prompt_title_e;

typedef roastty_readonly_e roastty_action_readonly_e;

typedef roastty_resize_split_e roastty_action_resize_split_direction_e;

typedef roastty_secure_input_e roastty_action_secure_input_e;

typedef roastty_split_direction_e roastty_action_split_direction_e;

typedef struct {
  uint32_t width;
  uint32_t height;
} roastty_action_cell_size_s;

typedef struct {
  uint32_t exit_code;
  uint64_t timetime_ms;
} roastty_surface_message_childexited_s;

typedef enum {
  ROASTTY_QUIT_TIMER_START,
  ROASTTY_QUIT_TIMER_STOP,
} roastty_action_quit_timer_e;

typedef struct {
  bool soft;
} roastty_action_reload_config_s;

typedef enum {
  ROASTTY_MOUSE_SHAPE_DEFAULT,
  ROASTTY_MOUSE_SHAPE_CONTEXT_MENU,
  ROASTTY_MOUSE_SHAPE_HELP,
  ROASTTY_MOUSE_SHAPE_POINTER,
  ROASTTY_MOUSE_SHAPE_PROGRESS,
  ROASTTY_MOUSE_SHAPE_WAIT,
  ROASTTY_MOUSE_SHAPE_CELL,
  ROASTTY_MOUSE_SHAPE_CROSSHAIR,
  ROASTTY_MOUSE_SHAPE_TEXT,
  ROASTTY_MOUSE_SHAPE_VERTICAL_TEXT,
  ROASTTY_MOUSE_SHAPE_ALIAS,
  ROASTTY_MOUSE_SHAPE_COPY,
  ROASTTY_MOUSE_SHAPE_MOVE,
  ROASTTY_MOUSE_SHAPE_NO_DROP,
  ROASTTY_MOUSE_SHAPE_NOT_ALLOWED,
  ROASTTY_MOUSE_SHAPE_GRAB,
  ROASTTY_MOUSE_SHAPE_GRABBING,
  ROASTTY_MOUSE_SHAPE_ALL_SCROLL,
  ROASTTY_MOUSE_SHAPE_COL_RESIZE,
  ROASTTY_MOUSE_SHAPE_ROW_RESIZE,
  ROASTTY_MOUSE_SHAPE_N_RESIZE,
  ROASTTY_MOUSE_SHAPE_E_RESIZE,
  ROASTTY_MOUSE_SHAPE_S_RESIZE,
  ROASTTY_MOUSE_SHAPE_W_RESIZE,
  ROASTTY_MOUSE_SHAPE_NE_RESIZE,
  ROASTTY_MOUSE_SHAPE_NW_RESIZE,
  ROASTTY_MOUSE_SHAPE_SE_RESIZE,
  ROASTTY_MOUSE_SHAPE_SW_RESIZE,
  ROASTTY_MOUSE_SHAPE_EW_RESIZE,
  ROASTTY_MOUSE_SHAPE_NS_RESIZE,
  ROASTTY_MOUSE_SHAPE_NESW_RESIZE,
  ROASTTY_MOUSE_SHAPE_NWSE_RESIZE,
  ROASTTY_MOUSE_SHAPE_ZOOM_IN,
  ROASTTY_MOUSE_SHAPE_ZOOM_OUT,
} roastty_action_mouse_shape_e;

typedef union {
  struct {
    const char *name;
    size_t len;
  } activate;
} roastty_action_key_table_u;

typedef enum {
  ROASTTY_KEY_TABLE_ACTIVATE,
  ROASTTY_KEY_TABLE_DEACTIVATE,
  ROASTTY_KEY_TABLE_DEACTIVATE_ALL,
} roastty_action_key_table_tag_e;

typedef struct {
  roastty_action_key_table_tag_e tag;
  roastty_action_key_table_u value;
} roastty_action_key_table_s;

typedef struct {
  uint32_t width;
  uint32_t height;
} roastty_action_initial_size_s;

typedef enum {
  ROASTTY_PROGRESS_STATE_REMOVE,
  ROASTTY_PROGRESS_STATE_SET,
  ROASTTY_PROGRESS_STATE_ERROR,
  ROASTTY_PROGRESS_STATE_INDETERMINATE,
  ROASTTY_PROGRESS_STATE_PAUSE,
} roastty_action_progress_report_state_e;

typedef enum {
  ROASTTY_ACTION_CLOSE_TAB_MODE_THIS,
  ROASTTY_ACTION_CLOSE_TAB_MODE_OTHER,
  ROASTTY_ACTION_CLOSE_TAB_MODE_RIGHT,
} roastty_action_close_tab_mode_e;

typedef struct {
  ssize_t amount;
} roastty_action_move_tab_s;

typedef enum {
  ROASTTY_RENDERER_HEALTH_HEALTHY,
  ROASTTY_RENDERER_HEALTH_UNHEALTHY,
} roastty_action_renderer_health_e;

typedef struct {
  // -1 if no exit code was reported, otherwise 0-255
  int16_t exit_code;
  // number of nanoseconds that command was running for
  uint64_t duration;
} roastty_action_command_finished_s;

typedef struct {
  ssize_t selected;
} roastty_action_search_selected_s;

typedef struct {
  uint64_t total;
  uint64_t offset;
  uint64_t len;
} roastty_action_scrollbar_s;

typedef enum {
  ROASTTY_MOUSE_VISIBLE,
  ROASTTY_MOUSE_HIDDEN,
} roastty_action_mouse_visibility_e;

typedef struct {
  const char* title;
  const char* body;
} roastty_action_desktop_notification_s;

typedef struct {
  const char* pwd;
} roastty_action_pwd_s;

typedef struct {
  const char* title;
} roastty_action_set_title_s;

typedef struct {
  uint32_t min_width;
  uint32_t min_height;
  uint32_t max_width;
  uint32_t max_height;
} roastty_action_size_limit_s;

typedef struct {
  bool active;
  roastty_input_trigger_s trigger;
} roastty_action_key_sequence_s;

typedef struct {
  roastty_action_open_url_kind_e kind;
  const char* url;
  uintptr_t len;
} roastty_action_open_url_s;

typedef enum {
  ROASTTY_ACTION_COLOR_KIND_FOREGROUND = -1,
  ROASTTY_ACTION_COLOR_KIND_BACKGROUND = -2,
  ROASTTY_ACTION_COLOR_KIND_CURSOR = -3,
} roastty_action_color_kind_e;

typedef struct {
  roastty_action_color_kind_e kind;
  uint8_t r;
  uint8_t g;
  uint8_t b;
} roastty_action_color_change_s;

typedef struct {
  const char* needle;
} roastty_action_start_search_s;

typedef struct {
  ssize_t total;
} roastty_action_search_total_s;

typedef struct {
  roastty_config_t config;
} roastty_action_config_change_s;

typedef struct {
  roastty_action_progress_report_state_e state;
  // -1 if no progress was reported, otherwise 0-100 indicating percent
  // completeness.
  int8_t progress;
} roastty_action_progress_report_s;

typedef struct {
  const char* url;
  size_t len;
} roastty_action_mouse_over_link_s;

typedef struct {
  uint16_t amount;
  roastty_action_resize_split_direction_e direction;
} roastty_action_resize_split_s;

typedef union {
  roastty_action_split_direction_e new_split;
  roastty_action_fullscreen_e toggle_fullscreen;
  roastty_action_move_tab_s move_tab;
  roastty_action_goto_tab_e goto_tab;
  roastty_action_goto_split_e goto_split;
  roastty_action_goto_window_e goto_window;
  roastty_action_resize_split_s resize_split;
  roastty_action_size_limit_s size_limit;
  roastty_action_initial_size_s initial_size;
  roastty_action_cell_size_s cell_size;
  roastty_action_scrollbar_s scrollbar;
  roastty_action_inspector_e inspector;
  roastty_action_desktop_notification_s desktop_notification;
  roastty_action_set_title_s set_title;
  roastty_action_set_title_s set_tab_title;
  roastty_action_prompt_title_e prompt_title;
  roastty_action_pwd_s pwd;
  roastty_action_mouse_shape_e mouse_shape;
  roastty_action_mouse_visibility_e mouse_visibility;
  roastty_action_mouse_over_link_s mouse_over_link;
  roastty_action_renderer_health_e renderer_health;
  roastty_action_quit_timer_e quit_timer;
  roastty_action_float_window_e float_window;
  roastty_action_secure_input_e secure_input;
  roastty_action_key_sequence_s key_sequence;
  roastty_action_key_table_s key_table;
  roastty_action_color_change_s color_change;
  roastty_action_reload_config_s reload_config;
  roastty_action_config_change_s config_change;
  roastty_action_open_url_s open_url;
  roastty_action_close_tab_mode_e close_tab_mode;
  roastty_surface_message_childexited_s child_exited;
  roastty_action_progress_report_s progress_report;
  roastty_action_command_finished_s command_finished;
  roastty_action_start_search_s start_search;
  roastty_action_search_total_s search_total;
  roastty_action_search_selected_s search_selected;
  roastty_action_readonly_e readonly;
} roastty_action_u;

typedef struct {
  roastty_action_tag_e tag;
  roastty_action_u action;
} roastty_action_s;

/* Exp 9 ABI cross-check: these must match the Rust `offset_of`/`size_of` tests. */
_Static_assert(sizeof(roastty_action_s) == 32, "roastty_action_s size");
_Static_assert(sizeof(roastty_action_u) == 24, "roastty_action_u size");
_Static_assert(offsetof(roastty_action_s, action) == 8, "roastty_action_s.action offset");
_Static_assert(sizeof(roastty_action_open_url_s) == 24, "roastty_action_open_url_s size");
_Static_assert(offsetof(roastty_action_open_url_s, url) == 8, "open_url.url offset");
_Static_assert(offsetof(roastty_action_open_url_s, len) == 16, "open_url.len offset");

// === Embedded config/misc tail (Issue 802 / Exp 10) ===

typedef union {
  float percentage;
  uint32_t pixels;
} roastty_quick_terminal_size_value_u;

typedef enum {
  ROASTTY_QUICK_TERMINAL_SIZE_NONE,
  ROASTTY_QUICK_TERMINAL_SIZE_PERCENTAGE,
  ROASTTY_QUICK_TERMINAL_SIZE_PIXELS,
} roastty_quick_terminal_size_tag_e;

typedef struct {
  const char* action_key;
  const char* action;
  const char* title;
  const char* description;
} roastty_command_s;

typedef struct {
  uint8_t r;
  uint8_t g;
  uint8_t b;
} roastty_config_color_s;

typedef struct {
  const roastty_config_color_s* colors;
  size_t len;
} roastty_config_color_list_s;

typedef struct {
  const roastty_command_s* commands;
  size_t len;
} roastty_config_command_list_s;

typedef struct {
  roastty_quick_terminal_size_tag_e tag;
  roastty_quick_terminal_size_value_u value;
} roastty_quick_terminal_size_s;

typedef struct {
  roastty_quick_terminal_size_s primary;
  roastty_quick_terminal_size_s secondary;
} roastty_config_quick_terminal_size_s;

_Static_assert(sizeof(roastty_config_color_s) == 3, "config_color_s size");
_Static_assert(sizeof(roastty_quick_terminal_size_s) == 8, "quick_terminal_size_s size");
_Static_assert(sizeof(roastty_command_s) == 4 * sizeof(void*), "command_s size");



typedef enum {
  ROASTTY_MOUSE_ACTION_PRESS = 0,
  ROASTTY_MOUSE_ACTION_RELEASE = 1,
  ROASTTY_MOUSE_ACTION_MOTION = 2,
} roastty_mouse_action_e;

typedef int roastty_input_scroll_mods_t;

typedef enum {
  ROASTTY_MOUSE_BUTTON_RELEASE = 0,
  ROASTTY_MOUSE_BUTTON_PRESS = 1,
} roastty_mouse_button_state_e;

typedef enum {
  ROASTTY_MOUSE_BUTTON_UNKNOWN = 0,
  ROASTTY_MOUSE_BUTTON_LEFT = 1,
  ROASTTY_MOUSE_BUTTON_RIGHT = 2,
  ROASTTY_MOUSE_BUTTON_MIDDLE = 3,
  ROASTTY_MOUSE_BUTTON_FOUR = 4,
  ROASTTY_MOUSE_BUTTON_FIVE = 5,
  ROASTTY_MOUSE_BUTTON_SIX = 6,
  ROASTTY_MOUSE_BUTTON_SEVEN = 7,
  ROASTTY_MOUSE_BUTTON_EIGHT = 8,
  ROASTTY_MOUSE_BUTTON_NINE = 9,
  ROASTTY_MOUSE_BUTTON_TEN = 10,
  ROASTTY_MOUSE_BUTTON_ELEVEN = 11,
} roastty_mouse_button_e;

typedef enum {
  ROASTTY_MOUSE_TRACKING_NONE = 0,
  ROASTTY_MOUSE_TRACKING_X10 = 1,
  ROASTTY_MOUSE_TRACKING_NORMAL = 2,
  ROASTTY_MOUSE_TRACKING_BUTTON = 3,
  ROASTTY_MOUSE_TRACKING_ANY = 4,
} roastty_mouse_tracking_mode_e;

typedef enum {
  ROASTTY_MOUSE_FORMAT_X10 = 0,
  ROASTTY_MOUSE_FORMAT_UTF8 = 1,
  ROASTTY_MOUSE_FORMAT_SGR = 2,
  ROASTTY_MOUSE_FORMAT_URXVT = 3,
  ROASTTY_MOUSE_FORMAT_SGR_PIXELS = 4,
} roastty_mouse_format_e;

typedef enum {
  ROASTTY_MOUSE_ENCODER_OPTION_EVENT = 0,
  ROASTTY_MOUSE_ENCODER_OPTION_FORMAT = 1,
  ROASTTY_MOUSE_ENCODER_OPTION_SIZE = 2,
  ROASTTY_MOUSE_ENCODER_OPTION_ANY_BUTTON_PRESSED = 3,
  ROASTTY_MOUSE_ENCODER_OPTION_TRACK_LAST_CELL = 4,
} roastty_mouse_encoder_option_e;

typedef struct {
  bool shift;
  bool alt;
  bool ctrl;
} roastty_mouse_mods_s;

typedef struct {
  float x;
  float y;
} roastty_mouse_position_s;

typedef struct {
  size_t size;
  uint32_t screen_width;
  uint32_t screen_height;
  uint32_t cell_width;
  uint32_t cell_height;
  uint32_t padding_top;
  uint32_t padding_bottom;
  uint32_t padding_right;
  uint32_t padding_left;
} roastty_mouse_encoder_size_s;

typedef void (*roastty_runtime_wakeup_cb)(void*);
typedef bool (*roastty_runtime_action_cb)(roastty_app_t,
                                          roastty_target_s,
                                          roastty_action_s);
typedef bool (*roastty_runtime_read_clipboard_cb)(void*,
                                                  roastty_clipboard_e,
                                                  void*);
typedef void (*roastty_runtime_confirm_read_clipboard_cb)(
    void*,
    const char*,
    void*,
    roastty_clipboard_request_e);
typedef void (*roastty_runtime_write_clipboard_cb)(void*,
                                                   roastty_clipboard_e,
                                                   const roastty_clipboard_content_s*,
                                                   size_t,
                                                   bool);
typedef void (*roastty_runtime_close_surface_cb)(void*, bool);

typedef struct {
  void* userdata;
  bool supports_selection_clipboard;
  roastty_runtime_wakeup_cb wakeup_cb;
  roastty_runtime_action_cb action_cb;
  roastty_runtime_read_clipboard_cb read_clipboard_cb;
  roastty_runtime_confirm_read_clipboard_cb confirm_read_clipboard_cb;
  roastty_runtime_write_clipboard_cb write_clipboard_cb;
  roastty_runtime_close_surface_cb close_surface_cb;
} roastty_runtime_config_s;

ROASTTY_API int roastty_init(uintptr_t, char**);
ROASTTY_API roastty_info_s roastty_info(void);
/* String values returned by roastty_build_info are borrowed process-static
 * values. They remain valid for the lifetime of the process and must not be
 * passed to roastty_string_free. */
ROASTTY_API roastty_result_e roastty_build_info(roastty_build_info_e, void*);
ROASTTY_API uint8_t* roastty_alloc(const roastty_allocator_s*, size_t);
ROASTTY_API void roastty_free(const roastty_allocator_s*, uint8_t*, size_t);
ROASTTY_API roastty_result_e roastty_sys_set(roastty_sys_option_e,
                                             const void*);
ROASTTY_API void roastty_sys_log_stderr(void*,
                                        roastty_sys_log_level_e,
                                        const uint8_t*,
                                        size_t,
                                        const uint8_t*,
                                        size_t);
ROASTTY_API void roastty_string_free(roastty_string_s);
ROASTTY_API void roastty_style_default(roastty_style_s*);
ROASTTY_API bool roastty_style_is_default(const roastty_style_s*);
ROASTTY_API roastty_result_e roastty_cell_get(roastty_cell_t,
                                              roastty_cell_data_e,
                                              void*);
ROASTTY_API roastty_result_e roastty_cell_get_multi(roastty_cell_t,
                                                    size_t,
                                                    const roastty_cell_data_e*,
                                                    void**,
                                                    size_t*);
ROASTTY_API roastty_result_e roastty_row_get(roastty_row_t,
                                             roastty_row_data_e,
                                             void*);
ROASTTY_API roastty_result_e roastty_row_get_multi(roastty_row_t,
                                                   size_t,
                                                   const roastty_row_data_e*,
                                                   void**,
                                                   size_t*);
ROASTTY_API roastty_result_e
roastty_render_state_new(roastty_render_state_t*);
ROASTTY_API void roastty_render_state_free(roastty_render_state_t);
ROASTTY_API roastty_result_e
roastty_render_state_row_iterator_new(roastty_render_state_row_iterator_t*);
ROASTTY_API void
roastty_render_state_row_iterator_free(roastty_render_state_row_iterator_t);
ROASTTY_API bool
roastty_render_state_row_iterator_next(roastty_render_state_row_iterator_t);
ROASTTY_API roastty_result_e
roastty_render_state_row_cells_new(roastty_render_state_row_cells_t*);
ROASTTY_API void
roastty_render_state_row_cells_free(roastty_render_state_row_cells_t);
ROASTTY_API bool
roastty_render_state_row_cells_next(roastty_render_state_row_cells_t);
ROASTTY_API roastty_result_e
roastty_render_state_row_cells_select(roastty_render_state_row_cells_t,
                                      uint16_t);
ROASTTY_API roastty_result_e
roastty_render_state_update(roastty_render_state_t, roastty_terminal_t);
ROASTTY_API roastty_result_e
roastty_render_state_get(roastty_render_state_t,
                         roastty_render_state_data_e,
                         void*);
ROASTTY_API roastty_result_e
roastty_render_state_get_multi(roastty_render_state_t,
                               size_t,
                               const roastty_render_state_data_e*,
                               void**,
                               size_t*);
ROASTTY_API roastty_result_e
roastty_render_state_set(roastty_render_state_t,
                         roastty_render_state_option_e,
                         const void*);
ROASTTY_API roastty_result_e
roastty_render_state_row_get(roastty_render_state_row_iterator_t,
                             roastty_render_state_row_data_e,
                             void*);
ROASTTY_API roastty_result_e
roastty_render_state_row_get_multi(
    roastty_render_state_row_iterator_t,
    size_t,
    const roastty_render_state_row_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_result_e
roastty_render_state_row_set(roastty_render_state_row_iterator_t,
                             roastty_render_state_row_option_e,
                             const void*);
ROASTTY_API roastty_result_e
roastty_render_state_row_cells_get(roastty_render_state_row_cells_t,
                                   roastty_render_state_row_cells_data_e,
                                   void*);
ROASTTY_API roastty_result_e
roastty_render_state_row_cells_get_multi(
    roastty_render_state_row_cells_t,
    size_t,
    const roastty_render_state_row_cells_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_result_e
roastty_render_state_colors_get(roastty_render_state_t,
                                roastty_render_state_colors_s*);

ROASTTY_API roastty_config_t roastty_config_new(void);
ROASTTY_API void roastty_config_free(roastty_config_t);
ROASTTY_API roastty_config_t roastty_config_clone(roastty_config_t);
ROASTTY_API void roastty_config_load_cli_args(roastty_config_t);
ROASTTY_API void roastty_config_load_file(roastty_config_t, const char*);
ROASTTY_API void roastty_config_load_default_files(roastty_config_t);
ROASTTY_API void roastty_config_load_recursive_files(roastty_config_t);
ROASTTY_API void roastty_config_finalize(roastty_config_t);
ROASTTY_API bool roastty_config_get(roastty_config_t,
                                    void*,
                                    const char*,
                                    uintptr_t);
ROASTTY_API roastty_input_trigger_s roastty_config_trigger(roastty_config_t,
                                                           const char*,
                                                           uintptr_t);
ROASTTY_API bool roastty_config_key_is_binding(roastty_config_t,
                                               roastty_key_event_t);
ROASTTY_API uint32_t roastty_config_diagnostics_count(roastty_config_t);
ROASTTY_API roastty_diagnostic_s roastty_config_get_diagnostic(roastty_config_t,
                                                               uint32_t);
ROASTTY_API roastty_string_s roastty_config_open_path(void);
ROASTTY_API roastty_result_e
roastty_size_report_encode(roastty_size_report_style_e,
                           roastty_size_report_size_s,
                           char*,
                           size_t,
                           size_t*);
ROASTTY_API roastty_result_e roastty_focus_encode(roastty_focus_event_e,
                                                  uint8_t*,
                                                  size_t,
                                                  size_t*);
ROASTTY_API bool roastty_paste_is_safe(const uint8_t*, size_t);
ROASTTY_API roastty_result_e roastty_paste_encode(uint8_t*,
                                                  size_t,
                                                  bool,
                                                  uint8_t*,
                                                  size_t,
                                                  size_t*);
ROASTTY_API roastty_result_e
roastty_mode_report_encode(roastty_mode_tag_t,
                           roastty_mode_report_state_e,
                           uint8_t*,
                           size_t,
                           size_t*);

ROASTTY_API roastty_app_t roastty_app_new(const roastty_runtime_config_s*,
                                          roastty_config_t);
ROASTTY_API void roastty_app_free(roastty_app_t);
ROASTTY_API void roastty_app_tick(roastty_app_t);
ROASTTY_API void* roastty_app_userdata(roastty_app_t);
ROASTTY_API void roastty_app_set_focus(roastty_app_t, bool);
ROASTTY_API void roastty_app_update_config(roastty_app_t, roastty_config_t);
ROASTTY_API bool roastty_app_needs_confirm_quit(roastty_app_t);
ROASTTY_API bool roastty_app_has_global_keybinds(roastty_app_t);
ROASTTY_API void roastty_app_set_color_scheme(roastty_app_t,
                                              roastty_color_scheme_e);

ROASTTY_API roastty_result_e roastty_terminal_new(uint16_t,
                                                  uint16_t,
                                                  size_t,
                                                  roastty_terminal_t*);
ROASTTY_API void roastty_terminal_free(roastty_terminal_t);
ROASTTY_API void roastty_terminal_reset(roastty_terminal_t);
ROASTTY_API roastty_result_e roastty_terminal_vt_write(roastty_terminal_t,
                                                       const uint8_t*,
                                                       size_t);
ROASTTY_API roastty_result_e roastty_terminal_set(roastty_terminal_t,
                                                  roastty_terminal_option_e,
                                                  const void*);
ROASTTY_API roastty_result_e roastty_terminal_mode_get(roastty_terminal_t,
                                                       roastty_mode_tag_t,
                                                       bool*);
ROASTTY_API roastty_result_e roastty_terminal_mode_set(roastty_terminal_t,
                                                       roastty_mode_tag_t,
                                                       bool);
ROASTTY_API roastty_result_e
roastty_terminal_read_screen_plain(roastty_terminal_t,
                                   bool,
                                   roastty_string_s*);
ROASTTY_API roastty_result_e roastty_terminal_title(roastty_terminal_t,
                                                    roastty_string_s*);
ROASTTY_API roastty_result_e roastty_terminal_pwd(roastty_terminal_t,
                                                  roastty_string_s*);
ROASTTY_API bool roastty_terminal_cursor_position(roastty_terminal_t,
                                                  uint16_t*,
                                                  uint16_t*);
ROASTTY_API roastty_result_e roastty_terminal_get(roastty_terminal_t,
                                                  roastty_terminal_data_e,
                                                  void*);
ROASTTY_API roastty_result_e roastty_terminal_get_multi(
    roastty_terminal_t,
    size_t,
    const roastty_terminal_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_get(
    roastty_kitty_graphics_t,
    roastty_kitty_graphics_data_e,
    void*);
ROASTTY_API roastty_kitty_graphics_image_t
roastty_kitty_graphics_image(roastty_kitty_graphics_t, uint32_t);
ROASTTY_API void
roastty_kitty_graphics_image_free(roastty_kitty_graphics_image_t);
ROASTTY_API roastty_result_e roastty_kitty_graphics_image_get(
    roastty_kitty_graphics_image_t,
    roastty_kitty_graphics_image_data_e,
    void*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_image_get_multi(
    roastty_kitty_graphics_image_t,
    size_t,
    const roastty_kitty_graphics_image_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_result_e
roastty_kitty_graphics_placement_iterator_new(
    roastty_kitty_graphics_placement_iterator_t*);
ROASTTY_API void roastty_kitty_graphics_placement_iterator_free(
    roastty_kitty_graphics_placement_iterator_t);
ROASTTY_API roastty_result_e
roastty_kitty_graphics_placement_iterator_set(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_placement_iterator_option_e,
    const void*);
ROASTTY_API bool roastty_kitty_graphics_placement_next(
    roastty_kitty_graphics_placement_iterator_t);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_get(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_placement_data_e,
    void*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_get_multi(
    roastty_kitty_graphics_placement_iterator_t,
    size_t,
    const roastty_kitty_graphics_placement_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_rect(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_image_t,
    roastty_terminal_t,
    roastty_grid_selection_s*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_pixel_size(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_image_t,
    roastty_terminal_t,
    uint32_t*,
    uint32_t*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_grid_size(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_image_t,
    roastty_terminal_t,
    uint32_t*,
    uint32_t*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_viewport_pos(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_image_t,
    roastty_terminal_t,
    int32_t*,
    int32_t*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_source_rect(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_image_t,
    uint32_t*,
    uint32_t*,
    uint32_t*,
    uint32_t*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_placement_render_info(
    roastty_kitty_graphics_placement_iterator_t,
    roastty_kitty_graphics_image_t,
    roastty_terminal_t,
    roastty_kitty_graphics_placement_render_info_s*);
ROASTTY_API roastty_result_e
roastty_kitty_graphics_render_placement_iterator_new(
    roastty_kitty_graphics_render_placement_iterator_t*);
ROASTTY_API void roastty_kitty_graphics_render_placement_iterator_free(
    roastty_kitty_graphics_render_placement_iterator_t);
ROASTTY_API roastty_result_e
roastty_kitty_graphics_render_placement_iterator_set(
    roastty_kitty_graphics_render_placement_iterator_t,
    roastty_kitty_graphics_placement_iterator_option_e,
    const void*);
ROASTTY_API roastty_result_e
roastty_kitty_graphics_render_placement_iterator_update(
    roastty_kitty_graphics_render_placement_iterator_t,
    roastty_terminal_t);
ROASTTY_API bool roastty_kitty_graphics_render_placement_next(
    roastty_kitty_graphics_render_placement_iterator_t);
ROASTTY_API roastty_result_e roastty_kitty_graphics_render_placement_get(
    roastty_kitty_graphics_render_placement_iterator_t,
    roastty_kitty_graphics_render_placement_data_e,
    void*);
ROASTTY_API roastty_result_e roastty_kitty_graphics_render_placement_get_multi(
    roastty_kitty_graphics_render_placement_iterator_t,
    size_t,
    const roastty_kitty_graphics_render_placement_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_kitty_graphics_image_t
roastty_kitty_graphics_render_placement_image(
    roastty_kitty_graphics_render_placement_iterator_t);
ROASTTY_API roastty_result_e
roastty_kitty_graphics_render_placement_render_info(
    roastty_kitty_graphics_render_placement_iterator_t,
    roastty_kitty_graphics_render_placement_info_s*);
ROASTTY_API roastty_result_e
roastty_terminal_take_pty_response(roastty_terminal_t, roastty_string_s*);
ROASTTY_API roastty_result_e
roastty_terminal_grid_ref(roastty_terminal_t,
                          roastty_grid_point_s,
                          roastty_grid_ref_s*);
ROASTTY_API roastty_result_e
roastty_grid_ref_cell(const roastty_grid_ref_s*, roastty_cell_t*);
ROASTTY_API roastty_result_e
roastty_grid_ref_row(const roastty_grid_ref_s*, roastty_row_t*);
ROASTTY_API roastty_result_e
roastty_grid_ref_graphemes(const roastty_grid_ref_s*,
                           uint32_t*,
                           size_t,
                           size_t*);
ROASTTY_API roastty_result_e
roastty_grid_ref_hyperlink_uri(const roastty_grid_ref_s*,
                               uint8_t*,
                               size_t,
                               size_t*);
ROASTTY_API roastty_result_e
roastty_grid_ref_style(const roastty_grid_ref_s*, roastty_style_s*);
ROASTTY_API roastty_result_e
roastty_terminal_point_from_grid_ref(roastty_terminal_t,
                                     const roastty_grid_ref_s*,
                                     roastty_point_tag_e,
                                     roastty_grid_point_coordinate_s*);
ROASTTY_API roastty_result_e
roastty_terminal_grid_ref_track(roastty_terminal_t,
                                roastty_grid_point_s,
                                roastty_tracked_grid_ref_t*);
ROASTTY_API void roastty_tracked_grid_ref_free(roastty_tracked_grid_ref_t);
ROASTTY_API bool
roastty_tracked_grid_ref_has_value(roastty_tracked_grid_ref_t);
ROASTTY_API roastty_result_e
roastty_tracked_grid_ref_snapshot(roastty_tracked_grid_ref_t,
                                  roastty_grid_ref_s*);
ROASTTY_API roastty_result_e
roastty_tracked_grid_ref_point(roastty_tracked_grid_ref_t,
                               roastty_point_tag_e,
                               roastty_grid_point_coordinate_s*);
ROASTTY_API roastty_result_e
roastty_tracked_grid_ref_set(roastty_tracked_grid_ref_t,
                             roastty_terminal_t,
                             roastty_grid_point_s);
ROASTTY_API roastty_result_e
roastty_terminal_select_word(roastty_terminal_t,
                             const roastty_terminal_select_word_options_s*,
                             roastty_grid_selection_s*);
ROASTTY_API roastty_result_e roastty_terminal_select_word_between(
    roastty_terminal_t,
    const roastty_terminal_select_word_between_options_s*,
    roastty_grid_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_select_line(roastty_terminal_t,
                             const roastty_terminal_select_line_options_s*,
                             roastty_grid_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_select_all(roastty_terminal_t, roastty_grid_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_select_output(roastty_terminal_t,
                               const roastty_grid_ref_s*,
                               roastty_grid_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_adjust(roastty_terminal_t,
                                  roastty_grid_selection_s*,
                                  roastty_selection_adjustment_e);
ROASTTY_API roastty_result_e
roastty_terminal_selection_order(roastty_terminal_t,
                                 const roastty_grid_selection_s*,
                                 roastty_selection_order_e*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_ordered(roastty_terminal_t,
                                   const roastty_grid_selection_s*,
                                   roastty_selection_order_e,
                                   roastty_grid_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_contains(roastty_terminal_t,
                                    const roastty_grid_selection_s*,
                                    roastty_grid_point_s,
                                    bool*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_equal(roastty_terminal_t,
                                 const roastty_grid_selection_s*,
                                 const roastty_grid_selection_s*,
                                 bool*);
ROASTTY_API roastty_result_e roastty_terminal_selection_format_buf(
    roastty_terminal_t,
    const roastty_terminal_selection_format_options_s*,
    uint8_t*,
    size_t,
    size_t*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_format(roastty_terminal_t,
                                  const roastty_terminal_selection_format_options_s*,
                                  roastty_string_s*);
ROASTTY_API roastty_result_e
roastty_formatter_terminal_new(roastty_formatter_t*,
                               roastty_terminal_t,
                               roastty_formatter_terminal_options_s);
ROASTTY_API roastty_result_e roastty_formatter_format_buf(roastty_formatter_t,
                                                          uint8_t*,
                                                          size_t,
                                                          size_t*);
ROASTTY_API roastty_result_e roastty_formatter_format(roastty_formatter_t,
                                                      roastty_string_s*);
ROASTTY_API void roastty_formatter_free(roastty_formatter_t);
ROASTTY_API roastty_result_e
roastty_selection_gesture_new(roastty_selection_gesture_t*);
ROASTTY_API void
roastty_selection_gesture_free(roastty_selection_gesture_t,
                               roastty_terminal_t);
ROASTTY_API void
roastty_selection_gesture_reset(roastty_selection_gesture_t,
                                roastty_terminal_t);
ROASTTY_API roastty_result_e roastty_selection_gesture_get(
    roastty_selection_gesture_t,
    roastty_terminal_t,
    roastty_selection_gesture_data_e,
    void*);
ROASTTY_API roastty_result_e roastty_selection_gesture_get_multi(
    roastty_selection_gesture_t,
    roastty_terminal_t,
    size_t,
    const roastty_selection_gesture_data_e*,
    void**,
    size_t*);
ROASTTY_API roastty_result_e
roastty_selection_gesture_event_new(roastty_selection_gesture_event_t*,
                                    roastty_selection_gesture_event_e);
ROASTTY_API void
roastty_selection_gesture_event_free(roastty_selection_gesture_event_t);
ROASTTY_API roastty_result_e roastty_selection_gesture_event_set(
    roastty_selection_gesture_event_t,
    roastty_selection_gesture_event_option_e,
    const void*);
ROASTTY_API roastty_result_e roastty_selection_gesture_handle_event(
    roastty_selection_gesture_t,
    roastty_terminal_t,
    roastty_selection_gesture_event_t,
    roastty_grid_selection_s*);

ROASTTY_API roastty_result_e roastty_key_event_new(roastty_key_event_t*);
ROASTTY_API void roastty_key_event_free(roastty_key_event_t);
ROASTTY_API roastty_result_e roastty_key_event_set_action(roastty_key_event_t,
                                                          int);
ROASTTY_API int roastty_key_event_get_action(roastty_key_event_t);
ROASTTY_API roastty_result_e roastty_key_event_set_key(roastty_key_event_t,
                                                       int);
ROASTTY_API int roastty_key_event_get_key(roastty_key_event_t);
ROASTTY_API roastty_result_e roastty_key_event_set_mods(roastty_key_event_t,
                                                        roastty_key_mods_s);
ROASTTY_API roastty_key_mods_s roastty_key_event_get_mods(roastty_key_event_t);
ROASTTY_API roastty_result_e roastty_key_event_set_consumed_mods(
    roastty_key_event_t,
    roastty_key_mods_s);
ROASTTY_API roastty_key_mods_s
roastty_key_event_get_consumed_mods(roastty_key_event_t);
ROASTTY_API roastty_result_e roastty_key_event_set_composing(roastty_key_event_t,
                                                             bool);
ROASTTY_API bool roastty_key_event_get_composing(roastty_key_event_t);
ROASTTY_API roastty_result_e roastty_key_event_set_utf8(roastty_key_event_t,
                                                        const uint8_t*,
                                                        size_t);
ROASTTY_API const uint8_t* roastty_key_event_get_utf8(roastty_key_event_t,
                                                      size_t*);
ROASTTY_API roastty_result_e roastty_key_event_set_unshifted_codepoint(
    roastty_key_event_t,
    uint32_t);
ROASTTY_API uint32_t
roastty_key_event_get_unshifted_codepoint(roastty_key_event_t);

ROASTTY_API roastty_result_e roastty_key_encoder_new(roastty_key_encoder_t*);
ROASTTY_API void roastty_key_encoder_free(roastty_key_encoder_t);
ROASTTY_API roastty_result_e roastty_key_encoder_setopt(roastty_key_encoder_t,
                                                        int,
                                                        const void*);
ROASTTY_API roastty_result_e roastty_key_encoder_encode(roastty_key_encoder_t,
                                                        roastty_key_event_t,
                                                        uint8_t*,
                                                        size_t,
                                                        size_t*);

ROASTTY_API roastty_result_e roastty_mouse_event_new(roastty_mouse_event_t*);
ROASTTY_API void roastty_mouse_event_free(roastty_mouse_event_t);
ROASTTY_API roastty_result_e roastty_mouse_event_set_action(roastty_mouse_event_t,
                                                            int);
ROASTTY_API int roastty_mouse_event_get_action(roastty_mouse_event_t);
ROASTTY_API roastty_result_e roastty_mouse_event_set_button(roastty_mouse_event_t,
                                                            int);
ROASTTY_API void roastty_mouse_event_clear_button(roastty_mouse_event_t);
ROASTTY_API bool roastty_mouse_event_get_button(roastty_mouse_event_t, int*);
ROASTTY_API void roastty_mouse_event_set_mods(roastty_mouse_event_t,
                                              roastty_mouse_mods_s);
ROASTTY_API roastty_mouse_mods_s roastty_mouse_event_get_mods(roastty_mouse_event_t);
ROASTTY_API void roastty_mouse_event_set_position(roastty_mouse_event_t,
                                                  roastty_mouse_position_s);
ROASTTY_API roastty_mouse_position_s
roastty_mouse_event_get_position(roastty_mouse_event_t);

ROASTTY_API roastty_result_e roastty_mouse_encoder_new(roastty_mouse_encoder_t*);
ROASTTY_API void roastty_mouse_encoder_free(roastty_mouse_encoder_t);
ROASTTY_API roastty_result_e roastty_mouse_encoder_setopt(roastty_mouse_encoder_t,
                                                          int,
                                                          const void*);
ROASTTY_API void roastty_mouse_encoder_reset(roastty_mouse_encoder_t);
ROASTTY_API roastty_result_e roastty_mouse_encoder_encode(roastty_mouse_encoder_t,
                                                          roastty_mouse_event_t,
                                                          uint8_t*,
                                                          size_t,
                                                          size_t*);

ROASTTY_API roastty_result_e roastty_osc_new(roastty_osc_parser_t*);
ROASTTY_API void roastty_osc_free(roastty_osc_parser_t);
ROASTTY_API void roastty_osc_reset(roastty_osc_parser_t);
ROASTTY_API void roastty_osc_next(roastty_osc_parser_t, uint8_t);
ROASTTY_API roastty_osc_command_t roastty_osc_end(roastty_osc_parser_t, int);
ROASTTY_API int roastty_osc_command_type(roastty_osc_command_t);
ROASTTY_API bool roastty_osc_command_data(roastty_osc_command_t, int, void*);

ROASTTY_API roastty_surface_config_s roastty_surface_config_new(void);
ROASTTY_API roastty_surface_t roastty_surface_new(roastty_app_t,
                                                  const roastty_surface_config_s*);
ROASTTY_API roastty_result_e roastty_surface_start(roastty_surface_t);
ROASTTY_API void roastty_surface_free(roastty_surface_t);
ROASTTY_API void* roastty_surface_userdata(roastty_surface_t);
ROASTTY_API roastty_app_t roastty_surface_app(roastty_surface_t);
ROASTTY_API roastty_surface_config_s
roastty_surface_inherited_config(roastty_surface_t, roastty_surface_context_e);
ROASTTY_API void roastty_surface_update_config(roastty_surface_t,
                                               roastty_config_t);
ROASTTY_API bool roastty_surface_needs_confirm_quit(roastty_surface_t);
ROASTTY_API bool roastty_surface_process_exited(roastty_surface_t);
ROASTTY_API bool roastty_surface_needs_render(roastty_surface_t);
ROASTTY_API void roastty_surface_refresh(roastty_surface_t);
ROASTTY_API void roastty_surface_draw(roastty_surface_t);
ROASTTY_API roastty_result_e
roastty_surface_render_state_update(roastty_surface_t, roastty_render_state_t);
ROASTTY_API void roastty_surface_set_content_scale(roastty_surface_t,
                                                   double,
                                                   double);
ROASTTY_API void roastty_surface_set_display_id(roastty_surface_t, uint32_t);
ROASTTY_API void roastty_surface_set_focus(roastty_surface_t, bool);
ROASTTY_API void roastty_surface_set_occlusion(roastty_surface_t, bool);
ROASTTY_API void roastty_surface_set_size(roastty_surface_t, uint32_t, uint32_t);
ROASTTY_API roastty_surface_size_s roastty_surface_size(roastty_surface_t);
ROASTTY_API uint64_t roastty_surface_foreground_pid(roastty_surface_t);
ROASTTY_API roastty_string_s roastty_surface_tty_name(roastty_surface_t);
ROASTTY_API void roastty_surface_set_color_scheme(roastty_surface_t,
                                                  roastty_color_scheme_e);
ROASTTY_API roastty_inspector_t roastty_surface_inspector(roastty_surface_t);
ROASTTY_API void roastty_inspector_free(roastty_surface_t);
ROASTTY_API bool roastty_inspector_metal_init(roastty_inspector_t, void*);
ROASTTY_API void roastty_inspector_metal_render(roastty_inspector_t, void*, void*);

/* Embedded config/misc tail (Issue 802 / Exp 10) — see roastty_action_* notes. */
ROASTTY_API void roastty_cli_try_action(void);
ROASTTY_API void roastty_set_window_background_blur(roastty_app_t, void*);
ROASTTY_API void roastty_inspector_set_focus(roastty_inspector_t, bool);
ROASTTY_API void roastty_inspector_set_content_scale(roastty_inspector_t,
                                                     double,
                                                     double);
ROASTTY_API void roastty_inspector_set_size(roastty_inspector_t,
                                            uint32_t,
                                            uint32_t);
ROASTTY_API void roastty_inspector_mouse_button(roastty_inspector_t,
                                                roastty_input_mouse_state_e,
                                                roastty_input_mouse_button_e,
                                                roastty_input_mods_e);
ROASTTY_API void roastty_inspector_mouse_pos(roastty_inspector_t,
                                             double,
                                             double);
ROASTTY_API void roastty_inspector_mouse_scroll(roastty_inspector_t,
                                                double,
                                                double,
                                                roastty_input_scroll_mods_t);
ROASTTY_API void roastty_inspector_key(roastty_inspector_t,
                                       roastty_input_action_e,
                                       roastty_input_key_e,
                                       roastty_input_mods_e);
ROASTTY_API void roastty_inspector_text(roastty_inspector_t, const char*);
ROASTTY_API void* roastty_surface_quicklook_font(roastty_surface_t);
ROASTTY_API bool roastty_surface_quicklook_word(roastty_surface_t,
                                                roastty_text_s*);
ROASTTY_API roastty_input_mods_e
roastty_surface_key_translation_mods(roastty_surface_t, roastty_input_mods_e);
ROASTTY_API bool roastty_surface_key(roastty_surface_t, roastty_input_key_s);
ROASTTY_API bool roastty_app_key(roastty_app_t, roastty_input_key_s);
ROASTTY_API void roastty_app_keyboard_changed(roastty_app_t);
ROASTTY_API bool roastty_surface_key_is_binding(roastty_surface_t,
                                                roastty_input_key_s,
                                                roastty_binding_flags_e*);
ROASTTY_API void roastty_surface_text(roastty_surface_t, const char*, uintptr_t);
ROASTTY_API void roastty_surface_preedit(roastty_surface_t,
                                         const char*,
                                         uintptr_t);
ROASTTY_API void roastty_surface_ime_point(roastty_surface_t,
                                           double*,
                                           double*,
                                           double*,
                                           double*);
ROASTTY_API void roastty_surface_complete_clipboard_request(roastty_surface_t,
                                                            const char*,
                                                            void*,
                                                            bool);
ROASTTY_API bool roastty_surface_mouse_captured(roastty_surface_t);
ROASTTY_API bool roastty_surface_mouse_button(roastty_surface_t,
                                              roastty_input_mouse_state_e,
                                              roastty_input_mouse_button_e,
                                              roastty_input_mods_e);
ROASTTY_API void roastty_surface_mouse_pos(roastty_surface_t,
                                           double,
                                           double,
                                           roastty_input_mods_e);
ROASTTY_API void roastty_surface_mouse_scroll(roastty_surface_t,
                                              double,
                                              double,
                                              roastty_input_scroll_mods_t);
ROASTTY_API void roastty_surface_mouse_pressure(roastty_surface_t,
                                                uint32_t,
                                                double);
ROASTTY_API bool roastty_surface_has_selection(roastty_surface_t);
ROASTTY_API bool roastty_surface_read_selection(roastty_surface_t,
                                                roastty_text_s*);
ROASTTY_API bool roastty_surface_read_text(roastty_surface_t,
                                           roastty_selection_s,
                                           roastty_text_s*);
ROASTTY_API void roastty_surface_free_text(roastty_surface_t, roastty_text_s*);
ROASTTY_API void roastty_surface_request_close(roastty_surface_t);
ROASTTY_API void roastty_surface_split(roastty_surface_t,
                                       roastty_split_direction_e);
ROASTTY_API void roastty_surface_split_focus(roastty_surface_t,
                                             roastty_goto_split_e);
ROASTTY_API void roastty_surface_split_resize(roastty_surface_t,
                                              roastty_resize_split_e,
                                              uint16_t);
ROASTTY_API void roastty_surface_split_equalize(roastty_surface_t);
ROASTTY_API bool roastty_surface_binding_action(roastty_surface_t,
                                                const char*,
                                                uintptr_t);

#ifdef __cplusplus
}
#endif

#endif
