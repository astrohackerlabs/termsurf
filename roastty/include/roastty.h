#ifndef ROASTTY_H
#define ROASTTY_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

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

typedef void* roastty_app_t;
typedef void* roastty_config_t;
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

typedef uint16_t roastty_mode_tag_t;

enum {
  ROASTTY_MODE_TAG_VALUE_MASK = 0x7fff,
  ROASTTY_MODE_TAG_ANSI_BIT = 0x8000,
};

typedef enum {
  ROASTTY_SUCCESS = 0,
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
  ROASTTY_TERMINAL_OPTION_SELECTION = 21,
} roastty_terminal_option_e;

typedef enum {
  ROASTTY_BUILD_MODE_DEBUG,
  ROASTTY_BUILD_MODE_RELEASE_SAFE,
  ROASTTY_BUILD_MODE_RELEASE_FAST,
  ROASTTY_BUILD_MODE_RELEASE_SMALL,
} roastty_build_mode_e;

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
} roastty_point_coordinate_s;

typedef union {
  roastty_point_coordinate_s active;
  roastty_point_coordinate_s viewport;
  roastty_point_coordinate_s screen;
  roastty_point_coordinate_s history;
  uint64_t _padding[2];
} roastty_point_value_u;

typedef struct {
  roastty_point_tag_e tag;
  roastty_point_value_u value;
} roastty_point_s;

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
} roastty_selection_s;

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
  const roastty_selection_s* selection;
} roastty_terminal_selection_format_options_s;

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

typedef enum {
  ROASTTY_KEY_UNIDENTIFIED = 0,
  ROASTTY_KEY_BACKQUOTE = 1,
  ROASTTY_KEY_BACKSLASH = 2,
  ROASTTY_KEY_BRACKET_LEFT = 3,
  ROASTTY_KEY_BRACKET_RIGHT = 4,
  ROASTTY_KEY_COMMA = 5,
  ROASTTY_KEY_DIGIT0 = 6,
  ROASTTY_KEY_DIGIT1 = 7,
  ROASTTY_KEY_DIGIT2 = 8,
  ROASTTY_KEY_DIGIT3 = 9,
  ROASTTY_KEY_DIGIT4 = 10,
  ROASTTY_KEY_DIGIT5 = 11,
  ROASTTY_KEY_DIGIT6 = 12,
  ROASTTY_KEY_DIGIT7 = 13,
  ROASTTY_KEY_DIGIT8 = 14,
  ROASTTY_KEY_DIGIT9 = 15,
  ROASTTY_KEY_EQUAL = 16,
  ROASTTY_KEY_INTL_BACKSLASH = 17,
  ROASTTY_KEY_INTL_RO = 18,
  ROASTTY_KEY_INTL_YEN = 19,
  ROASTTY_KEY_KEY_A = 20,
  ROASTTY_KEY_KEY_B = 21,
  ROASTTY_KEY_KEY_C = 22,
  ROASTTY_KEY_KEY_D = 23,
  ROASTTY_KEY_KEY_E = 24,
  ROASTTY_KEY_KEY_F = 25,
  ROASTTY_KEY_KEY_G = 26,
  ROASTTY_KEY_KEY_H = 27,
  ROASTTY_KEY_KEY_I = 28,
  ROASTTY_KEY_KEY_J = 29,
  ROASTTY_KEY_KEY_K = 30,
  ROASTTY_KEY_KEY_L = 31,
  ROASTTY_KEY_KEY_M = 32,
  ROASTTY_KEY_KEY_N = 33,
  ROASTTY_KEY_KEY_O = 34,
  ROASTTY_KEY_KEY_P = 35,
  ROASTTY_KEY_KEY_Q = 36,
  ROASTTY_KEY_KEY_R = 37,
  ROASTTY_KEY_KEY_S = 38,
  ROASTTY_KEY_KEY_T = 39,
  ROASTTY_KEY_KEY_U = 40,
  ROASTTY_KEY_KEY_V = 41,
  ROASTTY_KEY_KEY_W = 42,
  ROASTTY_KEY_KEY_X = 43,
  ROASTTY_KEY_KEY_Y = 44,
  ROASTTY_KEY_KEY_Z = 45,
  ROASTTY_KEY_MINUS = 46,
  ROASTTY_KEY_PERIOD = 47,
  ROASTTY_KEY_QUOTE = 48,
  ROASTTY_KEY_SEMICOLON = 49,
  ROASTTY_KEY_SLASH = 50,
  ROASTTY_KEY_ALT_LEFT = 51,
  ROASTTY_KEY_ALT_RIGHT = 52,
  ROASTTY_KEY_BACKSPACE = 53,
  ROASTTY_KEY_CAPS_LOCK = 54,
  ROASTTY_KEY_CONTEXT_MENU = 55,
  ROASTTY_KEY_CONTROL_LEFT = 56,
  ROASTTY_KEY_CONTROL_RIGHT = 57,
  ROASTTY_KEY_ENTER = 58,
  ROASTTY_KEY_META_LEFT = 59,
  ROASTTY_KEY_META_RIGHT = 60,
  ROASTTY_KEY_SHIFT_LEFT = 61,
  ROASTTY_KEY_SHIFT_RIGHT = 62,
  ROASTTY_KEY_SPACE = 63,
  ROASTTY_KEY_TAB = 64,
  ROASTTY_KEY_CONVERT = 65,
  ROASTTY_KEY_KANA_MODE = 66,
  ROASTTY_KEY_NON_CONVERT = 67,
  ROASTTY_KEY_DELETE = 68,
  ROASTTY_KEY_END = 69,
  ROASTTY_KEY_HELP = 70,
  ROASTTY_KEY_HOME = 71,
  ROASTTY_KEY_INSERT = 72,
  ROASTTY_KEY_PAGE_DOWN = 73,
  ROASTTY_KEY_PAGE_UP = 74,
  ROASTTY_KEY_ARROW_DOWN = 75,
  ROASTTY_KEY_ARROW_LEFT = 76,
  ROASTTY_KEY_ARROW_RIGHT = 77,
  ROASTTY_KEY_ARROW_UP = 78,
  ROASTTY_KEY_NUM_LOCK = 79,
  ROASTTY_KEY_NUMPAD0 = 80,
  ROASTTY_KEY_NUMPAD1 = 81,
  ROASTTY_KEY_NUMPAD2 = 82,
  ROASTTY_KEY_NUMPAD3 = 83,
  ROASTTY_KEY_NUMPAD4 = 84,
  ROASTTY_KEY_NUMPAD5 = 85,
  ROASTTY_KEY_NUMPAD6 = 86,
  ROASTTY_KEY_NUMPAD7 = 87,
  ROASTTY_KEY_NUMPAD8 = 88,
  ROASTTY_KEY_NUMPAD9 = 89,
  ROASTTY_KEY_NUMPAD_ADD = 90,
  ROASTTY_KEY_NUMPAD_BACKSPACE = 91,
  ROASTTY_KEY_NUMPAD_CLEAR = 92,
  ROASTTY_KEY_NUMPAD_CLEAR_ENTRY = 93,
  ROASTTY_KEY_NUMPAD_COMMA = 94,
  ROASTTY_KEY_NUMPAD_DECIMAL = 95,
  ROASTTY_KEY_NUMPAD_DIVIDE = 96,
  ROASTTY_KEY_NUMPAD_ENTER = 97,
  ROASTTY_KEY_NUMPAD_EQUAL = 98,
  ROASTTY_KEY_NUMPAD_MEMORY_ADD = 99,
  ROASTTY_KEY_NUMPAD_MEMORY_CLEAR = 100,
  ROASTTY_KEY_NUMPAD_MEMORY_RECALL = 101,
  ROASTTY_KEY_NUMPAD_MEMORY_STORE = 102,
  ROASTTY_KEY_NUMPAD_MEMORY_SUBTRACT = 103,
  ROASTTY_KEY_NUMPAD_MULTIPLY = 104,
  ROASTTY_KEY_NUMPAD_PAREN_LEFT = 105,
  ROASTTY_KEY_NUMPAD_PAREN_RIGHT = 106,
  ROASTTY_KEY_NUMPAD_SUBTRACT = 107,
  ROASTTY_KEY_NUMPAD_SEPARATOR = 108,
  ROASTTY_KEY_NUMPAD_UP = 109,
  ROASTTY_KEY_NUMPAD_DOWN = 110,
  ROASTTY_KEY_NUMPAD_RIGHT = 111,
  ROASTTY_KEY_NUMPAD_LEFT = 112,
  ROASTTY_KEY_NUMPAD_BEGIN = 113,
  ROASTTY_KEY_NUMPAD_HOME = 114,
  ROASTTY_KEY_NUMPAD_END = 115,
  ROASTTY_KEY_NUMPAD_INSERT = 116,
  ROASTTY_KEY_NUMPAD_DELETE = 117,
  ROASTTY_KEY_NUMPAD_PAGE_UP = 118,
  ROASTTY_KEY_NUMPAD_PAGE_DOWN = 119,
  ROASTTY_KEY_ESCAPE = 120,
  ROASTTY_KEY_F1 = 121,
  ROASTTY_KEY_F2 = 122,
  ROASTTY_KEY_F3 = 123,
  ROASTTY_KEY_F4 = 124,
  ROASTTY_KEY_F5 = 125,
  ROASTTY_KEY_F6 = 126,
  ROASTTY_KEY_F7 = 127,
  ROASTTY_KEY_F8 = 128,
  ROASTTY_KEY_F9 = 129,
  ROASTTY_KEY_F10 = 130,
  ROASTTY_KEY_F11 = 131,
  ROASTTY_KEY_F12 = 132,
  ROASTTY_KEY_F13 = 133,
  ROASTTY_KEY_F14 = 134,
  ROASTTY_KEY_F15 = 135,
  ROASTTY_KEY_F16 = 136,
  ROASTTY_KEY_F17 = 137,
  ROASTTY_KEY_F18 = 138,
  ROASTTY_KEY_F19 = 139,
  ROASTTY_KEY_F20 = 140,
  ROASTTY_KEY_F21 = 141,
  ROASTTY_KEY_F22 = 142,
  ROASTTY_KEY_F23 = 143,
  ROASTTY_KEY_F24 = 144,
  ROASTTY_KEY_F25 = 145,
  ROASTTY_KEY_FN = 146,
  ROASTTY_KEY_FN_LOCK = 147,
  ROASTTY_KEY_PRINT_SCREEN = 148,
  ROASTTY_KEY_SCROLL_LOCK = 149,
  ROASTTY_KEY_PAUSE = 150,
  ROASTTY_KEY_BROWSER_BACK = 151,
  ROASTTY_KEY_BROWSER_FAVORITES = 152,
  ROASTTY_KEY_BROWSER_FORWARD = 153,
  ROASTTY_KEY_BROWSER_HOME = 154,
  ROASTTY_KEY_BROWSER_REFRESH = 155,
  ROASTTY_KEY_BROWSER_SEARCH = 156,
  ROASTTY_KEY_BROWSER_STOP = 157,
  ROASTTY_KEY_EJECT = 158,
  ROASTTY_KEY_LAUNCH_APP1 = 159,
  ROASTTY_KEY_LAUNCH_APP2 = 160,
  ROASTTY_KEY_LAUNCH_MAIL = 161,
  ROASTTY_KEY_MEDIA_PLAY_PAUSE = 162,
  ROASTTY_KEY_MEDIA_SELECT = 163,
  ROASTTY_KEY_MEDIA_STOP = 164,
  ROASTTY_KEY_MEDIA_TRACK_NEXT = 165,
  ROASTTY_KEY_MEDIA_TRACK_PREVIOUS = 166,
  ROASTTY_KEY_POWER = 167,
  ROASTTY_KEY_SLEEP = 168,
  ROASTTY_KEY_AUDIO_VOLUME_DOWN = 169,
  ROASTTY_KEY_AUDIO_VOLUME_MUTE = 170,
  ROASTTY_KEY_AUDIO_VOLUME_UP = 171,
  ROASTTY_KEY_WAKE_UP = 172,
  ROASTTY_KEY_COPY = 173,
  ROASTTY_KEY_CUT = 174,
  ROASTTY_KEY_PASTE = 175,
} roastty_key_e;

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
  ROASTTY_CLIPBOARD_REQUEST_STANDARD,
  ROASTTY_CLIPBOARD_REQUEST_SELECTION,
} roastty_clipboard_request_e;

typedef struct {
  const char* mime;
  const char* data;
} roastty_clipboard_content_s;

typedef enum {
  ROASTTY_TARGET_APP,
  ROASTTY_TARGET_SURFACE,
} roastty_target_tag_e;

typedef struct {
  roastty_target_tag_e tag;
  roastty_surface_t surface;
} roastty_target_s;

typedef struct {
  int tag;
  uintptr_t storage[8];
} roastty_action_s;

typedef enum {
  ROASTTY_MOUSE_ACTION_PRESS = 0,
  ROASTTY_MOUSE_ACTION_RELEASE = 1,
  ROASTTY_MOUSE_ACTION_MOTION = 2,
} roastty_mouse_action_e;

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
ROASTTY_API roastty_result_e
roastty_terminal_take_pty_response(roastty_terminal_t, roastty_string_s*);
ROASTTY_API roastty_result_e
roastty_terminal_grid_ref(roastty_terminal_t,
                          roastty_point_s,
                          roastty_grid_ref_s*);
ROASTTY_API roastty_result_e
roastty_terminal_point_from_grid_ref(roastty_terminal_t,
                                     const roastty_grid_ref_s*,
                                     roastty_point_tag_e,
                                     roastty_point_coordinate_s*);
ROASTTY_API roastty_result_e
roastty_terminal_grid_ref_track(roastty_terminal_t,
                                roastty_point_s,
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
                               roastty_point_coordinate_s*);
ROASTTY_API roastty_result_e
roastty_tracked_grid_ref_set(roastty_tracked_grid_ref_t,
                             roastty_terminal_t,
                             roastty_point_s);
ROASTTY_API roastty_result_e
roastty_terminal_select_word(roastty_terminal_t,
                             const roastty_terminal_select_word_options_s*,
                             roastty_selection_s*);
ROASTTY_API roastty_result_e roastty_terminal_select_word_between(
    roastty_terminal_t,
    const roastty_terminal_select_word_between_options_s*,
    roastty_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_select_line(roastty_terminal_t,
                             const roastty_terminal_select_line_options_s*,
                             roastty_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_select_all(roastty_terminal_t, roastty_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_select_output(roastty_terminal_t,
                               const roastty_grid_ref_s*,
                               roastty_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_adjust(roastty_terminal_t,
                                  roastty_selection_s*,
                                  roastty_selection_adjustment_e);
ROASTTY_API roastty_result_e
roastty_terminal_selection_order(roastty_terminal_t,
                                 const roastty_selection_s*,
                                 roastty_selection_order_e*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_ordered(roastty_terminal_t,
                                   const roastty_selection_s*,
                                   roastty_selection_order_e,
                                   roastty_selection_s*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_contains(roastty_terminal_t,
                                    const roastty_selection_s*,
                                    roastty_point_s,
                                    bool*);
ROASTTY_API roastty_result_e
roastty_terminal_selection_equal(roastty_terminal_t,
                                 const roastty_selection_s*,
                                 const roastty_selection_s*,
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
    roastty_selection_s*);

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
ROASTTY_API void roastty_surface_free(roastty_surface_t);
ROASTTY_API void* roastty_surface_userdata(roastty_surface_t);
ROASTTY_API roastty_app_t roastty_surface_app(roastty_surface_t);
ROASTTY_API void roastty_surface_update_config(roastty_surface_t,
                                               roastty_config_t);
ROASTTY_API bool roastty_surface_needs_confirm_quit(roastty_surface_t);
ROASTTY_API bool roastty_surface_process_exited(roastty_surface_t);
ROASTTY_API void roastty_surface_set_content_scale(roastty_surface_t,
                                                   double,
                                                   double);
ROASTTY_API void roastty_surface_set_focus(roastty_surface_t, bool);
ROASTTY_API void roastty_surface_set_occlusion(roastty_surface_t, bool);
ROASTTY_API void roastty_surface_set_size(roastty_surface_t, uint32_t, uint32_t);
ROASTTY_API roastty_surface_size_s roastty_surface_size(roastty_surface_t);
ROASTTY_API uint64_t roastty_surface_foreground_pid(roastty_surface_t);
ROASTTY_API roastty_string_s roastty_surface_tty_name(roastty_surface_t);
ROASTTY_API void roastty_surface_set_color_scheme(roastty_surface_t,
                                                  roastty_color_scheme_e);
ROASTTY_API void roastty_surface_request_close(roastty_surface_t);

#ifdef __cplusplus
}
#endif

#endif
