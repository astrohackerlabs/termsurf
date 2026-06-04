//! Custom (shadertoy-style) shader support.
//!
//! The `CustomShaderUniforms` value type — the uniform struct custom shaders
//! read — and its renderer-init defaults. A faithful port of upstream
//! `renderer/shadertoy.zig`'s `Uniforms` `extern struct`; the per-frame/state
//! update methods, the `Target` enum, and the shader loading are ported in later
//! slices.
#![allow(dead_code)]
// This shadertoy layer is consumed by later slices.

use crate::renderer::shader::CellTextVertex;

/// The uniform struct custom shaders read (upstream `shadertoy.Uniforms`). The
/// `#[repr(C, align(16))]` layout with explicit padding reproduces upstream's
/// `extern struct` `align(16)` field offsets (Rust's `[f32; 4]` has alignment 4,
/// so the padding — not the field alignment — places the vectors at their
/// 16-aligned offsets). `size_of == 4496`, `align_of == 16`.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CustomShaderUniforms {
    pub(crate) resolution: [f32; 3],
    pub(crate) time: f32,
    pub(crate) time_delta: f32,
    pub(crate) frame_rate: f32,
    pub(crate) frame: i32,
    _pad0: [u8; 4],
    pub(crate) channel_time: [[f32; 4]; 4],
    pub(crate) channel_resolution: [[f32; 4]; 4],
    pub(crate) mouse: [f32; 4],
    pub(crate) date: [f32; 4],
    pub(crate) sample_rate: f32,
    _pad1: [u8; 12],
    pub(crate) current_cursor: [f32; 4],
    pub(crate) previous_cursor: [f32; 4],
    pub(crate) current_cursor_color: [f32; 4],
    pub(crate) previous_cursor_color: [f32; 4],
    pub(crate) current_cursor_style: i32,
    pub(crate) previous_cursor_style: i32,
    pub(crate) cursor_visible: i32,
    pub(crate) cursor_change_time: f32,
    pub(crate) time_focus: f32,
    pub(crate) focus: i32,
    _pad2: [u8; 8],
    pub(crate) palette: [[f32; 4]; 256],
    pub(crate) background_color: [f32; 4],
    pub(crate) foreground_color: [f32; 4],
    pub(crate) cursor_color: [f32; 4],
    pub(crate) cursor_text: [f32; 4],
    pub(crate) selection_background_color: [f32; 4],
    pub(crate) selection_foreground_color: [f32; 4],
}

impl CustomShaderUniforms {
    /// The renderer-init defaults (upstream's `init` literal): all zero except
    /// `resolution = [0, 0, 1]`, `frame_rate = 60`, and `focus = 1`.
    pub(crate) fn new() -> Self {
        Self {
            resolution: [0.0, 0.0, 1.0],
            time: 0.0,
            time_delta: 0.0,
            frame_rate: 60.0,
            frame: 0,
            _pad0: [0; 4],
            channel_time: [[0.0; 4]; 4],
            channel_resolution: [[0.0; 4]; 4],
            mouse: [0.0; 4],
            date: [0.0; 4],
            sample_rate: 0.0,
            _pad1: [0; 12],
            current_cursor: [0.0; 4],
            previous_cursor: [0.0; 4],
            current_cursor_color: [0.0; 4],
            previous_cursor_color: [0.0; 4],
            current_cursor_style: 0,
            previous_cursor_style: 0,
            cursor_visible: 0,
            cursor_change_time: 0.0,
            time_focus: 0.0,
            focus: 1,
            _pad2: [0; 8],
            palette: [[0.0; 4]; 256],
            background_color: [0.0; 4],
            foreground_color: [0.0; 4],
            cursor_color: [0.0; 4],
            cursor_text: [0.0; 4],
            selection_background_color: [0.0; 4],
            selection_foreground_color: [0.0; 4],
        }
    }

    /// Update the per-frame time and resolution fields (the time/resolution
    /// group of upstream `updateCustomShaderUniformsForFrame`): `time` (seconds
    /// since the first frame), `time_delta` (seconds since the last frame), the
    /// `frame` counter (incremented), `resolution` (the screen size, `z = 1`),
    /// and `channel_resolution[0]`. The caller owns the clock (computes the
    /// seconds); the cursor-glyph update is a later slice.
    pub(crate) fn update_for_frame(
        &mut self,
        time_secs: f32,
        time_delta_secs: f32,
        screen_width: u32,
        screen_height: u32,
    ) {
        self.time = time_secs;
        self.time_delta = time_delta_secs;
        self.frame += 1;
        let (w, h) = (screen_width as f32, screen_height as f32);
        self.resolution = [w, h, 1.0];
        self.channel_resolution[0] = [w, h, 1.0, 0.0];
    }

    /// Update the cursor uniforms from the cursor glyph (upstream
    /// `updateCustomShaderUniformsForFrame`'s cursor half, Metal
    /// `custom_shader_y_is_down = true`): compute the cursor's pixel rect
    /// (`[left + bearingX, top + cellH - bearingY + glyphH, glyphW, glyphH]`) and
    /// its normalized color; on a change, shift `current` → `previous` and stamp
    /// `cursor_change_time = time`. No cursor glyph → no update.
    pub(crate) fn update_cursor(
        &mut self,
        cursor: Option<CellTextVertex>,
        cell_width: u32,
        cell_height: u32,
        padding_left: u32,
        padding_top: u32,
    ) {
        let Some(cursor) = cursor else {
            return;
        };
        let mut pixel_x = (cursor.grid_pos[0] as u32 * cell_width + padding_left) as f32;
        let mut pixel_y = (cursor.grid_pos[1] as u32 * cell_height + padding_top) as f32;
        pixel_x += f32::from(cursor.bearings[0]);
        // Metal: custom_shader_y_is_down = true.
        pixel_y += cell_height as f32;
        pixel_y -= f32::from(cursor.bearings[1]);
        pixel_y += cursor.glyph_size[1] as f32;

        let new_cursor = [
            pixel_x,
            pixel_y,
            cursor.glyph_size[0] as f32,
            cursor.glyph_size[1] as f32,
        ];
        let cursor_color = [
            f32::from(cursor.color[0]) / 255.0,
            f32::from(cursor.color[1]) / 255.0,
            f32::from(cursor.color[2]) / 255.0,
            f32::from(cursor.color[3]) / 255.0,
        ];

        if new_cursor != self.current_cursor || cursor_color != self.current_cursor_color {
            self.previous_cursor = self.current_cursor;
            self.previous_cursor_color = self.current_cursor_color;
            self.current_cursor = new_cursor;
            self.current_cursor_color = cursor_color;
            self.cursor_change_time = self.time;
        }
    }

    /// Update the focus uniforms (upstream `updateCustomShaderUniformsForFrame`'s
    /// focus block): `focus` is `1` when `focused`, else `0`; `time_focus` is
    /// stamped with the frame `time` when focus was just gained
    /// (`focus_changed && focused`). Returns the new `focus_changed` flag (cleared
    /// to `false` when consumed — upstream resets `custom_shader_focused_changed`).
    pub(crate) fn update_focus(&mut self, focused: bool, focus_changed: bool) -> bool {
        self.focus = i32::from(focused);
        if focus_changed && focused {
            self.time_focus = self.time;
            return false;
        }
        focus_changed
    }
}

#[cfg(test)]
mod tests {
    use super::{CellTextVertex, CustomShaderUniforms};
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn custom_shader_uniforms_layout_matches_extern_struct() {
        assert_eq!(size_of::<CustomShaderUniforms>(), 4496);
        assert_eq!(align_of::<CustomShaderUniforms>(), 16);

        // The std140 field offsets (each `align(16)` field on a 16-multiple).
        assert_eq!(offset_of!(CustomShaderUniforms, resolution), 0);
        assert_eq!(offset_of!(CustomShaderUniforms, time), 12);
        assert_eq!(offset_of!(CustomShaderUniforms, frame), 24);
        assert_eq!(offset_of!(CustomShaderUniforms, channel_time), 32);
        assert_eq!(offset_of!(CustomShaderUniforms, channel_resolution), 96);
        assert_eq!(offset_of!(CustomShaderUniforms, mouse), 160);
        assert_eq!(offset_of!(CustomShaderUniforms, date), 176);
        assert_eq!(offset_of!(CustomShaderUniforms, sample_rate), 192);
        assert_eq!(offset_of!(CustomShaderUniforms, current_cursor), 208);
        assert_eq!(offset_of!(CustomShaderUniforms, current_cursor_style), 272);
        assert_eq!(offset_of!(CustomShaderUniforms, focus), 292);
        assert_eq!(offset_of!(CustomShaderUniforms, palette), 304);
        assert_eq!(offset_of!(CustomShaderUniforms, background_color), 4400);
        assert_eq!(
            offset_of!(CustomShaderUniforms, selection_foreground_color),
            4480
        );
    }

    #[test]
    fn custom_shader_uniforms_new_matches_init_defaults() {
        let u = CustomShaderUniforms::new();
        // All zero except resolution, frame_rate, and focus.
        assert_eq!(u.resolution, [0.0, 0.0, 1.0]);
        assert_eq!(u.frame_rate, 60.0);
        assert_eq!(u.focus, 1);
        // Representative zeroed fields.
        assert_eq!(u.time, 0.0);
        assert_eq!(u.frame, 0);
        assert_eq!(u.cursor_visible, 0);
        assert_eq!(u.palette[0], [0.0; 4]);
        assert_eq!(u.background_color, [0.0; 4]);
    }

    #[test]
    fn update_for_frame_sets_time_and_resolution() {
        let mut u = CustomShaderUniforms::new();

        u.update_for_frame(1.5, 0.016, 800, 600);
        assert_eq!(u.time, 1.5);
        assert_eq!(u.time_delta, 0.016);
        assert_eq!(u.frame, 1);
        assert_eq!(u.resolution, [800.0, 600.0, 1.0]);
        assert_eq!(u.channel_resolution[0], [800.0, 600.0, 1.0, 0.0]);

        // The frame counter increments across calls.
        u.update_for_frame(1.6, 0.016, 800, 600);
        assert_eq!(u.frame, 2);

        // The other fields are untouched.
        assert_eq!(u.focus, 1);
        assert_eq!(u.palette[0], [0.0; 4]);
        assert_eq!(u.channel_resolution[1], [0.0; 4]);
    }

    fn cursor_glyph(
        grid_pos: [u16; 2],
        glyph_size: [u32; 2],
        bearings: [i16; 2],
        color: [u8; 4],
    ) -> CellTextVertex {
        use crate::renderer::shader::{CellTextAtlas, CellTextFlags};
        CellTextVertex {
            glyph_pos: [0, 0],
            glyph_size,
            bearings,
            grid_pos,
            color,
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::default(),
            _padding: [0, 0],
        }
    }

    #[test]
    fn update_cursor_computes_pixel_rect_and_tracks_changes() {
        let mut u = CustomShaderUniforms::new();
        u.time = 5.0;

        // grid_pos [2,3], glyph_size [10,20], bearings [1,2], red; cell 8×16,
        // padding left 4 / top 5. x = 2·8+4+1 = 21; y = 3·16+5+16−2+20 = 87.
        let glyph = cursor_glyph([2, 3], [10, 20], [1, 2], [255, 0, 0, 255]);
        u.update_cursor(Some(glyph), 8, 16, 4, 5);
        assert_eq!(u.current_cursor, [21.0, 87.0, 10.0, 20.0]);
        assert_eq!(u.current_cursor_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(u.previous_cursor, [0.0; 4]); // the old current
        assert_eq!(u.cursor_change_time, 5.0);

        // The same glyph at a later time → no change (previous and change time
        // stay).
        u.time = 6.0;
        u.update_cursor(Some(glyph), 8, 16, 4, 5);
        assert_eq!(u.previous_cursor, [0.0; 4]);
        assert_eq!(u.cursor_change_time, 5.0);

        // A different glyph → previous becomes the prior current, change time
        // updates.
        let glyph2 = cursor_glyph([0, 0], [10, 20], [0, 0], [0, 0, 255, 255]);
        u.update_cursor(Some(glyph2), 8, 16, 4, 5);
        assert_eq!(u.previous_cursor, [21.0, 87.0, 10.0, 20.0]);
        assert_eq!(u.cursor_change_time, 6.0);

        // None → no-op.
        let before = u;
        u.update_cursor(None, 8, 16, 4, 5);
        assert_eq!(u, before);
    }

    #[test]
    fn update_focus_sets_focus_and_stamps_on_gain() {
        let mut u = CustomShaderUniforms::new();
        u.time = 5.0;

        // Focus gained → focus 1, time_focus stamped, flag consumed.
        assert!(!u.update_focus(true, true));
        assert_eq!(u.focus, 1);
        assert_eq!(u.time_focus, 5.0);

        // Focused, no change → focus 1, time_focus unchanged, flag stays false.
        let mut u = CustomShaderUniforms::new();
        u.time = 5.0;
        assert!(!u.update_focus(true, false));
        assert_eq!(u.focus, 1);
        assert_eq!(u.time_focus, 0.0);

        // Unfocused but "changed" → focus 0, time_focus NOT stamped (not gained),
        // and the flag is NOT consumed (returns true).
        let mut u = CustomShaderUniforms::new();
        u.time = 5.0;
        assert!(u.update_focus(false, true));
        assert_eq!(u.focus, 0);
        assert_eq!(u.time_focus, 0.0);

        // Unfocused, no change → focus 0, returns false.
        assert!(!u.update_focus(false, false));
        assert_eq!(u.focus, 0);

        // The other fields are untouched.
        assert_eq!(u.frame, 0);
        assert_eq!(u.resolution, [0.0, 0.0, 1.0]);
    }
}
