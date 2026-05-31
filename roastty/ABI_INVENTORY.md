# Roastty ABI Inventory

This inventory maps the upstream Ghostty C ABI concepts that informed the
Roastty lifecycle skeleton to the renamed Roastty ABI. Upstream names are
reference material only. The app-facing Roastty ABI must use `roastty_*` symbols
and `ROASTTY_` constants.

## Implemented In Experiment 3

| Upstream reference                    | Roastty ABI                           |
| ------------------------------------- | ------------------------------------- |
| `ghostty_init`                        | `roastty_init`                        |
| `ghostty_info`                        | `roastty_info`                        |
| `ghostty_string_free`                 | `roastty_string_free`                 |
| `ghostty_config_new`                  | `roastty_config_new`                  |
| `ghostty_config_free`                 | `roastty_config_free`                 |
| `ghostty_config_clone`                | `roastty_config_clone`                |
| `ghostty_config_load_cli_args`        | `roastty_config_load_cli_args`        |
| `ghostty_config_load_file`            | `roastty_config_load_file`            |
| `ghostty_config_load_default_files`   | `roastty_config_load_default_files`   |
| `ghostty_config_load_recursive_files` | `roastty_config_load_recursive_files` |
| `ghostty_config_finalize`             | `roastty_config_finalize`             |
| `ghostty_config_diagnostics_count`    | `roastty_config_diagnostics_count`    |
| `ghostty_config_get_diagnostic`       | `roastty_config_get_diagnostic`       |
| `ghostty_config_open_path`            | `roastty_config_open_path`            |
| `ghostty_app_new`                     | `roastty_app_new`                     |
| `ghostty_app_free`                    | `roastty_app_free`                    |
| `ghostty_app_tick`                    | `roastty_app_tick`                    |
| `ghostty_app_userdata`                | `roastty_app_userdata`                |
| `ghostty_app_set_focus`               | `roastty_app_set_focus`               |
| `ghostty_app_update_config`           | `roastty_app_update_config`           |
| `ghostty_app_needs_confirm_quit`      | `roastty_app_needs_confirm_quit`      |
| `ghostty_app_has_global_keybinds`     | `roastty_app_has_global_keybinds`     |
| `ghostty_app_set_color_scheme`        | `roastty_app_set_color_scheme`        |
| `ghostty_surface_config_new`          | `roastty_surface_config_new`          |
| `ghostty_surface_new`                 | `roastty_surface_new`                 |
| `ghostty_surface_free`                | `roastty_surface_free`                |
| `ghostty_surface_userdata`            | `roastty_surface_userdata`            |
| `ghostty_surface_app`                 | `roastty_surface_app`                 |
| `ghostty_surface_update_config`       | `roastty_surface_update_config`       |
| `ghostty_surface_needs_confirm_quit`  | `roastty_surface_needs_confirm_quit`  |
| `ghostty_surface_process_exited`      | `roastty_surface_process_exited`      |
| `ghostty_surface_set_content_scale`   | `roastty_surface_set_content_scale`   |
| `ghostty_surface_set_focus`           | `roastty_surface_set_focus`           |
| `ghostty_surface_set_occlusion`       | `roastty_surface_set_occlusion`       |
| `ghostty_surface_set_size`            | `roastty_surface_set_size`            |
| `ghostty_surface_size`                | `roastty_surface_size`                |
| `ghostty_surface_foreground_pid`      | `roastty_surface_foreground_pid`      |
| `ghostty_surface_tty_name`            | `roastty_surface_tty_name`            |
| `ghostty_surface_set_color_scheme`    | `roastty_surface_set_color_scheme`    |
| `ghostty_surface_request_close`       | `roastty_surface_request_close`       |

## Deferred Swift-Used Concepts

These upstream symbols are referenced under `vendor/ghostty/macos/Sources/`.
They require behavior beyond the inert lifecycle skeleton, so their Roastty
equivalents are deferred.

| Upstream reference                           | Future Roastty ABI                           |
| -------------------------------------------- | -------------------------------------------- |
| `ghostty_app_key`                            | `roastty_app_key`                            |
| `ghostty_app_keyboard_changed`               | `roastty_app_keyboard_changed`               |
| `ghostty_app_open_config`                    | `roastty_app_open_config`                    |
| `ghostty_config_get`                         | `roastty_config_get`                         |
| `ghostty_config_key_is_binding`              | `roastty_config_key_is_binding`              |
| `ghostty_config_trigger`                     | `roastty_config_trigger`                     |
| `ghostty_inspector_free`                     | `roastty_inspector_free`                     |
| `ghostty_inspector_key`                      | `roastty_inspector_key`                      |
| `ghostty_inspector_metal_init`               | `roastty_inspector_metal_init`               |
| `ghostty_inspector_metal_render`             | `roastty_inspector_metal_render`             |
| `ghostty_inspector_metal_shutdown`           | `roastty_inspector_metal_shutdown`           |
| `ghostty_inspector_mouse_button`             | `roastty_inspector_mouse_button`             |
| `ghostty_inspector_mouse_pos`                | `roastty_inspector_mouse_pos`                |
| `ghostty_inspector_mouse_scroll`             | `roastty_inspector_mouse_scroll`             |
| `ghostty_inspector_set_content_scale`        | `roastty_inspector_set_content_scale`        |
| `ghostty_inspector_set_focus`                | `roastty_inspector_set_focus`                |
| `ghostty_inspector_set_size`                 | `roastty_inspector_set_size`                 |
| `ghostty_inspector_text`                     | `roastty_inspector_text`                     |
| `ghostty_surface_binding_action`             | `roastty_surface_binding_action`             |
| `ghostty_surface_complete_clipboard_request` | `roastty_surface_complete_clipboard_request` |
| `ghostty_surface_draw`                       | `roastty_surface_draw`                       |
| `ghostty_surface_free_text`                  | `roastty_surface_free_text`                  |
| `ghostty_surface_has_selection`              | `roastty_surface_has_selection`              |
| `ghostty_surface_ime_point`                  | `roastty_surface_ime_point`                  |
| `ghostty_surface_inherited_config`           | `roastty_surface_inherited_config`           |
| `ghostty_surface_inspector`                  | `roastty_surface_inspector`                  |
| `ghostty_surface_key`                        | `roastty_surface_key`                        |
| `ghostty_surface_key_is_binding`             | `roastty_surface_key_is_binding`             |
| `ghostty_surface_key_translation_mods`       | `roastty_surface_key_translation_mods`       |
| `ghostty_surface_mouse_button`               | `roastty_surface_mouse_button`               |
| `ghostty_surface_mouse_captured`             | `roastty_surface_mouse_captured`             |
| `ghostty_surface_mouse_pos`                  | `roastty_surface_mouse_pos`                  |
| `ghostty_surface_mouse_pressure`             | `roastty_surface_mouse_pressure`             |
| `ghostty_surface_mouse_scroll`               | `roastty_surface_mouse_scroll`               |
| `ghostty_surface_preedit`                    | `roastty_surface_preedit`                    |
| `ghostty_surface_quicklook_font`             | `roastty_surface_quicklook_font`             |
| `ghostty_surface_quicklook_word`             | `roastty_surface_quicklook_word`             |
| `ghostty_surface_read_selection`             | `roastty_surface_read_selection`             |
| `ghostty_surface_read_text`                  | `roastty_surface_read_text`                  |
| `ghostty_surface_refresh`                    | `roastty_surface_refresh`                    |
| `ghostty_surface_set_display_id`             | `roastty_surface_set_display_id`             |
| `ghostty_surface_split`                      | `roastty_surface_split`                      |
| `ghostty_surface_split_equalize`             | `roastty_surface_split_equalize`             |
| `ghostty_surface_split_focus`                | `roastty_surface_split_focus`                |
| `ghostty_surface_split_resize`               | `roastty_surface_split_resize`               |
| `ghostty_surface_text`                       | `roastty_surface_text`                       |

## Not Relevant To The Current Skeleton

These upstream symbols are published by `vendor/ghostty/include/ghostty.h` but
are not needed to prove the Roastty app/config/surface lifecycle skeleton.

| Upstream reference                   | Roastty disposition              |
| ------------------------------------ | -------------------------------- |
| `ghostty_benchmark_cli`              | Future CLI or benchmark decision |
| `ghostty_cli_try_action`             | Future CLI decision              |
| `ghostty_set_window_background_blur` | Future native-window decision    |
| `ghostty_translate`                  | Future localization decision     |
