#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

#include "roastty.h"

static void wakeup_cb(void *userdata) {
  assert(userdata == (void *)0xA991);
}

static bool action_cb(roastty_app_t app,
                      roastty_target_s target,
                      roastty_action_s action) {
  (void)app;
  (void)target;
  (void)action;
  return false;
}

static bool read_clipboard_cb(void *userdata,
                              roastty_clipboard_e clipboard,
                              void *state) {
  (void)userdata;
  (void)clipboard;
  (void)state;
  return false;
}

static void confirm_read_clipboard_cb(void *userdata,
                                      const char *str,
                                      void *state,
                                      roastty_clipboard_request_e request) {
  (void)userdata;
  (void)str;
  (void)state;
  (void)request;
}

static void write_clipboard_cb(void *userdata,
                               roastty_clipboard_e clipboard,
                               const roastty_clipboard_content_s *content,
                               size_t len,
                               bool confirm) {
  (void)userdata;
  (void)clipboard;
  (void)content;
  (void)len;
  (void)confirm;
}

static size_t close_surface_call_count = 0;
static void *close_surface_last_userdata = NULL;
static bool close_surface_last_needs_confirm = true;

static void close_surface_cb(void *userdata, bool needs_confirm) {
  close_surface_call_count++;
  close_surface_last_userdata = userdata;
  close_surface_last_needs_confirm = needs_confirm;
}

static size_t support_alloc_count = 0;
static size_t support_free_count = 0;
static void *support_last_ctx = NULL;
static size_t support_last_len = 0;
static uint8_t support_last_alignment = 0;

static void *support_alloc_cb(void *ctx,
                              size_t len,
                              uint8_t alignment,
                              uintptr_t ret_addr) {
  assert(ret_addr == 0);
  support_alloc_count++;
  support_last_ctx = ctx;
  support_last_len = len;
  support_last_alignment = alignment;
  return malloc(len);
}

static bool support_resize_cb(void *ctx,
                              void *memory,
                              size_t memory_len,
                              uint8_t alignment,
                              size_t new_len,
                              uintptr_t ret_addr) {
  (void)ctx;
  (void)memory;
  (void)memory_len;
  (void)alignment;
  (void)new_len;
  (void)ret_addr;
  return false;
}

static void *support_remap_cb(void *ctx,
                              void *memory,
                              size_t memory_len,
                              uint8_t alignment,
                              size_t new_len,
                              uintptr_t ret_addr) {
  (void)ctx;
  (void)memory;
  (void)memory_len;
  (void)alignment;
  (void)new_len;
  (void)ret_addr;
  return NULL;
}

static void support_free_cb(void *ctx,
                            void *memory,
                            size_t memory_len,
                            uint8_t alignment,
                            uintptr_t ret_addr) {
  assert(ret_addr == 0);
  support_free_count++;
  support_last_ctx = ctx;
  support_last_len = memory_len;
  support_last_alignment = alignment;
  free(memory);
}

static bool support_log_called = false;
static void *support_log_userdata = NULL;
static bool support_decode_called = false;
static void *support_decode_userdata = NULL;

static void support_log_cb(void *userdata,
                           roastty_sys_log_level_e level,
                           const uint8_t *scope,
                           size_t scope_len,
                           const uint8_t *message,
                           size_t message_len) {
  (void)level;
  (void)scope;
  (void)scope_len;
  (void)message;
  (void)message_len;
  support_log_called = true;
  support_log_userdata = userdata;
}

static bool support_decode_cb(void *userdata,
                              const roastty_allocator_s *allocator,
                              const uint8_t *data,
                              size_t data_len,
                              roastty_sys_image_s *out) {
  (void)allocator;
  (void)data;
  (void)data_len;
  support_decode_called = true;
  support_decode_userdata = userdata;
  out->width = 0;
  out->height = 0;
  out->data = NULL;
  out->data_len = 0;
  return true;
}

static roastty_terminal_t effect_terminal = NULL;
static void *effect_userdata = NULL;
static uint8_t effect_write_bytes[256] = {0};
static size_t effect_write_len = 0;
static size_t effect_write_count = 0;
static size_t effect_bell_count = 0;
static size_t effect_title_changed_count = 0;
static const char *effect_enquiry = NULL;
static size_t effect_enquiry_len = 0;
static const char *effect_xtversion = NULL;
static size_t effect_xtversion_len = 0;
static roastty_size_report_size_s effect_size = {0};
static bool effect_size_result = false;
static size_t effect_size_count = 0;
static roastty_color_scheme_e effect_color_scheme = ROASTTY_COLOR_SCHEME_LIGHT;
static bool effect_color_scheme_result = false;
static size_t effect_color_scheme_count = 0;
static roastty_device_attributes_s effect_device_attributes = {0};
static bool effect_device_attributes_result = false;
static size_t effect_device_attributes_count = 0;

static void reset_effect_state(void) {
  effect_terminal = NULL;
  effect_userdata = NULL;
  memset(effect_write_bytes, 0, sizeof(effect_write_bytes));
  effect_write_len = 0;
  effect_write_count = 0;
  effect_bell_count = 0;
  effect_title_changed_count = 0;
  effect_enquiry = NULL;
  effect_enquiry_len = 0;
  effect_xtversion = NULL;
  effect_xtversion_len = 0;
  effect_size = (roastty_size_report_size_s){0};
  effect_size_result = false;
  effect_size_count = 0;
  effect_color_scheme = ROASTTY_COLOR_SCHEME_LIGHT;
  effect_color_scheme_result = false;
  effect_color_scheme_count = 0;
  effect_device_attributes = (roastty_device_attributes_s){0};
  effect_device_attributes_result = false;
  effect_device_attributes_count = 0;
}

static void terminal_write_pty_cb(roastty_terminal_t terminal,
                                  void *userdata,
                                  const uint8_t *ptr,
                                  size_t len) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  effect_write_len = len;
  effect_write_count++;
  assert(len <= sizeof(effect_write_bytes));
  if (len > 0) {
    assert(ptr != NULL);
    memcpy(effect_write_bytes, ptr, len);
  }
}

static void terminal_bell_cb(roastty_terminal_t terminal, void *userdata) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  effect_bell_count++;
}

static roastty_string_s terminal_enquiry_cb(roastty_terminal_t terminal,
                                            void *userdata) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  roastty_string_s value = {
      .ptr = effect_enquiry,
      .len = effect_enquiry_len,
      .sentinel = false,
  };
  return value;
}

static roastty_string_s terminal_xtversion_cb(roastty_terminal_t terminal,
                                              void *userdata) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  roastty_string_s value = {
      .ptr = effect_xtversion,
      .len = effect_xtversion_len,
      .sentinel = false,
  };
  return value;
}

static void terminal_title_changed_cb(roastty_terminal_t terminal,
                                      void *userdata) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  effect_title_changed_count++;
}

static bool terminal_size_cb(roastty_terminal_t terminal,
                             void *userdata,
                             roastty_size_report_size_s *out_size) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  effect_size_count++;
  if (effect_size_result && out_size != NULL) {
    *out_size = effect_size;
  }
  return effect_size_result;
}

static bool terminal_color_scheme_cb(roastty_terminal_t terminal,
                                     void *userdata,
                                     roastty_color_scheme_e *out_scheme) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  effect_color_scheme_count++;
  if (effect_color_scheme_result && out_scheme != NULL) {
    *out_scheme = effect_color_scheme;
  }
  return effect_color_scheme_result;
}

static bool terminal_device_attributes_cb(
    roastty_terminal_t terminal,
    void *userdata,
    roastty_device_attributes_s *out_attrs) {
  effect_terminal = terminal;
  effect_userdata = userdata;
  effect_device_attributes_count++;
  if (effect_device_attributes_result && out_attrs != NULL) {
    *out_attrs = effect_device_attributes;
  }
  return effect_device_attributes_result;
}

static void assert_config_bool(roastty_config_t config,
                               const char *key,
                               bool expected) {
  bool value = !expected;
  assert(roastty_config_get(config, &value, key, strlen(key)));
  assert(value == expected);
}

static void assert_config_string(roastty_config_t config,
                                 const char *key,
                                 const char *expected) {
  const char *value = NULL;
  assert(roastty_config_get(config, &value, key, strlen(key)));
  assert(value != NULL);
  assert(strcmp(value, expected) == 0);
}

static void assert_config_double(roastty_config_t config,
                                 const char *key,
                                 double expected) {
  double value = -1.0;
  assert(roastty_config_get(config, &value, key, strlen(key)));
  assert(value == expected);
}

static void assert_config_uintptr(roastty_config_t config,
                                  const char *key,
                                  uintptr_t expected) {
  uintptr_t value = 0;
  assert(roastty_config_get(config, &value, key, strlen(key)));
  assert(value == expected);
}

static void assert_roastty_string_eq(roastty_string_s value,
                                     const char *expected) {
  size_t len = strlen(expected);
  assert(value.len == len);
  if (len == 0) {
    assert(value.ptr == NULL);
  } else {
    assert(value.ptr != NULL);
    assert(memcmp(value.ptr, expected, len) == 0);
  }
  assert(!value.sentinel);
  roastty_string_free(value);
}

static bool bytes_contains(const char *haystack,
                           size_t haystack_len,
                           const char *needle,
                           size_t needle_len) {
  if (needle_len == 0) {
    return true;
  }
  if (haystack_len < needle_len) {
    return false;
  }
  for (size_t i = 0; i <= haystack_len - needle_len; i++) {
    if (memcmp(haystack + i, needle, needle_len) == 0) {
      return true;
    }
  }
  return false;
}

static roastty_cell_t packed_cell(roastty_cell_content_tag_e tag,
                                  uint64_t content);
static roastty_cell_t packed_cell_with_flags(
    roastty_cell_t cell,
    uint16_t style_id,
    roastty_cell_wide_e wide,
    bool protected,
    bool hyperlink,
    roastty_cell_semantic_content_e semantic);
static roastty_row_t packed_row(bool wrap,
                                bool wrap_continuation,
                                bool grapheme,
                                bool styled,
                                bool hyperlink,
                                roastty_row_semantic_prompt_e semantic_prompt,
                                bool kitty_virtual_placeholder,
                                bool dirty);

static void assert_style_abi(void) {
  assert(ROASTTY_STYLE_COLOR_NONE == 0);
  assert(ROASTTY_STYLE_COLOR_PALETTE == 1);
  assert(ROASTTY_STYLE_COLOR_RGB == 2);

  assert(sizeof(roastty_style_color_value_u) == 8);
  assert(_Alignof(roastty_style_color_value_u) == 8);
  assert(sizeof(roastty_style_color_s) == 16);
  assert(_Alignof(roastty_style_color_s) == 8);
  assert(offsetof(roastty_style_color_s, tag) == 0);
  assert(offsetof(roastty_style_color_s, value) == 8);
  assert(sizeof(roastty_style_s) == 72);
  assert(_Alignof(roastty_style_s) == 8);
  assert(offsetof(roastty_style_s, size) == 0);
  assert(offsetof(roastty_style_s, fg_color) == 8);
  assert(offsetof(roastty_style_s, bg_color) == 24);
  assert(offsetof(roastty_style_s, underline_color) == 40);
  assert(offsetof(roastty_style_s, bold) == 56);
  assert(offsetof(roastty_style_s, italic) == 57);
  assert(offsetof(roastty_style_s, faint) == 58);
  assert(offsetof(roastty_style_s, blink) == 59);
  assert(offsetof(roastty_style_s, inverse) == 60);
  assert(offsetof(roastty_style_s, invisible) == 61);
  assert(offsetof(roastty_style_s, strikethrough) == 62);
  assert(offsetof(roastty_style_s, overline) == 63);
  assert(offsetof(roastty_style_s, underline) == 64);

  roastty_style_default(NULL);
  assert(!roastty_style_is_default(NULL));

  roastty_style_s style = {0};
  roastty_style_default(&style);
  assert(style.size == sizeof(roastty_style_s));
  assert(style.fg_color.tag == ROASTTY_STYLE_COLOR_NONE);
  assert(style.bg_color.tag == ROASTTY_STYLE_COLOR_NONE);
  assert(style.underline_color.tag == ROASTTY_STYLE_COLOR_NONE);
  assert(!style.bold);
  assert(!style.italic);
  assert(!style.faint);
  assert(!style.blink);
  assert(!style.inverse);
  assert(!style.invisible);
  assert(!style.strikethrough);
  assert(!style.overline);
  assert(style.underline == 0);
  assert(roastty_style_is_default(&style));

  style.size = sizeof(roastty_style_s) - 1;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);

  style.fg_color.tag = ROASTTY_STYLE_COLOR_PALETTE;
  style.fg_color.value.palette = 7;
  assert(style.fg_color.value.palette == 7);
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);

  style.bg_color.tag = ROASTTY_STYLE_COLOR_RGB;
  style.bg_color.value.rgb = (roastty_rgb_s){.r = 1, .g = 2, .b = 3};
  assert(style.bg_color.value.rgb.r == 1);
  assert(style.bg_color.value.rgb.g == 2);
  assert(style.bg_color.value.rgb.b == 3);
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);

  style.underline_color.tag = ROASTTY_STYLE_COLOR_RGB;
  style.underline_color.value.rgb = (roastty_rgb_s){.r = 4, .g = 5, .b = 6};
  assert(style.underline_color.value.rgb.r == 4);
  assert(style.underline_color.value.rgb.g == 5);
  assert(style.underline_color.value.rgb.b == 6);
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);

  style.bold = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.italic = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.faint = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.blink = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.inverse = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.invisible = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.strikethrough = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);
  style.overline = true;
  assert(!roastty_style_is_default(&style));
  roastty_style_default(&style);

  for (int underline = 1; underline <= 5; underline++) {
    style.underline = underline;
    assert(!roastty_style_is_default(&style));
    roastty_style_default(&style);
  }
}

static void assert_row_cell_abi(void) {
  assert(sizeof(roastty_cell_t) == 8);
  assert(_Alignof(roastty_cell_t) == 8);
  assert(sizeof(roastty_row_t) == 8);
  assert(_Alignof(roastty_row_t) == 8);

  assert(ROASTTY_CELL_CONTENT_CODEPOINT == 0);
  assert(ROASTTY_CELL_CONTENT_CODEPOINT_GRAPHEME == 1);
  assert(ROASTTY_CELL_CONTENT_BG_COLOR_PALETTE == 2);
  assert(ROASTTY_CELL_CONTENT_BG_COLOR_RGB == 3);
  assert(ROASTTY_CELL_WIDE_NARROW == 0);
  assert(ROASTTY_CELL_WIDE_WIDE == 1);
  assert(ROASTTY_CELL_WIDE_SPACER_TAIL == 2);
  assert(ROASTTY_CELL_WIDE_SPACER_HEAD == 3);
  assert(ROASTTY_CELL_SEMANTIC_OUTPUT == 0);
  assert(ROASTTY_CELL_SEMANTIC_INPUT == 1);
  assert(ROASTTY_CELL_SEMANTIC_PROMPT == 2);
  assert(ROASTTY_CELL_DATA_INVALID == 0);
  assert(ROASTTY_CELL_DATA_CODEPOINT == 1);
  assert(ROASTTY_CELL_DATA_CONTENT_TAG == 2);
  assert(ROASTTY_CELL_DATA_WIDE == 3);
  assert(ROASTTY_CELL_DATA_HAS_TEXT == 4);
  assert(ROASTTY_CELL_DATA_HAS_STYLING == 5);
  assert(ROASTTY_CELL_DATA_STYLE_ID == 6);
  assert(ROASTTY_CELL_DATA_HAS_HYPERLINK == 7);
  assert(ROASTTY_CELL_DATA_PROTECTED == 8);
  assert(ROASTTY_CELL_DATA_SEMANTIC == 9);
  assert(ROASTTY_CELL_DATA_COLOR_PALETTE == 10);
  assert(ROASTTY_CELL_DATA_COLOR_RGB == 11);
  assert(ROASTTY_ROW_SEMANTIC_NONE == 0);
  assert(ROASTTY_ROW_SEMANTIC_PROMPT == 1);
  assert(ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION == 2);
  assert(ROASTTY_ROW_DATA_INVALID == 0);
  assert(ROASTTY_ROW_DATA_WRAP == 1);
  assert(ROASTTY_ROW_DATA_WRAP_CONTINUATION == 2);
  assert(ROASTTY_ROW_DATA_GRAPHEME == 3);
  assert(ROASTTY_ROW_DATA_STYLED == 4);
  assert(ROASTTY_ROW_DATA_HYPERLINK == 5);
  assert(ROASTTY_ROW_DATA_SEMANTIC_PROMPT == 6);
  assert(ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER == 7);
  assert(ROASTTY_ROW_DATA_DIRTY == 8);

  roastty_cell_t cell = packed_cell_with_flags(
      packed_cell(ROASTTY_CELL_CONTENT_CODEPOINT, 'A'),
      0x1234,
      ROASTTY_CELL_WIDE_SPACER_HEAD,
      true,
      true,
      ROASTTY_CELL_SEMANTIC_PROMPT);
  uint32_t codepoint = 0;
  roastty_cell_content_tag_e content_tag = ROASTTY_CELL_CONTENT_BG_COLOR_RGB;
  roastty_cell_wide_e wide = ROASTTY_CELL_WIDE_NARROW;
  bool flag = false;
  uint16_t style_id = 0;
  roastty_cell_semantic_content_e semantic = ROASTTY_CELL_SEMANTIC_OUTPUT;

  assert(roastty_cell_get(cell,
                          ROASTTY_CELL_DATA_CODEPOINT,
                          &codepoint) == ROASTTY_SUCCESS);
  assert(codepoint == 'A');
  assert(roastty_cell_get(cell,
                          ROASTTY_CELL_DATA_CONTENT_TAG,
                          &content_tag) == ROASTTY_SUCCESS);
  assert(content_tag == ROASTTY_CELL_CONTENT_CODEPOINT);
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_WIDE, &wide) ==
         ROASTTY_SUCCESS);
  assert(wide == ROASTTY_CELL_WIDE_SPACER_HEAD);
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_HAS_TEXT, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_HAS_STYLING, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_STYLE_ID, &style_id) ==
         ROASTTY_SUCCESS);
  assert(style_id == 0x1234);
  flag = false;
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_HAS_HYPERLINK, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_PROTECTED, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_SEMANTIC, &semantic) ==
         ROASTTY_SUCCESS);
  assert(semantic == ROASTTY_CELL_SEMANTIC_PROMPT);

  roastty_cell_t rgb_cell =
      packed_cell(ROASTTY_CELL_CONTENT_BG_COLOR_RGB, 0x00112233);
  roastty_rgb_s rgb = {0};
  assert(roastty_cell_get(rgb_cell, ROASTTY_CELL_DATA_COLOR_RGB, &rgb) ==
         ROASTTY_SUCCESS);
  assert(rgb.r == 0x11);
  assert(rgb.g == 0x22);
  assert(rgb.b == 0x33);
  uint8_t palette = 0;
  assert(roastty_cell_get(rgb_cell, ROASTTY_CELL_DATA_COLOR_PALETTE, &palette) ==
         ROASTTY_SUCCESS);
  assert(palette == 0x33);
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_INVALID, &flag) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_cell_get(cell, ROASTTY_CELL_DATA_CODEPOINT, NULL) ==
         ROASTTY_INVALID_VALUE);

  roastty_cell_data_e cell_keys[] = {
      ROASTTY_CELL_DATA_CODEPOINT,
      ROASTTY_CELL_DATA_WIDE,
      ROASTTY_CELL_DATA_INVALID,
  };
  codepoint = 0;
  wide = ROASTTY_CELL_WIDE_NARROW;
  void *cell_values[] = {&codepoint, &wide, &flag};
  size_t written = 99;
  assert(roastty_cell_get_multi(cell,
                                2,
                                cell_keys,
                                cell_values,
                                &written) == ROASTTY_SUCCESS);
  assert(written == 2);
  assert(codepoint == 'A');
  assert(wide == ROASTTY_CELL_WIDE_SPACER_HEAD);
  written = 99;
  assert(roastty_cell_get_multi(cell,
                                3,
                                cell_keys,
                                cell_values,
                                &written) == ROASTTY_INVALID_VALUE);
  assert(written == 2);
  written = 99;
  assert(roastty_cell_get_multi(cell, 0, cell_keys, cell_values, &written) ==
         ROASTTY_SUCCESS);
  assert(written == 0);
  assert(roastty_cell_get_multi(cell, 1, cell_keys, NULL, &written) ==
         ROASTTY_INVALID_VALUE);

  roastty_row_t row = packed_row(true,
                                 true,
                                 true,
                                 true,
                                 true,
                                 ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION,
                                 true,
                                 true);
  roastty_row_semantic_prompt_e row_semantic = ROASTTY_ROW_SEMANTIC_NONE;
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_WRAP, &flag) == ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_WRAP_CONTINUATION, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_GRAPHEME, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_STYLED, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_HYPERLINK, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  assert(roastty_row_get(row,
                         ROASTTY_ROW_DATA_SEMANTIC_PROMPT,
                         &row_semantic) == ROASTTY_SUCCESS);
  assert(row_semantic == ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION);
  flag = false;
  assert(roastty_row_get(row,
                         ROASTTY_ROW_DATA_KITTY_VIRTUAL_PLACEHOLDER,
                         &flag) == ROASTTY_SUCCESS);
  assert(flag);
  flag = false;
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_DIRTY, &flag) ==
         ROASTTY_SUCCESS);
  assert(flag);
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_INVALID, &flag) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_row_get(row, ROASTTY_ROW_DATA_WRAP, NULL) ==
         ROASTTY_INVALID_VALUE);

  roastty_row_data_e row_keys[] = {
      ROASTTY_ROW_DATA_WRAP,
      ROASTTY_ROW_DATA_SEMANTIC_PROMPT,
      ROASTTY_ROW_DATA_INVALID,
  };
  bool wrap = false;
  row_semantic = ROASTTY_ROW_SEMANTIC_NONE;
  void *row_values[] = {&wrap, &row_semantic, &flag};
  written = 99;
  assert(roastty_row_get_multi(row,
                               2,
                               row_keys,
                               row_values,
                               &written) == ROASTTY_SUCCESS);
  assert(written == 2);
  assert(wrap);
  assert(row_semantic == ROASTTY_ROW_SEMANTIC_PROMPT_CONTINUATION);
  written = 99;
  assert(roastty_row_get_multi(row,
                               3,
                               row_keys,
                               row_values,
                               &written) == ROASTTY_INVALID_VALUE);
  assert(written == 2);
  assert(roastty_row_get_multi(row, 1, NULL, row_values, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_row_get_multi(row, 1, row_keys, NULL, &written) ==
         ROASTTY_INVALID_VALUE);
}

static void assert_rgb_eq(roastty_rgb_s value, uint8_t r, uint8_t g, uint8_t b) {
  assert(value.r == r);
  assert(value.g == g);
  assert(value.b == b);
}

static void assert_render_state_abi(void) {
  assert(ROASTTY_RENDER_STATE_DIRTY_FALSE == 0);
  assert(ROASTTY_RENDER_STATE_DIRTY_PARTIAL == 1);
  assert(ROASTTY_RENDER_STATE_DIRTY_FULL == 2);

  assert(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR == 0);
  assert(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK == 1);
  assert(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE == 2);
  assert(ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW == 3);

  assert(ROASTTY_RENDER_STATE_DATA_INVALID == 0);
  assert(ROASTTY_RENDER_STATE_DATA_COLS == 1);
  assert(ROASTTY_RENDER_STATE_DATA_ROWS == 2);
  assert(ROASTTY_RENDER_STATE_DATA_DIRTY == 3);
  assert(ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR == 4);
  assert(ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND == 5);
  assert(ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND == 6);
  assert(ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR == 7);
  assert(ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE == 8);
  assert(ROASTTY_RENDER_STATE_DATA_COLOR_PALETTE == 9);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE == 10);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE == 11);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING == 12);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT == 13);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE == 14);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X == 15);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y == 16);
  assert(ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL == 17);
  assert(ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR == 18);
  assert(ROASTTY_RENDER_STATE_OPTION_DIRTY == 0);
  assert(ROASTTY_RENDER_STATE_ROW_DATA_INVALID == 0);
  assert(ROASTTY_RENDER_STATE_ROW_DATA_DIRTY == 1);
  assert(ROASTTY_RENDER_STATE_ROW_DATA_RAW == 2);
  assert(ROASTTY_RENDER_STATE_ROW_DATA_CELLS == 3);
  assert(ROASTTY_RENDER_STATE_ROW_DATA_SELECTION == 4);
  assert(ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY == 0);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_INVALID == 0);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW == 1);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE == 2);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN == 3);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF == 4);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR == 5);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR == 6);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED == 7);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING == 8);
  assert(ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8 == 9);
  assert(ROASTTY_KITTY_GRAPHICS_DATA_INVALID == 0);
  assert(ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR == 1);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_INVALID == 0);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID == 1);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID == 2);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL == 3);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_X_OFFSET == 4);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Y_OFFSET == 5);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_X == 6);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_Y == 7);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_WIDTH == 8);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_SOURCE_HEIGHT == 9);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS == 10);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS == 11);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z == 12);
  assert(ROASTTY_KITTY_PLACEMENT_LAYER_ALL == 0);
  assert(ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_BG == 1);
  assert(ROASTTY_KITTY_PLACEMENT_LAYER_BELOW_TEXT == 2);
  assert(ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT == 3);
  assert(ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER == 0);
  assert(ROASTTY_KITTY_IMAGE_FORMAT_RGB == 0);
  assert(ROASTTY_KITTY_IMAGE_FORMAT_RGBA == 1);
  assert(ROASTTY_KITTY_IMAGE_FORMAT_PNG == 2);
  assert(ROASTTY_KITTY_IMAGE_FORMAT_GRAY_ALPHA == 3);
  assert(ROASTTY_KITTY_IMAGE_FORMAT_GRAY == 4);
  assert(ROASTTY_KITTY_IMAGE_COMPRESSION_NONE == 0);
  assert(ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE == 1);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_INVALID == 0);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID == 1);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER == 2);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH == 3);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT == 4);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT == 5);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION == 6);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR == 7);
  assert(ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN == 8);

  assert(sizeof(roastty_render_state_colors_s) == 792);
  assert(_Alignof(roastty_render_state_colors_s) == 8);
  assert(offsetof(roastty_render_state_colors_s, size) == 0);
  assert(offsetof(roastty_render_state_colors_s, background) == 8);
  assert(offsetof(roastty_render_state_colors_s, foreground) == 11);
  assert(offsetof(roastty_render_state_colors_s, cursor) == 14);
  assert(offsetof(roastty_render_state_colors_s, cursor_has_value) == 17);
  assert(offsetof(roastty_render_state_colors_s, palette) == 18);
  assert(sizeof(roastty_render_state_row_selection_s) == 16);
  assert(_Alignof(roastty_render_state_row_selection_s) == 8);
  assert(offsetof(roastty_render_state_row_selection_s, size) == 0);
  assert(offsetof(roastty_render_state_row_selection_s, start_x) == 8);
  assert(offsetof(roastty_render_state_row_selection_s, end_x) == 10);
  assert(sizeof(roastty_buffer_s) == 24);
  assert(_Alignof(roastty_buffer_s) == 8);
  assert(offsetof(roastty_buffer_s, ptr) == 0);
  assert(offsetof(roastty_buffer_s, cap) == 8);
  assert(offsetof(roastty_buffer_s, len) == 16);

  roastty_render_state_t state = NULL;
  assert(roastty_render_state_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_new(&state) == ROASTTY_SUCCESS);
  assert(state != NULL);

  roastty_render_state_row_iterator_t iterator = NULL;
  assert(roastty_render_state_row_iterator_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_row_iterator_new(&iterator) == ROASTTY_SUCCESS);
  assert(iterator != NULL);
  assert(!roastty_render_state_row_iterator_next(NULL));
  assert(!roastty_render_state_row_iterator_next(iterator));
  roastty_render_state_row_cells_t cells = NULL;
  assert(roastty_render_state_row_cells_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_row_cells_new(&cells) == ROASTTY_SUCCESS);
  assert(cells != NULL);
  assert(!roastty_render_state_row_cells_next(NULL));
  assert(!roastty_render_state_row_cells_next(cells));
  roastty_kitty_graphics_render_placement_iterator_t render_placements = NULL;
  assert(roastty_kitty_graphics_render_placement_iterator_new(NULL) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_kitty_graphics_render_placement_iterator_new(&render_placements) ==
         ROASTTY_SUCCESS);
  assert(render_placements != NULL);
  assert(!roastty_kitty_graphics_render_placement_next(NULL));
  assert(!roastty_kitty_graphics_render_placement_next(render_placements));

  uint16_t dim = 999;
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_COLS, &dim) ==
         ROASTTY_SUCCESS);
  assert(dim == 0);
  dim = 999;
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_ROWS, &dim) ==
         ROASTTY_SUCCESS);
  assert(dim == 0);

  roastty_render_state_dirty_e dirty = ROASTTY_RENDER_STATE_DIRTY_FULL;
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_DIRTY, &dirty) ==
         ROASTTY_SUCCESS);
  assert(dirty == ROASTTY_RENDER_STATE_DIRTY_FALSE);

  roastty_rgb_s rgb = {9, 9, 9};
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_COLOR_BACKGROUND,
                                  &rgb) == ROASTTY_SUCCESS);
  assert_rgb_eq(rgb, 0, 0, 0);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_COLOR_FOREGROUND,
                                  &rgb) == ROASTTY_SUCCESS);
  assert_rgb_eq(rgb, 255, 255, 255);
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR, &rgb) ==
         ROASTTY_NO_VALUE);

  bool flag = true;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_COLOR_CURSOR_HAS_VALUE,
                                  &flag) == ROASTTY_SUCCESS);
  assert(!flag);
  flag = false;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE,
                                  &flag) == ROASTTY_SUCCESS);
  assert(flag);
  flag = true;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING,
                                  &flag) == ROASTTY_SUCCESS);
  assert(!flag);
  flag = true;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_PASSWORD_INPUT,
                                  &flag) == ROASTTY_SUCCESS);
  assert(!flag);
  flag = true;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE,
                                  &flag) == ROASTTY_SUCCESS);
  assert(!flag);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
                                  &dim) == ROASTTY_NO_VALUE);
  roastty_render_state_row_iterator_t null_iterator = NULL;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                                  &null_iterator) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                                  &iterator) == ROASTTY_SUCCESS);
  assert(!roastty_render_state_row_iterator_next(iterator));
  roastty_kitty_graphics_render_placement_iterator_t null_render_placements = NULL;
  assert(roastty_render_state_get(
             state,
             ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR,
             &null_render_placements) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(
             state,
             ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR,
             &render_placements) == ROASTTY_SUCCESS);
  assert(!roastty_kitty_graphics_render_placement_next(render_placements));
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_INVALID,
                                  &dim) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(state, 99, &dim) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(NULL,
                                  ROASTTY_RENDER_STATE_DATA_COLS,
                                  &dim) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_COLS,
                                  NULL) == ROASTTY_INVALID_VALUE);

  roastty_render_state_colors_s colors = {0};
  colors.size = sizeof(colors);
  colors.cursor.r = 9;
  assert(roastty_render_state_colors_get(NULL, &colors) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_colors_get(state, NULL) == ROASTTY_INVALID_VALUE);
  colors.size = sizeof(size_t) - 1;
  assert(roastty_render_state_colors_get(state, &colors) == ROASTTY_INVALID_VALUE);
  colors.size = sizeof(colors);
  assert(roastty_render_state_colors_get(state, &colors) == ROASTTY_SUCCESS);
  assert(colors.size == sizeof(colors));
  assert_rgb_eq(colors.background, 0, 0, 0);
  assert_rgb_eq(colors.foreground, 255, 255, 255);
  assert(!colors.cursor_has_value);
  assert(colors.cursor.r == 9);

  roastty_render_state_colors_s partial = {0};
  partial.size = offsetof(roastty_render_state_colors_s, cursor_has_value) +
                 sizeof(bool);
  assert(roastty_render_state_colors_get(state, &partial) == ROASTTY_SUCCESS);
  assert(partial.size == offsetof(roastty_render_state_colors_s, cursor_has_value) +
                             sizeof(bool));
  assert_rgb_eq(partial.background, 0, 0, 0);
  assert_rgb_eq(partial.foreground, 255, 255, 255);
  assert(!partial.cursor_has_value);

  roastty_render_state_dirty_e partial_dirty = ROASTTY_RENDER_STATE_DIRTY_PARTIAL;
  assert(roastty_render_state_set(NULL,
                                  ROASTTY_RENDER_STATE_OPTION_DIRTY,
                                  &partial_dirty) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_set(state,
                                  ROASTTY_RENDER_STATE_OPTION_DIRTY,
                                  NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_set(state, 99, &partial_dirty) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_set(state,
                                  ROASTTY_RENDER_STATE_OPTION_DIRTY,
                                  &partial_dirty) == ROASTTY_SUCCESS);
  dirty = ROASTTY_RENDER_STATE_DIRTY_FALSE;
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_DIRTY, &dirty) ==
         ROASTTY_SUCCESS);
  assert(dirty == ROASTTY_RENDER_STATE_DIRTY_PARTIAL);
  int invalid_dirty = 99;
  assert(roastty_render_state_set(state,
                                  ROASTTY_RENDER_STATE_OPTION_DIRTY,
                                  &invalid_dirty) == ROASTTY_INVALID_VALUE);
  dirty = ROASTTY_RENDER_STATE_DIRTY_FALSE;
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_DIRTY, &dirty) ==
         ROASTTY_SUCCESS);
  assert(dirty == ROASTTY_RENDER_STATE_DIRTY_PARTIAL);

  roastty_render_state_data_e keys[] = {
      ROASTTY_RENDER_STATE_DATA_COLS,
      ROASTTY_RENDER_STATE_DATA_ROWS,
      ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
  };
  uint16_t cols = 1;
  uint16_t rows = 2;
  uint16_t viewport_x = 3;
  void *values[] = {&cols, &rows, &viewport_x};
  size_t written = 999;
  assert(roastty_render_state_get_multi(state,
                                        2,
                                        keys,
                                        values,
                                        &written) == ROASTTY_SUCCESS);
  assert(written == 2);
  assert(cols == 0);
  assert(rows == 0);
  written = 999;
  assert(roastty_render_state_get_multi(state,
                                        3,
                                        keys,
                                        values,
                                        &written) == ROASTTY_NO_VALUE);
  assert(written == 2);
  assert(roastty_render_state_get_multi(state, 0, keys, values, &written) ==
         ROASTTY_SUCCESS);
  assert(written == 0);
  assert(roastty_render_state_get_multi(state, 1, NULL, values, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get_multi(state, 1, keys, NULL, &written) ==
         ROASTTY_INVALID_VALUE);

  roastty_terminal_t terminal = NULL;
  assert(roastty_terminal_new(80, 24, 10, &terminal) == ROASTTY_SUCCESS);
  assert(terminal != NULL);
  const char *styled = "\x1b[38;2;1;2;3m\x1b[48;2;4;5;6m\x1b[1mA";
  assert(roastty_terminal_vt_write(terminal,
                                   (const uint8_t *)styled,
                                   strlen(styled)) == ROASTTY_SUCCESS);
  assert(roastty_render_state_update(NULL, terminal) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_update(state, NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_update(state, terminal) == ROASTTY_SUCCESS);
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_COLS, &cols) ==
         ROASTTY_SUCCESS);
  assert(cols == 80);
  assert(roastty_render_state_get(state, ROASTTY_RENDER_STATE_DATA_ROWS, &rows) ==
         ROASTTY_SUCCESS);
  assert(rows == 24);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_HAS_VALUE,
                                  &flag) == ROASTTY_SUCCESS);
  assert(flag);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_X,
                                  &viewport_x) == ROASTTY_SUCCESS);
  assert(viewport_x == 1);
  uint16_t viewport_y = 7;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_Y,
                                  &viewport_y) == ROASTTY_SUCCESS);
  assert(viewport_y == 0);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VIEWPORT_WIDE_TAIL,
                                  &flag) == ROASTTY_SUCCESS);
  assert(!flag);
  int visual = -1;
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VISUAL_STYLE,
                                  &visual) == ROASTTY_SUCCESS);
  assert(visual == ROASTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_VISIBLE,
                                  &flag) == ROASTTY_SUCCESS);
  assert(flag);
  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_CURSOR_BLINKING,
                                  &flag) == ROASTTY_SUCCESS);
  assert(!flag);
  assert(roastty_render_state_get(
             NULL,
             ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR,
             &render_placements) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(
             state,
             ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR,
             NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_render_state_get(
             state,
             ROASTTY_RENDER_STATE_DATA_KITTY_RENDER_PLACEMENT_ITERATOR,
             &render_placements) == ROASTTY_SUCCESS);
  assert(!roastty_kitty_graphics_render_placement_next(render_placements));

  assert(roastty_render_state_get(state,
                                  ROASTTY_RENDER_STATE_DATA_ROW_ITERATOR,
                                  &iterator) == ROASTTY_SUCCESS);
  size_t row_count = 0;
  while (roastty_render_state_row_iterator_next(iterator)) {
    bool row_dirty = false;
    roastty_row_t raw = 0;
    assert(roastty_render_state_row_get(iterator,
                                        ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                                        &row_dirty) == ROASTTY_SUCCESS);
    assert(roastty_render_state_row_get(iterator,
                                        ROASTTY_RENDER_STATE_ROW_DATA_RAW,
                                        &raw) == ROASTTY_SUCCESS);
    bool raw_dirty = false;
    assert(roastty_row_get(raw, ROASTTY_ROW_DATA_DIRTY, &raw_dirty) ==
           ROASTTY_SUCCESS);
    assert(row_dirty == raw_dirty);
    roastty_render_state_row_cells_t null_cells = NULL;
    assert(roastty_render_state_row_get(iterator,
                                        ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                                        &null_cells) == ROASTTY_INVALID_VALUE);
    assert(roastty_render_state_row_get(iterator,
                                        ROASTTY_RENDER_STATE_ROW_DATA_CELLS,
                                        &cells) == ROASTTY_SUCCESS);
    assert(roastty_render_state_row_cells_next(cells));
    roastty_cell_t cell = 0;
    assert(roastty_render_state_row_cells_get(cells,
                                              ROASTTY_RENDER_STATE_ROW_CELLS_DATA_RAW,
                                              &cell) == ROASTTY_SUCCESS);
    bool cell_selected = true;
    assert(roastty_render_state_row_cells_get(cells,
                                              ROASTTY_RENDER_STATE_ROW_CELLS_DATA_SELECTED,
                                              &cell_selected) == ROASTTY_SUCCESS);
    assert(!cell_selected);
    bool has_styling = true;
    assert(roastty_render_state_row_cells_get(
               cells,
               ROASTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING,
               &has_styling) == ROASTTY_SUCCESS);
    assert(has_styling == (row_count == 0));

    roastty_style_s style = {0};
    style.size = sizeof(style);
    assert(roastty_render_state_row_cells_get(cells,
                                              ROASTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE,
                                              &style) == ROASTTY_SUCCESS);
    assert(style.size == sizeof(style));
    if (row_count == 0) {
      assert(style.fg_color.tag == ROASTTY_STYLE_COLOR_RGB);
      assert(style.bg_color.tag == ROASTTY_STYLE_COLOR_RGB);
      assert(style.bold);
      roastty_rgb_s fg = {0};
      assert(roastty_render_state_row_cells_get(
                 cells,
                 ROASTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR,
                 &fg) == ROASTTY_SUCCESS);
      assert_rgb_eq(fg, 1, 2, 3);
      roastty_rgb_s bg = {0};
      assert(roastty_render_state_row_cells_get(
                 cells,
                 ROASTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR,
                 &bg) == ROASTTY_SUCCESS);
      assert_rgb_eq(bg, 4, 5, 6);

      uint32_t graphemes_len = 0;
      assert(roastty_render_state_row_cells_get(
                 cells,
                 ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
                 &graphemes_len) == ROASTTY_SUCCESS);
      assert(graphemes_len == 1);
      uint8_t utf8_bytes[1] = {0};
      roastty_buffer_s utf8 = {
          .ptr = utf8_bytes,
          .cap = sizeof(utf8_bytes),
          .len = 0,
      };
      assert(roastty_render_state_row_cells_get(
                 cells,
                 ROASTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8,
                 &utf8) == ROASTTY_SUCCESS);
      assert(utf8.len == 1);
      assert(utf8_bytes[0] == 'A');
    } else {
      assert(style.fg_color.tag == ROASTTY_STYLE_COLOR_NONE);
      assert(style.bg_color.tag == ROASTTY_STYLE_COLOR_NONE);
    }
    row_count++;
  }
  assert(row_count == 24);
  size_t row_written = 999;
  row_written = 999;
  assert(roastty_render_state_row_cells_get_multi(cells,
                                                  0,
                                                  NULL,
                                                  NULL,
                                                  &row_written) == ROASTTY_SUCCESS);
  assert(row_written == 0);
  row_written = 999;
  assert(roastty_render_state_row_get_multi(iterator,
                                            0,
                                            NULL,
                                            NULL,
                                            &row_written) == ROASTTY_SUCCESS);
  assert(row_written == 0);
  bool row_dirty = false;
  assert(roastty_render_state_row_get(iterator,
                                      ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                                      &row_dirty) == ROASTTY_SUCCESS);
  row_dirty = false;
  assert(roastty_render_state_row_set(iterator,
                                      ROASTTY_RENDER_STATE_ROW_OPTION_DIRTY,
                                      &row_dirty) == ROASTTY_SUCCESS);
  row_dirty = true;
  assert(roastty_render_state_row_get(iterator,
                                      ROASTTY_RENDER_STATE_ROW_DATA_DIRTY,
                                      &row_dirty) == ROASTTY_SUCCESS);
  assert(!row_dirty);

  roastty_terminal_free(terminal);
  roastty_render_state_row_cells_free(cells);
  roastty_render_state_row_cells_free(NULL);
  roastty_render_state_row_iterator_free(iterator);
  roastty_render_state_row_iterator_free(NULL);
  roastty_kitty_graphics_render_placement_iterator_free(render_placements);
  roastty_kitty_graphics_render_placement_iterator_free(NULL);
  roastty_render_state_free(state);
  roastty_render_state_free(NULL);
}

static void terminal_write(roastty_terminal_t terminal, const char *bytes) {
  assert(roastty_terminal_vt_write(terminal,
                                   (const uint8_t *)bytes,
                                   strlen(bytes)) == ROASTTY_SUCCESS);
}

static roastty_grid_ref_s terminal_grid_ref_at(roastty_terminal_t terminal,
                                               uint16_t x,
                                               uint32_t y) {
  roastty_grid_ref_s ref = {0};
  roastty_point_s point = {
      .tag = ROASTTY_POINT_ACTIVE,
      .value = {.active = {.x = x, .y = y}},
  };
  assert(roastty_terminal_grid_ref(terminal, point, &ref) == ROASTTY_SUCCESS);
  return ref;
}

static roastty_tracked_grid_ref_t
terminal_tracked_grid_ref_at(roastty_terminal_t terminal, uint16_t x, uint32_t y) {
  roastty_tracked_grid_ref_t ref = NULL;
  roastty_point_s point = {
      .tag = ROASTTY_POINT_ACTIVE,
      .value = {.active = {.x = x, .y = y}},
  };
  assert(roastty_terminal_grid_ref_track(terminal, point, &ref) ==
         ROASTTY_SUCCESS);
  assert(ref != NULL);
  return ref;
}

static roastty_cell_t packed_cell(roastty_cell_content_tag_e tag,
                                  uint64_t content) {
  return ((uint64_t)tag) | (content << 2);
}

static roastty_cell_t packed_cell_with_flags(roastty_cell_t cell,
                                             uint16_t style_id,
                                             roastty_cell_wide_e wide,
                                             bool protected,
                                             bool hyperlink,
                                             roastty_cell_semantic_content_e semantic) {
  cell |= ((uint64_t)style_id) << 26;
  cell |= ((uint64_t)wide) << 42;
  if (protected) {
    cell |= ((uint64_t)1) << 44;
  }
  if (hyperlink) {
    cell |= ((uint64_t)1) << 45;
  }
  cell |= ((uint64_t)semantic) << 46;
  return cell;
}

static roastty_row_t packed_row(bool wrap,
                                bool wrap_continuation,
                                bool grapheme,
                                bool styled,
                                bool hyperlink,
                                roastty_row_semantic_prompt_e semantic_prompt,
                                bool kitty_virtual_placeholder,
                                bool dirty) {
  roastty_row_t row = 0;
  if (wrap) {
    row |= ((uint64_t)1) << 32;
  }
  if (wrap_continuation) {
    row |= ((uint64_t)1) << 33;
  }
  if (grapheme) {
    row |= ((uint64_t)1) << 34;
  }
  if (styled) {
    row |= ((uint64_t)1) << 35;
  }
  if (hyperlink) {
    row |= ((uint64_t)1) << 36;
  }
  row |= ((uint64_t)semantic_prompt) << 37;
  if (kitty_virtual_placeholder) {
    row |= ((uint64_t)1) << 39;
  }
  if (dirty) {
    row |= ((uint64_t)1) << 40;
  }
  return row;
}

static roastty_key_mods_s empty_key_mods(void) {
  roastty_key_mods_s mods = {
      .shift = false,
      .ctrl = false,
      .alt = false,
      .super = false,
      .caps_lock = false,
      .num_lock = false,
      .shift_side = ROASTTY_KEY_SIDE_LEFT,
      .ctrl_side = ROASTTY_KEY_SIDE_LEFT,
      .alt_side = ROASTTY_KEY_SIDE_LEFT,
      .super_side = ROASTTY_KEY_SIDE_LEFT,
  };
  return mods;
}

static void set_key_encoder_option(roastty_key_encoder_t encoder,
                                   roastty_key_encoder_option_e option,
                                   const void *value) {
  assert(roastty_key_encoder_setopt(encoder, option, value) == ROASTTY_SUCCESS);
}

static void assert_key_event_and_encoder_abi(void) {
  roastty_key_event_free(NULL);
  roastty_key_encoder_free(NULL);

  assert(ROASTTY_KEY_UNIDENTIFIED == 0);
  assert(ROASTTY_KEY_KEY_A == 20);
  assert(ROASTTY_KEY_ALT_LEFT == 51);
  assert(ROASTTY_KEY_ARROW_UP == 78);
  assert(ROASTTY_KEY_NUMPAD0 == 80);
  assert(ROASTTY_KEY_F1 == 121);
  assert(ROASTTY_KEY_BROWSER_BACK == 151);
  assert(ROASTTY_KEY_PASTE == 175);

  roastty_key_event_t event = NULL;
  assert(roastty_key_event_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_key_event_new(&event) == ROASTTY_SUCCESS);
  assert(event != NULL);

  assert(roastty_key_event_set_action(event, ROASTTY_KEY_ACTION_REPEAT) ==
         ROASTTY_SUCCESS);
  assert(roastty_key_event_get_action(event) == ROASTTY_KEY_ACTION_REPEAT);
  assert(roastty_key_event_set_action(event, 9999) == ROASTTY_INVALID_VALUE);
  assert(roastty_key_event_get_action(event) == ROASTTY_KEY_ACTION_REPEAT);

  assert(roastty_key_event_set_key(event, ROASTTY_KEY_ARROW_UP) ==
         ROASTTY_SUCCESS);
  assert(roastty_key_event_get_key(event) == ROASTTY_KEY_ARROW_UP);
  assert(roastty_key_event_set_key(event, 176) == ROASTTY_INVALID_VALUE);
  assert(roastty_key_event_get_key(event) == ROASTTY_KEY_ARROW_UP);

  roastty_key_mods_s mods = empty_key_mods();
  mods.shift = true;
  mods.ctrl = true;
  mods.shift_side = ROASTTY_KEY_SIDE_RIGHT;
  mods.ctrl_side = ROASTTY_KEY_SIDE_RIGHT;
  assert(roastty_key_event_set_mods(event, mods) == ROASTTY_SUCCESS);
  roastty_key_mods_s got_mods = roastty_key_event_get_mods(event);
  assert(got_mods.shift);
  assert(got_mods.ctrl);
  assert(got_mods.shift_side == ROASTTY_KEY_SIDE_RIGHT);
  assert(got_mods.ctrl_side == ROASTTY_KEY_SIDE_RIGHT);
  mods.shift_side = 2;
  assert(roastty_key_event_set_mods(event, mods) == ROASTTY_INVALID_VALUE);

  roastty_key_mods_s consumed = empty_key_mods();
  consumed.alt = true;
  consumed.alt_side = ROASTTY_KEY_SIDE_RIGHT;
  assert(roastty_key_event_set_consumed_mods(event, consumed) ==
         ROASTTY_SUCCESS);
  roastty_key_mods_s got_consumed = roastty_key_event_get_consumed_mods(event);
  assert(got_consumed.alt);
  assert(got_consumed.alt_side == ROASTTY_KEY_SIDE_RIGHT);

  assert(roastty_key_event_set_composing(event, true) == ROASTTY_SUCCESS);
  assert(roastty_key_event_get_composing(event));
  assert(roastty_key_event_set_unshifted_codepoint(event, 'A') ==
         ROASTTY_SUCCESS);
  assert(roastty_key_event_get_unshifted_codepoint(event) == 'A');

  uint8_t text[] = {'o', 'k'};
  assert(roastty_key_event_set_utf8(event, text, sizeof(text)) ==
         ROASTTY_SUCCESS);
  text[0] = 'n';
  size_t text_len = 0;
  const uint8_t *text_ptr = roastty_key_event_get_utf8(event, &text_len);
  assert(text_ptr != NULL);
  assert(text_len == 2);
  assert(memcmp(text_ptr, "ok", text_len) == 0);
  uint8_t bad_utf8[] = {0xff};
  assert(roastty_key_event_set_utf8(event, bad_utf8, sizeof(bad_utf8)) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_key_event_set_utf8(event, NULL, 1) == ROASTTY_INVALID_VALUE);
  assert(roastty_key_event_set_utf8(event, NULL, 0) == ROASTTY_SUCCESS);
  assert(roastty_key_event_get_utf8(event, &text_len) == NULL);
  assert(text_len == 0);
  assert(roastty_key_event_get_utf8(NULL, &text_len) == NULL);
  assert(text_len == 0);

  roastty_key_encoder_t encoder = NULL;
  assert(roastty_key_encoder_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_key_encoder_new(&encoder) == ROASTTY_SUCCESS);
  assert(encoder != NULL);
  assert(roastty_key_encoder_setopt(encoder,
                                    ROASTTY_KEY_ENCODER_OPTION_ALT_ESC_PREFIX,
                                    NULL) == ROASTTY_INVALID_VALUE);
  int invalid_option = 9999;
  assert(roastty_key_encoder_setopt(encoder, invalid_option, &invalid_option) ==
         ROASTTY_INVALID_VALUE);
  uint8_t bad_flags = 0x20;
  assert(roastty_key_encoder_setopt(encoder,
                                    ROASTTY_KEY_ENCODER_OPTION_KITTY_FLAGS,
                                    &bad_flags) == ROASTTY_INVALID_VALUE);
  int bad_option_as_alt = 4;
  assert(roastty_key_encoder_setopt(
             encoder,
             ROASTTY_KEY_ENCODER_OPTION_MACOS_OPTION_AS_ALT,
             &bad_option_as_alt) == ROASTTY_INVALID_VALUE);

  bool enabled = true;
  bool disabled = false;
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_CURSOR_KEY_APPLICATION,
                         &enabled);
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_KEYPAD_KEY_APPLICATION,
                         &enabled);
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_IGNORE_KEYPAD_WITH_NUMLOCK,
                         &enabled);
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_ALT_ESC_PREFIX,
                         &enabled);
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_MODIFY_OTHER_KEYS_STATE_2,
                         &disabled);
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_BACKARROW_KEY_MODE,
                         &enabled);
  int option_as_alt = ROASTTY_OPTION_AS_ALT_RIGHT;
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_MACOS_OPTION_AS_ALT,
                         &option_as_alt);

  assert(roastty_key_event_set_action(event, ROASTTY_KEY_ACTION_PRESS) ==
         ROASTTY_SUCCESS);
  assert(roastty_key_event_set_composing(event, false) == ROASTTY_SUCCESS);
  assert(roastty_key_event_set_key(event, ROASTTY_KEY_KEY_C) == ROASTTY_SUCCESS);
  mods = empty_key_mods();
  mods.ctrl = true;
  assert(roastty_key_event_set_mods(event, mods) == ROASTTY_SUCCESS);
  assert(roastty_key_event_set_consumed_mods(event, empty_key_mods()) ==
         ROASTTY_SUCCESS);
  assert(roastty_key_event_set_utf8(event, NULL, 0) == ROASTTY_SUCCESS);

  size_t written = 0;
  assert(roastty_key_encoder_encode(NULL, event, NULL, 0, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_key_encoder_encode(encoder, NULL, NULL, 0, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_key_encoder_encode(encoder, event, NULL, 1, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_key_encoder_encode(encoder, event, NULL, 0, NULL) ==
         ROASTTY_INVALID_VALUE);

  assert(roastty_key_encoder_encode(encoder, event, NULL, 0, &written) ==
         ROASTTY_OUT_OF_SPACE);
  assert(written == 1);
  uint8_t buf[64] = {0};
  assert(roastty_key_encoder_encode(encoder, event, buf, sizeof(buf), &written) ==
         ROASTTY_SUCCESS);
  assert(written == 1);
  assert(buf[0] == 0x03);

  uint8_t kitty_flags = 0x1f;
  set_key_encoder_option(encoder,
                         ROASTTY_KEY_ENCODER_OPTION_KITTY_FLAGS,
                         &kitty_flags);
  assert(roastty_key_event_set_action(event, ROASTTY_KEY_ACTION_RELEASE) ==
         ROASTTY_SUCCESS);
  assert(roastty_key_event_set_key(event, ROASTTY_KEY_CONTROL_LEFT) ==
         ROASTTY_SUCCESS);
  mods = empty_key_mods();
  mods.ctrl = true;
  assert(roastty_key_event_set_mods(event, mods) == ROASTTY_SUCCESS);
  assert(roastty_key_encoder_encode(encoder, event, NULL, 0, &written) ==
         ROASTTY_OUT_OF_SPACE);
  assert(written == strlen("\x1b[57442;5:3u"));
  assert(roastty_key_encoder_encode(encoder, event, buf, sizeof(buf), &written) ==
         ROASTTY_SUCCESS);
  assert(memcmp(buf, "\x1b[57442;5:3u", written) == 0);

  roastty_key_encoder_free(encoder);
  roastty_key_event_free(event);
}

static void feed_osc(roastty_osc_parser_t parser, const char *bytes) {
  for (size_t i = 0; bytes[i] != '\0'; i++) {
    roastty_osc_next(parser, (uint8_t)bytes[i]);
  }
}

static void assert_osc_parser_abi(void) {
  roastty_osc_free(NULL);
  roastty_osc_reset(NULL);
  roastty_osc_next(NULL, 'x');
  assert(roastty_osc_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_osc_end(NULL, 0) == NULL);
  assert(roastty_osc_command_type(NULL) == ROASTTY_OSC_COMMAND_INVALID);
  assert(!roastty_osc_command_data(NULL,
                                   ROASTTY_OSC_COMMAND_DATA_CHANGE_WINDOW_TITLE_STR,
                                   NULL));

  assert(ROASTTY_OSC_COMMAND_INVALID == 0);
  assert(ROASTTY_OSC_COMMAND_CHANGE_WINDOW_TITLE == 1);
  assert(ROASTTY_OSC_COMMAND_CHANGE_WINDOW_ICON == 2);
  assert(ROASTTY_OSC_COMMAND_SEMANTIC_PROMPT == 3);
  assert(ROASTTY_OSC_COMMAND_CONTEXT_SIGNAL == 24);

  roastty_osc_parser_t parser = NULL;
  assert(roastty_osc_new(&parser) == ROASTTY_SUCCESS);
  assert(parser != NULL);

  feed_osc(parser, "0;from-c");
  roastty_osc_command_t command = roastty_osc_end(parser, 0);
  assert(command != NULL);
  assert(roastty_osc_command_type(command) ==
         ROASTTY_OSC_COMMAND_CHANGE_WINDOW_TITLE);

  const char *title = NULL;
  assert(roastty_osc_command_data(command,
                                  ROASTTY_OSC_COMMAND_DATA_CHANGE_WINDOW_TITLE_STR,
                                  &title));
  assert(title != NULL);
  assert(strcmp(title, "from-c") == 0);

  const char *unchanged = (const char *)0x1;
  assert(!roastty_osc_command_data(command,
                                   ROASTTY_OSC_COMMAND_DATA_INVALID,
                                   &unchanged));
  assert(unchanged == (const char *)0x1);
  assert(!roastty_osc_command_data(command,
                                   ROASTTY_OSC_COMMAND_DATA_CHANGE_WINDOW_TITLE_STR,
                                   NULL));

  feed_osc(parser, "0;second");
  command = roastty_osc_end(parser, 0);
  assert(command != NULL);
  title = NULL;
  assert(roastty_osc_command_data(command,
                                  ROASTTY_OSC_COMMAND_DATA_CHANGE_WINDOW_TITLE_STR,
                                  &title));
  assert(title != NULL);
  assert(strcmp(title, "second") == 0);

  feed_osc(parser, "7;file://host/path");
  command = roastty_osc_end(parser, 0);
  assert(command != NULL);
  assert(roastty_osc_command_type(command) == ROASTTY_OSC_COMMAND_REPORT_PWD);
  title = (const char *)0x1;
  assert(!roastty_osc_command_data(command,
                                   ROASTTY_OSC_COMMAND_DATA_CHANGE_WINDOW_TITLE_STR,
                                   &title));
  assert(title == (const char *)0x1);

  feed_osc(parser, "4;2;?");
  command = roastty_osc_end(parser, 0x07);
  assert(command != NULL);
  assert(roastty_osc_command_type(command) == ROASTTY_OSC_COMMAND_COLOR_OPERATION);

  feed_osc(parser, "0;bad");
  assert(roastty_osc_end(parser, 9999) == NULL);

  roastty_osc_reset(parser);
  roastty_osc_free(parser);
}

static void assert_mouse_event_abi(void) {
  roastty_mouse_event_free(NULL);

  roastty_mouse_event_t event = NULL;
  assert(roastty_mouse_event_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_event_new(&event) == ROASTTY_SUCCESS);
  assert(event != NULL);

  assert(roastty_mouse_event_set_action(event, ROASTTY_MOUSE_ACTION_MOTION) ==
         ROASTTY_SUCCESS);
  assert(roastty_mouse_event_get_action(event) == ROASTTY_MOUSE_ACTION_MOTION);
  assert(roastty_mouse_event_set_action(event, ROASTTY_MOUSE_ACTION_RELEASE) ==
         ROASTTY_SUCCESS);
  assert(roastty_mouse_event_get_action(event) == ROASTTY_MOUSE_ACTION_RELEASE);
  assert(roastty_mouse_event_set_action(event, ROASTTY_MOUSE_ACTION_PRESS) ==
         ROASTTY_SUCCESS);
  assert(roastty_mouse_event_set_action(event, 9999) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_event_get_action(event) == ROASTTY_MOUSE_ACTION_PRESS);

  int button = -1;
  assert(!roastty_mouse_event_get_button(event, &button));
  assert(roastty_mouse_event_set_button(event, ROASTTY_MOUSE_BUTTON_LEFT) ==
         ROASTTY_SUCCESS);
  assert(roastty_mouse_event_get_button(event, &button));
  assert(button == ROASTTY_MOUSE_BUTTON_LEFT);
  assert(roastty_mouse_event_get_button(event, NULL));
  assert(roastty_mouse_event_set_button(event, ROASTTY_MOUSE_BUTTON_EIGHT) ==
         ROASTTY_SUCCESS);
  assert(roastty_mouse_event_get_button(event, &button));
  assert(button == ROASTTY_MOUSE_BUTTON_EIGHT);
  assert(roastty_mouse_event_set_button(event, 9999) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_event_get_button(event, &button));
  assert(button == ROASTTY_MOUSE_BUTTON_EIGHT);
  roastty_mouse_event_clear_button(event);
  assert(!roastty_mouse_event_get_button(event, &button));

  roastty_mouse_mods_s mods = {
      .shift = true,
      .alt = false,
      .ctrl = true,
  };
  roastty_mouse_event_set_mods(event, mods);
  roastty_mouse_mods_s got_mods = roastty_mouse_event_get_mods(event);
  assert(got_mods.shift);
  assert(!got_mods.alt);
  assert(got_mods.ctrl);

  roastty_mouse_position_s pos = {
      .x = 12.5f,
      .y = -4.0f,
  };
  roastty_mouse_event_set_position(event, pos);
  roastty_mouse_position_s got_pos = roastty_mouse_event_get_position(event);
  assert(got_pos.x == 12.5f);
  assert(got_pos.y == -4.0f);

  roastty_mouse_event_free(event);
}

static void set_mouse_encoder_option(roastty_mouse_encoder_t encoder,
                                     roastty_mouse_encoder_option_e option,
                                     const void *value) {
  assert(roastty_mouse_encoder_setopt(encoder, option, value) == ROASTTY_SUCCESS);
}

static void assert_mouse_encoder_abi(void) {
  roastty_mouse_encoder_free(NULL);
  roastty_mouse_encoder_reset(NULL);

  roastty_mouse_encoder_t encoder = NULL;
  assert(roastty_mouse_encoder_new(NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_new(&encoder) == ROASTTY_SUCCESS);
  assert(encoder != NULL);

  assert(roastty_mouse_encoder_setopt(NULL,
                                      ROASTTY_MOUSE_ENCODER_OPTION_EVENT,
                                      NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_setopt(encoder,
                                      ROASTTY_MOUSE_ENCODER_OPTION_EVENT,
                                      NULL) == ROASTTY_INVALID_VALUE);

  int event_mode = ROASTTY_MOUSE_TRACKING_ANY;
  set_mouse_encoder_option(encoder, ROASTTY_MOUSE_ENCODER_OPTION_EVENT, &event_mode);
  int format = ROASTTY_MOUSE_FORMAT_SGR;
  set_mouse_encoder_option(encoder, ROASTTY_MOUSE_ENCODER_OPTION_FORMAT, &format);
  roastty_mouse_encoder_size_s size = {
      .size = sizeof(roastty_mouse_encoder_size_s),
      .screen_width = 1000,
      .screen_height = 1000,
      .cell_width = 1,
      .cell_height = 1,
      .padding_top = 0,
      .padding_bottom = 0,
      .padding_right = 0,
      .padding_left = 0,
  };
  set_mouse_encoder_option(encoder, ROASTTY_MOUSE_ENCODER_OPTION_SIZE, &size);
  bool any_button_pressed = true;
  set_mouse_encoder_option(encoder,
                           ROASTTY_MOUSE_ENCODER_OPTION_ANY_BUTTON_PRESSED,
                           &any_button_pressed);

  int invalid_enum = 9999;
  assert(roastty_mouse_encoder_setopt(encoder,
                                      ROASTTY_MOUSE_ENCODER_OPTION_EVENT,
                                      &invalid_enum) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_setopt(encoder,
                                      ROASTTY_MOUSE_ENCODER_OPTION_FORMAT,
                                      &invalid_enum) == ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_setopt(encoder, 9999, &invalid_enum) ==
         ROASTTY_INVALID_VALUE);
  roastty_mouse_encoder_size_s bad_size = size;
  bad_size.cell_width = 0;
  assert(roastty_mouse_encoder_setopt(encoder,
                                      ROASTTY_MOUSE_ENCODER_OPTION_SIZE,
                                      &bad_size) == ROASTTY_INVALID_VALUE);
  bad_size = size;
  bad_size.size = sizeof(size_t);
  assert(roastty_mouse_encoder_setopt(encoder,
                                      ROASTTY_MOUSE_ENCODER_OPTION_SIZE,
                                      &bad_size) == ROASTTY_INVALID_VALUE);
  struct tiny_size {
    size_t size;
  };
  struct tiny_size tiny_size = {
      .size = sizeof(tiny_size),
  };
  assert(roastty_mouse_encoder_setopt(encoder,
                                      ROASTTY_MOUSE_ENCODER_OPTION_SIZE,
                                      &tiny_size) == ROASTTY_INVALID_VALUE);

  roastty_mouse_event_t event = NULL;
  assert(roastty_mouse_event_new(&event) == ROASTTY_SUCCESS);
  assert(roastty_mouse_event_set_action(event, ROASTTY_MOUSE_ACTION_PRESS) ==
         ROASTTY_SUCCESS);
  assert(roastty_mouse_event_set_button(event, ROASTTY_MOUSE_BUTTON_LEFT) ==
         ROASTTY_SUCCESS);
  roastty_mouse_position_s pos = {
      .x = 0.0f,
      .y = 0.0f,
  };
  roastty_mouse_event_set_position(event, pos);

  size_t written = 0;
  assert(roastty_mouse_encoder_encode(NULL, event, NULL, 0, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_encode(encoder, NULL, NULL, 0, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_encode(encoder, event, NULL, 1, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_mouse_encoder_encode(encoder, event, NULL, 0, NULL) ==
         ROASTTY_INVALID_VALUE);

  assert(roastty_mouse_encoder_encode(encoder, event, NULL, 0, &written) ==
         ROASTTY_OUT_OF_SPACE);
  assert(written == strlen("\x1b[<0;1;1M"));

  uint8_t tiny[2] = {0};
  assert(roastty_mouse_encoder_encode(encoder, event, tiny, sizeof(tiny), &written) ==
         ROASTTY_OUT_OF_SPACE);
  assert(written == strlen("\x1b[<0;1;1M"));

  uint8_t buf[32] = {0};
  assert(roastty_mouse_encoder_encode(encoder, event, buf, sizeof(buf), &written) ==
         ROASTTY_SUCCESS);
  assert(written == strlen("\x1b[<0;1;1M"));
  assert(memcmp(buf, "\x1b[<0;1;1M", written) == 0);

  bool track_last_cell = true;
  set_mouse_encoder_option(encoder,
                           ROASTTY_MOUSE_ENCODER_OPTION_TRACK_LAST_CELL,
                           &track_last_cell);
  assert(roastty_mouse_event_set_action(event, ROASTTY_MOUSE_ACTION_MOTION) ==
         ROASTTY_SUCCESS);
  pos.x = 5.0f;
  pos.y = 6.0f;
  roastty_mouse_event_set_position(event, pos);

  assert(roastty_mouse_encoder_encode(encoder, event, NULL, 0, &written) ==
         ROASTTY_OUT_OF_SPACE);
  assert(written > 0);
  assert(roastty_mouse_encoder_encode(encoder, event, buf, sizeof(buf), &written) ==
         ROASTTY_SUCCESS);
  assert(written > 0);
  assert(roastty_mouse_encoder_encode(encoder, event, buf, sizeof(buf), &written) ==
         ROASTTY_SUCCESS);
  assert(written == 0);
  roastty_mouse_encoder_reset(encoder);
  assert(roastty_mouse_encoder_encode(encoder, event, buf, sizeof(buf), &written) ==
         ROASTTY_SUCCESS);
  assert(written > 0);

  roastty_mouse_event_free(event);
  roastty_mouse_encoder_free(encoder);
}

static void assert_support_abi(void) {
  assert(ROASTTY_OPTIMIZE_DEBUG == 0);
  assert(ROASTTY_OPTIMIZE_RELEASE_SAFE == 1);
  assert(ROASTTY_OPTIMIZE_RELEASE_SMALL == 2);
  assert(ROASTTY_OPTIMIZE_RELEASE_FAST == 3);
  assert(ROASTTY_BUILD_INFO_INVALID == 0);
  assert(ROASTTY_BUILD_INFO_SIMD == 1);
  assert(ROASTTY_BUILD_INFO_KITTY_GRAPHICS == 2);
  assert(ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE == 3);
  assert(ROASTTY_BUILD_INFO_OPTIMIZE == 4);
  assert(ROASTTY_BUILD_INFO_VERSION_STRING == 5);
  assert(ROASTTY_BUILD_INFO_VERSION_MAJOR == 6);
  assert(ROASTTY_BUILD_INFO_VERSION_MINOR == 7);
  assert(ROASTTY_BUILD_INFO_VERSION_PATCH == 8);
  assert(ROASTTY_BUILD_INFO_VERSION_PRE == 9);
  assert(ROASTTY_BUILD_INFO_VERSION_BUILD == 10);
  assert(ROASTTY_MODS_NONE == 0);
  assert(ROASTTY_MODS_SHIFT == (1 << 0));
  assert(ROASTTY_MODS_CTRL == (1 << 1));
  assert(ROASTTY_MODS_ALT == (1 << 2));
  assert(ROASTTY_MODS_SUPER == (1 << 3));
  assert(ROASTTY_MODS_CAPS == (1 << 4));
  assert(ROASTTY_MODS_NUM == (1 << 5));
  assert(ROASTTY_MODS_SHIFT_RIGHT == (1 << 6));
  assert(ROASTTY_MODS_CTRL_RIGHT == (1 << 7));
  assert(ROASTTY_MODS_ALT_RIGHT == (1 << 8));
  assert(ROASTTY_MODS_SUPER_RIGHT == (1 << 9));
  assert(ROASTTY_TARGET_APP == 0);
  assert(ROASTTY_TARGET_SURFACE == 1);
  assert(ROASTTY_ACTION_QUIT == 0);
  assert(ROASTTY_ACTION_NEW_WINDOW == 1);
  assert(ROASTTY_ACTION_NEW_TAB == 2);
  assert(ROASTTY_ACTION_CLOSE_TAB == 3);
  assert(ROASTTY_ACTION_NEW_SPLIT == 4);
  assert(ROASTTY_ACTION_CLOSE_ALL_WINDOWS == 5);
  assert(ROASTTY_ACTION_TOGGLE_MAXIMIZE == 6);
  assert(ROASTTY_ACTION_TOGGLE_FULLSCREEN == 7);
  assert(ROASTTY_ACTION_TOGGLE_TAB_OVERVIEW == 8);
  assert(ROASTTY_ACTION_TOGGLE_WINDOW_DECORATIONS == 9);
  assert(ROASTTY_ACTION_TOGGLE_QUICK_TERMINAL == 10);
  assert(ROASTTY_ACTION_TOGGLE_COMMAND_PALETTE == 11);
  assert(ROASTTY_ACTION_TOGGLE_VISIBILITY == 12);
  assert(ROASTTY_ACTION_TOGGLE_BACKGROUND_OPACITY == 13);
  assert(ROASTTY_ACTION_MOVE_TAB == 14);
  assert(ROASTTY_ACTION_GOTO_TAB == 15);
  assert(ROASTTY_ACTION_GOTO_SPLIT == 16);
  assert(ROASTTY_ACTION_GOTO_WINDOW == 17);
  assert(ROASTTY_ACTION_RESIZE_SPLIT == 18);
  assert(ROASTTY_ACTION_EQUALIZE_SPLITS == 19);
  assert(ROASTTY_ACTION_TOGGLE_SPLIT_ZOOM == 20);
  assert(ROASTTY_ACTION_RESET_WINDOW_SIZE == 23);
  assert(ROASTTY_ACTION_INSPECTOR == 28);
  assert(ROASTTY_ACTION_SHOW_GTK_INSPECTOR == 29);
  assert(ROASTTY_ACTION_OPEN_CONFIG == 40);
  assert(ROASTTY_ACTION_RELOAD_CONFIG == 47);
  assert(ROASTTY_ACTION_CHECK_FOR_UPDATES == 53);
  assert(ROASTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD == 57);
  assert(ROASTTY_ACTION_START_SEARCH == 59);
  assert(ROASTTY_ACTION_END_SEARCH == 60);
  assert(ROASTTY_ACTION_READONLY == 63);
  assert(ROASTTY_ACTION_COPY_TITLE_TO_CLIPBOARD == 64);
  assert(ROASTTY_ACTION_FLOAT_WINDOW == 42);
  assert(ROASTTY_ACTION_SECURE_INPUT == 43);
  assert(ROASTTY_READONLY_ON == 0);
  assert(ROASTTY_READONLY_OFF == 1);
  assert(ROASTTY_ACTION_CLOSE_WINDOW == 49);
  assert(ROASTTY_INSPECTOR_TOGGLE == 0);
  assert(ROASTTY_INSPECTOR_SHOW == 1);
  assert(ROASTTY_INSPECTOR_HIDE == 2);
  assert(ROASTTY_FLOAT_WINDOW_ON == 0);
  assert(ROASTTY_FLOAT_WINDOW_OFF == 1);
  assert(ROASTTY_FLOAT_WINDOW_TOGGLE == 2);
  assert(ROASTTY_SECURE_INPUT_ON == 0);
  assert(ROASTTY_SECURE_INPUT_OFF == 1);
  assert(ROASTTY_SECURE_INPUT_TOGGLE == 2);
  assert(ROASTTY_CLOSE_TAB_THIS == 0);
  assert(ROASTTY_CLOSE_TAB_OTHER == 1);
  assert(ROASTTY_CLOSE_TAB_RIGHT == 2);
  assert(ROASTTY_GOTO_WINDOW_PREVIOUS == 0);
  assert(ROASTTY_GOTO_WINDOW_NEXT == 1);
  assert(ROASTTY_GOTO_TAB_PREVIOUS == -1);
  assert(ROASTTY_GOTO_TAB_NEXT == -2);
  assert(ROASTTY_GOTO_TAB_LAST == -3);
  assert(ROASTTY_FULLSCREEN_NATIVE == 0);
  assert(ROASTTY_FULLSCREEN_MACOS_NON_NATIVE == 1);
  assert(ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU == 2);
  assert(ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH == 3);
  assert(ROASTTY_SPLIT_DIRECTION_RIGHT == 0);
  assert(ROASTTY_SPLIT_DIRECTION_DOWN == 1);
  assert(ROASTTY_SPLIT_DIRECTION_LEFT == 2);
  assert(ROASTTY_SPLIT_DIRECTION_UP == 3);
  assert(ROASTTY_GOTO_SPLIT_PREVIOUS == 0);
  assert(ROASTTY_GOTO_SPLIT_NEXT == 1);
  assert(ROASTTY_GOTO_SPLIT_UP == 2);
  assert(ROASTTY_GOTO_SPLIT_LEFT == 3);
  assert(ROASTTY_GOTO_SPLIT_DOWN == 4);
  assert(ROASTTY_GOTO_SPLIT_RIGHT == 5);
  assert(ROASTTY_RESIZE_SPLIT_UP == 0);
  assert(ROASTTY_RESIZE_SPLIT_DOWN == 1);
  assert(ROASTTY_RESIZE_SPLIT_LEFT == 2);
  assert(ROASTTY_RESIZE_SPLIT_RIGHT == 3);
  roastty_keybind_flags_t keybind_flags = 0;
  assert(sizeof(keybind_flags) == sizeof(uint8_t));
  roastty_input_scroll_mods_t scroll_mods = 0;
  assert(sizeof(scroll_mods) == sizeof(int));
  assert(ROASTTY_MOUSE_BUTTON_RELEASE == 0);
  assert(ROASTTY_MOUSE_BUTTON_PRESS == 1);
  assert(ROASTTY_SYS_LOG_LEVEL_ERROR == 0);
  assert(ROASTTY_SYS_LOG_LEVEL_WARNING == 1);
  assert(ROASTTY_SYS_LOG_LEVEL_INFO == 2);
  assert(ROASTTY_SYS_LOG_LEVEL_DEBUG == 3);
  assert(ROASTTY_SYS_OPT_USERDATA == 0);
  assert(ROASTTY_SYS_OPT_DECODE_PNG == 1);
  assert(ROASTTY_SYS_OPT_LOG == 2);
  assert(sizeof(roastty_allocator_s) == sizeof(void *) * 2);
  assert(offsetof(roastty_allocator_s, ctx) == 0);
  assert(offsetof(roastty_allocator_s, vtable) == sizeof(void *));
  assert(offsetof(roastty_allocator_vtable_s, alloc) == 0);
  assert(offsetof(roastty_allocator_vtable_s, resize) == sizeof(void *));
  assert(offsetof(roastty_allocator_vtable_s, remap) == sizeof(void *) * 2);
  assert(offsetof(roastty_allocator_vtable_s, free) == sizeof(void *) * 3);
  assert(sizeof(roastty_sys_image_s) == 24);
  assert(offsetof(roastty_sys_image_s, width) == 0);
  assert(offsetof(roastty_sys_image_s, height) == 4);
  assert(offsetof(roastty_sys_image_s, data) == 8);
  assert(offsetof(roastty_sys_image_s, data_len) == 16);
  assert(sizeof(roastty_text_s) == 40);
  assert(offsetof(roastty_text_s, tl_px_x) == 0);
  assert(offsetof(roastty_text_s, tl_px_y) == 8);
  assert(offsetof(roastty_text_s, offset_start) == 16);
  assert(offsetof(roastty_text_s, offset_len) == 20);
  assert(offsetof(roastty_text_s, text) == 24);
  assert(offsetof(roastty_text_s, text_len) == 32);

  bool bool_value = true;
  assert(roastty_build_info(ROASTTY_BUILD_INFO_SIMD, &bool_value) ==
         ROASTTY_SUCCESS);
  assert(!bool_value);
  assert(roastty_build_info(ROASTTY_BUILD_INFO_KITTY_GRAPHICS, &bool_value) ==
         ROASTTY_SUCCESS);
  assert(!bool_value);
  assert(roastty_build_info(ROASTTY_BUILD_INFO_TMUX_CONTROL_MODE, &bool_value) ==
         ROASTTY_SUCCESS);
  assert(!bool_value);
  roastty_optimize_mode_e optimize = (roastty_optimize_mode_e)99;
  assert(roastty_build_info(ROASTTY_BUILD_INFO_OPTIMIZE, &optimize) ==
         ROASTTY_SUCCESS);
  assert(optimize == ROASTTY_OPTIMIZE_DEBUG ||
         optimize == ROASTTY_OPTIMIZE_RELEASE_FAST);
  roastty_string_s version = {0};
  assert(roastty_build_info(ROASTTY_BUILD_INFO_VERSION_STRING, &version) ==
         ROASTTY_SUCCESS);
  assert(version.ptr != NULL);
  assert(version.len > 0);
  assert(!version.sentinel);
  assert(roastty_build_info(ROASTTY_BUILD_INFO_VERSION_BUILD, &version) ==
         ROASTTY_SUCCESS);
  assert(version.ptr != NULL);
  assert(version.len == 0);
  assert(!version.sentinel);
  size_t component = SIZE_MAX;
  assert(roastty_build_info(ROASTTY_BUILD_INFO_VERSION_MAJOR, &component) ==
         ROASTTY_SUCCESS);
  assert(component == 0);
  assert(roastty_build_info(ROASTTY_BUILD_INFO_INVALID, &bool_value) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_build_info((roastty_build_info_e)99, &bool_value) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_build_info(ROASTTY_BUILD_INFO_SIMD, NULL) ==
         ROASTTY_INVALID_VALUE);

  uint8_t *default_buf = roastty_alloc(NULL, 8);
  assert(default_buf != NULL);
  memset(default_buf, 0xab, 8);
  roastty_free(NULL, default_buf, 8);
  assert(roastty_alloc(NULL, 0) == NULL);
  roastty_free(NULL, NULL, 8);

  roastty_allocator_vtable_s vtable = {
      .alloc = support_alloc_cb,
      .resize = support_resize_cb,
      .remap = support_remap_cb,
      .free = support_free_cb,
  };
  roastty_allocator_s allocator = {
      .ctx = (void *)0xfeed,
      .vtable = &vtable,
  };
  support_alloc_count = 0;
  support_free_count = 0;
  uint8_t *custom_buf = roastty_alloc(&allocator, 13);
  assert(custom_buf != NULL);
  assert(support_alloc_count == 1);
  assert(support_last_ctx == (void *)0xfeed);
  assert(support_last_len == 13);
  assert(support_last_alignment == 1);
  roastty_free(&allocator, custom_buf, 13);
  assert(support_free_count == 1);
  assert(roastty_alloc(&allocator, 0) == NULL);
  assert(support_alloc_count == 1);
  roastty_allocator_s malformed = {
      .ctx = NULL,
      .vtable = NULL,
  };
  assert(roastty_alloc(&malformed, 1) == NULL);
  roastty_free(&malformed, (uint8_t *)0x1, 1);
  roastty_allocator_vtable_s no_free = {
      .alloc = support_alloc_cb,
      .resize = support_resize_cb,
      .remap = support_remap_cb,
      .free = NULL,
  };
  malformed = (roastty_allocator_s){
      .ctx = NULL,
      .vtable = &no_free,
  };
  roastty_free(&malformed, (uint8_t *)0x1, 1);

  assert(roastty_sys_set(ROASTTY_SYS_OPT_USERDATA, (const void *)0xbeef) ==
         ROASTTY_SUCCESS);
  assert(roastty_sys_set(ROASTTY_SYS_OPT_LOG, (const void *)support_log_cb) ==
         ROASTTY_SUCCESS);
  assert(roastty_sys_set(ROASTTY_SYS_OPT_DECODE_PNG,
                         (const void *)support_decode_cb) == ROASTTY_SUCCESS);
  support_log_cb((void *)0xbeef,
                 ROASTTY_SYS_LOG_LEVEL_INFO,
                 NULL,
                 0,
                 NULL,
                 0);
  assert(support_log_called);
  assert(support_log_userdata == (void *)0xbeef);
  roastty_sys_image_s image = {0};
  assert(support_decode_cb((void *)0xbeef, NULL, NULL, 0, &image));
  assert(support_decode_called);
  assert(support_decode_userdata == (void *)0xbeef);
  assert(roastty_sys_set(ROASTTY_SYS_OPT_LOG, NULL) == ROASTTY_SUCCESS);
  assert(roastty_sys_set(ROASTTY_SYS_OPT_DECODE_PNG, NULL) == ROASTTY_SUCCESS);
  assert(roastty_sys_set((roastty_sys_option_e)99, NULL) ==
         ROASTTY_INVALID_VALUE);
  roastty_sys_log_stderr(NULL,
                         (roastty_sys_log_level_e)99,
                         (const uint8_t *)"scope",
                         5,
                         (const uint8_t *)"message",
                         7);
}

static void assert_terminal_abi(void) {
  roastty_terminal_free(NULL);

  assert(ROASTTY_SUCCESS == 0);
  assert(ROASTTY_OUT_OF_MEMORY == 1);
  assert(ROASTTY_INVALID_VALUE == 2);
  assert(ROASTTY_OUT_OF_SPACE == 3);
  assert(ROASTTY_NO_VALUE == 4);
  assert(ROASTTY_TERMINAL_DATA_INVALID == 0);
  assert(ROASTTY_TERMINAL_DATA_COLS == 1);
  assert(ROASTTY_TERMINAL_DATA_ROWS == 2);
  assert(ROASTTY_TERMINAL_DATA_CURSOR_X == 3);
  assert(ROASTTY_TERMINAL_DATA_CURSOR_Y == 4);
  assert(ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP == 5);
  assert(ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN == 6);
  assert(ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE == 7);
  assert(ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS == 8);
  assert(ROASTTY_TERMINAL_DATA_SCROLLBAR == 9);
  assert(ROASTTY_TERMINAL_DATA_CURSOR_STYLE == 10);
  assert(ROASTTY_TERMINAL_DATA_MOUSE_TRACKING == 11);
  assert(ROASTTY_TERMINAL_DATA_TITLE == 12);
  assert(ROASTTY_TERMINAL_DATA_PWD == 13);
  assert(ROASTTY_TERMINAL_DATA_TOTAL_ROWS == 14);
  assert(ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS == 15);
  assert(ROASTTY_TERMINAL_DATA_WIDTH_PX == 16);
  assert(ROASTTY_TERMINAL_DATA_HEIGHT_PX == 17);
  assert(ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND == 18);
  assert(ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND == 19);
  assert(ROASTTY_TERMINAL_DATA_COLOR_CURSOR == 20);
  assert(ROASTTY_TERMINAL_DATA_COLOR_PALETTE == 21);
  assert(ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT == 22);
  assert(ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT == 23);
  assert(ROASTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT == 24);
  assert(ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT == 25);
  assert(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT == 26);
  assert(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE == 27);
  assert(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE == 28);
  assert(ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM == 29);
  assert(ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS == 30);
  assert(ROASTTY_TERMINAL_DATA_SELECTION == 31);
  assert(ROASTTY_TERMINAL_DATA_VIEWPORT_ACTIVE == 32);
  assert(ROASTTY_TERMINAL_SCREEN_PRIMARY == 0);
  assert(ROASTTY_TERMINAL_SCREEN_ALTERNATE == 1);
  assert(ROASTTY_TERMINAL_OPTION_USERDATA == 0);
  assert(ROASTTY_TERMINAL_OPTION_WRITE_PTY == 1);
  assert(ROASTTY_TERMINAL_OPTION_BELL == 2);
  assert(ROASTTY_TERMINAL_OPTION_ENQUIRY == 3);
  assert(ROASTTY_TERMINAL_OPTION_XTVERSION == 4);
  assert(ROASTTY_TERMINAL_OPTION_TITLE_CHANGED == 5);
  assert(ROASTTY_TERMINAL_OPTION_SIZE_CB == 6);
  assert(ROASTTY_TERMINAL_OPTION_COLOR_SCHEME == 7);
  assert(ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES == 8);
  assert(ROASTTY_TERMINAL_OPTION_TITLE == 9);
  assert(ROASTTY_TERMINAL_OPTION_PWD == 10);
  assert(ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_STORAGE_LIMIT == 15);
  assert(ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_FILE == 16);
  assert(ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_TEMP_FILE == 17);
  assert(ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_SHARED_MEM == 18);
  assert(ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES == 19);
  assert(ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES_KITTY == 20);
  assert(ROASTTY_COLOR_SCHEME_LIGHT == 0);
  assert(ROASTTY_COLOR_SCHEME_DARK == 1);
  assert(ROASTTY_SIZE_REPORT_MODE_2048 == 0);
  assert(ROASTTY_SIZE_REPORT_CSI_14_T == 1);
  assert(ROASTTY_SIZE_REPORT_CSI_16_T == 2);
  assert(ROASTTY_SIZE_REPORT_CSI_18_T == 3);
  assert(ROASTTY_POINT_ACTIVE == 0);
  assert(ROASTTY_POINT_VIEWPORT == 1);
  assert(ROASTTY_POINT_SCREEN == 2);
  assert(ROASTTY_POINT_HISTORY == 3);
  assert(sizeof(roastty_mode_tag_t) == sizeof(uint16_t));
  assert(ROASTTY_MODE_TAG_VALUE_MASK == 0x7fff);
  assert(ROASTTY_MODE_TAG_ANSI_BIT == 0x8000);
  assert(sizeof(roastty_point_coordinate_s) == 8);
  assert(_Alignof(roastty_point_coordinate_s) == 4);
  assert(offsetof(roastty_point_coordinate_s, x) == 0);
  assert(offsetof(roastty_point_coordinate_s, y) == 4);
  assert(sizeof(roastty_point_value_u) == 16);
  assert(_Alignof(roastty_point_value_u) == 8);
  assert(offsetof(roastty_point_value_u, active) == 0);
  assert(offsetof(roastty_point_value_u, viewport) == 0);
  assert(offsetof(roastty_point_value_u, screen) == 0);
  assert(offsetof(roastty_point_value_u, history) == 0);
  assert(offsetof(roastty_point_value_u, _padding) == 0);
  assert(sizeof(roastty_point_s) == 24);
  assert(_Alignof(roastty_point_s) == 8);
  assert(offsetof(roastty_point_s, tag) == 0);
  assert(offsetof(roastty_point_s, value) == 8);
  assert(sizeof(roastty_grid_ref_s) == 24);
  assert(_Alignof(roastty_grid_ref_s) == 8);
  assert(offsetof(roastty_grid_ref_s, size) == 0);
  assert(offsetof(roastty_grid_ref_s, node) == 8);
  assert(offsetof(roastty_grid_ref_s, x) == 16);
  assert(offsetof(roastty_grid_ref_s, y) == 18);
  assert(sizeof(roastty_tracked_grid_ref_t) == sizeof(void *));
  assert(sizeof(roastty_selection_s) == 64);
  assert(_Alignof(roastty_selection_s) == 8);
  assert(offsetof(roastty_selection_s, size) == 0);
  assert(offsetof(roastty_selection_s, start) == 8);
  assert(offsetof(roastty_selection_s, end) == 32);
  assert(offsetof(roastty_selection_s, rectangle) == 56);
  assert(sizeof(roastty_terminal_select_word_options_s) == 48);
  assert(offsetof(roastty_terminal_select_word_options_s, ref) == 8);
  assert(offsetof(roastty_terminal_select_word_options_s,
                  boundary_codepoints) == 32);
  assert(sizeof(roastty_terminal_select_word_between_options_s) == 72);
  assert(_Alignof(roastty_terminal_select_word_between_options_s) == 8);
  assert(offsetof(roastty_terminal_select_word_between_options_s, size) == 0);
  assert(offsetof(roastty_terminal_select_word_between_options_s, start) == 8);
  assert(offsetof(roastty_terminal_select_word_between_options_s, end) == 32);
  assert(offsetof(roastty_terminal_select_word_between_options_s,
                  boundary_codepoints) == 56);
  assert(offsetof(roastty_terminal_select_word_between_options_s,
                  boundary_codepoints_len) == 64);
  assert(sizeof(roastty_terminal_select_line_options_s) == 56);
  assert(_Alignof(roastty_terminal_select_line_options_s) == 8);
  assert(offsetof(roastty_terminal_select_line_options_s, size) == 0);
  assert(offsetof(roastty_terminal_select_line_options_s, ref) == 8);
  assert(offsetof(roastty_terminal_select_line_options_s, whitespace) == 32);
  assert(offsetof(roastty_terminal_select_line_options_s, whitespace_len) ==
         40);
  assert(offsetof(roastty_terminal_select_line_options_s,
                  semantic_prompt_boundary) == 48);
  assert(sizeof(roastty_terminal_selection_format_options_s) == 24);
  assert(offsetof(roastty_terminal_selection_format_options_s, emit) == 8);
  assert(offsetof(roastty_terminal_selection_format_options_s, selection) == 16);
  assert(ROASTTY_TERMINAL_OPTION_SELECTION == 21);
  assert(ROASTTY_SELECTION_FORMAT_PLAIN == 0);
  assert(ROASTTY_SELECTION_FORMAT_VT == 1);
  assert(ROASTTY_SELECTION_FORMAT_HTML == 2);
  assert(ROASTTY_FORMATTER_FORMAT_PLAIN == 0);
  assert(ROASTTY_FORMATTER_FORMAT_VT == 1);
  assert(ROASTTY_FORMATTER_FORMAT_HTML == 2);
  assert(ROASTTY_FOCUS_EVENT_GAINED == 0);
  assert(ROASTTY_FOCUS_EVENT_LOST == 1);
  assert(ROASTTY_MODE_REPORT_NOT_RECOGNIZED == 0);
  assert(ROASTTY_MODE_REPORT_SET == 1);
  assert(ROASTTY_MODE_REPORT_RESET == 2);
  assert(ROASTTY_MODE_REPORT_PERMANENTLY_SET == 3);
  assert(ROASTTY_MODE_REPORT_PERMANENTLY_RESET == 4);
  assert(sizeof(roastty_formatter_t) == sizeof(void *));
  assert(sizeof(roastty_formatter_screen_extra_s) == 16);
  assert(_Alignof(roastty_formatter_screen_extra_s) == 8);
  assert(offsetof(roastty_formatter_screen_extra_s, cursor) == 8);
  assert(offsetof(roastty_formatter_screen_extra_s, charsets) == 13);
  assert(sizeof(roastty_formatter_terminal_extra_s) == 32);
  assert(offsetof(roastty_formatter_terminal_extra_s, palette) == 8);
  assert(offsetof(roastty_formatter_terminal_extra_s, keyboard) == 13);
  assert(offsetof(roastty_formatter_terminal_extra_s, screen) == 16);
  assert(sizeof(roastty_formatter_terminal_options_s) == 56);
  assert(offsetof(roastty_formatter_terminal_options_s, emit) == 8);
  assert(offsetof(roastty_formatter_terminal_options_s, extra) == 16);
  assert(offsetof(roastty_formatter_terminal_options_s, selection) == 48);
  assert(ROASTTY_SELECTION_ORDER_FORWARD == 0);
  assert(ROASTTY_SELECTION_ORDER_REVERSE == 1);
  assert(ROASTTY_SELECTION_ORDER_MIRRORED_FORWARD == 2);
  assert(ROASTTY_SELECTION_ORDER_MIRRORED_REVERSE == 3);
  assert(ROASTTY_SELECTION_ADJUST_LEFT == 0);
  assert(ROASTTY_SELECTION_ADJUST_END_OF_LINE == 9);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_PRESS == 0);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_RELEASE == 1);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_DRAG == 2);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_AUTOSCROLL_TICK == 3);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_DEEP_PRESS == 4);
  assert(ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT == 0);
  assert(ROASTTY_SELECTION_GESTURE_DATA_DRAGGED == 1);
  assert(ROASTTY_SELECTION_GESTURE_DATA_AUTOSCROLL == 2);
  assert(ROASTTY_SELECTION_GESTURE_DATA_BEHAVIOR == 3);
  assert(ROASTTY_SELECTION_GESTURE_DATA_ANCHOR == 4);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF == 0);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION == 1);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_DISTANCE == 2);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS == 3);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_INTERVAL_NS == 4);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_WORD_BOUNDARY_CODEPOINTS == 5);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_BEHAVIORS == 6);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_RECTANGLE == 7);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_GEOMETRY == 8);
  assert(ROASTTY_SELECTION_GESTURE_EVENT_OPTION_VIEWPORT == 9);
  assert(ROASTTY_SELECTION_GESTURE_AUTOSCROLL_NONE == 0);
  assert(ROASTTY_SELECTION_GESTURE_AUTOSCROLL_UP == 1);
  assert(ROASTTY_SELECTION_GESTURE_AUTOSCROLL_DOWN == 2);
  assert(ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL == 0);
  assert(ROASTTY_SELECTION_GESTURE_BEHAVIOR_WORD == 1);
  assert(ROASTTY_SELECTION_GESTURE_BEHAVIOR_LINE == 2);
  assert(ROASTTY_SELECTION_GESTURE_BEHAVIOR_OUTPUT == 3);
  assert(sizeof(roastty_surface_position_s) == 16);
  assert(_Alignof(roastty_surface_position_s) == 8);
  assert(offsetof(roastty_surface_position_s, x) == 0);
  assert(offsetof(roastty_surface_position_s, y) == 8);
  assert(sizeof(roastty_codepoints_s) == 16);
  assert(_Alignof(roastty_codepoints_s) == 8);
  assert(offsetof(roastty_codepoints_s, ptr) == 0);
  assert(offsetof(roastty_codepoints_s, len) == 8);
  assert(sizeof(roastty_selection_gesture_behaviors_s) == 12);
  assert(offsetof(roastty_selection_gesture_behaviors_s, single_click) == 0);
  assert(offsetof(roastty_selection_gesture_behaviors_s, double_click) == 4);
  assert(offsetof(roastty_selection_gesture_behaviors_s, triple_click) == 8);
  assert(sizeof(roastty_selection_gesture_geometry_s) == 16);
  assert(offsetof(roastty_selection_gesture_geometry_s, columns) == 0);
  assert(offsetof(roastty_selection_gesture_geometry_s, cell_width) == 4);
  assert(offsetof(roastty_selection_gesture_geometry_s, padding_left) == 8);
  assert(offsetof(roastty_selection_gesture_geometry_s, screen_height) == 12);

  roastty_size_report_size_s report_size = {
      .rows = 24,
      .columns = 80,
      .cell_width = 9,
      .cell_height = 18,
  };
  char report_buf[64] = {0};
  size_t report_written = 0;
  assert(roastty_size_report_encode(ROASTTY_SIZE_REPORT_CSI_14_T,
                                    report_size,
                                    report_buf,
                                    sizeof(report_buf),
                                    &report_written) == ROASTTY_SUCCESS);
  assert(report_written == strlen("\x1b[4;432;720t"));
  assert(memcmp(report_buf, "\x1b[4;432;720t", report_written) == 0);
  report_written = 0;
  assert(roastty_size_report_encode(ROASTTY_SIZE_REPORT_CSI_18_T,
                                    report_size,
                                    NULL,
                                    0,
                                    &report_written) == ROASTTY_OUT_OF_SPACE);
  assert(report_written == strlen("\x1b[8;24;80t"));
  report_written = 999;
  assert(roastty_size_report_encode((roastty_size_report_style_e)99,
                                    report_size,
                                    report_buf,
                                    sizeof(report_buf),
                                    &report_written) == ROASTTY_INVALID_VALUE);
  assert(report_written == 0);

  uint8_t encode_buf[64] = {0};
  size_t encode_written = 0;
  assert(roastty_focus_encode(ROASTTY_FOCUS_EVENT_GAINED,
                              encode_buf,
                              sizeof(encode_buf),
                              &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[I"));
  assert(memcmp(encode_buf, "\x1b[I", encode_written) == 0);
  assert(roastty_focus_encode(ROASTTY_FOCUS_EVENT_LOST,
                              encode_buf,
                              sizeof(encode_buf),
                              &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[O"));
  assert(memcmp(encode_buf, "\x1b[O", encode_written) == 0);
  encode_written = 0;
  assert(roastty_focus_encode(ROASTTY_FOCUS_EVENT_GAINED,
                              NULL,
                              1,
                              &encode_written) == ROASTTY_OUT_OF_SPACE);
  assert(encode_written == strlen("\x1b[I"));
  assert(roastty_focus_encode((roastty_focus_event_e)99,
                              encode_buf,
                              sizeof(encode_buf),
                              &encode_written) == ROASTTY_INVALID_VALUE);
  assert(encode_written == 0);

  assert(roastty_paste_is_safe((const uint8_t *)"hello", 5));
  assert(roastty_paste_is_safe(NULL, 42));
  assert(!roastty_paste_is_safe((const uint8_t *)"hello\n", 6));
  assert(!roastty_paste_is_safe((const uint8_t *)"he\x1b[201~llo", 10));
  uint8_t paste_data[] = "hello\nworld";
  assert(roastty_paste_encode(paste_data,
                              strlen((char *)paste_data),
                              false,
                              encode_buf,
                              sizeof(encode_buf),
                              &encode_written) == ROASTTY_SUCCESS);
  assert(memcmp(encode_buf, "hello\rworld", encode_written) == 0);
  assert(memcmp(paste_data, "hello\rworld", encode_written) == 0);
  uint8_t bracketed_paste[] = "hello";
  assert(roastty_paste_encode(bracketed_paste,
                              strlen((char *)bracketed_paste),
                              true,
                              encode_buf,
                              sizeof(encode_buf),
                              &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[200~hello\x1b[201~"));
  assert(memcmp(encode_buf, "\x1b[200~hello\x1b[201~", encode_written) == 0);

  uint8_t unsafe_paste[] = {'a', 0x1b, 'b', 0x03, 'c', 0x7f, '\0'};
  assert(roastty_paste_encode(unsafe_paste,
                              6,
                              true,
                              NULL,
                              9,
                              &encode_written) == ROASTTY_OUT_OF_SPACE);
  assert(memcmp(unsafe_paste, "a b c ", 6) == 0);
  assert(encode_written == strlen("\x1b[200~a b c \x1b[201~"));
  assert(roastty_paste_encode(NULL,
                              99,
                              false,
                              NULL,
                              0,
                              &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == 0);
  assert(roastty_paste_encode(NULL,
                              99,
                              true,
                              NULL,
                              0,
                              &encode_written) == ROASTTY_OUT_OF_SPACE);
  assert(encode_written == strlen("\x1b[200~\x1b[201~"));

  assert(roastty_mode_report_encode(1,
                                    ROASTTY_MODE_REPORT_SET,
                                    encode_buf,
                                    sizeof(encode_buf),
                                    &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[?1;1$y"));
  assert(memcmp(encode_buf, "\x1b[?1;1$y", encode_written) == 0);
  assert(roastty_mode_report_encode(ROASTTY_MODE_TAG_ANSI_BIT | 4,
                                    ROASTTY_MODE_REPORT_RESET,
                                    encode_buf,
                                    sizeof(encode_buf),
                                    &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[4;2$y"));
  assert(memcmp(encode_buf, "\x1b[4;2$y", encode_written) == 0);
  assert(roastty_mode_report_encode(9999,
                                    ROASTTY_MODE_REPORT_NOT_RECOGNIZED,
                                    encode_buf,
                                    sizeof(encode_buf),
                                    &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[?9999;0$y"));
  assert(memcmp(encode_buf, "\x1b[?9999;0$y", encode_written) == 0);
  assert(roastty_mode_report_encode(ROASTTY_MODE_TAG_VALUE_MASK,
                                    ROASTTY_MODE_REPORT_PERMANENTLY_RESET,
                                    encode_buf,
                                    sizeof(encode_buf),
                                    &encode_written) == ROASTTY_SUCCESS);
  assert(encode_written == strlen("\x1b[?32767;4$y"));
  assert(memcmp(encode_buf, "\x1b[?32767;4$y", encode_written) == 0);
  assert(roastty_mode_report_encode(1,
                                    (roastty_mode_report_state_e)99,
                                    encode_buf,
                                    sizeof(encode_buf),
                                    &encode_written) == ROASTTY_INVALID_VALUE);
  assert(encode_written == 0);

  roastty_terminal_t terminal = NULL;
  assert(roastty_terminal_new(5, 3, SIZE_MAX, NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_new(0, 3, SIZE_MAX, &terminal) ==
         ROASTTY_INVALID_VALUE);
  assert(terminal == NULL);
  assert(roastty_terminal_new(5, 0, SIZE_MAX, &terminal) ==
         ROASTTY_INVALID_VALUE);
  assert(terminal == NULL);

  assert(roastty_terminal_new(10, 4, SIZE_MAX, &terminal) == ROASTTY_SUCCESS);
  assert(terminal != NULL);

  assert(roastty_terminal_vt_write(NULL, NULL, 0) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_vt_write(terminal, NULL, 1) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_vt_write(terminal, NULL, 0) == ROASTTY_SUCCESS);

  roastty_point_s point = {
      .tag = ROASTTY_POINT_ACTIVE,
      .value = {.active = {.x = 1, .y = 0}},
  };
  roastty_grid_ref_s grid_ref = {0};
  assert(roastty_terminal_grid_ref(NULL, point, &grid_ref) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_grid_ref(terminal, point, NULL) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_grid_ref(terminal,
                                   (roastty_point_s){
                                       .tag = (roastty_point_tag_e)99,
                                       .value = {.active = {.x = 0, .y = 0}},
                                   },
                                   &grid_ref) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_grid_ref(terminal,
                                   (roastty_point_s){
                                       .tag = ROASTTY_POINT_ACTIVE,
                                       .value = {.active = {.x = 10, .y = 0}},
                                   },
                                   &grid_ref) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_grid_ref(terminal, point, &grid_ref) ==
         ROASTTY_SUCCESS);
  assert(grid_ref.size == sizeof(roastty_grid_ref_s));
  assert(grid_ref.node != NULL);
  assert(grid_ref.x == 1);
  assert(grid_ref.y == 0);

  roastty_cell_t grid_cell = 0;
  assert(roastty_grid_ref_cell(&grid_ref, &grid_cell) == ROASTTY_SUCCESS);
  uint32_t grid_codepoint = 0;
  assert(roastty_cell_get(grid_cell,
                          ROASTTY_CELL_DATA_CODEPOINT,
                          &grid_codepoint) == ROASTTY_SUCCESS);
  assert(roastty_grid_ref_cell(&grid_ref, NULL) == ROASTTY_SUCCESS);

  roastty_row_t grid_row = 0;
  assert(roastty_grid_ref_row(&grid_ref, &grid_row) == ROASTTY_SUCCESS);
  bool grid_row_dirty = true;
  assert(roastty_row_get(grid_row,
                         ROASTTY_ROW_DATA_DIRTY,
                         &grid_row_dirty) == ROASTTY_SUCCESS);
  assert(roastty_grid_ref_row(&grid_ref, NULL) == ROASTTY_SUCCESS);

  roastty_style_s grid_style = {.size = sizeof(roastty_style_s)};
  assert(roastty_grid_ref_style(&grid_ref, &grid_style) == ROASTTY_SUCCESS);
  assert(roastty_grid_ref_style(&grid_ref, NULL) == ROASTTY_SUCCESS);

  size_t grid_len = 999;
  uint32_t grid_graphemes[4] = {0};
  assert(roastty_grid_ref_graphemes(&grid_ref,
                                    grid_graphemes,
                                    4,
                                    &grid_len) == ROASTTY_SUCCESS);
  assert(grid_len <= 4);
  assert(roastty_grid_ref_graphemes(&grid_ref, NULL, 0, NULL) ==
         ROASTTY_INVALID_VALUE);

  uint8_t grid_uri[32] = {0};
  grid_len = 999;
  assert(roastty_grid_ref_hyperlink_uri(&grid_ref,
                                        grid_uri,
                                        sizeof(grid_uri),
                                        &grid_len) == ROASTTY_SUCCESS);
  assert(grid_len == 0);
  assert(roastty_grid_ref_hyperlink_uri(&grid_ref, grid_uri, 1, NULL) ==
         ROASTTY_INVALID_VALUE);

  roastty_point_coordinate_s coord = {0};
  assert(roastty_terminal_point_from_grid_ref(NULL,
                                              &grid_ref,
                                              ROASTTY_POINT_ACTIVE,
                                              &coord) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              NULL,
                                              ROASTTY_POINT_ACTIVE,
                                              &coord) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &grid_ref,
                                              ROASTTY_POINT_ACTIVE,
                                              NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &grid_ref,
                                              (roastty_point_tag_e)99,
                                              &coord) == ROASTTY_INVALID_VALUE);
  roastty_grid_ref_s undersized = grid_ref;
  undersized.size = sizeof(roastty_grid_ref_s) - 1;
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &undersized,
                                              ROASTTY_POINT_ACTIVE,
                                              &coord) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &grid_ref,
                                              ROASTTY_POINT_ACTIVE,
                                              &coord) == ROASTTY_SUCCESS);
  assert(coord.x == 1);
  assert(coord.y == 0);

  roastty_grid_ref_s forged = grid_ref;
  forged.x = 10;
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &forged,
                                              ROASTTY_POINT_ACTIVE,
                                              &coord) == ROASTTY_INVALID_VALUE);
  forged = grid_ref;
  forged.y = 99;
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &forged,
                                              ROASTTY_POINT_ACTIVE,
                                              &coord) == ROASTTY_INVALID_VALUE);

  point = (roastty_point_s){
      .tag = ROASTTY_POINT_VIEWPORT,
      .value = {.viewport = {.x = 2, .y = 0}},
  };
  assert(roastty_terminal_grid_ref(terminal, point, &grid_ref) ==
         ROASTTY_SUCCESS);
  assert(roastty_terminal_point_from_grid_ref(terminal,
                                              &grid_ref,
                                              ROASTTY_POINT_VIEWPORT,
                                              &coord) == ROASTTY_SUCCESS);
  assert(coord.x == 2);
  assert(coord.y == 0);

  roastty_tracked_grid_ref_t tracked = NULL;
  assert(roastty_terminal_grid_ref_track(NULL, point, &tracked) ==
         ROASTTY_INVALID_VALUE);
  assert(tracked == NULL);
  assert(roastty_terminal_grid_ref_track(terminal, point, NULL) ==
         ROASTTY_INVALID_VALUE);
  tracked = terminal_tracked_grid_ref_at(terminal, 2, 0);
  assert(roastty_tracked_grid_ref_has_value(tracked));
  assert(roastty_tracked_grid_ref_snapshot(NULL, NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_tracked_grid_ref_snapshot(tracked, NULL) == ROASTTY_SUCCESS);
  assert(roastty_tracked_grid_ref_point(tracked,
                                        ROASTTY_POINT_VIEWPORT,
                                        NULL) == ROASTTY_SUCCESS);
  roastty_grid_ref_s tracked_snapshot = {0};
  assert(roastty_tracked_grid_ref_snapshot(tracked, &tracked_snapshot) ==
         ROASTTY_SUCCESS);
  assert(tracked_snapshot.size == sizeof(roastty_grid_ref_s));
  assert(tracked_snapshot.node != NULL);
  assert(tracked_snapshot.x == 2);
  assert(roastty_tracked_grid_ref_point(tracked,
                                        ROASTTY_POINT_VIEWPORT,
                                        &coord) == ROASTTY_SUCCESS);
  assert(coord.x == 2);

  assert(roastty_tracked_grid_ref_set(tracked,
                                      NULL,
                                      (roastty_point_s){
                                          .tag = ROASTTY_POINT_ACTIVE,
                                          .value = {.active = {.x = 0, .y = 0}},
                                      }) == ROASTTY_INVALID_VALUE);
  assert(roastty_tracked_grid_ref_set(tracked,
                                      terminal,
                                      (roastty_point_s){
                                          .tag = ROASTTY_POINT_ACTIVE,
                                          .value = {.active = {.x = 3, .y = 0}},
                                      }) == ROASTTY_SUCCESS);
  assert(roastty_tracked_grid_ref_point(tracked,
                                        ROASTTY_POINT_ACTIVE,
                                        &coord) == ROASTTY_SUCCESS);
  assert(coord.x == 3);
  roastty_tracked_grid_ref_free(tracked);

  tracked = terminal_tracked_grid_ref_at(terminal, 0, 0);
  terminal_write(terminal, "\nscroll\nscroll\nscroll\n");
  assert(roastty_tracked_grid_ref_has_value(tracked));
  assert(roastty_tracked_grid_ref_snapshot(tracked, &tracked_snapshot) ==
         ROASTTY_SUCCESS);
  roastty_tracked_grid_ref_free(tracked);

  tracked = terminal_tracked_grid_ref_at(terminal, 0, 0);
  roastty_terminal_reset(terminal);
  assert(!roastty_tracked_grid_ref_has_value(tracked));
  assert(roastty_tracked_grid_ref_snapshot(tracked, NULL) == ROASTTY_NO_VALUE);
  assert(roastty_tracked_grid_ref_point(tracked,
                                        ROASTTY_POINT_ACTIVE,
                                        NULL) == ROASTTY_NO_VALUE);
  roastty_tracked_grid_ref_free(tracked);

  roastty_terminal_t tracked_free_terminal = NULL;
  assert(roastty_terminal_new(5, 3, SIZE_MAX, &tracked_free_terminal) ==
         ROASTTY_SUCCESS);
  tracked = terminal_tracked_grid_ref_at(tracked_free_terminal, 0, 0);
  roastty_terminal_free(tracked_free_terminal);
  assert(!roastty_tracked_grid_ref_has_value(tracked));
  assert(roastty_tracked_grid_ref_snapshot(tracked, NULL) == ROASTTY_NO_VALUE);
  assert(roastty_tracked_grid_ref_set(tracked,
                                      tracked_free_terminal,
                                      (roastty_point_s){
                                          .tag = ROASTTY_POINT_ACTIVE,
                                          .value = {.active = {.x = 0, .y = 0}},
                                      }) == ROASTTY_INVALID_VALUE);
  roastty_tracked_grid_ref_free(tracked);

  roastty_terminal_t selection_terminal = NULL;
  assert(roastty_terminal_new(20, 3, SIZE_MAX, &selection_terminal) ==
         ROASTTY_SUCCESS);
  terminal_write(selection_terminal, "Hello World\r\nsecond line");
  roastty_terminal_select_word_options_s word_options = {
      .size = sizeof(roastty_terminal_select_word_options_s),
      .ref = terminal_grid_ref_at(selection_terminal, 7, 0),
      .boundary_codepoints = NULL,
      .boundary_codepoints_len = 0,
  };
  roastty_selection_s selection = {0};
  assert(roastty_terminal_select_word(selection_terminal,
                                      &word_options,
                                      &selection) == ROASTTY_SUCCESS);
  assert(selection.size == sizeof(roastty_selection_s));
  assert(selection.start.size == sizeof(roastty_grid_ref_s));
  assert(selection.end.size == sizeof(roastty_grid_ref_s));
  assert(selection.start.x == 6);
  assert(selection.end.x == 10);

  roastty_terminal_select_word_between_options_s between_options = {
      .size = sizeof(roastty_terminal_select_word_between_options_s),
      .start = terminal_grid_ref_at(selection_terminal, 1, 0),
      .end = terminal_grid_ref_at(selection_terminal, 7, 0),
      .boundary_codepoints = NULL,
      .boundary_codepoints_len = 0,
  };
  roastty_selection_s between_selection = {0};
  assert(roastty_terminal_select_word_between(selection_terminal,
                                              &between_options,
                                              &between_selection) ==
         ROASTTY_SUCCESS);
  assert(between_selection.start.x == 0);
  assert(between_selection.end.x == 4);

  assert(roastty_terminal_set(selection_terminal,
                              ROASTTY_TERMINAL_OPTION_SELECTION,
                              &selection) == ROASTTY_SUCCESS);
  roastty_selection_s active_selection = {0};
  assert(roastty_terminal_get(selection_terminal,
                              ROASTTY_TERMINAL_DATA_SELECTION,
                              &active_selection) == ROASTTY_SUCCESS);
  assert(active_selection.start.x == 6);
  assert(active_selection.end.x == 10);

  roastty_terminal_selection_format_options_s format_options = {
      .size = sizeof(roastty_terminal_selection_format_options_s),
      .emit = ROASTTY_SELECTION_FORMAT_PLAIN,
      .unwrap = true,
      .trim = true,
      .selection = NULL,
  };
  size_t required = 0;
  assert(roastty_terminal_selection_format_buf(selection_terminal,
                                              &format_options,
                                              NULL,
                                              1,
                                              &required) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_selection_format_buf(selection_terminal,
                                              &format_options,
                                              NULL,
                                              0,
                                              &required) ==
         ROASTTY_OUT_OF_SPACE);
  assert(required == 5);
  uint8_t tiny_selection_buf[2] = {0};
  assert(roastty_terminal_selection_format_buf(selection_terminal,
                                              &format_options,
                                              tiny_selection_buf,
                                              sizeof(tiny_selection_buf),
                                              &required) ==
         ROASTTY_OUT_OF_SPACE);
  assert(required == 5);
  uint8_t selection_buf[16] = {0};
  assert(roastty_terminal_selection_format_buf(selection_terminal,
                                              &format_options,
                                              selection_buf,
                                              sizeof(selection_buf),
                                              &required) == ROASTTY_SUCCESS);
  assert(required == 5);
  assert(memcmp(selection_buf, "World", required) == 0);

  roastty_string_s formatted_selection = {0};
  assert(roastty_terminal_selection_format(selection_terminal,
                                           &format_options,
                                           &formatted_selection) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(formatted_selection, "World");

  roastty_selection_order_e order = (roastty_selection_order_e)-1;
  assert(roastty_terminal_selection_order(selection_terminal,
                                          &selection,
                                          &order) == ROASTTY_SUCCESS);
  assert(order == ROASTTY_SELECTION_ORDER_FORWARD);
  bool contains = false;
  assert(roastty_terminal_selection_contains(
             selection_terminal,
             &selection,
             (roastty_point_s){
                 .tag = ROASTTY_POINT_SCREEN,
                 .value = {.screen = {.x = 8, .y = 0}},
             },
             &contains) == ROASTTY_SUCCESS);
  assert(contains);
  bool equal = false;
  assert(roastty_terminal_selection_equal(selection_terminal,
                                          &selection,
                                          &active_selection,
                                          &equal) == ROASTTY_SUCCESS);
  assert(equal);
  assert(roastty_terminal_selection_adjust(
             selection_terminal,
             &active_selection,
             ROASTTY_SELECTION_ADJUST_END_OF_LINE) == ROASTTY_SUCCESS);
  assert(active_selection.end.x == 19);

  roastty_selection_s reversed = {0};
  assert(roastty_terminal_selection_ordered(selection_terminal,
                                           &selection,
                                           ROASTTY_SELECTION_ORDER_REVERSE,
                                           &reversed) == ROASTTY_SUCCESS);
  assert(reversed.start.x == 10);
  assert(reversed.end.x == 6);

  roastty_terminal_select_line_options_s line_options = {
      .size = sizeof(roastty_terminal_select_line_options_s),
      .ref = terminal_grid_ref_at(selection_terminal, 2, 1),
      .whitespace = NULL,
      .whitespace_len = 0,
      .semantic_prompt_boundary = false,
  };
  assert(roastty_terminal_select_line(selection_terminal,
                                      &line_options,
                                      &selection) == ROASTTY_SUCCESS);
  assert(selection.start.x == 0);
  assert(selection.start.y == 1);
  assert(roastty_terminal_select_all(selection_terminal, &selection) ==
         ROASTTY_SUCCESS);
  assert(selection.start.x == 0);
  assert(selection.start.y == 0);

  roastty_selection_s output_selection = {0};
  assert(roastty_terminal_select_output(selection_terminal,
                                        &selection.start,
                                        &output_selection) == ROASTTY_NO_VALUE);

  roastty_formatter_terminal_options_s formatter_options = {
      .size = sizeof(roastty_formatter_terminal_options_s),
      .emit = ROASTTY_FORMATTER_FORMAT_PLAIN,
      .unwrap = true,
      .trim = true,
      .extra = {
          .size = sizeof(roastty_formatter_terminal_extra_s),
          .screen = {.size = sizeof(roastty_formatter_screen_extra_s)},
      },
      .selection = NULL,
  };
  roastty_formatter_t formatter = NULL;
  assert(roastty_formatter_terminal_new(NULL,
                                        selection_terminal,
                                        formatter_options) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_formatter_terminal_new(&formatter,
                                        NULL,
                                        formatter_options) ==
         ROASTTY_INVALID_VALUE);
  assert(formatter == NULL);
  roastty_formatter_terminal_options_s invalid_formatter_options =
      formatter_options;
  invalid_formatter_options.emit = (roastty_formatter_format_e)99;
  assert(roastty_formatter_terminal_new(&formatter,
                                        selection_terminal,
                                        invalid_formatter_options) ==
         ROASTTY_INVALID_VALUE);
  assert(formatter == NULL);

  assert(roastty_formatter_terminal_new(&formatter,
                                        selection_terminal,
                                        formatter_options) ==
         ROASTTY_SUCCESS);
  assert(formatter != NULL);
  assert(roastty_formatter_format_buf(formatter,
                                      NULL,
                                      42,
                                      &required) == ROASTTY_OUT_OF_SPACE);
  assert(required == strlen("Hello World\nsecond line"));
  uint8_t formatter_buf[32] = {0};
  assert(roastty_formatter_format_buf(formatter,
                                      formatter_buf,
                                      sizeof(formatter_buf),
                                      &required) == ROASTTY_SUCCESS);
  assert(memcmp(formatter_buf, "Hello World\nsecond line", required) == 0);
  roastty_string_s formatted_terminal = {0};
  assert(roastty_formatter_format(formatter, &formatted_terminal) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(formatted_terminal, "Hello World\nsecond line");
  roastty_formatter_free(formatter);

  formatter_options.selection = &active_selection;
  assert(roastty_formatter_terminal_new(&formatter,
                                        selection_terminal,
                                        formatter_options) ==
         ROASTTY_SUCCESS);
  assert(roastty_formatter_format(formatter, &formatted_terminal) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(formatted_terminal, "World");
  roastty_formatter_free(formatter);

  formatter_options.selection = NULL;
  formatter_options.emit = ROASTTY_FORMATTER_FORMAT_VT;
  formatter_options.extra.palette = true;
  formatter_options.extra.screen.cursor = true;
  assert(roastty_formatter_terminal_new(&formatter,
                                        selection_terminal,
                                        formatter_options) ==
         ROASTTY_SUCCESS);
  assert(roastty_formatter_format(formatter, &formatted_terminal) ==
         ROASTTY_SUCCESS);
  assert(formatted_terminal.len > strlen("Hello World\nsecond line"));
  assert(formatted_terminal.ptr != NULL);
  assert(bytes_contains(formatted_terminal.ptr,
                        formatted_terminal.len,
                        "\x1b]4;",
                        4));
  roastty_string_free(formatted_terminal);
  roastty_formatter_free(formatter);
  roastty_formatter_free(NULL);

  roastty_selection_gesture_t gesture = NULL;
  assert(roastty_selection_gesture_new(&gesture) == ROASTTY_SUCCESS);
  assert(gesture != NULL);
  assert(roastty_selection_gesture_new(NULL) == ROASTTY_INVALID_VALUE);

  uint8_t click_count = 255;
  bool dragged = true;
  roastty_selection_gesture_autoscroll_e autoscroll =
      ROASTTY_SELECTION_GESTURE_AUTOSCROLL_UP;
  roastty_selection_gesture_behavior_e behavior =
      ROASTTY_SELECTION_GESTURE_BEHAVIOR_WORD;
  roastty_selection_gesture_data_e gesture_keys[] = {
      ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT,
      ROASTTY_SELECTION_GESTURE_DATA_DRAGGED,
      ROASTTY_SELECTION_GESTURE_DATA_AUTOSCROLL,
      ROASTTY_SELECTION_GESTURE_DATA_BEHAVIOR,
  };
  void *gesture_values[] = {&click_count, &dragged, &autoscroll, &behavior};
  size_t gesture_written = 0;
  assert(roastty_selection_gesture_get_multi(gesture,
                                             selection_terminal,
                                             4,
                                             gesture_keys,
                                             gesture_values,
                                             &gesture_written) ==
         ROASTTY_SUCCESS);
  assert(gesture_written == 4);
  assert(click_count == 0);
  assert(!dragged);
  assert(autoscroll == ROASTTY_SELECTION_GESTURE_AUTOSCROLL_NONE);
  assert(behavior == ROASTTY_SELECTION_GESTURE_BEHAVIOR_CELL);

  roastty_selection_gesture_event_t press = NULL;
  assert(roastty_selection_gesture_event_new(
             &press,
             ROASTTY_SELECTION_GESTURE_EVENT_PRESS) == ROASTTY_SUCCESS);
  assert(press != NULL);
  assert(roastty_selection_gesture_event_new(NULL,
                                             ROASTTY_SELECTION_GESTURE_EVENT_PRESS) ==
         ROASTTY_INVALID_VALUE);
  roastty_selection_gesture_event_t invalid_event = NULL;
  assert(roastty_selection_gesture_event_new(
             &invalid_event,
             (roastty_selection_gesture_event_e)99) == ROASTTY_INVALID_VALUE);
  assert(invalid_event == NULL);

  roastty_grid_ref_s press_ref = terminal_grid_ref_at(selection_terminal, 1, 0);
  roastty_surface_position_s press_pos = {.x = 10.0, .y = 0.0};
  double repeat_distance = 20.0;
  uint64_t time_ns = 1;
  uint64_t repeat_interval = 100;
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF,
             &press_ref) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION,
             &press_pos) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_DISTANCE,
             &repeat_distance) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS,
             &time_ns) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REPEAT_INTERVAL_NS,
             &repeat_interval) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_GEOMETRY,
             NULL) == ROASTTY_INVALID_VALUE);

  roastty_selection_s gesture_selection = {0};
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                press,
                                                &gesture_selection) ==
         ROASTTY_NO_VALUE);
  assert(roastty_selection_gesture_get(gesture,
                                       selection_terminal,
                                       ROASTTY_SELECTION_GESTURE_DATA_CLICK_COUNT,
                                       &click_count) == ROASTTY_SUCCESS);
  assert(click_count == 1);

  time_ns = 2;
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS,
             &time_ns) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                press,
                                                &gesture_selection) ==
         ROASTTY_SUCCESS);
  assert(gesture_selection.start.x == 0);
  assert(gesture_selection.end.x == 4);

  roastty_selection_gesture_reset(gesture, selection_terminal);
  time_ns = 200;
  assert(roastty_selection_gesture_event_set(
             press,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_TIME_NS,
             &time_ns) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                press,
                                                NULL) == ROASTTY_NO_VALUE);

  roastty_selection_gesture_event_t drag = NULL;
  assert(roastty_selection_gesture_event_new(
             &drag,
             ROASTTY_SELECTION_GESTURE_EVENT_DRAG) == ROASTTY_SUCCESS);
  roastty_grid_ref_s drag_ref = terminal_grid_ref_at(selection_terminal, 3, 0);
  roastty_surface_position_s drag_pos = {.x = 39.0, .y = 10.0};
  roastty_selection_gesture_geometry_s geometry = {
      .columns = 20,
      .cell_width = 10,
      .padding_left = 0,
      .screen_height = 100,
  };
  assert(roastty_selection_gesture_event_set(
             drag,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_REF,
             &drag_ref) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             drag,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_POSITION,
             &drag_pos) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_event_set(
             drag,
             ROASTTY_SELECTION_GESTURE_EVENT_OPTION_GEOMETRY,
             &geometry) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                drag,
                                                &gesture_selection) ==
         ROASTTY_SUCCESS);
  assert(gesture_selection.start.x == 1);
  assert(gesture_selection.end.x == 3);

  roastty_selection_gesture_reset(gesture, selection_terminal);
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                press,
                                                NULL) == ROASTTY_NO_VALUE);
  roastty_selection_gesture_event_t deep_press = NULL;
  assert(roastty_selection_gesture_event_new(
             &deep_press,
             ROASTTY_SELECTION_GESTURE_EVENT_DEEP_PRESS) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                deep_press,
                                                &gesture_selection) ==
         ROASTTY_SUCCESS);
  assert(gesture_selection.start.x == 0);
  assert(gesture_selection.end.x == 4);

  roastty_selection_gesture_event_t missing_drag = NULL;
  assert(roastty_selection_gesture_event_new(
             &missing_drag,
             ROASTTY_SELECTION_GESTURE_EVENT_DRAG) == ROASTTY_SUCCESS);
  assert(roastty_selection_gesture_handle_event(gesture,
                                                selection_terminal,
                                                missing_drag,
                                                &gesture_selection) ==
         ROASTTY_INVALID_VALUE);

  roastty_selection_gesture_event_free(missing_drag);
  roastty_selection_gesture_event_free(deep_press);
  roastty_selection_gesture_event_free(drag);
  roastty_selection_gesture_event_free(press);
  roastty_selection_gesture_free(gesture, selection_terminal);

  roastty_terminal_free(selection_terminal);

  char title_buf[] = "c title";
  roastty_string_s title_input = {
      .ptr = title_buf,
      .len = strlen(title_buf),
      .sentinel = false,
  };
  assert(roastty_terminal_set(NULL,
                              ROASTTY_TERMINAL_OPTION_TITLE,
                              &title_input) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_set(terminal,
                              (roastty_terminal_option_e)9999,
                              &title_input) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_set(terminal,
                              (roastty_terminal_option_e)22,
                              &title_input) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_TITLE,
                              &title_input) == ROASTTY_SUCCESS);
  memset(title_buf, 'x', strlen(title_buf));
  roastty_string_s title = {0};
  assert(roastty_terminal_title(terminal, &title) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(title, "c title");

  char pwd_buf[] = "file://host/c-pwd";
  roastty_string_s pwd_input = {
      .ptr = pwd_buf,
      .len = strlen(pwd_buf),
      .sentinel = false,
  };
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_PWD,
                              &pwd_input) == ROASTTY_SUCCESS);
  memset(pwd_buf, 'y', strlen(pwd_buf));
  roastty_string_s pwd = {0};
  assert(roastty_terminal_pwd(terminal, &pwd) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(pwd, "file://host/c-pwd");

  roastty_string_s empty_input = {
      .ptr = NULL,
      .len = 0,
      .sentinel = false,
  };
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_TITLE,
                              &empty_input) == ROASTTY_SUCCESS);
  assert(roastty_terminal_title(terminal, &title) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(title, "");
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_PWD,
                              NULL) == ROASTTY_SUCCESS);
  assert(roastty_terminal_pwd(terminal, &pwd) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(pwd, "");

  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_TITLE,
                              &title_input) == ROASTTY_SUCCESS);
  roastty_string_s invalid_inner_null = {
      .ptr = NULL,
      .len = 1,
      .sentinel = false,
  };
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_TITLE,
                              &invalid_inner_null) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_title(terminal, &title) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(title, "xxxxxxx");

  terminal_write(terminal, "abc");
  roastty_string_s plain = {0};
  assert(roastty_terminal_read_screen_plain(terminal, false, &plain) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(plain, "abc");
  assert(roastty_terminal_read_screen_plain(terminal, false, NULL) ==
         ROASTTY_INVALID_VALUE);

  uint16_t column = 0;
  uint16_t row = 0;
  assert(roastty_terminal_cursor_position(terminal, &column, &row));
  assert(column == 3);
  assert(row == 0);
  assert(!roastty_terminal_cursor_position(terminal, NULL, &row));
  assert(!roastty_terminal_cursor_position(NULL, &column, &row));

  uint16_t cols = 0;
  uint16_t rows = 0;
  assert(roastty_terminal_get(terminal, ROASTTY_TERMINAL_DATA_COLS, &cols) ==
         ROASTTY_SUCCESS);
  assert(cols == 10);
  assert(roastty_terminal_get(terminal, ROASTTY_TERMINAL_DATA_ROWS, &rows) ==
         ROASTTY_SUCCESS);
  assert(rows == 4);
  column = 0;
  row = 0;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_CURSOR_X,
                              &column) == ROASTTY_SUCCESS);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_CURSOR_Y,
                              &row) == ROASTTY_SUCCESS);
  assert(column == 3);
  assert(row == 0);
  bool pending_wrap = true;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP,
                              &pending_wrap) == ROASTTY_SUCCESS);
  assert(!pending_wrap);
  roastty_terminal_screen_e active_screen = ROASTTY_TERMINAL_SCREEN_ALTERNATE;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                              &active_screen) == ROASTTY_SUCCESS);
  assert(active_screen == ROASTTY_TERMINAL_SCREEN_PRIMARY);
  bool cursor_visible = false;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_CURSOR_VISIBLE,
                              &cursor_visible) == ROASTTY_SUCCESS);
  assert(cursor_visible);
  uint8_t key_flags = 99;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS,
                              &key_flags) == ROASTTY_SUCCESS);
  assert(key_flags == 0);
  uint64_t kitty_storage_limit = 0;
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT,
             &kitty_storage_limit) == ROASTTY_SUCCESS);
  assert(kitty_storage_limit == 320000000);
  bool kitty_medium_file = true;
  bool kitty_medium_temp_file = true;
  bool kitty_medium_shared_mem = true;
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE,
             &kitty_medium_file) == ROASTTY_SUCCESS);
  assert(!kitty_medium_file);
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE,
             &kitty_medium_temp_file) == ROASTTY_SUCCESS);
  assert(!kitty_medium_temp_file);
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM,
             &kitty_medium_shared_mem) == ROASTTY_SUCCESS);
  assert(!kitty_medium_shared_mem);
  uint64_t new_kitty_storage_limit = 123;
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_STORAGE_LIMIT,
             &new_kitty_storage_limit) == ROASTTY_SUCCESS);
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT,
             &kitty_storage_limit) == ROASTTY_SUCCESS);
  assert(kitty_storage_limit == 123);
  kitty_medium_file = true;
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_FILE,
             &kitty_medium_file) == ROASTTY_SUCCESS);
  kitty_medium_file = false;
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE,
             &kitty_medium_file) == ROASTTY_SUCCESS);
  assert(kitty_medium_file);
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_FILE,
             NULL) == ROASTTY_SUCCESS);
  kitty_medium_file = false;
  assert(roastty_terminal_get(
             terminal,
             ROASTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE,
             &kitty_medium_file) == ROASTTY_SUCCESS);
  assert(kitty_medium_file);
  kitty_medium_temp_file = true;
  kitty_medium_shared_mem = true;
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_TEMP_FILE,
             &kitty_medium_temp_file) == ROASTTY_SUCCESS);
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_KITTY_IMAGE_MEDIUM_SHARED_MEM,
             &kitty_medium_shared_mem) == ROASTTY_SUCCESS);
  size_t apc_max_bytes = 2;
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES,
                              &apc_max_bytes) == ROASTTY_SUCCESS);
  size_t kitty_apc_max_bytes = 256;
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES_KITTY,
                              &kitty_apc_max_bytes) == ROASTTY_SUCCESS);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES_KITTY,
                              NULL) == ROASTTY_SUCCESS);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_APC_MAX_BYTES,
                              NULL) == ROASTTY_SUCCESS);
  bool mouse_tracking = true;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                              &mouse_tracking) == ROASTTY_SUCCESS);
  assert(!mouse_tracking);
  roastty_kitty_graphics_t graphics = NULL;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_KITTY_GRAPHICS,
                              &graphics) == ROASTTY_SUCCESS);
  assert(graphics != NULL);
  size_t total_rows = 0;
  size_t scrollback_rows = 99;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_TOTAL_ROWS,
                              &total_rows) == ROASTTY_SUCCESS);
  assert(total_rows == 4);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_SCROLLBACK_ROWS,
                              &scrollback_rows) == ROASTTY_SUCCESS);
  assert(scrollback_rows == 0);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_TITLE,
                              &total_rows) == ROASTTY_NO_VALUE);
  assert(roastty_terminal_get(terminal,
                              (roastty_terminal_data_e)-1,
                              &total_rows) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_get(terminal,
                              (roastty_terminal_data_e)33,
                              &total_rows) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_INVALID,
                              &total_rows) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLS,
                              NULL) == ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_get(NULL,
                              ROASTTY_TERMINAL_DATA_COLS,
                              &cols) == ROASTTY_INVALID_VALUE);

  assert(sizeof(roastty_rgb_s) == 3);
  assert(_Alignof(roastty_rgb_s) == 1);
  assert(ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND == 11);
  assert(ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND == 12);
  assert(ROASTTY_TERMINAL_OPTION_COLOR_CURSOR == 13);
  assert(ROASTTY_TERMINAL_OPTION_COLOR_PALETTE == 14);

  roastty_rgb_s rgb_out = {.r = 9, .g = 8, .b = 7};
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
                              &rgb_out) == ROASTTY_NO_VALUE);
  assert(rgb_out.r == 9 && rgb_out.g == 8 && rgb_out.b == 7);
  roastty_rgb_s rgb = {.r = 1, .g = 2, .b = 3};
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_FOREGROUND,
                              &rgb) == ROASTTY_SUCCESS);
  rgb = (roastty_rgb_s){.r = 4, .g = 5, .b = 6};
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_BACKGROUND,
                              &rgb) == ROASTTY_SUCCESS);
  rgb = (roastty_rgb_s){.r = 7, .g = 8, .b = 9};
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_CURSOR,
                              &rgb) == ROASTTY_SUCCESS);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_FOREGROUND,
                              &rgb_out) == ROASTTY_SUCCESS);
  assert(rgb_out.r == 1 && rgb_out.g == 2 && rgb_out.b == 3);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT,
                              &rgb_out) == ROASTTY_SUCCESS);
  assert(rgb_out.r == 4 && rgb_out.g == 5 && rgb_out.b == 6);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_CURSOR,
                              NULL) == ROASTTY_SUCCESS);
  rgb_out = (roastty_rgb_s){.r = 9, .g = 8, .b = 7};
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_CURSOR,
                              &rgb_out) == ROASTTY_NO_VALUE);
  assert(rgb_out.r == 9 && rgb_out.g == 8 && rgb_out.b == 7);

  roastty_palette_t initial_palette = {0};
  roastty_palette_t initial_default_palette = {0};
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_PALETTE,
                              &initial_palette) == ROASTTY_SUCCESS);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT,
                              &initial_default_palette) == ROASTTY_SUCCESS);
  assert(memcmp(initial_palette,
                initial_default_palette,
                sizeof(initial_palette)) == 0);
  roastty_palette_t palette = {0};
  memcpy(palette, initial_default_palette, sizeof(palette));
  palette[1] = (roastty_rgb_s){.r = 0x11, .g = 0x22, .b = 0x33};
  palette[2] = (roastty_rgb_s){.r = 0x44, .g = 0x55, .b = 0x66};
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                              &palette) == ROASTTY_SUCCESS);
  palette[1] = (roastty_rgb_s){0};
  roastty_palette_t got_palette = {0};
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_PALETTE,
                              &got_palette) == ROASTTY_SUCCESS);
  assert(got_palette[1].r == 0x11);
  assert(got_palette[1].g == 0x22);
  assert(got_palette[1].b == 0x33);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT,
                              &got_palette) == ROASTTY_SUCCESS);
  assert(got_palette[2].r == 0x44);
  assert(got_palette[2].g == 0x55);
  assert(got_palette[2].b == 0x66);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_PALETTE,
                              NULL) == ROASTTY_SUCCESS);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT,
                              &got_palette) == ROASTTY_SUCCESS);
  assert(memcmp(got_palette,
                initial_default_palette,
                sizeof(got_palette)) == 0);

  roastty_mode_tag_t ansi_insert = ROASTTY_MODE_TAG_ANSI_BIT | 4;
  roastty_mode_tag_t dec_wraparound = 7;
  roastty_mode_tag_t dec_alt_screen = 1049;
  roastty_mode_tag_t invalid_ansi_mouse = ROASTTY_MODE_TAG_ANSI_BIT | 9;
  roastty_mode_tag_t unknown_dec = 9999;
  bool mode_value = true;
  assert(roastty_terminal_mode_get(NULL, ansi_insert, &mode_value) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_mode_get(terminal, ansi_insert, NULL) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_mode_get(terminal, invalid_ansi_mouse, &mode_value) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_mode_get(terminal, unknown_dec, &mode_value) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_mode_set(NULL, ansi_insert, true) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_mode_set(terminal, invalid_ansi_mouse, true) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_mode_set(terminal, unknown_dec, true) ==
         ROASTTY_INVALID_VALUE);

  assert(roastty_terminal_mode_get(terminal, ansi_insert, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(!mode_value);
  assert(roastty_terminal_mode_set(terminal, ansi_insert, true) ==
         ROASTTY_SUCCESS);
  assert(roastty_terminal_mode_get(terminal, ansi_insert, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(mode_value);
  assert(roastty_terminal_mode_set(terminal, ansi_insert, false) ==
         ROASTTY_SUCCESS);
  assert(roastty_terminal_mode_get(terminal, ansi_insert, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(!mode_value);

  assert(roastty_terminal_mode_get(terminal, dec_wraparound, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(mode_value);
  assert(roastty_terminal_mode_set(terminal, dec_wraparound, false) ==
         ROASTTY_SUCCESS);
  assert(roastty_terminal_mode_get(terminal, dec_wraparound, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(!mode_value);
  assert(roastty_terminal_mode_set(terminal, dec_wraparound, true) ==
         ROASTTY_SUCCESS);

  assert(roastty_terminal_mode_set(terminal, dec_alt_screen, true) ==
         ROASTTY_SUCCESS);
  assert(roastty_terminal_mode_get(terminal, dec_alt_screen, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(mode_value);
  active_screen = ROASTTY_TERMINAL_SCREEN_ALTERNATE;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                              &active_screen) == ROASTTY_SUCCESS);
  assert(active_screen == ROASTTY_TERMINAL_SCREEN_PRIMARY);
  assert(roastty_terminal_mode_set(terminal, dec_alt_screen, false) ==
         ROASTTY_SUCCESS);

  roastty_mode_tag_t mouse_modes[] = {9, 1000, 1002, 1003};
  for (size_t i = 0; i < sizeof(mouse_modes) / sizeof(mouse_modes[0]); i++) {
    assert(roastty_terminal_mode_set(terminal, mouse_modes[i], true) ==
           ROASTTY_SUCCESS);
    mouse_tracking = false;
    assert(roastty_terminal_get(terminal,
                                ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                                &mouse_tracking) == ROASTTY_SUCCESS);
    assert(mouse_tracking);
    assert(roastty_terminal_mode_set(terminal, mouse_modes[i], false) ==
           ROASTTY_SUCCESS);
  }
  mouse_tracking = true;
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                              &mouse_tracking) == ROASTTY_SUCCESS);
  assert(!mouse_tracking);

  terminal_write(terminal, "\x1b]0;reset me\x07\x1b[?1049hALT\x1b[?1000h");
  roastty_terminal_reset(NULL);
  roastty_terminal_reset(terminal);
  assert(roastty_terminal_get(terminal, ROASTTY_TERMINAL_DATA_COLS, &cols) ==
         ROASTTY_SUCCESS);
  assert(cols == 10);
  assert(roastty_terminal_get(terminal, ROASTTY_TERMINAL_DATA_ROWS, &rows) ==
         ROASTTY_SUCCESS);
  assert(rows == 4);
  assert(roastty_terminal_mode_get(terminal, ansi_insert, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(!mode_value);
  assert(roastty_terminal_mode_get(terminal, dec_wraparound, &mode_value) ==
         ROASTTY_SUCCESS);
  assert(mode_value);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_ACTIVE_SCREEN,
                              &active_screen) == ROASTTY_SUCCESS);
  assert(active_screen == ROASTTY_TERMINAL_SCREEN_PRIMARY);
  assert(roastty_terminal_get(terminal,
                              ROASTTY_TERMINAL_DATA_MOUSE_TRACKING,
                              &mouse_tracking) == ROASTTY_SUCCESS);
  assert(!mouse_tracking);
  assert(roastty_terminal_read_screen_plain(terminal, false, &plain) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(plain, "");
  terminal_write(terminal, "abc");

  roastty_terminal_data_e keys[] = {
      ROASTTY_TERMINAL_DATA_COLS,
      ROASTTY_TERMINAL_DATA_ROWS,
      ROASTTY_TERMINAL_DATA_CURSOR_X,
  };
  void *values[] = {&cols, &rows, &column};
  size_t written = 999;
  assert(roastty_terminal_get_multi(terminal, 3, keys, values, &written) ==
         ROASTTY_SUCCESS);
  assert(written == 3);
  assert(cols == 10);
  assert(rows == 4);
  assert(column == 3);
  assert(roastty_terminal_get_multi(terminal, 0, keys, values, &written) ==
         ROASTTY_SUCCESS);
  assert(written == 0);
  roastty_terminal_data_e deferred_keys[] = {
      ROASTTY_TERMINAL_DATA_COLS,
      ROASTTY_TERMINAL_DATA_TITLE,
      ROASTTY_TERMINAL_DATA_ROWS,
  };
  void *deferred_values[] = {&cols, &total_rows, &rows};
  written = 999;
  assert(roastty_terminal_get_multi(terminal,
                                    3,
                                    deferred_keys,
                                    deferred_values,
                                    &written) == ROASTTY_NO_VALUE);
  assert(written == 1);
  roastty_terminal_data_e invalid_keys[] = {
      ROASTTY_TERMINAL_DATA_COLS,
      (roastty_terminal_data_e)33,
  };
  void *invalid_values[] = {&cols, &rows};
  written = 999;
  assert(roastty_terminal_get_multi(terminal,
                                    2,
                                    invalid_keys,
                                    invalid_values,
                                    &written) == ROASTTY_INVALID_VALUE);
  assert(written == 1);
  void *null_values[] = {&cols, NULL};
  written = 999;
  assert(roastty_terminal_get_multi(terminal,
                                    2,
                                    keys,
                                    null_values,
                                    &written) == ROASTTY_INVALID_VALUE);
  assert(written == 1);
  assert(roastty_terminal_get_multi(NULL, 1, keys, values, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_get_multi(terminal, 1, NULL, values, &written) ==
         ROASTTY_INVALID_VALUE);
  assert(roastty_terminal_get_multi(terminal, 1, keys, NULL, &written) ==
         ROASTTY_INVALID_VALUE);

  roastty_string_s response = {0};
  terminal_write(
      terminal,
      "\x1b_Ga=T,f=32,s=1,v=1,i=7,p=4,c=3,r=2,z=1,C=1;AQIDBA==\x1b\\");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1b_Gi=7,p=4;OK\x1b\\");

  roastty_kitty_graphics_image_t image =
      roastty_kitty_graphics_image(graphics, 7);
  assert(image != NULL);
  uint32_t image_id = 0;
  uint32_t image_number = 99;
  uint32_t image_width = 0;
  uint32_t image_height = 0;
  roastty_kitty_image_format_e image_format =
      ROASTTY_KITTY_IMAGE_FORMAT_RGB;
  roastty_kitty_image_compression_e image_compression =
      ROASTTY_KITTY_IMAGE_COMPRESSION_ZLIB_DEFLATE;
  const uint8_t *image_data = NULL;
  size_t image_data_len = 0;
  roastty_kitty_graphics_image_data_e image_keys[] = {
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_ID,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_NUMBER,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_WIDTH,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_HEIGHT,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_FORMAT,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_COMPRESSION,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_PTR,
      ROASTTY_KITTY_GRAPHICS_IMAGE_DATA_DATA_LEN,
  };
  void *image_values[] = {
      &image_id,
      &image_number,
      &image_width,
      &image_height,
      &image_format,
      &image_compression,
      &image_data,
      &image_data_len,
  };
  written = 99;
  assert(roastty_kitty_graphics_image_get_multi(image,
                                                8,
                                                image_keys,
                                                image_values,
                                                &written) == ROASTTY_SUCCESS);
  assert(written == 8);
  assert(image_id == 7);
  assert(image_number == 0);
  assert(image_width == 1);
  assert(image_height == 1);
  assert(image_format == ROASTTY_KITTY_IMAGE_FORMAT_RGBA);
  assert(image_compression == ROASTTY_KITTY_IMAGE_COMPRESSION_NONE);
  assert(image_data_len == 4);
  assert(image_data != NULL);
  assert(memcmp(image_data, "\x01\x02\x03\x04", 4) == 0);

  roastty_kitty_graphics_placement_iterator_t placement_iterator = NULL;
  assert(roastty_kitty_graphics_placement_iterator_new(&placement_iterator) ==
         ROASTTY_SUCCESS);
  assert(placement_iterator != NULL);
  assert(roastty_kitty_graphics_get(
             graphics,
             ROASTTY_KITTY_GRAPHICS_DATA_PLACEMENT_ITERATOR,
             &placement_iterator) == ROASTTY_SUCCESS);
  roastty_kitty_placement_layer_e layer =
      ROASTTY_KITTY_PLACEMENT_LAYER_ABOVE_TEXT;
  assert(roastty_kitty_graphics_placement_iterator_set(
             placement_iterator,
             ROASTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_LAYER,
             &layer) == ROASTTY_SUCCESS);
  assert(roastty_kitty_graphics_placement_next(placement_iterator));
  uint32_t placement_image_id = 0;
  uint32_t placement_id = 0;
  bool placement_virtual = true;
  uint32_t placement_columns = 0;
  uint32_t placement_rows = 0;
  int32_t placement_z = 0;
  roastty_kitty_graphics_placement_data_e placement_keys[] = {
      ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IMAGE_ID,
      ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_PLACEMENT_ID,
      ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_IS_VIRTUAL,
      ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_COLUMNS,
      ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_ROWS,
      ROASTTY_KITTY_GRAPHICS_PLACEMENT_DATA_Z,
  };
  void *placement_values[] = {
      &placement_image_id,
      &placement_id,
      &placement_virtual,
      &placement_columns,
      &placement_rows,
      &placement_z,
  };
  written = 99;
  assert(roastty_kitty_graphics_placement_get_multi(placement_iterator,
                                                    6,
                                                    placement_keys,
                                                    placement_values,
                                                    &written) == ROASTTY_SUCCESS);
  assert(written == 6);
  assert(placement_image_id == 7);
  assert(placement_id == 4);
  assert(!placement_virtual);
  assert(placement_columns == 3);
  assert(placement_rows == 2);
  assert(placement_z == 1);

  assert(sizeof(roastty_kitty_graphics_placement_render_info_s) == 56);
  assert(_Alignof(roastty_kitty_graphics_placement_render_info_s) == 8);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s, size) == 0);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  pixel_width) == 8);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  pixel_height) == 12);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  grid_cols) == 16);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  grid_rows) == 20);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  viewport_col) == 24);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  viewport_row) == 28);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  viewport_visible) == 32);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  source_x) == 36);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  source_y) == 40);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  source_width) == 44);
  assert(offsetof(roastty_kitty_graphics_placement_render_info_s,
                  source_height) == 48);

  roastty_selection_s placement_rect = {.size = sizeof(placement_rect)};
  assert(roastty_kitty_graphics_placement_rect(placement_iterator,
                                               image,
                                               terminal,
                                               &placement_rect) ==
         ROASTTY_SUCCESS);
  assert(placement_rect.rectangle);
  assert(placement_rect.start.x == 3);
  assert(placement_rect.start.y == 0);
  assert(placement_rect.end.x == 5);
  assert(placement_rect.end.y == 1);

  uint32_t placement_pixel_width = 0;
  uint32_t placement_pixel_height = 0;
  assert(roastty_kitty_graphics_placement_pixel_size(
             placement_iterator,
             image,
             terminal,
             &placement_pixel_width,
             &placement_pixel_height) == ROASTTY_SUCCESS);
  assert(placement_pixel_width == 3);
  assert(placement_pixel_height == 2);

  uint32_t placement_grid_cols = 0;
  uint32_t placement_grid_rows = 0;
  assert(roastty_kitty_graphics_placement_grid_size(placement_iterator,
                                                    image,
                                                    terminal,
                                                    &placement_grid_cols,
                                                    &placement_grid_rows) ==
         ROASTTY_SUCCESS);
  assert(placement_grid_cols == 3);
  assert(placement_grid_rows == 2);

  uint32_t placement_source_x = 99;
  uint32_t placement_source_y = 99;
  uint32_t placement_source_width = 99;
  uint32_t placement_source_height = 99;
  assert(roastty_kitty_graphics_placement_source_rect(
             placement_iterator,
             image,
             &placement_source_x,
             &placement_source_y,
             &placement_source_width,
             &placement_source_height) == ROASTTY_SUCCESS);
  assert(placement_source_x == 0);
  assert(placement_source_y == 0);
  assert(placement_source_width == 1);
  assert(placement_source_height == 1);

  int32_t placement_viewport_col = -99;
  int32_t placement_viewport_row = -99;
  assert(roastty_kitty_graphics_placement_viewport_pos(placement_iterator,
                                                       image,
                                                       terminal,
                                                       &placement_viewport_col,
                                                       &placement_viewport_row) ==
         ROASTTY_SUCCESS);
  assert(placement_viewport_col == 3);
  assert(placement_viewport_row == 0);

  roastty_kitty_graphics_placement_render_info_s placement_info = {
      .size = sizeof(placement_info)};
  assert(roastty_kitty_graphics_placement_render_info(placement_iterator,
                                                      image,
                                                      terminal,
                                                      &placement_info) ==
         ROASTTY_SUCCESS);
  assert(placement_info.size == sizeof(placement_info));
  assert(placement_info.pixel_width == placement_pixel_width);
  assert(placement_info.pixel_height == placement_pixel_height);
  assert(placement_info.grid_cols == placement_grid_cols);
  assert(placement_info.grid_rows == placement_grid_rows);
  assert(placement_info.viewport_col == placement_viewport_col);
  assert(placement_info.viewport_row == placement_viewport_row);
  assert(placement_info.viewport_visible);
  assert(placement_info.source_x == placement_source_x);
  assert(placement_info.source_y == placement_source_y);
  assert(placement_info.source_width == placement_source_width);
  assert(placement_info.source_height == placement_source_height);

  roastty_kitty_graphics_placement_render_info_s undersized_info = {
      .size = sizeof(undersized_info) - 1, .pixel_width = 123};
  assert(roastty_kitty_graphics_placement_render_info(placement_iterator,
                                                      image,
                                                      terminal,
                                                      &undersized_info) ==
         ROASTTY_INVALID_VALUE);
  assert(undersized_info.pixel_width == 123);

  assert(!roastty_kitty_graphics_placement_next(placement_iterator));
  roastty_kitty_graphics_placement_iterator_free(placement_iterator);

  assert(sizeof(roastty_kitty_graphics_render_placement_info_s) == 80);
  assert(_Alignof(roastty_kitty_graphics_render_placement_info_s) == 8);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s, size) == 0);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s, image_id) ==
         8);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s,
                  placement_id) == 12);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s,
                  is_virtual) == 16);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s, x_offset) ==
         20);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s, y_offset) ==
         24);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s,
                  pixel_width) == 28);
  assert(offsetof(roastty_kitty_graphics_render_placement_info_s, z) == 72);
  assert(ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_IMAGE_ID == 1);
  assert(ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_X_OFFSET == 14);
  assert(ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_Z == 16);

  roastty_kitty_graphics_render_placement_iterator_t render_iterator = NULL;
  assert(roastty_kitty_graphics_render_placement_iterator_new(
             &render_iterator) == ROASTTY_SUCCESS);
  assert(render_iterator != NULL);
  assert(roastty_kitty_graphics_render_placement_iterator_update(
             render_iterator,
             terminal) == ROASTTY_SUCCESS);
  assert(roastty_kitty_graphics_render_placement_next(render_iterator));

  uint32_t render_image_id = 0;
  uint32_t render_placement_id = 0;
  bool render_virtual = true;
  uint32_t render_x_offset = 0;
  uint32_t render_y_offset = 0;
  roastty_kitty_graphics_render_placement_data_e render_keys[] = {
      ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_IMAGE_ID,
      ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_PLACEMENT_ID,
      ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_IS_VIRTUAL,
      ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_X_OFFSET,
      ROASTTY_KITTY_GRAPHICS_RENDER_PLACEMENT_DATA_Y_OFFSET,
  };
  void *render_values[] = {
      &render_image_id,
      &render_placement_id,
      &render_virtual,
      &render_x_offset,
      &render_y_offset,
  };
  written = 99;
  assert(roastty_kitty_graphics_render_placement_get_multi(
             render_iterator,
             5,
             render_keys,
             render_values,
             &written) == ROASTTY_SUCCESS);
  assert(written == 5);
  assert(render_image_id == 7);
  assert(render_placement_id == 4);
  assert(!render_virtual);
  assert(render_x_offset == 0);
  assert(render_y_offset == 0);

  roastty_kitty_graphics_render_placement_info_s render_info = {
      .size = sizeof(render_info)};
  assert(roastty_kitty_graphics_render_placement_render_info(
             render_iterator,
             &render_info) == ROASTTY_SUCCESS);
  assert(render_info.size == sizeof(render_info));
  assert(render_info.image_id == 7);
  assert(render_info.placement_id == 4);
  assert(!render_info.is_virtual);
  assert(render_info.pixel_width == placement_pixel_width);
  assert(render_info.pixel_height == placement_pixel_height);
  assert(render_info.grid_cols == placement_grid_cols);
  assert(render_info.grid_rows == placement_grid_rows);
  assert(render_info.viewport_col == placement_viewport_col);
  assert(render_info.viewport_row == placement_viewport_row);
  assert(render_info.viewport_visible);
  assert(render_info.source_x == placement_source_x);
  assert(render_info.source_y == placement_source_y);
  assert(render_info.source_width == placement_source_width);
  assert(render_info.source_height == placement_source_height);

  roastty_kitty_graphics_image_t render_image =
      roastty_kitty_graphics_render_placement_image(render_iterator);
  assert(render_image != NULL);
  roastty_kitty_graphics_image_free(render_image);

  roastty_kitty_graphics_render_placement_info_s undersized_render_info = {
      .size = sizeof(undersized_render_info) - 1, .image_id = 123};
  assert(roastty_kitty_graphics_render_placement_render_info(
             render_iterator,
             &undersized_render_info) == ROASTTY_INVALID_VALUE);
  assert(undersized_render_info.image_id == 123);

  assert(!roastty_kitty_graphics_render_placement_next(render_iterator));
  roastty_kitty_graphics_render_placement_iterator_free(render_iterator);
  roastty_kitty_graphics_image_free(image);

  uint8_t utf8_a = 0xc3;
  uint8_t utf8_b = 0xa9;
  assert(roastty_terminal_vt_write(terminal, &utf8_a, 1) == ROASTTY_SUCCESS);
  assert(roastty_terminal_vt_write(terminal, &utf8_b, 1) == ROASTTY_SUCCESS);

  terminal_write(terminal, "\x1b]0;from ");
  terminal_write(terminal, "c\x07");
  title = (roastty_string_s){0};
  assert(roastty_terminal_title(terminal, &title) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(title, "from c");

  terminal_write(terminal, "\x1b]1337;CurrentDir=file://host/");
  terminal_write(terminal, "c\x07");
  pwd = (roastty_string_s){0};
  assert(roastty_terminal_pwd(terminal, &pwd) == ROASTTY_SUCCESS);
  assert_roastty_string_eq(pwd, "file://host/c");

  terminal_write(terminal, "\x1b[");
  terminal_write(terminal, "6n");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1b[1;5R");
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "");

  reset_effect_state();
  void *effect_user = (void *)0x1234;
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_USERDATA,
                              effect_user) == ROASTTY_SUCCESS);
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_WRITE_PTY,
             (const void *)(roastty_terminal_write_pty_cb)terminal_write_pty_cb) ==
         ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[6n");
  assert(effect_terminal == terminal);
  assert(effect_userdata == effect_user);
  assert(effect_write_count == 1);
  assert(effect_write_len == strlen("\x1b[1;5R"));
  assert(memcmp(effect_write_bytes, "\x1b[1;5R", effect_write_len) == 0);
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1b[1;5R");
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_WRITE_PTY,
                              NULL) == ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[6n");
  assert(effect_write_count == 1);
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1b[1;5R");

  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_BELL,
             (const void *)(roastty_terminal_bell_cb)terminal_bell_cb) ==
         ROASTTY_SUCCESS);
  terminal_write(terminal, "\x07");
  assert(effect_bell_count == 1);
  assert(effect_terminal == terminal);
  assert(effect_userdata == effect_user);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_BELL,
                              NULL) == ROASTTY_SUCCESS);
  terminal_write(terminal, "\x07");
  assert(effect_bell_count == 1);

  const char enquiry[] = "CENQ";
  effect_enquiry = enquiry;
  effect_enquiry_len = strlen(enquiry);
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_ENQUIRY,
             (const void *)(roastty_terminal_enquiry_cb)terminal_enquiry_cb) ==
         ROASTTY_SUCCESS);
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_WRITE_PTY,
             (const void *)(roastty_terminal_write_pty_cb)terminal_write_pty_cb) ==
         ROASTTY_SUCCESS);
  effect_write_count = 0;
  terminal_write(terminal, "\x05");
  assert(effect_write_count == 1);
  assert(effect_write_len == strlen(enquiry));
  assert(memcmp(effect_write_bytes, enquiry, effect_write_len) == 0);
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, enquiry);
  char long_enquiry[256];
  memset(long_enquiry, 'x', sizeof(long_enquiry));
  effect_enquiry = long_enquiry;
  effect_enquiry_len = sizeof(long_enquiry);
  terminal_write(terminal, "\x05");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "");

  const char xtversion[] = "roastty-c";
  effect_xtversion = xtversion;
  effect_xtversion_len = strlen(xtversion);
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_XTVERSION,
             (const void *)(roastty_terminal_xtversion_cb)terminal_xtversion_cb) ==
         ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[>0q");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1bP>|roastty-c\x1b\\");
  effect_xtversion = NULL;
  effect_xtversion_len = 1;
  terminal_write(terminal, "\x1b[>0q");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1bP>|libroastty\x1b\\");

  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_TITLE_CHANGED,
             (const void *)(roastty_terminal_title_changed_cb)
                 terminal_title_changed_cb) == ROASTTY_SUCCESS);
  effect_title_changed_count = 0;
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_TITLE,
                              &title_input) == ROASTTY_SUCCESS);
  assert(effect_title_changed_count == 0);
  terminal_write(terminal, "\x1b]2;from callback\x07");
  assert(effect_title_changed_count == 1);
  assert(effect_terminal == terminal);
  assert(effect_userdata == effect_user);
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_TITLE_CHANGED,
                              NULL) == ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b]2;from callback 2\x07");
  assert(effect_title_changed_count == 1);

  effect_size = report_size;
  effect_size_result = true;
  effect_write_count = 0;
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_SIZE_CB,
             (const void *)(roastty_terminal_size_cb)terminal_size_cb) ==
         ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[14t\x1b[16t\x1b[18t");
  assert(effect_size_count == 3);
  assert(effect_terminal == terminal);
  assert(effect_userdata == effect_user);
  assert(effect_write_count == 3);
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response,
                           "\x1b[4;432;720t\x1b[6;18;9t\x1b[8;24;80t");
  effect_size_result = false;
  terminal_write(terminal, "\x1b[14t");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "");
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_SIZE_CB,
                              NULL) == ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[14t");
  assert(effect_size_count == 4);

  effect_color_scheme = ROASTTY_COLOR_SCHEME_DARK;
  effect_color_scheme_result = true;
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_COLOR_SCHEME,
             (const void *)(roastty_terminal_color_scheme_cb)
                 terminal_color_scheme_cb) == ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[?996n");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1b[?997;1n");
  effect_color_scheme = ROASTTY_COLOR_SCHEME_LIGHT;
  terminal_write(terminal, "\x1b[?996n");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "\x1b[?997;2n");
  effect_color_scheme = (roastty_color_scheme_e)99;
  terminal_write(terminal, "\x1b[?996n");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "");
  effect_color_scheme_result = false;
  terminal_write(terminal, "\x1b[?996n");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response, "");
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_COLOR_SCHEME,
                              NULL) == ROASTTY_SUCCESS);

  effect_device_attributes.primary.conformance_level = 777;
  effect_device_attributes.primary.features[0] = 444;
  effect_device_attributes.primary.features[1] = 555;
  effect_device_attributes.primary.num_features = 2;
  effect_device_attributes.secondary.device_type = 888;
  effect_device_attributes.secondary.firmware_version = 99;
  effect_device_attributes.secondary.rom_cartridge = 7;
  effect_device_attributes.tertiary.unit_id = 0xAABBCCDD;
  effect_device_attributes_result = true;
  assert(roastty_terminal_set(
             terminal,
             ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES,
             (const void *)(roastty_terminal_device_attributes_cb)
                 terminal_device_attributes_cb) == ROASTTY_SUCCESS);
  terminal_write(terminal, "\x1b[c\x1b[>c\x1b[=c");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response,
                           "\x1b[?777;444;555c\x1b[>888;99;7c"
                           "\x1bP!|AABBCCDD\x1b\\");
  assert(effect_device_attributes_count == 3);
  effect_device_attributes_result = false;
  terminal_write(terminal, "\x1b[c\x1b[>c\x1b[=c");
  response = (roastty_string_s){0};
  assert(roastty_terminal_take_pty_response(terminal, &response) ==
         ROASTTY_SUCCESS);
  assert_roastty_string_eq(response,
                           "\x1b[?62;22c\x1b[>1;0;0c"
                           "\x1bP!|00000000\x1b\\");
  assert(roastty_terminal_set(terminal,
                              ROASTTY_TERMINAL_OPTION_DEVICE_ATTRIBUTES,
                              NULL) == ROASTTY_SUCCESS);

  assert(roastty_terminal_title(NULL, &title) == ROASTTY_INVALID_VALUE);
  assert(title.ptr == NULL);
  assert(title.len == 0);
  assert(!title.sentinel);
  assert(roastty_terminal_title(terminal, NULL) == ROASTTY_INVALID_VALUE);

  roastty_terminal_free(terminal);
}

int main(int argc, char **argv) {
  assert(roastty_init((uintptr_t)argc, argv) == ROASTTY_SUCCESS);
  assert(ROASTTY_ACTION_NEW_TAB == 2);
  assert(ROASTTY_ACTION_CLOSE_TAB == 3);
  assert(ROASTTY_ACTION_TOGGLE_MAXIMIZE == 6);
  assert(ROASTTY_ACTION_TOGGLE_FULLSCREEN == 7);
  assert(ROASTTY_ACTION_TOGGLE_TAB_OVERVIEW == 8);
  assert(ROASTTY_ACTION_TOGGLE_WINDOW_DECORATIONS == 9);
  assert(ROASTTY_ACTION_TOGGLE_COMMAND_PALETTE == 11);
  assert(ROASTTY_ACTION_TOGGLE_BACKGROUND_OPACITY == 13);
  assert(ROASTTY_ACTION_MOVE_TAB == 14);
  assert(ROASTTY_ACTION_GOTO_TAB == 15);
  assert(ROASTTY_ACTION_GOTO_WINDOW == 17);
  assert(ROASTTY_ACTION_TOGGLE_SPLIT_ZOOM == 20);
  assert(ROASTTY_ACTION_RESET_WINDOW_SIZE == 23);
  assert(ROASTTY_ACTION_INSPECTOR == 28);
  assert(ROASTTY_ACTION_SET_TITLE == 32);
  assert(ROASTTY_ACTION_SET_TAB_TITLE == 33);
  assert(ROASTTY_ACTION_PROMPT_TITLE == 34);
  assert(ROASTTY_ACTION_FLOAT_WINDOW == 42);
  assert(ROASTTY_ACTION_SECURE_INPUT == 43);
  assert(ROASTTY_ACTION_CLOSE_WINDOW == 49);
  assert(ROASTTY_ACTION_UNDO == 51);
  assert(ROASTTY_ACTION_REDO == 52);
  assert(ROASTTY_ACTION_SHOW_ON_SCREEN_KEYBOARD == 57);
  assert(ROASTTY_INSPECTOR_TOGGLE == 0);
  assert(ROASTTY_INSPECTOR_SHOW == 1);
  assert(ROASTTY_INSPECTOR_HIDE == 2);
  assert(ROASTTY_FLOAT_WINDOW_ON == 0);
  assert(ROASTTY_FLOAT_WINDOW_OFF == 1);
  assert(ROASTTY_FLOAT_WINDOW_TOGGLE == 2);
  assert(ROASTTY_SECURE_INPUT_ON == 0);
  assert(ROASTTY_SECURE_INPUT_OFF == 1);
  assert(ROASTTY_SECURE_INPUT_TOGGLE == 2);
  assert(ROASTTY_CLOSE_TAB_THIS == 0);
  assert(ROASTTY_CLOSE_TAB_OTHER == 1);
  assert(ROASTTY_CLOSE_TAB_RIGHT == 2);
  assert(ROASTTY_GOTO_WINDOW_PREVIOUS == 0);
  assert(ROASTTY_GOTO_WINDOW_NEXT == 1);
  assert(ROASTTY_GOTO_TAB_PREVIOUS == -1);
  assert(ROASTTY_GOTO_TAB_NEXT == -2);
  assert(ROASTTY_GOTO_TAB_LAST == -3);
  assert(ROASTTY_FULLSCREEN_NATIVE == 0);
  assert(ROASTTY_FULLSCREEN_MACOS_NON_NATIVE == 1);
  assert(ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_VISIBLE_MENU == 2);
  assert(ROASTTY_FULLSCREEN_MACOS_NON_NATIVE_PADDED_NOTCH == 3);
  assert(ROASTTY_PROMPT_TITLE_SURFACE == 0);
  assert(ROASTTY_PROMPT_TITLE_TAB == 1);
  assert(ROASTTY_CLIPBOARD_REQUEST_PASTE == 0);
  assert(ROASTTY_CLIPBOARD_REQUEST_OSC_52_READ == 1);
  assert(ROASTTY_CLIPBOARD_REQUEST_OSC_52_WRITE == 2);

  roastty_config_free(NULL);
  roastty_app_free(NULL);
  roastty_surface_free(NULL);
  roastty_config_load_cli_args(NULL);
  roastty_config_load_default_files(NULL);
  roastty_config_load_recursive_files(NULL);
  roastty_config_load_file(NULL, NULL);
  roastty_config_finalize(NULL);
  assert(roastty_config_diagnostics_count(NULL) == 0);
  assert(roastty_config_get_diagnostic(NULL, 0).message != NULL);
  roastty_input_trigger_s null_trigger =
      roastty_config_trigger(NULL, "new_window", 10);
  assert(null_trigger.tag == ROASTTY_TRIGGER_PHYSICAL);
  assert(null_trigger.key.physical == ROASTTY_KEY_UNIDENTIFIED);
  assert(null_trigger.mods == ROASTTY_MODS_NONE);
  assert(!roastty_config_key_is_binding(NULL, NULL));
  assert(roastty_app_userdata(NULL) == NULL);
  roastty_app_tick(NULL);
  roastty_app_set_focus(NULL, true);
  roastty_app_set_color_scheme(NULL, ROASTTY_COLOR_SCHEME_DARK);
  roastty_app_update_config(NULL, NULL);
  assert(!roastty_app_needs_confirm_quit(NULL));
  assert(!roastty_app_has_global_keybinds(NULL));
  assert(roastty_surface_userdata(NULL) == NULL);
  assert(roastty_surface_app(NULL) == NULL);
  assert(roastty_surface_key_translation_mods(
             NULL,
             (roastty_input_mods_e)(ROASTTY_MODS_ALT | ROASTTY_MODS_ALT_RIGHT |
                                    (1 << 20))) ==
         (ROASTTY_MODS_ALT | ROASTTY_MODS_ALT_RIGHT));
  roastty_surface_config_s null_inherited =
      roastty_surface_inherited_config(NULL, ROASTTY_SURFACE_CONTEXT_SPLIT);
  assert(null_inherited.context == ROASTTY_SURFACE_CONTEXT_SPLIT);
  assert(null_inherited.userdata == NULL);
  assert(null_inherited.working_directory == NULL);
  assert(null_inherited.command == NULL);
  assert(null_inherited.env_vars == NULL);
  assert(null_inherited.env_var_count == 0);
  assert(null_inherited.initial_input == NULL);
  roastty_surface_update_config(NULL, NULL);
  assert(!roastty_surface_needs_confirm_quit(NULL));
  assert(!roastty_surface_process_exited(NULL));
  roastty_surface_complete_clipboard_request(NULL, NULL, NULL, false);
  roastty_surface_refresh(NULL);
  roastty_surface_set_content_scale(NULL, 1.0, 1.0);
  roastty_surface_set_display_id(NULL, 42);
  roastty_surface_set_focus(NULL, true);
  roastty_surface_set_occlusion(NULL, true);
  roastty_surface_set_color_scheme(NULL, ROASTTY_COLOR_SCHEME_DARK);
  roastty_surface_set_size(NULL, 1, 1);
  roastty_surface_text(NULL, "hello", 5);
  roastty_surface_preedit(NULL, "pre", 3);
  roastty_surface_split(NULL, ROASTTY_SPLIT_DIRECTION_RIGHT);
  roastty_surface_split_focus(NULL, ROASTTY_GOTO_SPLIT_NEXT);
  roastty_surface_split_resize(NULL, ROASTTY_RESIZE_SPLIT_UP, 10);
  roastty_surface_split_equalize(NULL);
  assert(!roastty_surface_binding_action(NULL, "new_split:right", 15));
  double ime_x = 1.0;
  double ime_y = 2.0;
  double ime_width = 3.0;
  double ime_height = 4.0;
  roastty_surface_ime_point(NULL, &ime_x, &ime_y, &ime_width, &ime_height);
  assert(ime_x == 0.0);
  assert(ime_y == 0.0);
  assert(ime_width == 0.0);
  assert(ime_height == 0.0);
  roastty_surface_ime_point(NULL, NULL, NULL, NULL, NULL);
  roastty_keybind_flags_t binding_flags = 0xff;
  assert(!roastty_surface_key(NULL, NULL));
  assert(!roastty_surface_key_is_binding(NULL, NULL, &binding_flags));
  assert(binding_flags == 0);
  assert(!roastty_surface_key_is_binding(NULL, NULL, NULL));
  roastty_text_s null_read_text = {0};
  roastty_selection_s null_selection = {0};
  null_selection.size = sizeof(roastty_selection_s);
  assert(!roastty_surface_mouse_captured(NULL));
  assert(!roastty_surface_mouse_button(
      NULL, ROASTTY_MOUSE_BUTTON_PRESS, ROASTTY_MOUSE_BUTTON_LEFT,
      ROASTTY_MODS_SHIFT));
  roastty_surface_mouse_pos(NULL, 1.0, 2.0, ROASTTY_MODS_SHIFT);
  roastty_surface_mouse_scroll(NULL, 1.0, 2.0,
                               (roastty_input_scroll_mods_t)0x1ff);
  roastty_surface_mouse_pressure(NULL, 1, 0.5);
  assert(!roastty_surface_has_selection(NULL));
  assert(!roastty_surface_read_selection(NULL, &null_read_text));
  assert(null_read_text.tl_px_x == -1.0);
  assert(null_read_text.tl_px_y == -1.0);
  assert(null_read_text.offset_start == 0);
  assert(null_read_text.offset_len == 0);
  assert(null_read_text.text == NULL);
  assert(null_read_text.text_len == 0);
  assert(!roastty_surface_read_selection(NULL, NULL));
  assert(!roastty_surface_read_text(NULL, null_selection, &null_read_text));
  assert(null_read_text.tl_px_x == -1.0);
  assert(null_read_text.tl_px_y == -1.0);
  assert(null_read_text.offset_start == 0);
  assert(null_read_text.offset_len == 0);
  assert(null_read_text.text == NULL);
  assert(null_read_text.text_len == 0);
  assert(!roastty_surface_read_text(NULL, null_selection, NULL));
  assert(roastty_surface_quicklook_font(NULL) == NULL);
  assert(!roastty_surface_quicklook_word(NULL, &null_read_text));
  assert(null_read_text.tl_px_x == -1.0);
  assert(null_read_text.tl_px_y == -1.0);
  assert(null_read_text.offset_start == 0);
  assert(null_read_text.offset_len == 0);
  assert(null_read_text.text == NULL);
  assert(null_read_text.text_len == 0);
  assert(!roastty_surface_quicklook_word(NULL, NULL));
  roastty_surface_free_text(NULL, &null_read_text);
  roastty_surface_free_text(NULL, NULL);
  roastty_surface_size_s null_size = roastty_surface_size(NULL);
  assert(null_size.width_px == 0);
  assert(null_size.height_px == 0);
  assert(null_size.columns == 0);
  assert(null_size.rows == 0);
  assert(null_size.cell_width_px == 0);
  assert(null_size.cell_height_px == 0);
  assert(roastty_surface_foreground_pid(NULL) == 0);
  roastty_surface_request_close(NULL);
  assert_mouse_event_abi();
  assert_mouse_encoder_abi();
  assert_key_event_and_encoder_abi();
  assert_osc_parser_abi();
  assert_style_abi();
  assert_row_cell_abi();
  assert_support_abi();
  assert_render_state_abi();
  assert_terminal_abi();

  roastty_info_s info = roastty_info();
  assert(info.version != NULL);
  assert(info.version_len > 0);

  roastty_config_t config = roastty_config_new();
  assert(config != NULL);
  roastty_config_load_cli_args(config);
  roastty_config_load_default_files(config);
  roastty_config_load_recursive_files(config);
  roastty_config_load_file(config, "/tmp/nonexistent-roastty-config");
  roastty_config_finalize(config);
  assert(roastty_config_diagnostics_count(config) == 0);
  roastty_diagnostic_s diagnostic = roastty_config_get_diagnostic(config, 0);
  assert(diagnostic.message != NULL);

  bool bool_value = false;
  assert(!roastty_config_get(NULL,
                             &bool_value,
                             "initial-window",
                             strlen("initial-window")));
  assert(!roastty_config_get(config,
                             NULL,
                             "initial-window",
                             strlen("initial-window")));
  assert(!roastty_config_get(config, &bool_value, NULL, strlen("initial-window")));
  assert(!roastty_config_get(config,
                             &bool_value,
                             "not-a-real-key",
                             strlen("not-a-real-key")));

  assert_config_bool(config, "initial-window", true);
  assert_config_bool(config, "quit-after-last-window-closed", false);
  assert_config_string(config, "window-save-state", "default");
  assert_config_string(config, "window-decoration", "auto");
  assert_config_string(config, "window-theme", "auto");
  assert_config_double(config, "background-opacity", 1.0);
  assert_config_double(config, "bell-audio-volume", 0.5);
  assert_config_uintptr(config, "notify-on-command-finish-after", 5000);

  int16_t optional_position = 123;
  assert(!roastty_config_get(config,
                             &optional_position,
                             "window-position-x",
                             strlen("window-position-x")));
  assert(optional_position == 123);
  assert(!roastty_config_get(config,
                             &optional_position,
                             "window-position-y",
                             strlen("window-position-y")));
  assert(optional_position == 123);

  const char *nullable_title = (const char *)0x1;
  assert(roastty_config_get(config, &nullable_title, "title", strlen("title")));
  assert(nullable_title == NULL);

  roastty_config_path_s path = {
      .path = (const char *)0x1,
      .optional = true,
  };
  assert(!roastty_config_get(config,
                             &path,
                             "bell-audio-path",
                             strlen("bell-audio-path")));
  assert(path.path == (const char *)0x1);
  assert(path.optional == true);

  const char padded_key[] = "window-theme-with-extra-bytes";
  const char *theme = NULL;
  assert(roastty_config_get(config, &theme, padded_key, strlen("window-theme")));
  assert(theme != NULL);
  assert(strcmp(theme, "auto") == 0);

  roastty_config_t clone = roastty_config_clone(config);
  assert(clone != NULL);
  roastty_config_free(clone);

  roastty_string_s open_path = roastty_config_open_path();
  assert(open_path.ptr != NULL);
  assert(open_path.len == strlen("roastty-config"));
  assert(open_path.sentinel == false);
  roastty_string_free(open_path);

  uintptr_t app_userdata = 0xA991;
  roastty_runtime_config_s runtime = {
      .userdata = (void *)app_userdata,
      .supports_selection_clipboard = true,
      .wakeup_cb = wakeup_cb,
      .action_cb = action_cb,
      .read_clipboard_cb = read_clipboard_cb,
      .confirm_read_clipboard_cb = confirm_read_clipboard_cb,
      .write_clipboard_cb = write_clipboard_cb,
      .close_surface_cb = close_surface_cb,
  };

  roastty_app_t app = roastty_app_new(&runtime, config);
  assert(app != NULL);
  assert((uintptr_t)roastty_app_userdata(app) == app_userdata);
  roastty_app_tick(app);
  roastty_app_set_focus(app, true);
  roastty_app_set_color_scheme(app, ROASTTY_COLOR_SCHEME_DARK);
  roastty_app_update_config(app, config);
  roastty_input_trigger_s trigger =
      roastty_config_trigger(config, "new_window", 10);
  assert(trigger.tag == ROASTTY_TRIGGER_PHYSICAL);
  assert(trigger.key.physical == ROASTTY_KEY_UNIDENTIFIED);
  assert(trigger.mods == ROASTTY_MODS_NONE);
  assert(!roastty_config_key_is_binding(config, NULL));
  assert(!roastty_app_needs_confirm_quit(app));
  assert(!roastty_app_has_global_keybinds(app));

  uintptr_t surface_userdata = 0x5A5A;
  roastty_surface_config_s surface_config = roastty_surface_config_new();
  surface_config.userdata = (void *)surface_userdata;
  surface_config.scale_factor = 2.0;
  surface_config.context = ROASTTY_SURFACE_CONTEXT_WINDOW;

  roastty_app_t app_with_null_runtime = roastty_app_new(NULL, config);
  assert(app_with_null_runtime != NULL);
  assert(roastty_app_userdata(app_with_null_runtime) == NULL);
  roastty_app_free(app_with_null_runtime);

  roastty_surface_t surface_with_null_config = roastty_surface_new(app, NULL);
  assert(surface_with_null_config != NULL);
  assert(roastty_surface_app(surface_with_null_config) == app);
  roastty_surface_free(surface_with_null_config);

  assert(roastty_surface_new(NULL, &surface_config) == NULL);

  roastty_surface_t surface = roastty_surface_new(app, &surface_config);
  assert(surface != NULL);
  assert(roastty_surface_app(surface) == app);
  assert((uintptr_t)roastty_surface_userdata(surface) == surface_userdata);
  assert(roastty_surface_key_translation_mods(
             surface,
             (roastty_input_mods_e)(ROASTTY_MODS_SHIFT | ROASTTY_MODS_CTRL |
                                    ROASTTY_MODS_CTRL_RIGHT)) ==
         (ROASTTY_MODS_SHIFT | ROASTTY_MODS_CTRL | ROASTTY_MODS_CTRL_RIGHT));
  roastty_surface_config_s inherited =
      roastty_surface_inherited_config(surface, ROASTTY_SURFACE_CONTEXT_SPLIT);
  assert(inherited.context == ROASTTY_SURFACE_CONTEXT_SPLIT);
  assert(inherited.userdata == NULL);
  assert(inherited.working_directory == NULL);
  assert(inherited.command == NULL);
  assert(inherited.env_vars == NULL);
  assert(inherited.env_var_count == 0);
  assert(inherited.initial_input == NULL);

  roastty_surface_set_content_scale(surface, 2.0, 2.0);
  roastty_surface_set_display_id(surface, 77);
  roastty_surface_set_focus(surface, true);
  roastty_surface_set_occlusion(surface, false);
  roastty_surface_set_color_scheme(surface, ROASTTY_COLOR_SCHEME_LIGHT);
  roastty_surface_set_size(surface, 1024, 768);

  roastty_surface_size_s size = roastty_surface_size(surface);
  assert(size.width_px == 1024);
  assert(size.height_px == 768);
  assert(size.columns == 0);
  assert(size.rows == 0);
  assert(size.cell_width_px == 0);
  assert(size.cell_height_px == 0);

  assert(roastty_surface_foreground_pid(surface) == 0);
  assert(!roastty_surface_needs_confirm_quit(surface));
  assert(!roastty_surface_process_exited(surface));
  roastty_surface_complete_clipboard_request(surface, NULL, NULL, false);
  roastty_surface_complete_clipboard_request(surface, "paste", NULL, true);
  assert(!roastty_surface_needs_render(surface));
  roastty_surface_draw(surface);
  assert(roastty_surface_needs_render(surface));
  roastty_surface_text(surface, "hello", 5);
  roastty_surface_preedit(surface, "pre", 3);
  ime_x = 1.0;
  ime_y = 2.0;
  ime_width = 3.0;
  ime_height = 4.0;
  roastty_surface_ime_point(surface, &ime_x, &ime_y, &ime_width, &ime_height);
  assert(ime_x == 0.0);
  assert(ime_y == 0.0);
  assert(ime_width == 0.0);
  assert(ime_height == 0.0);
  roastty_key_event_t surface_key_event = NULL;
  assert(roastty_key_event_new(&surface_key_event) == ROASTTY_SUCCESS);
  assert(surface_key_event != NULL);
  assert(!roastty_surface_key(surface, surface_key_event));
  binding_flags = 0xff;
  assert(!roastty_surface_key_is_binding(surface, surface_key_event,
                                         &binding_flags));
  assert(binding_flags == 0);
  assert(!roastty_surface_key_is_binding(surface, surface_key_event, NULL));
  roastty_key_event_free(surface_key_event);
  roastty_surface_split(surface, ROASTTY_SPLIT_DIRECTION_RIGHT);
  roastty_surface_split_focus(surface, ROASTTY_GOTO_SPLIT_NEXT);
  roastty_surface_split_resize(surface, ROASTTY_RESIZE_SPLIT_UP, 10);
  roastty_surface_split_equalize(surface);
  assert(!roastty_surface_binding_action(surface, NULL, 1));
  assert(!roastty_surface_binding_action(surface, NULL, 0));
  assert(!roastty_surface_binding_action(surface, "unknown", 7));
  assert(!roastty_surface_binding_action(surface, "quit:", 5));
  assert(!roastty_surface_binding_action(surface, "quit:now", 8));
  assert(!roastty_surface_binding_action(surface, "close_all_windows:", 18));
  assert(!roastty_surface_binding_action(surface, "close_all_windows:now", 21));
  assert(!roastty_surface_binding_action(surface, "toggle_quick_terminal:", 22));
  assert(!roastty_surface_binding_action(surface, "toggle_quick_terminal:now", 25));
  assert(!roastty_surface_binding_action(surface, "toggle_visibility:", 18));
  assert(!roastty_surface_binding_action(surface, "toggle_visibility:now", 21));
  assert(!roastty_surface_binding_action(surface, "show_gtk_inspector:", 19));
  assert(!roastty_surface_binding_action(surface, "show_gtk_inspector:now", 22));
  assert(!roastty_surface_binding_action(surface, "open_config:", 12));
  assert(!roastty_surface_binding_action(surface, "open_config:now", 15));
  assert(!roastty_surface_binding_action(surface, "reload_config:", 14));
  assert(!roastty_surface_binding_action(surface, "reload_config:now", 17));
  assert(!roastty_surface_binding_action(surface, "check_for_updates:", 18));
  assert(!roastty_surface_binding_action(surface, "check_for_updates:now", 21));
  assert(!roastty_surface_binding_action(surface, "new_window:", 11));
  assert(!roastty_surface_binding_action(surface, "new_window:now", 14));
  assert(!roastty_surface_binding_action(surface, "start_search:", 13));
  assert(!roastty_surface_binding_action(surface, "start_search:needle", 19));
  assert(!roastty_surface_binding_action(surface, "end_search:", 11));
  assert(!roastty_surface_binding_action(surface, "end_search:now", 14));
  assert(!roastty_surface_binding_action(surface, "search_selection:", 17));
  assert(!roastty_surface_binding_action(surface, "search_selection:now", 20));
  assert(!roastty_surface_binding_action(surface, "new_tab:", 8));
  assert(!roastty_surface_binding_action(surface, "new_tab:now", 11));
  assert(!roastty_surface_binding_action(surface, "close_tab:", 10));
  assert(!roastty_surface_binding_action(surface, "close_tab:all", 13));
  assert(!roastty_surface_binding_action(surface, "close_tab:this:extra", 20));
  assert(!roastty_surface_binding_action(surface, "close_tab: this", 15));
  assert(!roastty_surface_binding_action(surface, "close_tab:this ", 15));
  assert(!roastty_surface_binding_action(surface, "previous_tab:", 13));
  assert(!roastty_surface_binding_action(surface, "previous_tab:now", 16));
  assert(!roastty_surface_binding_action(surface, "next_tab:", 9));
  assert(!roastty_surface_binding_action(surface, "next_tab:now", 12));
  assert(!roastty_surface_binding_action(surface, "last_tab:", 9));
  assert(!roastty_surface_binding_action(surface, "last_tab:now", 12));
  assert(!roastty_surface_binding_action(surface, "goto_tab", 8));
  assert(!roastty_surface_binding_action(surface, "goto_tab:", 9));
  assert(!roastty_surface_binding_action(surface, "goto_tab:-1", 11));
  assert(!roastty_surface_binding_action(surface, "goto_tab: 1", 11));
  assert(!roastty_surface_binding_action(surface, "goto_tab:1 ", 11));
  assert(!roastty_surface_binding_action(surface, "goto_tab:1:2", 12));
  assert(!roastty_surface_binding_action(surface, "goto_tab:abc", 12));
  assert(!roastty_surface_binding_action(
      surface, "goto_tab:18446744073709551616", 29));
  assert(!roastty_surface_binding_action(surface, "move_tab", 8));
  assert(!roastty_surface_binding_action(surface, "move_tab:", 9));
  assert(!roastty_surface_binding_action(surface, "move_tab: 1", 11));
  assert(!roastty_surface_binding_action(surface, "move_tab:1 ", 11));
  assert(!roastty_surface_binding_action(surface, "move_tab:1:2", 12));
  assert(!roastty_surface_binding_action(surface, "move_tab:abc", 12));
  assert(!roastty_surface_binding_action(
      surface, "move_tab:9223372036854775808", 28));
  assert(!roastty_surface_binding_action(
      surface, "move_tab:-9223372036854775809", 29));
  assert(!roastty_surface_binding_action(surface, "toggle_tab_overview:", 20));
  assert(!roastty_surface_binding_action(surface, "toggle_tab_overview:now", 23));
  assert(!roastty_surface_binding_action(surface, "toggle_window_decorations:", 26));
  assert(!roastty_surface_binding_action(
      surface, "toggle_window_decorations:now", 29));
  assert(!roastty_surface_binding_action(surface, "toggle_command_palette:", 23));
  assert(!roastty_surface_binding_action(surface, "toggle_command_palette:now", 26));
  assert(!roastty_surface_binding_action(surface, "toggle_background_opacity:", 26));
  assert(!roastty_surface_binding_action(
      surface, "toggle_background_opacity:now", 29));
  assert(!roastty_surface_binding_action(surface, "show_on_screen_keyboard:", 24));
  assert(!roastty_surface_binding_action(surface, "show_on_screen_keyboard:now", 27));
  assert(!roastty_surface_binding_action(surface, "toggle_mouse_reporting:", 23));
  assert(!roastty_surface_binding_action(surface, "toggle_mouse_reporting:now", 26));
  assert(!roastty_surface_binding_action(surface, "toggle_readonly:", 16));
  assert(!roastty_surface_binding_action(surface, "toggle_readonly:now", 19));
  assert(!roastty_surface_binding_action(surface, "toggle_window_float_on_top:", 27));
  assert(!roastty_surface_binding_action(
      surface, "toggle_window_float_on_top:now", 30));
  assert(!roastty_surface_binding_action(surface, "toggle_secure_input:", 21));
  assert(!roastty_surface_binding_action(surface, "toggle_secure_input:now", 24));
  assert(!roastty_surface_binding_action(surface, "inspector", 9));
  assert(!roastty_surface_binding_action(surface, "inspector:", 10));
  assert(!roastty_surface_binding_action(surface, "inspector:open", 14));
  assert(!roastty_surface_binding_action(surface, "inspector:toggle:extra", 22));
  assert(!roastty_surface_binding_action(surface, "inspector: toggle", 17));
  assert(!roastty_surface_binding_action(surface, "inspector:toggle ", 17));
  assert(!roastty_surface_binding_action(surface, "close_window:", 13));
  assert(!roastty_surface_binding_action(surface, "close_window:now", 16));
  assert(!roastty_surface_binding_action(surface, "undo:", 5));
  assert(!roastty_surface_binding_action(surface, "undo:now", 8));
  assert(!roastty_surface_binding_action(surface, "redo:", 5));
  assert(!roastty_surface_binding_action(surface, "redo:now", 8));
  assert(!roastty_surface_binding_action(surface, "goto_window", 11));
  assert(!roastty_surface_binding_action(surface, "goto_window:", 12));
  assert(!roastty_surface_binding_action(surface, "goto_window:previous:extra", 26));
  assert(!roastty_surface_binding_action(surface, "goto_window: previous", 21));
  assert(!roastty_surface_binding_action(surface, "goto_window:previous ", 21));
  assert(!roastty_surface_binding_action(surface, "goto_window:left", 16));
  assert(!roastty_surface_binding_action(surface, "toggle_split_zoom:", 18));
  assert(!roastty_surface_binding_action(surface, "toggle_split_zoom:now", 21));
  assert(!roastty_surface_binding_action(surface, "reset_window_size:", 18));
  assert(!roastty_surface_binding_action(surface, "reset_window_size:now", 21));
  assert(!roastty_surface_binding_action(surface, "toggle_maximize:", 16));
  assert(!roastty_surface_binding_action(surface, "toggle_maximize:now", 19));
  assert(!roastty_surface_binding_action(surface, "toggle_fullscreen:", 18));
  assert(!roastty_surface_binding_action(surface, "toggle_fullscreen:now", 21));
  assert(!roastty_surface_binding_action(surface, "close_surface:now", 17));
  assert(!roastty_surface_binding_action(surface, "text", 4));
  assert(!roastty_surface_binding_action(surface, "csi", 3));
  assert(!roastty_surface_binding_action(surface, "esc", 3));
  assert(!roastty_surface_binding_action(surface, "prompt_surface_title:", 21));
  assert(!roastty_surface_binding_action(surface, "prompt_surface_title:now", 24));
  assert(!roastty_surface_binding_action(surface, "prompt_tab_title:", 17));
  assert(!roastty_surface_binding_action(surface, "prompt_tab_title:now", 20));
  assert(!roastty_surface_binding_action(surface, "set_surface_title:a", 19));
  assert(!roastty_surface_binding_action(surface, "set_tab_title:a", 14));
  assert(!roastty_surface_binding_action(surface, "reset:", 6));
  assert(!roastty_surface_binding_action(surface, "reset:now", 9));
  assert(!roastty_surface_binding_action(surface, "clear_screen:", 13));
  assert(!roastty_surface_binding_action(surface, "clear_screen:now", 16));
  assert(!roastty_surface_binding_action(surface, "select_all:", 11));
  assert(!roastty_surface_binding_action(surface, "select_all:now", 14));
  assert(!roastty_surface_binding_action(surface, "adjust_selection", 16));
  assert(!roastty_surface_binding_action(surface, "adjust_selection:", 17));
  assert(!roastty_surface_binding_action(surface, "adjust_selection:diagonal", 25));
  assert(!roastty_surface_binding_action(surface, "adjust_selection:left:right", 27));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:", 18));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:rtf", 21));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:plain:extra", 29));
  assert(!roastty_surface_binding_action(surface, "copy_url_to_clipboard:", 22));
  assert(!roastty_surface_binding_action(surface, "copy_url_to_clipboard:now", 25));
  assert(!roastty_surface_binding_action(surface, "write_selection_file", 20));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:", 21));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:copy,", 26));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:,plain", 27));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:copy,rtf", 29));
  assert(!roastty_surface_binding_action(
      surface, "write_selection_file:copy,html,extra", 36));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:paste,", 27));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:paste,rtf", 30));
  assert(!roastty_surface_binding_action(
      surface, "write_selection_file:paste,html,extra", 37));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:open", 25));
  assert(!roastty_surface_binding_action(surface, "write_screen_file", 17));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:", 18));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:copy,", 23));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:,plain", 24));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:copy,rtf", 26));
  assert(!roastty_surface_binding_action(
      surface, "write_screen_file:copy,html,extra", 33));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:paste,", 24));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:paste,rtf", 27));
  assert(!roastty_surface_binding_action(
      surface, "write_screen_file:paste,html,extra", 34));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:open", 22));
  assert(!roastty_surface_binding_action(surface, "copy_title_to_clipboard:", 24));
  assert(!roastty_surface_binding_action(surface, "copy_title_to_clipboard:now", 27));
  assert(!roastty_surface_binding_action(surface, "paste_from_clipboard:", 21));
  assert(!roastty_surface_binding_action(surface, "paste_from_clipboard:now", 24));
  assert(!roastty_surface_binding_action(surface, "paste_from_selection:", 21));
  assert(!roastty_surface_binding_action(surface, "paste_from_selection:now", 24));
  assert(!roastty_surface_binding_action(surface, "increase_font_size", 18));
  assert(!roastty_surface_binding_action(surface, "increase_font_size:", 19));
  assert(!roastty_surface_binding_action(surface, "increase_font_size:nan", 22));
  assert(!roastty_surface_binding_action(surface, "increase_font_size:1:2", 22));
  assert(!roastty_surface_binding_action(surface, "decrease_font_size", 18));
  assert(!roastty_surface_binding_action(surface, "decrease_font_size:", 19));
  assert(!roastty_surface_binding_action(surface, "decrease_font_size:nan", 22));
  assert(!roastty_surface_binding_action(surface, "decrease_font_size:1:2", 22));
  assert(!roastty_surface_binding_action(surface, "reset_font_size:", 16));
  assert(!roastty_surface_binding_action(surface, "reset_font_size:now", 19));
  assert(!roastty_surface_binding_action(surface, "set_font_size", 13));
  assert(!roastty_surface_binding_action(surface, "set_font_size:", 14));
  assert(!roastty_surface_binding_action(surface, "set_font_size:nan", 17));
  assert(!roastty_surface_binding_action(surface, "set_font_size:1:2", 17));
  assert(!roastty_surface_binding_action(surface, "scroll_to_top:", 14));
  assert(!roastty_surface_binding_action(surface, "scroll_to_top:now", 17));
  assert(!roastty_surface_binding_action(surface, "scroll_to_bottom:", 17));
  assert(!roastty_surface_binding_action(surface, "scroll_to_bottom:now", 20));
  assert(!roastty_surface_binding_action(surface, "scroll_to_row:", 14));
  assert(!roastty_surface_binding_action(surface, "scroll_to_row:abc", 17));
  assert(!roastty_surface_binding_action(surface, "scroll_to_row:-1", 16));
  assert(!roastty_surface_binding_action(
      surface, "scroll_to_row:18446744073709551616", 34));
  assert(!roastty_surface_binding_action(surface, "scroll_to_selection:", 20));
  assert(!roastty_surface_binding_action(surface, "scroll_to_selection:now", 23));
  assert(!roastty_surface_binding_action(surface, "scroll_page_up:", 15));
  assert(!roastty_surface_binding_action(surface, "scroll_page_up:now", 18));
  assert(!roastty_surface_binding_action(surface, "scroll_page_down:", 17));
  assert(!roastty_surface_binding_action(surface, "scroll_page_down:now", 20));
  assert(!roastty_surface_binding_action(surface, "scroll_page_lines:", 18));
  assert(!roastty_surface_binding_action(surface, "scroll_page_lines:abc", 21));
  assert(!roastty_surface_binding_action(surface, "scroll_page_lines:32768", 23));
  assert(!roastty_surface_binding_action(surface, "scroll_page_lines:-32769", 24));
  assert(!roastty_surface_binding_action(surface, "scroll_page_fractional:", 23));
  assert(!roastty_surface_binding_action(surface, "scroll_page_fractional:abc", 26));
  assert(!roastty_surface_binding_action(surface, "scroll_page_fractional:nan", 26));
  assert(!roastty_surface_binding_action(surface, "scroll_page_fractional:inf", 26));
  assert(!roastty_surface_binding_action(surface, "scroll_page_fractional:1e20", 27));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:", 15));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:abc", 18));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:32768", 20));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:-32769", 21));
  assert(!roastty_surface_binding_action(surface, "new_split:right", 15));
  assert(roastty_surface_binding_action(surface, "text:hello", 10));
  assert(roastty_surface_binding_action(surface, "csi:", 4));
  assert(roastty_surface_binding_action(surface, "esc:", 4));
  assert(!roastty_surface_binding_action(surface, "quit", 4));
  assert(!roastty_surface_binding_action(surface, "close_all_windows", 17));
  assert(!roastty_surface_binding_action(surface, "toggle_quick_terminal", 21));
  assert(!roastty_surface_binding_action(surface, "toggle_visibility", 17));
  assert(!roastty_surface_binding_action(surface, "show_gtk_inspector", 18));
  assert(!roastty_surface_binding_action(surface, "open_config", 11));
  assert(!roastty_surface_binding_action(surface, "reload_config", 13));
  assert(!roastty_surface_binding_action(surface, "check_for_updates", 17));
  assert(!roastty_surface_binding_action(surface, "new_window", 10));
  assert(!roastty_surface_binding_action(surface, "start_search", 12));
  assert(!roastty_surface_binding_action(surface, "end_search", 10));
  assert(!roastty_surface_binding_action(surface, "search_selection", 16));
  assert(!roastty_surface_binding_action(surface, "new_tab", 7));
  assert(!roastty_surface_binding_action(surface, "close_tab", 9));
  assert(!roastty_surface_binding_action(surface, "close_tab:this", 14));
  assert(!roastty_surface_binding_action(surface, "close_tab:other", 15));
  assert(!roastty_surface_binding_action(surface, "close_tab:right", 15));
  assert(!roastty_surface_binding_action(surface, "previous_tab", 12));
  assert(!roastty_surface_binding_action(surface, "next_tab", 8));
  assert(!roastty_surface_binding_action(surface, "last_tab", 8));
  assert(!roastty_surface_binding_action(surface, "goto_tab:1", 10));
  assert(!roastty_surface_binding_action(surface, "goto_tab:+2", 11));
  assert(!roastty_surface_binding_action(surface, "move_tab:-1", 11));
  assert(!roastty_surface_binding_action(surface, "move_tab:+1", 11));
  assert(!roastty_surface_binding_action(surface, "move_tab:0", 10));
  assert(!roastty_surface_binding_action(surface, "toggle_tab_overview", 19));
  assert(!roastty_surface_binding_action(surface, "toggle_window_decorations", 25));
  assert(!roastty_surface_binding_action(surface, "toggle_command_palette", 22));
  assert(!roastty_surface_binding_action(surface, "toggle_background_opacity", 25));
  assert(!roastty_surface_binding_action(surface, "show_on_screen_keyboard", 23));
  assert(roastty_surface_binding_action(surface, "toggle_mouse_reporting", 22));
  assert(roastty_surface_binding_action(surface, "toggle_readonly", 15));
  assert(!roastty_surface_binding_action(surface, "toggle_window_float_on_top", 26));
  assert(!roastty_surface_binding_action(surface, "toggle_secure_input", 20));
  assert(!roastty_surface_binding_action(surface, "inspector:toggle", 16));
  assert(!roastty_surface_binding_action(surface, "inspector:show", 14));
  assert(!roastty_surface_binding_action(surface, "inspector:hide", 14));
  assert(!roastty_surface_binding_action(surface, "close_window", 12));
  assert(!roastty_surface_binding_action(surface, "undo", 4));
  assert(!roastty_surface_binding_action(surface, "redo", 4));
  assert(!roastty_surface_binding_action(surface, "goto_window:previous", 20));
  assert(!roastty_surface_binding_action(surface, "goto_window:next", 16));
  assert(!roastty_surface_binding_action(surface, "toggle_split_zoom", 17));
  assert(!roastty_surface_binding_action(surface, "reset_window_size", 17));
  assert(!roastty_surface_binding_action(surface, "toggle_maximize", 15));
  assert(!roastty_surface_binding_action(surface, "toggle_fullscreen", 17));
  assert(!roastty_surface_binding_action(surface, "prompt_surface_title", 20));
  assert(!roastty_surface_binding_action(surface, "prompt_tab_title", 16));
  assert(!roastty_surface_binding_action(surface, "set_surface_title:", 18));
  assert(!roastty_surface_binding_action(surface, "set_tab_title:", 13));
  assert(roastty_surface_binding_action(surface, "reset", 5));
  assert(!roastty_surface_binding_action(surface, "clear_screen", 12));
  assert(!roastty_surface_binding_action(surface, "select_all", 10));
  assert(!roastty_surface_binding_action(surface, "adjust_selection:left", 21));
  assert(!roastty_surface_binding_action(surface, "adjust_selection:page_down", 26));
  assert(!roastty_surface_binding_action(
      surface, "adjust_selection:beginning_of_line", 34));
  assert(!roastty_surface_binding_action(surface, "adjust_selection:end_of_line", 28));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard", 17));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:plain", 23));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:vt", 20));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:html", 22));
  assert(!roastty_surface_binding_action(surface, "copy_to_clipboard:mixed", 23));
  assert(!roastty_surface_binding_action(surface, "copy_url_to_clipboard", 21));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:copy", 25));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:copy,plain", 31));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:copy,vt", 28));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:copy,html", 30));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:paste", 26));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:paste,plain", 32));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:paste,vt", 29));
  assert(!roastty_surface_binding_action(surface, "write_selection_file:paste,html", 31));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:copy", 22));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:copy,plain", 28));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:copy,vt", 25));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:copy,html", 27));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:paste", 23));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:paste,plain", 29));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:paste,vt", 26));
  assert(!roastty_surface_binding_action(surface, "write_screen_file:paste,html", 28));
  assert(!roastty_surface_binding_action(surface, "copy_title_to_clipboard", 23));
  assert(!roastty_surface_binding_action(surface, "paste_from_clipboard", 20));
  assert(!roastty_surface_binding_action(surface, "paste_from_selection", 20));
  assert(roastty_surface_binding_action(surface, "increase_font_size:1", 20));
  assert(roastty_surface_binding_action(surface, "decrease_font_size:1", 20));
  assert(roastty_surface_binding_action(surface, "reset_font_size", 15));
  assert(roastty_surface_binding_action(surface, "set_font_size:14", 16));
  assert(roastty_surface_binding_action(surface, "scroll_to_top", 13));
  assert(roastty_surface_binding_action(surface, "scroll_to_bottom", 16));
  assert(roastty_surface_binding_action(surface, "scroll_to_row:0", 15));
  assert(roastty_surface_binding_action(surface, "scroll_to_row:+1", 16));
  assert(roastty_surface_binding_action(surface, "scroll_to_row:2", 15));
  assert(!roastty_surface_binding_action(surface, "scroll_to_selection", 19));
  assert(roastty_surface_binding_action(surface, "scroll_page_up", 14));
  assert(roastty_surface_binding_action(surface, "scroll_page_down", 16));
  assert(roastty_surface_binding_action(surface, "scroll_page_lines:-2", 20));
  assert(roastty_surface_binding_action(surface, "scroll_page_lines:+2", 20));
  assert(roastty_surface_binding_action(surface, "scroll_page_lines:2", 19));
  assert(roastty_surface_binding_action(surface, "scroll_page_lines:0", 19));
  assert(roastty_surface_binding_action(surface, "scroll_page_fractional:-0.5", 27));
  assert(roastty_surface_binding_action(surface, "scroll_page_fractional:+0.5", 27));
  assert(roastty_surface_binding_action(surface, "scroll_page_fractional:5e-1", 27));
  assert(roastty_surface_binding_action(surface, "scroll_page_fractional:0", 24));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:-1", 17));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:+1", 17));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:1", 16));
  assert(!roastty_surface_binding_action(surface, "jump_to_prompt:0", 16));
  assert(roastty_surface_binding_action(surface, "close_surface", 13));
  roastty_surface_preedit(surface, NULL, 3);
  roastty_surface_preedit(surface, NULL, 0);
  roastty_text_s read_text = {0};
  roastty_selection_s empty_selection = {0};
  empty_selection.size = sizeof(roastty_selection_s);
  assert(!roastty_surface_mouse_captured(surface));
  assert(!roastty_surface_mouse_button(
      surface, ROASTTY_MOUSE_BUTTON_PRESS, ROASTTY_MOUSE_BUTTON_LEFT,
      ROASTTY_MODS_SHIFT));
  roastty_surface_mouse_pos(surface, 1.0, 2.0, ROASTTY_MODS_SHIFT);
  roastty_surface_mouse_scroll(surface, 1.0, 2.0,
                               (roastty_input_scroll_mods_t)0x1ff);
  roastty_surface_mouse_pressure(surface, 1, 0.5);

  assert(roastty_surface_inspector(NULL) == NULL);
  roastty_inspector_free(NULL);
  roastty_inspector_set_focus(NULL, true);
  roastty_inspector_set_content_scale(NULL, 1.0, 1.0);
  roastty_inspector_set_size(NULL, 1, 2);
  roastty_inspector_mouse_button(NULL,
                                 ROASTTY_MOUSE_BUTTON_PRESS,
                                 ROASTTY_MOUSE_BUTTON_LEFT,
                                 ROASTTY_MODS_SHIFT);
  roastty_inspector_mouse_pos(NULL, 1.0, 2.0);
  roastty_inspector_mouse_scroll(NULL, 1.0, 2.0, 0);
  roastty_inspector_key(NULL,
                        ROASTTY_KEY_ACTION_PRESS,
                        ROASTTY_KEY_KEY_A,
                        ROASTTY_MODS_CTRL);
  roastty_inspector_text(NULL, "ignored");

  roastty_inspector_t inspector = roastty_surface_inspector(surface);
  assert(inspector != NULL);
  assert(roastty_surface_inspector(surface) == inspector);
  roastty_inspector_set_focus(inspector, true);
  roastty_inspector_set_content_scale(inspector, 2.0, 2.0);
  roastty_inspector_set_size(inspector, 640, 480);
  roastty_inspector_mouse_button(inspector,
                                 ROASTTY_MOUSE_BUTTON_PRESS,
                                 ROASTTY_MOUSE_BUTTON_LEFT,
                                 ROASTTY_MODS_SHIFT);
  roastty_inspector_mouse_pos(inspector, 3.0, 4.0);
  roastty_inspector_mouse_scroll(inspector, 5.0, 6.0, 0);
  roastty_inspector_key(inspector,
                        ROASTTY_KEY_ACTION_REPEAT,
                        ROASTTY_KEY_KEY_B,
                        ROASTTY_MODS_ALT);
  roastty_inspector_text(inspector, "inspector");
  roastty_inspector_text(inspector, NULL);
  roastty_inspector_free(surface);
  assert(roastty_surface_inspector(surface) != NULL);

  assert(!roastty_surface_has_selection(surface));
  assert(!roastty_surface_read_selection(surface, NULL));
  assert(!roastty_surface_read_selection(surface, &read_text));
  assert(read_text.tl_px_x == -1.0);
  assert(read_text.tl_px_y == -1.0);
  assert(read_text.offset_start == 0);
  assert(read_text.offset_len == 0);
  assert(read_text.text == NULL);
  assert(read_text.text_len == 0);
  assert(!roastty_surface_read_text(surface, empty_selection, NULL));
  assert(!roastty_surface_read_text(surface, empty_selection, &read_text));
  assert(read_text.tl_px_x == -1.0);
  assert(read_text.tl_px_y == -1.0);
  assert(read_text.offset_start == 0);
  assert(read_text.offset_len == 0);
  assert(read_text.text == NULL);
  assert(read_text.text_len == 0);
  assert(roastty_surface_quicklook_font(surface) == NULL);
  assert(!roastty_surface_quicklook_word(surface, NULL));
  assert(!roastty_surface_quicklook_word(surface, &read_text));
  assert(read_text.tl_px_x == -1.0);
  assert(read_text.tl_px_y == -1.0);
  assert(read_text.offset_start == 0);
  assert(read_text.offset_len == 0);
  assert(read_text.text == NULL);
  assert(read_text.text_len == 0);
  roastty_surface_free_text(surface, &read_text);

  roastty_surface_t refresh_surface = roastty_surface_new(app, &surface_config);
  assert(refresh_surface != NULL);
  assert(!roastty_surface_needs_render(refresh_surface));
  roastty_surface_refresh(refresh_surface);
  assert(roastty_surface_needs_render(refresh_surface));
  roastty_surface_free(refresh_surface);

  roastty_string_s tty_name = roastty_surface_tty_name(surface);
  assert(tty_name.ptr != NULL);
  assert(tty_name.len == strlen("roastty-skeleton-tty"));
  assert(tty_name.sentinel == true);
  roastty_string_free(tty_name);

  close_surface_call_count = 0;
  close_surface_last_userdata = NULL;
  close_surface_last_needs_confirm = true;
  roastty_surface_request_close(surface);
  assert(close_surface_call_count == 1);
  assert((uintptr_t)close_surface_last_userdata == surface_userdata);
  assert(!close_surface_last_needs_confirm);
  roastty_surface_free(surface);

  roastty_string_s empty_tty = roastty_surface_tty_name(NULL);
  assert(empty_tty.ptr == NULL);
  assert(empty_tty.len == 0);
  assert(empty_tty.sentinel == false);
  roastty_string_free(empty_tty);

  for (int i = 0; i < 16; i++) {
    roastty_surface_t loop_surface = roastty_surface_new(app, &surface_config);
    assert(loop_surface != NULL);
    roastty_surface_free(loop_surface);
  }

  roastty_app_free(app);
  roastty_config_free(config);
  return 0;
}
