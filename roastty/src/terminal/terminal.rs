//! Terminal state.

use std::ffi::{c_char, c_void};
use std::ptr::NonNull;

use crate::input::key_encode;

use super::charsets;
use super::color;
use super::cursor;
use super::dcs;
use super::device_attributes;
use super::device_status;
use super::kitty::graphics_command::{Command, Parser, Response};
use super::kitty::graphics_exec;
use super::kitty::graphics_image::{Image, LoadingImageLimits, MAX_IMAGE_SIZE};
use super::kitty::graphics_storage::{CellMetrics, Placement, PlacementKey, DEFAULT_TOTAL_LIMIT};
use super::kitty::graphics_unicode;
use super::modes;
use super::mouse;
use super::osc;
use super::page::SemanticPrompt;
use super::page_list::{
    CodepointMapEntry, DragGeometry, GridRef, GridRefPointError, PageListAllocError,
    PageOutputFormat, PageStringWithPinMap, Pin, PromptClickMode, RenderRowSnapshot,
};
use super::screen::{
    BasicPrintError, EraseDisplayError, Screen, ScreenCursorHyperlinkId, ScreenFormatter,
    ScreenFormatterContent, ScreenFormatterExtra, ScreenFormatterOptions,
};
use super::selection;
use super::selection_codepoints;
use super::selection_gesture::{SelectionGestureAnchor, SelectionGestureGeometry};
use super::sgr;
use super::size::CellCountInt;
use super::size_report;
use super::stream::{self, Action, Handler};
use super::style;
use super::tabstops;
use super::tmux;
use crate::font::run::RunOptions;

const TABSTOP_INTERVAL: usize = 8;

#[derive(Debug)]
pub(crate) struct Terminal {
    size: TerminalSize,
    screens: TerminalScreens,
    colors: TerminalColors,
    modes: modes::ModeState,
    scrolling_region: ScrollingRegion,
    tabstops: tabstops::Tabstops,
    pty_response: Vec<u8>,
    effects: TerminalEffects,
    stream: stream::Stream,
    dcs: dcs::Handler,
    tmux_viewer: Option<tmux::TmuxViewer>,
    tmux_windows: Vec<tmux::TmuxWindow>,
    kitty_graphics: KittyGraphicsApc,
    kitty_config: KittyGraphicsConfig,
    flags: TerminalFlags,
    title: TerminalTitle,
    pwd: TerminalPwd,
    mouse_shape: mouse::MouseShape,
    title_report: bool,
    next_implicit_hyperlink_id: u32,
    previous_char: Option<char>,
    pending_clipboard_events: Vec<TerminalClipboardEvent>,
    pending_bell_count: usize,
    default_cursor_visual_style: cursor::VisualStyle,
    default_cursor_blink: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptClickAction {
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub(super) struct TerminalScreens {
    primary: Screen,
    alternate: Option<Screen>,
    active: TerminalScreenKey,
    primary_generation: u64,
    alternate_generation: u64,
    primary_owner_id: u64,
    alternate_owner_id: u64,
    next_screen_owner_id: u64,
    active_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalScreenKey {
    Primary,
    Alternate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalScreen {
    Primary,
    Alternate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClearScreenResult {
    NotPerformed,
    Performed,
    SendFormFeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalPointTag {
    Active,
    Viewport,
    Screen,
    History,
}

impl TerminalPointTag {
    pub(crate) fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Active),
            1 => Some(Self::Viewport),
            2 => Some(Self::Screen),
            3 => Some(Self::History),
            _ => None,
        }
    }
}

impl From<TerminalPointTag> for super::point::Tag {
    fn from(value: TerminalPointTag) -> Self {
        match value {
            TerminalPointTag::Active => Self::Active,
            TerminalPointTag::Viewport => Self::Viewport,
            TerminalPointTag::Screen => Self::Screen,
            TerminalPointTag::History => Self::History,
        }
    }
}

/// Embedded `point_coord_e` — how an embedded `point_s` resolves within its tag's
/// region (Issue 802 / Exp 11). `Exact` uses the `(x, y)` grid coordinate; `TopLeft`
/// / `BottomRight` ignore it and resolve to the region's corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbeddedPointCoord {
    Exact,
    TopLeft,
    BottomRight,
}

impl EmbeddedPointCoord {
    pub(crate) fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Exact),
            1 => Some(Self::TopLeft),
            2 => Some(Self::BottomRight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalGridRef {
    pub(crate) node: *const (),
    pub(crate) x: CellCountInt,
    pub(crate) y: CellCountInt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalSelection {
    pub(crate) start: TerminalGridRef,
    pub(crate) end: TerminalGridRef,
    pub(crate) rectangle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalTrackedGridRef {
    pin: NonNull<Pin>,
    screen_key: TerminalScreenKey,
    screen_generation: u64,
    screen_owner_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalClipboardEvent {
    Osc52 {
        kind: u8,
        data: Vec<u8>,
    },
    Kitty {
        metadata: Vec<u8>,
        payload: Option<Vec<u8>>,
        terminator: osc::Terminator,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSelectionFormat {
    Plain,
    Vt,
    Html,
}

impl TerminalSelectionFormat {
    pub(crate) fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Plain),
            1 => Some(Self::Vt),
            2 => Some(Self::Html),
            _ => None,
        }
    }
}

impl From<TerminalSelectionFormat> for PageOutputFormat {
    fn from(value: TerminalSelectionFormat) -> Self {
        match value {
            TerminalSelectionFormat::Plain => Self::Plain,
            TerminalSelectionFormat::Vt => Self::Vt,
            TerminalSelectionFormat::Html => Self::Html,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSelectionOrder {
    Forward,
    Reverse,
    MirroredForward,
    MirroredReverse,
}

impl TerminalSelectionOrder {
    pub(crate) fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Forward),
            1 => Some(Self::Reverse),
            2 => Some(Self::MirroredForward),
            3 => Some(Self::MirroredReverse),
            _ => None,
        }
    }

    pub(crate) fn raw(self) -> i32 {
        match self {
            Self::Forward => 0,
            Self::Reverse => 1,
            Self::MirroredForward => 2,
            Self::MirroredReverse => 3,
        }
    }
}

impl From<selection::Order> for TerminalSelectionOrder {
    fn from(value: selection::Order) -> Self {
        match value {
            selection::Order::Forward => Self::Forward,
            selection::Order::Reverse => Self::Reverse,
            selection::Order::MirroredForward => Self::MirroredForward,
            selection::Order::MirroredReverse => Self::MirroredReverse,
        }
    }
}

impl From<TerminalSelectionOrder> for selection::Order {
    fn from(value: TerminalSelectionOrder) -> Self {
        match value {
            TerminalSelectionOrder::Forward => Self::Forward,
            TerminalSelectionOrder::Reverse => Self::Reverse,
            TerminalSelectionOrder::MirroredForward => Self::MirroredForward,
            TerminalSelectionOrder::MirroredReverse => Self::MirroredReverse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSelectionAdjustment {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    BeginningOfLine,
    EndOfLine,
}

impl TerminalSelectionAdjustment {
    pub(crate) fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            2 => Some(Self::Up),
            3 => Some(Self::Down),
            4 => Some(Self::Home),
            5 => Some(Self::End),
            6 => Some(Self::PageUp),
            7 => Some(Self::PageDown),
            8 => Some(Self::BeginningOfLine),
            9 => Some(Self::EndOfLine),
            _ => None,
        }
    }
}

impl From<TerminalSelectionAdjustment> for selection::Adjustment {
    fn from(value: TerminalSelectionAdjustment) -> Self {
        match value {
            TerminalSelectionAdjustment::Left => Self::Left,
            TerminalSelectionAdjustment::Right => Self::Right,
            TerminalSelectionAdjustment::Up => Self::Up,
            TerminalSelectionAdjustment::Down => Self::Down,
            TerminalSelectionAdjustment::Home => Self::Home,
            TerminalSelectionAdjustment::End => Self::End,
            TerminalSelectionAdjustment::PageUp => Self::PageUp,
            TerminalSelectionAdjustment::PageDown => Self::PageDown,
            TerminalSelectionAdjustment::BeginningOfLine => Self::BeginningOfLine,
            TerminalSelectionAdjustment::EndOfLine => Self::EndOfLine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalGridRefPointError {
    InvalidValue,
    NoValue,
}

impl From<GridRef> for TerminalGridRef {
    fn from(value: GridRef) -> Self {
        Self {
            node: value.node,
            x: value.x,
            y: value.y,
        }
    }
}

impl From<TerminalGridRef> for GridRef {
    fn from(value: TerminalGridRef) -> Self {
        Self {
            node: value.node,
            x: value.x,
            y: value.y,
        }
    }
}

impl TerminalGridRef {
    pub(crate) fn cell_raw(self) -> Result<u64, TerminalGridRefPointError> {
        GridRef::from(self).cell_raw().map_err(Into::into)
    }

    pub(crate) fn row_raw(self) -> Result<u64, TerminalGridRefPointError> {
        GridRef::from(self).row_raw().map_err(Into::into)
    }

    pub(crate) fn graphemes(self) -> Result<Vec<u32>, TerminalGridRefPointError> {
        GridRef::from(self).graphemes().map_err(Into::into)
    }

    pub(crate) fn hyperlink_uri(self) -> Result<Vec<u8>, TerminalGridRefPointError> {
        GridRef::from(self).hyperlink_uri().map_err(Into::into)
    }

    pub(crate) fn style(self) -> Result<super::style::Style, TerminalGridRefPointError> {
        GridRef::from(self).style().map_err(Into::into)
    }
}

impl From<GridRefPointError> for TerminalGridRefPointError {
    fn from(value: GridRefPointError) -> Self {
        match value {
            GridRefPointError::InvalidValue => Self::InvalidValue,
            GridRefPointError::NoValue => Self::NoValue,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalEffectString {
    pub(crate) ptr: *const c_char,
    pub(crate) len: usize,
    pub(crate) sentinel: bool,
}

pub(crate) type TerminalWritePtyCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *const u8, usize);
pub(crate) type TerminalBellCallback = unsafe extern "C" fn(*mut c_void, *mut c_void);
pub(crate) type TerminalEnquiryCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void) -> TerminalEffectString;
pub(crate) type TerminalXtversionCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void) -> TerminalEffectString;
pub(crate) type TerminalTitleChangedCallback = unsafe extern "C" fn(*mut c_void, *mut c_void);
pub(crate) type TerminalSizeCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut size_report::Size) -> bool;
pub(crate) type TerminalColorSchemeCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32) -> bool;
pub(crate) type TerminalDeviceAttributesCallback =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut TerminalDeviceAttributes) -> bool;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalDeviceAttributesPrimary {
    pub(crate) conformance_level: u16,
    pub(crate) features: [u16; 64],
    pub(crate) num_features: usize,
}

impl Default for TerminalDeviceAttributesPrimary {
    fn default() -> Self {
        Self {
            conformance_level: 0,
            features: [0; 64],
            num_features: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TerminalDeviceAttributesSecondary {
    pub(crate) device_type: u16,
    pub(crate) firmware_version: u16,
    pub(crate) rom_cartridge: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TerminalDeviceAttributesTertiary {
    pub(crate) unit_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TerminalDeviceAttributes {
    pub(crate) primary: TerminalDeviceAttributesPrimary,
    pub(crate) secondary: TerminalDeviceAttributesSecondary,
    pub(crate) tertiary: TerminalDeviceAttributesTertiary,
}

#[derive(Clone, Copy)]
pub(crate) struct TerminalEffects {
    handle: *mut c_void,
    userdata: *mut c_void,
    write_pty: Option<TerminalWritePtyCallback>,
    bell: Option<TerminalBellCallback>,
    enquiry: Option<TerminalEnquiryCallback>,
    xtversion: Option<TerminalXtversionCallback>,
    title_changed: Option<TerminalTitleChangedCallback>,
    size: Option<TerminalSizeCallback>,
    color_scheme: Option<TerminalColorSchemeCallback>,
    device_attributes: Option<TerminalDeviceAttributesCallback>,
}

impl std::fmt::Debug for TerminalEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalEffects")
            .field("handle", &self.handle)
            .field("userdata", &self.userdata)
            .field("write_pty", &self.write_pty.is_some())
            .field("bell", &self.bell.is_some())
            .field("enquiry", &self.enquiry.is_some())
            .field("xtversion", &self.xtversion.is_some())
            .field("title_changed", &self.title_changed.is_some())
            .field("size", &self.size.is_some())
            .field("color_scheme", &self.color_scheme.is_some())
            .field("device_attributes", &self.device_attributes.is_some())
            .finish()
    }
}

impl Default for TerminalEffects {
    fn default() -> Self {
        Self {
            handle: std::ptr::null_mut(),
            userdata: std::ptr::null_mut(),
            write_pty: None,
            bell: None,
            enquiry: None,
            xtversion: None,
            title_changed: None,
            size: None,
            color_scheme: None,
            device_attributes: None,
        }
    }
}

fn copied_effect_string<const MAX_LEN: usize>(value: TerminalEffectString) -> Option<Vec<u8>> {
    if value.ptr.is_null() || value.len == 0 || value.len > MAX_LEN {
        return None;
    }

    let bytes = unsafe { std::slice::from_raw_parts(value.ptr.cast::<u8>(), value.len) };
    Some(bytes.to_vec())
}

impl TerminalScreens {
    fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
    ) -> Result<Self, PageListAllocError> {
        Ok(Self {
            primary: Screen::init(cols, rows, max_scrollback_rows)?,
            alternate: None,
            active: TerminalScreenKey::Primary,
            primary_generation: 0,
            alternate_generation: 0,
            primary_owner_id: 1,
            alternate_owner_id: 0,
            next_screen_owner_id: 2,
            active_epoch: 0,
        })
    }

    fn active(&self) -> &Screen {
        match self.active {
            TerminalScreenKey::Primary => &self.primary,
            TerminalScreenKey::Alternate => self
                .alternate
                .as_ref()
                .expect("alternate screen must exist when active"),
        }
    }

    fn active_mut(&mut self) -> &mut Screen {
        match self.active {
            TerminalScreenKey::Primary => &mut self.primary,
            TerminalScreenKey::Alternate => self
                .alternate
                .as_mut()
                .expect("alternate screen must exist when active"),
        }
    }

    fn active_key(&self) -> TerminalScreenKey {
        self.active
    }

    fn active_generation(&self) -> u64 {
        self.generation(self.active)
    }

    fn active_owner_id(&self) -> u64 {
        self.owner_id(self.active)
            .expect("active screen must exist")
    }

    fn active_epoch(&self) -> u64 {
        self.active_epoch
    }

    fn generation(&self, key: TerminalScreenKey) -> u64 {
        match key {
            TerminalScreenKey::Primary => self.primary_generation,
            TerminalScreenKey::Alternate => self.alternate_generation,
        }
    }

    fn owner_id(&self, key: TerminalScreenKey) -> Option<u64> {
        match key {
            TerminalScreenKey::Primary => Some(self.primary_owner_id),
            TerminalScreenKey::Alternate => {
                self.alternate.as_ref().map(|_| self.alternate_owner_id)
            }
        }
    }

    fn screen(&self, key: TerminalScreenKey) -> Option<&Screen> {
        match key {
            TerminalScreenKey::Primary => Some(&self.primary),
            TerminalScreenKey::Alternate => self.alternate.as_ref(),
        }
    }

    fn screen_mut(&mut self, key: TerminalScreenKey) -> Option<&mut Screen> {
        match key {
            TerminalScreenKey::Primary => Some(&mut self.primary),
            TerminalScreenKey::Alternate => self.alternate.as_mut(),
        }
    }

    fn ensure_alternate(
        &mut self,
        cols: CellCountInt,
        rows: CellCountInt,
        kitty_config: KittyGraphicsConfig,
    ) -> Result<(), PageListAllocError> {
        if self.alternate.is_none() {
            let mut alternate = Screen::init(cols, rows, Some(0))?;
            alternate
                .apply_kitty_config(kitty_config.image_storage_limit, kitty_config.image_limits);
            self.alternate = Some(alternate);
            self.alternate_generation = self.alternate_generation.wrapping_add(1);
            self.alternate_owner_id = self.next_screen_owner_id;
            self.next_screen_owner_id = self.next_screen_owner_id.wrapping_add(1).max(1);
        }
        Ok(())
    }

    fn switch_to(
        &mut self,
        key: TerminalScreenKey,
        cols: CellCountInt,
        rows: CellCountInt,
        kitty_config: KittyGraphicsConfig,
    ) -> Result<Option<TerminalScreenKey>, PageListAllocError> {
        if self.active == key {
            return Ok(None);
        }

        if key == TerminalScreenKey::Alternate {
            self.ensure_alternate(cols, rows, kitty_config)?;
        }

        let old = self.active;
        let charset = self.active().charset_state();
        self.active_mut().clear_cursor_hyperlink();
        self.active = key;
        self.active_epoch = self.active_epoch.wrapping_add(1);
        self.active_mut().set_charset_state(charset);
        self.active_mut().mark_active_rows_dirty();
        Ok(Some(old))
    }

    fn copy_cursor_from_to(&mut self, from: TerminalScreenKey, to: TerminalScreenKey) {
        if from == to {
            return;
        }

        match (from, to) {
            (TerminalScreenKey::Primary, TerminalScreenKey::Alternate) => {
                let alternate = self
                    .alternate
                    .as_mut()
                    .expect("alternate screen must exist before cursor copy");
                alternate.copy_cursor_from_without_hyperlink(&self.primary);
            }
            (TerminalScreenKey::Alternate, TerminalScreenKey::Primary) => {
                let alternate = self
                    .alternate
                    .as_ref()
                    .expect("alternate screen must exist before cursor copy");
                self.primary.copy_cursor_from_without_hyperlink(alternate);
            }
            _ => {}
        }
    }

    fn reset(&mut self, kitty_config: KittyGraphicsConfig) {
        self.primary
            .reset_with_kitty_config(kitty_config.image_storage_limit, kitty_config.image_limits);
        self.primary_generation = self.primary_generation.wrapping_add(1);
        if self.alternate.is_some() {
            self.alternate_generation = self.alternate_generation.wrapping_add(1);
        }
        self.alternate = None;
        self.alternate_owner_id = 0;
        if self.active != TerminalScreenKey::Primary {
            self.active_epoch = self.active_epoch.wrapping_add(1);
        }
        self.active = TerminalScreenKey::Primary;
    }

    fn apply_kitty_config(&mut self, kitty_config: KittyGraphicsConfig) {
        self.primary
            .apply_kitty_config(kitty_config.image_storage_limit, kitty_config.image_limits);
        if let Some(alternate) = self.alternate.as_mut() {
            alternate
                .apply_kitty_config(kitty_config.image_storage_limit, kitty_config.image_limits);
        }
    }

    #[cfg(test)]
    fn alternate_initialized_for_tests(&self) -> bool {
        self.alternate.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalSize {
    cols: CellCountInt,
    rows: CellCountInt,
}

#[derive(Debug, Clone, Copy)]
struct TerminalColors {
    palette: color::DynamicPalette,
    foreground: color::DynamicRgb,
    background: color::DynamicRgb,
    cursor: color::DynamicRgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalColorKind {
    Foreground,
    Background,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScrollingRegion {
    top: CellCountInt,
    bottom: CellCountInt,
    left: CellCountInt,
    right: CellCountInt,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TerminalFlags {
    modify_other_keys_2: bool,
    mouse_event: mouse::MouseEventMode,
    mouse_format: mouse::MouseFormat,
    mouse_shift_capture: Option<bool>,
    /// Set by the renderer when the viewport/active area changes, so the search thread re-searches
    /// the viewport (upstream `Terminal.flags.search_viewport_dirty`). roastty has no renderer port
    /// yet, so only the search thread's `feed` reads/clears it and tests set it.
    search_viewport_dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KittyGraphicsConfig {
    image_storage_limit: usize,
    image_limits: LoadingImageLimits,
    apc_max_bytes: Option<usize>,
    kitty_apc_max_bytes: Option<usize>,
}

impl Default for KittyGraphicsConfig {
    fn default() -> Self {
        Self {
            image_storage_limit: DEFAULT_TOTAL_LIMIT,
            image_limits: LoadingImageLimits::DIRECT,
            apc_max_bytes: None,
            kitty_apc_max_bytes: None,
        }
    }
}

impl KittyGraphicsConfig {
    fn effective_kitty_apc_max_bytes(self) -> usize {
        self.kitty_apc_max_bytes
            .or(self.apc_max_bytes)
            .unwrap_or(MAX_IMAGE_SIZE)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TerminalTitle {
    text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TerminalPwd {
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalStreamError {
    PageAlloc,
    ManagedCellUnsupported,
    InvalidPoint,
    UnsupportedCodepoint(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KittyImageMedium {
    File,
    TemporaryFile,
    SharedMemory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalInitError {
    PageAlloc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalInitOptions {
    pub(crate) cursor_visual_style: cursor::VisualStyle,
    pub(crate) cursor_blink: Option<bool>,
    pub(crate) title_report: bool,
}

impl Default for TerminalInitOptions {
    fn default() -> Self {
        Self {
            cursor_visual_style: cursor::VisualStyle::Block,
            cursor_blink: None,
            title_report: false,
        }
    }
}

struct TerminalStreamHandler<'a> {
    screens: &'a mut TerminalScreens,
    size: TerminalSize,
    colors: &'a mut TerminalColors,
    modes: &'a mut modes::ModeState,
    scrolling_region: &'a mut ScrollingRegion,
    tabstops: &'a mut tabstops::Tabstops,
    pty_response: &'a mut Vec<u8>,
    effects: &'a mut TerminalEffects,
    title: &'a mut TerminalTitle,
    pwd: &'a mut TerminalPwd,
    mouse_shape: &'a mut mouse::MouseShape,
    title_report: &'a bool,
    next_implicit_hyperlink_id: &'a mut u32,
    previous_char: &'a mut Option<char>,
    dcs: &'a mut dcs::Handler,
    tmux_viewer: &'a mut Option<tmux::TmuxViewer>,
    tmux_windows: &'a mut Vec<tmux::TmuxWindow>,
    kitty_graphics: &'a mut KittyGraphicsApc,
    kitty_config: &'a mut KittyGraphicsConfig,
    flags: &'a mut TerminalFlags,
    pending_clipboard_events: &'a mut Vec<TerminalClipboardEvent>,
    pending_bell_count: &'a mut usize,
    default_cursor_visual_style: cursor::VisualStyle,
    default_cursor_blink: Option<bool>,
}

#[derive(Debug)]
struct KittyGraphicsApc {
    state: KittyGraphicsApcState,
    max_bytes: usize,
    metrics_px: Option<(u32, u32)>,
}

#[derive(Debug)]
enum KittyGraphicsApcState {
    Idle,
    PendingFirstByte,
    Draining,
    Parsing(Parser),
    Failed,
}

impl Default for KittyGraphicsApc {
    fn default() -> Self {
        Self {
            state: KittyGraphicsApcState::Idle,
            max_bytes: MAX_IMAGE_SIZE,
            metrics_px: None,
        }
    }
}

impl KittyGraphicsApc {
    fn reset(&mut self) {
        self.state = KittyGraphicsApcState::Idle;
    }

    fn cell_metrics(&self, size: TerminalSize) -> CellMetrics {
        let columns = u32::from(size.cols);
        let rows = u32::from(size.rows);
        let (width_px, height_px) = self.metrics_px.unwrap_or((columns, rows));
        CellMetrics {
            columns,
            rows,
            width_px,
            height_px,
        }
    }

    #[cfg(test)]
    fn set_cell_metrics_for_tests(&mut self, width_px: u32, height_px: u32) {
        self.metrics_px = Some((width_px, height_px));
    }

    fn start(&mut self) {
        self.state = KittyGraphicsApcState::PendingFirstByte;
    }

    fn put(&mut self, byte: u8) {
        match &mut self.state {
            KittyGraphicsApcState::Idle => {}
            KittyGraphicsApcState::PendingFirstByte => {
                if byte == b'G' {
                    self.state = KittyGraphicsApcState::Parsing(Parser::new(self.max_bytes));
                } else {
                    self.state = KittyGraphicsApcState::Draining;
                }
            }
            KittyGraphicsApcState::Parsing(parser) => {
                if parser.feed(byte).is_err() {
                    self.state = KittyGraphicsApcState::Failed;
                }
            }
            KittyGraphicsApcState::Draining | KittyGraphicsApcState::Failed => {}
        }
    }

    fn end(&mut self) -> Option<Command> {
        let state = std::mem::replace(&mut self.state, KittyGraphicsApcState::Idle);
        match state {
            KittyGraphicsApcState::Parsing(mut parser) => parser.complete().ok(),
            KittyGraphicsApcState::Idle
            | KittyGraphicsApcState::PendingFirstByte
            | KittyGraphicsApcState::Draining
            | KittyGraphicsApcState::Failed => None,
        }
    }

    #[cfg(test)]
    fn set_max_bytes_for_tests(&mut self, max_bytes: usize) {
        self.set_max_bytes(max_bytes);
    }

    fn set_max_bytes(&mut self, max_bytes: usize) {
        self.max_bytes = max_bytes;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerminalFormatterOptions<'a> {
    screen: ScreenFormatterOptions<'a>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerminalFormatter<'a> {
    terminal: &'a Terminal,
    options: TerminalFormatterOptions<'a>,
    content: ScreenFormatterContent,
    extra: TerminalFormatterExtra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TerminalFormatterExtra {
    palette: bool,
    modes: bool,
    scrolling_region: bool,
    tabstops: bool,
    keyboard: bool,
    pwd: bool,
    screen: ScreenFormatterExtra,
}

/// The `CSI ? 997` color-scheme report bytes for `scheme` (0 = light → `;2n`, 1 = dark → `;1n`),
/// or `None` for an unknown scheme. Shared by the DSR query path
/// (`TerminalStreamHandler::write_color_scheme_report`) and the live change report
/// (`Terminal::report_color_scheme_change`) — Issue 802 / Exp 36.
fn color_scheme_report_bytes(scheme: i32) -> Option<&'static [u8]> {
    match scheme {
        1 => Some(b"\x1b[?997;1n"), // dark
        0 => Some(b"\x1b[?997;2n"), // light
        _ => None,
    }
}

impl Terminal {
    /// Whether the renderer marked the viewport/active area dirty (upstream
    /// `Terminal.flags.search_viewport_dirty`). Raw-pointer read for the search thread's `feed`,
    /// which holds cached pointers into this terminal's screens — so this projects through a raw
    /// pointer rather than materializing a `&Terminal`.
    ///
    /// # Safety
    /// `t` must be live and exclusively accessed (the search thread holds the lock).
    pub(in crate::terminal) unsafe fn search_viewport_dirty(t: NonNull<Terminal>) -> bool {
        // SAFETY: caller's contract — `t` is live.
        unsafe { core::ptr::addr_of!((*t.as_ptr()).flags.search_viewport_dirty).read() }
    }

    /// Clear the viewport-dirty flag (upstream `t.flags.search_viewport_dirty = false`).
    ///
    /// # Safety
    /// As `search_viewport_dirty`.
    pub(in crate::terminal) unsafe fn clear_search_viewport_dirty(t: NonNull<Terminal>) {
        // SAFETY: caller's contract — `t` is live.
        unsafe { core::ptr::addr_of_mut!((*t.as_ptr()).flags.search_viewport_dirty).write(false) }
    }

    /// Mark the viewport dirty (upstream's renderer write; here for the future renderer port and
    /// tests).
    ///
    /// # Safety
    /// As `search_viewport_dirty`.
    pub(in crate::terminal) unsafe fn mark_search_viewport_dirty(t: NonNull<Terminal>) {
        // SAFETY: caller's contract — `t` is live.
        unsafe { core::ptr::addr_of_mut!((*t.as_ptr()).flags.search_viewport_dirty).write(true) }
    }

    /// The active screen key (upstream `t.screens.active_key`).
    ///
    /// # Safety
    /// As `search_viewport_dirty`.
    pub(in crate::terminal) unsafe fn search_active_screen_key(
        t: NonNull<Terminal>,
    ) -> TerminalScreenKey {
        // SAFETY: caller's contract — `t` is live.
        unsafe { core::ptr::addr_of!((*t.as_ptr()).screens.active).read() }
    }

    /// The present screens as raw pointers (upstream iterating `t.screens.all`): `primary` always,
    /// `alternate` only when it exists. Pointers are projected so they alias this terminal's screen
    /// storage directly (no intermediate `&mut Terminal`).
    ///
    /// # Safety
    /// As `search_viewport_dirty`.
    pub(in crate::terminal) unsafe fn present_screen_ptrs(
        t: NonNull<Terminal>,
    ) -> Vec<(TerminalScreenKey, NonNull<Screen>)> {
        // SAFETY: caller's contract — `t` is live.
        let screens = unsafe { core::ptr::addr_of_mut!((*t.as_ptr()).screens) };
        let mut out = Vec::new();
        // SAFETY: `primary` always exists.
        let primary = unsafe { core::ptr::addr_of_mut!((*screens).primary) };
        out.push((TerminalScreenKey::Primary, NonNull::new(primary).unwrap()));
        // SAFETY: project the alternate through a *shared* reference (only its pointer identity is
        // needed). Using `as_ref` rather than `as_mut` avoids a mutable retag that could invalidate a
        // cached `ScreenSearch` pointer to the same alternate screen.
        if let Some(alt) = unsafe { (*core::ptr::addr_of!((*screens).alternate)).as_ref() } {
            out.push((TerminalScreenKey::Alternate, NonNull::from(alt)));
        }
        out
    }

    pub(crate) fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
    ) -> Result<Self, TerminalInitError> {
        Self::init_with_options(
            cols,
            rows,
            max_scrollback_rows,
            TerminalInitOptions::default(),
        )
    }

    pub(crate) fn init_with_options(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
        options: TerminalInitOptions,
    ) -> Result<Self, TerminalInitError> {
        let size = TerminalSize { cols, rows };
        let mut screens = TerminalScreens::init(cols, rows, max_scrollback_rows)
            .map_err(|_| TerminalInitError::PageAlloc)?;
        screens
            .active_mut()
            .set_cursor_visual_style(options.cursor_visual_style);
        let mut modes = modes::ModeState::default();
        modes.set(
            modes::Mode::CursorBlinking,
            options.cursor_blink.unwrap_or(true),
        );
        Ok(Self {
            size,
            screens,
            colors: TerminalColors {
                palette: color::DynamicPalette::init(color::DEFAULT_PALETTE),
                foreground: color::DynamicRgb::unset(),
                background: color::DynamicRgb::unset(),
                cursor: color::DynamicRgb::unset(),
            },
            modes,
            scrolling_region: ScrollingRegion::full(size),
            tabstops: tabstops::Tabstops::new(cols as usize, TABSTOP_INTERVAL)
                .map_err(|_| TerminalInitError::PageAlloc)?,
            pty_response: Vec::new(),
            effects: TerminalEffects::default(),
            stream: stream::Stream::init(),
            dcs: dcs::Handler::new(),
            tmux_viewer: None,
            tmux_windows: Vec::new(),
            kitty_graphics: KittyGraphicsApc::default(),
            kitty_config: KittyGraphicsConfig::default(),
            flags: TerminalFlags::default(),
            title: TerminalTitle::default(),
            pwd: TerminalPwd::default(),
            mouse_shape: mouse::MouseShape::Text,
            title_report: options.title_report,
            next_implicit_hyperlink_id: 0,
            previous_char: None,
            pending_clipboard_events: Vec::new(),
            pending_bell_count: 0,
            default_cursor_visual_style: options.cursor_visual_style,
            default_cursor_blink: options.cursor_blink,
        })
    }

    pub(crate) fn next_slice(&mut self, input: &[u8]) -> Result<(), TerminalStreamError> {
        let Terminal {
            size,
            screens,
            colors,
            modes,
            scrolling_region,
            tabstops,
            pty_response,
            effects,
            stream,
            dcs,
            tmux_viewer,
            tmux_windows,
            kitty_graphics,
            kitty_config,
            title,
            pwd,
            mouse_shape,
            title_report,
            next_implicit_hyperlink_id,
            previous_char,
            pending_clipboard_events,
            pending_bell_count,
            flags,
            default_cursor_visual_style,
            default_cursor_blink,
            ..
        } = self;
        let mut handler = TerminalStreamHandler {
            screens,
            size: *size,
            colors,
            modes,
            scrolling_region,
            tabstops,
            pty_response,
            effects,
            title,
            pwd,
            mouse_shape,
            title_report,
            next_implicit_hyperlink_id,
            previous_char,
            dcs,
            tmux_viewer,
            tmux_windows,
            kitty_graphics,
            kitty_config,
            flags,
            pending_clipboard_events,
            pending_bell_count,
            default_cursor_visual_style: *default_cursor_visual_style,
            default_cursor_blink: *default_cursor_blink,
        };
        stream.next_slice(input, &mut handler)
    }

    pub(crate) fn reset(&mut self) {
        self.screens.reset(self.kitty_config);
        self.modes.reset();
        self.scrolling_region = ScrollingRegion::full(self.size);
        self.tabstops.reset(TABSTOP_INTERVAL);
        self.title.clear();
        self.pwd.clear();
        self.dcs = dcs::Handler::new();
        self.tmux_viewer = None;
        self.tmux_windows.clear();
        self.kitty_graphics.reset();
        self.flags = TerminalFlags::default();
        self.previous_char = None;
        self.pending_clipboard_events.clear();
    }

    pub(crate) fn drain_clipboard_events(&mut self) -> Vec<TerminalClipboardEvent> {
        std::mem::take(&mut self.pending_clipboard_events)
    }

    pub(crate) fn take_pending_bell_count(&mut self) -> usize {
        std::mem::take(&mut self.pending_bell_count)
    }

    pub(crate) fn title(&self) -> &str {
        self.title.as_str()
    }

    pub(crate) fn pwd(&self) -> Option<&str> {
        self.pwd.logical_str()
    }

    pub(crate) fn set_title(&mut self, value: Option<String>) {
        self.title.text = value.unwrap_or_default();
    }

    pub(crate) fn set_pwd(&mut self, stored_value: Option<String>) {
        self.pwd.text = stored_value.unwrap_or_default();
    }

    pub(crate) fn set_effect_handle(&mut self, handle: *mut c_void) {
        self.effects.handle = handle;
    }

    pub(crate) fn set_effect_userdata(&mut self, userdata: *mut c_void) {
        self.effects.userdata = userdata;
    }

    pub(crate) fn set_write_pty_callback(&mut self, callback: Option<TerminalWritePtyCallback>) {
        self.effects.write_pty = callback;
    }

    pub(crate) fn set_bell_callback(&mut self, callback: Option<TerminalBellCallback>) {
        self.effects.bell = callback;
    }

    pub(crate) fn set_enquiry_callback(&mut self, callback: Option<TerminalEnquiryCallback>) {
        self.effects.enquiry = callback;
    }

    pub(crate) fn set_xtversion_callback(&mut self, callback: Option<TerminalXtversionCallback>) {
        self.effects.xtversion = callback;
    }

    pub(crate) fn set_title_changed_callback(
        &mut self,
        callback: Option<TerminalTitleChangedCallback>,
    ) {
        self.effects.title_changed = callback;
    }

    pub(crate) fn set_title_report(&mut self, enabled: bool) {
        self.title_report = enabled;
    }

    pub(crate) fn set_size_callback(&mut self, callback: Option<TerminalSizeCallback>) {
        self.effects.size = callback;
    }

    pub(crate) fn set_color_scheme_callback(
        &mut self,
        callback: Option<TerminalColorSchemeCallback>,
    ) {
        self.effects.color_scheme = callback;
    }

    pub(crate) fn set_device_attributes_callback(
        &mut self,
        callback: Option<TerminalDeviceAttributesCallback>,
    ) {
        self.effects.device_attributes = callback;
    }

    pub(crate) fn has_effect_callbacks(&self) -> bool {
        self.effects.write_pty.is_some()
            || self.effects.bell.is_some()
            || self.effects.enquiry.is_some()
            || self.effects.xtversion.is_some()
            || self.effects.title_changed.is_some()
            || self.effects.size.is_some()
            || self.effects.color_scheme.is_some()
            || self.effects.device_attributes.is_some()
    }

    pub(crate) fn color_effective(&self, kind: TerminalColorKind) -> Option<(u8, u8, u8)> {
        self.dynamic_color_by_kind(kind).get().map(rgb_tuple)
    }

    pub(crate) fn color_default(&self, kind: TerminalColorKind) -> Option<(u8, u8, u8)> {
        self.dynamic_color_by_kind(kind)
            .default_rgb()
            .map(rgb_tuple)
    }

    pub(crate) fn set_color_default(&mut self, kind: TerminalColorKind, rgb: Option<(u8, u8, u8)>) {
        self.dynamic_color_mut_by_kind(kind)
            .set_default(rgb.map(rgb_from_tuple));
    }

    fn dynamic_color_mut_by_kind(&mut self, kind: TerminalColorKind) -> &mut color::DynamicRgb {
        match kind {
            TerminalColorKind::Foreground => &mut self.colors.foreground,
            TerminalColorKind::Background => &mut self.colors.background,
            TerminalColorKind::Cursor => &mut self.colors.cursor,
        }
    }

    fn dynamic_color_by_kind(&self, kind: TerminalColorKind) -> &color::DynamicRgb {
        match kind {
            TerminalColorKind::Foreground => &self.colors.foreground,
            TerminalColorKind::Background => &self.colors.background,
            TerminalColorKind::Cursor => &self.colors.cursor,
        }
    }

    pub(crate) fn palette_current(&self) -> [(u8, u8, u8); 256] {
        palette_tuple(*self.colors.palette.current())
    }

    pub(crate) fn palette_default(&self) -> [(u8, u8, u8); 256] {
        palette_tuple(*self.colors.palette.original())
    }

    pub(crate) fn set_palette_default(&mut self, palette: Option<[(u8, u8, u8); 256]>) {
        self.colors.palette.change_default(match palette {
            Some(palette) => palette_from_tuple(palette),
            None => color::DEFAULT_PALETTE,
        });
    }

    pub(crate) fn cursor_position(&self) -> (CellCountInt, CellCountInt) {
        self.screens.active().cursor_position()
    }

    /// The cursor's VIEWPORT position, or `None` when scrolled into scrollback so the cursor's
    /// active row isn't visible (Issue 802 / Exp 24). Used by the renderer's cursor-block path.
    pub(crate) fn cursor_viewport_position(&self) -> Option<(CellCountInt, CellCountInt)> {
        self.screens.active().cursor_viewport_position()
    }

    pub(crate) fn columns(&self) -> CellCountInt {
        self.size.cols
    }

    pub(crate) fn rows(&self) -> CellCountInt {
        self.size.rows
    }

    pub(crate) fn cursor_pending_wrap(&self) -> bool {
        self.screens.active().cursor_pending_wrap()
    }

    pub(crate) fn active_screen(&self) -> TerminalScreen {
        match self.screens.active_key() {
            TerminalScreenKey::Primary => TerminalScreen::Primary,
            TerminalScreenKey::Alternate => TerminalScreen::Alternate,
        }
    }

    pub(crate) fn cursor_is_at_prompt(&self) -> bool {
        match self.screens.active_key() {
            TerminalScreenKey::Primary => self.screens.active().cursor_is_at_prompt(),
            TerminalScreenKey::Alternate => false,
        }
    }

    pub(crate) fn clear_screen(
        &mut self,
        history: bool,
    ) -> Result<ClearScreenResult, TerminalStreamError> {
        if self.screens.active_key() == TerminalScreenKey::Alternate {
            return Ok(ClearScreenResult::NotPerformed);
        }

        let at_prompt = self.cursor_is_at_prompt();
        let rows = self.size.rows;
        let cols = self.size.cols;
        let screen = self.screens.active_mut();
        screen.clear_selection();
        if history && screen.total_rows() > usize::from(rows) {
            screen.erase_display_basic(stream::EraseDisplayMode::Scrollback, rows, cols, false)?;
        }

        if !at_prompt {
            screen.clear_screen_rows_above_cursor()?;
            return Ok(ClearScreenResult::Performed);
        }

        screen.erase_display_basic(stream::EraseDisplayMode::Complete, rows, cols, false)?;
        Ok(ClearScreenResult::SendFormFeed)
    }

    pub(in crate::terminal) fn switch_tmux_screen(
        &mut self,
        screen: TerminalScreen,
    ) -> Result<(), TerminalStreamError> {
        let target = match screen {
            TerminalScreen::Primary => TerminalScreenKey::Primary,
            TerminalScreen::Alternate => TerminalScreenKey::Alternate,
        };
        self.screens
            .switch_to(target, self.size.cols, self.size.rows, self.kitty_config)
            .map(|_| ())
            .map_err(|_| TerminalStreamError::PageAlloc)
    }

    pub(in crate::terminal) fn prepare_tmux_visible_capture(
        &mut self,
    ) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .erase_display_basic(
                stream::EraseDisplayMode::Complete,
                self.size.rows,
                self.size.cols,
                false,
            )
            .map_err(TerminalStreamError::from)?;
        self.screens
            .active_mut()
            .cursor_position_basic(1, 1, self.size.rows, self.size.cols);
        Ok(())
    }

    pub(in crate::terminal) fn finish_tmux_history_capture(
        &mut self,
    ) -> Result<(), TerminalStreamError> {
        self.screens.active_mut().carriage_return_basic();
        for _ in 0..self.size.rows {
            self.screens
                .active_mut()
                .line_feed_basic(self.size.rows, self.size.cols)
                .map_err(TerminalStreamError::from)?;
        }
        self.screens
            .active_mut()
            .cursor_position_basic(1, 1, self.size.rows, self.size.cols);
        Ok(())
    }

    pub(in crate::terminal) fn apply_tmux_cursor_state(
        &mut self,
        screen: TerminalScreen,
        cursor_x: usize,
        cursor_y: usize,
        cursor_shape: &str,
    ) {
        let target = match screen {
            TerminalScreen::Primary => TerminalScreenKey::Primary,
            TerminalScreen::Alternate => TerminalScreenKey::Alternate,
        };
        let Some(screen) = self.screens.screen_mut(target) else {
            return;
        };

        if let (Ok(x), Ok(y)) = (
            CellCountInt::try_from(cursor_x),
            CellCountInt::try_from(cursor_y),
        ) {
            if x < self.size.cols && y < self.size.rows {
                screen.cursor_position_basic(
                    y.saturating_add(1),
                    x.saturating_add(1),
                    self.size.rows,
                    self.size.cols,
                );
            }
        }

        match cursor_shape {
            "block" => screen.set_cursor_visual_style(cursor::VisualStyle::Block),
            "underline" => screen.set_cursor_visual_style(cursor::VisualStyle::Underline),
            "bar" => screen.set_cursor_visual_style(cursor::VisualStyle::Bar),
            _ => {}
        }
    }

    pub(in crate::terminal) fn apply_tmux_alternate_saved_cursor_state(
        &mut self,
        cursor_x: usize,
        cursor_y: usize,
    ) {
        let Some(screen) = self.screens.screen_mut(TerminalScreenKey::Alternate) else {
            return;
        };
        let (Ok(x), Ok(y)) = (
            CellCountInt::try_from(cursor_x),
            CellCountInt::try_from(cursor_y),
        ) else {
            return;
        };
        let cols = screen.cols();
        let rows = screen.rows();
        if x < cols && y < rows {
            screen.cursor_position_basic(y.saturating_add(1), x.saturating_add(1), rows, cols);
        }
    }

    pub(in crate::terminal) fn apply_tmux_mode_state(
        &mut self,
        cursor_visible: bool,
        cursor_blinking: bool,
        insert: bool,
        wraparound: bool,
        keypad_keys: bool,
        cursor_keys: bool,
        origin: bool,
        focus_event: bool,
        bracketed_paste: bool,
    ) {
        // Pane-state restoration mirrors upstream's direct mode writes; do not
        // route this through normal mode handling, which has cursor side effects.
        self.modes.set(modes::Mode::CursorVisible, cursor_visible);
        self.modes.set(modes::Mode::CursorBlinking, cursor_blinking);
        self.modes.set(modes::Mode::Insert, insert);
        self.modes.set(modes::Mode::Wraparound, wraparound);
        self.modes.set(modes::Mode::KeypadKeys, keypad_keys);
        self.modes.set(modes::Mode::CursorKeys, cursor_keys);
        self.modes.set(modes::Mode::Origin, origin);
        self.modes.set(modes::Mode::FocusEvent, focus_event);
        self.modes.set(modes::Mode::BracketedPaste, bracketed_paste);
    }

    pub(in crate::terminal) fn apply_tmux_mouse_mode_state(
        &mut self,
        mouse_all: bool,
        mouse_any: bool,
        mouse_button: bool,
        mouse_standard: bool,
        mouse_utf8: bool,
        mouse_sgr: bool,
    ) {
        // Upstream tmux pane-state restoration updates mode bits directly here;
        // Roastty's mouse runtime caches stay unchanged until input integration.
        self.modes.set(modes::Mode::MouseEventAny, mouse_all);
        self.modes.set(modes::Mode::MouseEventButton, mouse_any);
        self.modes.set(modes::Mode::MouseEventNormal, mouse_button);
        self.modes.set(modes::Mode::MouseEventX10, mouse_standard);
        self.modes.set(modes::Mode::MouseFormatUtf8, mouse_utf8);
        self.modes.set(modes::Mode::MouseFormatSgr, mouse_sgr);
    }

    pub(in crate::terminal) fn apply_tmux_scroll_region_state(
        &mut self,
        top: usize,
        bottom: usize,
    ) {
        let (Ok(top), Ok(bottom)) = (CellCountInt::try_from(top), CellCountInt::try_from(bottom))
        else {
            return;
        };
        let region = ScrollingRegion {
            top,
            bottom,
            left: self.scrolling_region.left,
            right: self.scrolling_region.right,
        };
        if region.is_valid(self.size) {
            self.scrolling_region = region;
        }
    }

    pub(in crate::terminal) fn apply_tmux_tabstops_state(&mut self, pane_tabs: &str) {
        self.tabstops.reset(0);
        for tab in pane_tabs.split(',') {
            let Ok(col) = tab.parse::<usize>() else {
                continue;
            };
            if col < self.tabstops.cols() {
                self.tabstops.set(col);
            }
        }
    }

    pub(crate) fn cursor_visible(&self) -> bool {
        self.modes.get(modes::Mode::CursorVisible)
    }

    pub(crate) fn cursor_blinking(&self) -> bool {
        self.modes.get(modes::Mode::CursorBlinking)
    }

    pub(crate) fn cursor_visual_style(&self) -> cursor::VisualStyle {
        self.screens.active().cursor_visual_style()
    }

    pub(crate) fn kitty_keyboard_flags(&self) -> u8 {
        self.screens.active().kitty_keyboard_flags().int()
    }

    pub(crate) fn key_encode_options(&self) -> key_encode::Options {
        key_encode::Options {
            cursor_key_application: self.modes.get(modes::Mode::CursorKeys),
            keypad_key_application: self.modes.get(modes::Mode::KeypadKeys),
            backarrow_key_mode: self.modes.get(modes::Mode::BackarrowKeyMode),
            ignore_keypad_with_numlock: self.modes.get(modes::Mode::IgnoreKeypadWithNumlock),
            alt_esc_prefix: self.modes.get(modes::Mode::AltEscPrefix),
            modify_other_keys_state_2: self.flags.modify_other_keys_2,
            kitty_flags: self.screens.active().kitty_keyboard_flags(),
            ..key_encode::Options::default()
        }
    }

    pub(crate) fn kitty_images(&self) -> &super::kitty::graphics_storage::ImageStorage {
        self.screens.active().kitty_images()
    }

    pub(crate) fn kitty_image_storage_limit(&self) -> usize {
        self.screens.active().kitty_images().total_limit
    }

    pub(crate) fn kitty_image_medium_enabled(&self, medium: KittyImageMedium) -> bool {
        let limits = self.screens.active().kitty_images().image_limits;
        match medium {
            KittyImageMedium::File => limits.file,
            KittyImageMedium::TemporaryFile => limits.temporary_file,
            KittyImageMedium::SharedMemory => limits.shared_memory,
        }
    }

    pub(crate) fn set_kitty_image_storage_limit(&mut self, limit: usize) {
        self.kitty_config.image_storage_limit = limit;
        self.screens.apply_kitty_config(self.kitty_config);
    }

    pub(crate) fn set_kitty_image_medium(&mut self, medium: KittyImageMedium, enabled: bool) {
        match medium {
            KittyImageMedium::File => self.kitty_config.image_limits.file = enabled,
            KittyImageMedium::TemporaryFile => {
                self.kitty_config.image_limits.temporary_file = enabled
            }
            KittyImageMedium::SharedMemory => {
                self.kitty_config.image_limits.shared_memory = enabled
            }
        }
        self.screens.apply_kitty_config(self.kitty_config);
    }

    pub(crate) fn set_apc_max_bytes(&mut self, max_bytes: Option<usize>) {
        self.kitty_config.apc_max_bytes = max_bytes;
        self.apply_effective_kitty_apc_max_bytes();
    }

    pub(crate) fn set_kitty_apc_max_bytes(&mut self, max_bytes: Option<usize>) {
        self.kitty_config.kitty_apc_max_bytes = max_bytes;
        self.apply_effective_kitty_apc_max_bytes();
    }

    fn apply_effective_kitty_apc_max_bytes(&mut self) {
        self.kitty_graphics
            .set_max_bytes(self.kitty_config.effective_kitty_apc_max_bytes());
    }

    pub(crate) fn kitty_cell_metrics(&self) -> CellMetrics {
        self.kitty_graphics.cell_metrics(self.size)
    }

    #[cfg(test)]
    pub(crate) fn set_kitty_cell_metrics_for_tests(&mut self, width_px: u32, height_px: u32) {
        self.kitty_graphics
            .set_cell_metrics_for_tests(width_px, height_px);
    }

    pub(crate) fn kitty_placement(&self, key: PlacementKey) -> Option<Placement> {
        self.screens
            .active()
            .kitty_images()
            .placement_by_key(key)
            .copied()
    }

    pub(crate) fn kitty_placement_selection(
        &self,
        key: PlacementKey,
        image: &Image,
    ) -> Option<TerminalSelection> {
        let placement = self.kitty_placement(key)?;
        let metrics = self.kitty_cell_metrics();
        let (start, end) = self
            .screens
            .active()
            .kitty_placement_grid_refs(placement, image, metrics)?;
        Some(TerminalSelection {
            start: start.into(),
            end: end.into(),
            rectangle: true,
        })
    }

    pub(crate) fn kitty_placement_viewport_pos(
        &self,
        key: PlacementKey,
        image: &Image,
    ) -> Option<(i32, i32, bool)> {
        let placement = self.kitty_placement(key)?;
        Some(self.screens.active().kitty_placement_viewport_pos(
            placement,
            image,
            self.kitty_cell_metrics(),
            u32::from(self.size.rows),
        ))
    }

    pub(crate) fn mouse_tracking(&self) -> bool {
        self.modes.get(modes::Mode::MouseEventX10)
            || self.modes.get(modes::Mode::MouseEventNormal)
            || self.modes.get(modes::Mode::MouseEventButton)
            || self.modes.get(modes::Mode::MouseEventAny)
    }

    pub(crate) fn mouse_event_mode(&self) -> mouse::MouseEventMode {
        self.flags.mouse_event
    }

    pub(crate) fn mouse_format(&self) -> mouse::MouseFormat {
        self.flags.mouse_format
    }

    pub(crate) fn total_rows(&self) -> usize {
        self.screens.active().total_rows()
    }

    pub(crate) fn scrollback_rows(&self) -> usize {
        self.screens.active().scrollback_rows()
    }

    pub(crate) fn grid_ref(
        &self,
        tag: TerminalPointTag,
        coord: super::point::Coordinate,
    ) -> Option<TerminalGridRef> {
        let point = match tag {
            TerminalPointTag::Active => super::point::Point::active(coord),
            TerminalPointTag::Viewport => super::point::Point::viewport(coord),
            TerminalPointTag::Screen => super::point::Point::screen(coord),
            TerminalPointTag::History => super::point::Point::history(coord),
        };

        self.screens.active().grid_ref(point).map(Into::into)
    }

    /// Resolve an embedded `selection_s` (two `point_s` each `(tag, coord, x, y)`) into a
    /// terminal selection (Issue 802 / Exp 11), matching `apprt/embedded.zig`: `Exact`
    /// resolves the `(x, y)` grid point, `TopLeft`/`BottomRight` resolve the tag region's
    /// corner. Returns `None` if either endpoint can't be resolved.
    pub(crate) fn resolve_embedded_selection(
        &self,
        start: (
            TerminalPointTag,
            EmbeddedPointCoord,
            super::point::Coordinate,
        ),
        end: (
            TerminalPointTag,
            EmbeddedPointCoord,
            super::point::Coordinate,
        ),
        rectangle: bool,
    ) -> Option<TerminalSelection> {
        Some(TerminalSelection {
            start: self.resolve_embedded_grid_ref(start.0, start.1, start.2)?,
            end: self.resolve_embedded_grid_ref(end.0, end.1, end.2)?,
            rectangle,
        })
    }

    fn resolve_embedded_grid_ref(
        &self,
        tag: TerminalPointTag,
        coord_kind: EmbeddedPointCoord,
        coord: super::point::Coordinate,
    ) -> Option<TerminalGridRef> {
        let screen = self.screens.active();
        let grid_ref = match coord_kind {
            EmbeddedPointCoord::Exact => {
                // Clamp to the screen bounds before resolving, matching upstream
                // (`embedded.zig`: `clamped_x = @min(x, cols-1)`, `clamped_y = @min(y, rows-1)`)
                // — an out-of-edge exact point (routine for pixel->cell mouse drags) yields the
                // edge cell rather than failing.
                let clamped = super::point::Coordinate::new(
                    coord.x.min(screen.cols().saturating_sub(1)),
                    coord.y.min(u32::from(screen.rows().saturating_sub(1))),
                );
                let point = match tag {
                    TerminalPointTag::Active => super::point::Point::active(clamped),
                    TerminalPointTag::Viewport => super::point::Point::viewport(clamped),
                    TerminalPointTag::Screen => super::point::Point::screen(clamped),
                    TerminalPointTag::History => super::point::Point::history(clamped),
                };
                screen.grid_ref(point)?
            }
            EmbeddedPointCoord::TopLeft => {
                super::page_list::GridRef::from(screen.pages().get_top_left(tag.into()))
            }
            EmbeddedPointCoord::BottomRight => {
                super::page_list::GridRef::from(screen.pages().get_bottom_right(tag.into())?)
            }
        };
        Some(grid_ref.into())
    }

    pub(crate) fn track_grid_ref(
        &mut self,
        tag: TerminalPointTag,
        coord: super::point::Coordinate,
    ) -> Option<TerminalTrackedGridRef> {
        let point = match tag {
            TerminalPointTag::Active => super::point::Point::active(coord),
            TerminalPointTag::Viewport => super::point::Point::viewport(coord),
            TerminalPointTag::Screen => super::point::Point::screen(coord),
            TerminalPointTag::History => super::point::Point::history(coord),
        };
        let pin = self.screens.active().pin(point)?;
        let screen_key = self.screens.active_key();
        let screen_generation = self.screens.active_generation();
        let screen_owner_id = self.screens.active_owner_id();
        let pin = self.screens.active_mut().track_pin(pin)?;

        Some(TerminalTrackedGridRef {
            pin,
            screen_key,
            screen_generation,
            screen_owner_id,
        })
    }

    pub(crate) fn untrack_grid_ref(&mut self, tracked: TerminalTrackedGridRef) {
        if self.screens.owner_id(tracked.screen_key) != Some(tracked.screen_owner_id) {
            return;
        }
        if let Some(screen) = self.screens.screen_mut(tracked.screen_key) {
            screen.untrack_pin(tracked.pin);
        }
    }

    fn tracked_grid_ref_pin(&self, tracked: &TerminalTrackedGridRef) -> Option<Pin> {
        if self.screens.generation(tracked.screen_key) != tracked.screen_generation
            || self.screens.owner_id(tracked.screen_key) != Some(tracked.screen_owner_id)
        {
            return None;
        }
        self.screens
            .screen(tracked.screen_key)?
            .tracked_pin_value(tracked.pin)
    }

    pub(crate) fn tracked_grid_ref_snapshot(
        &self,
        tracked: &TerminalTrackedGridRef,
    ) -> Option<TerminalGridRef> {
        Some(GridRef::from(self.tracked_grid_ref_pin(tracked)?).into())
    }

    pub(crate) fn tracked_grid_ref_point(
        &self,
        tracked: &TerminalTrackedGridRef,
        tag: TerminalPointTag,
    ) -> Result<super::point::Coordinate, TerminalGridRefPointError> {
        let grid_ref = self
            .tracked_grid_ref_snapshot(tracked)
            .ok_or(TerminalGridRefPointError::NoValue)?;
        let screen = self
            .screens
            .screen(tracked.screen_key)
            .ok_or(TerminalGridRefPointError::NoValue)?;
        screen
            .point_from_grid_ref(grid_ref.node, grid_ref.x, grid_ref.y, tag.into())
            .map_err(Into::into)
    }

    pub(crate) fn active_pin(&self, coord: super::point::Coordinate) -> Option<Pin> {
        self.screens
            .active()
            .pin(super::point::Point::active(coord))
    }

    pub(crate) fn viewport_pin(&self, coord: super::point::Coordinate) -> Option<Pin> {
        self.screens
            .active()
            .pin(super::point::Point::viewport(coord))
    }

    pub(crate) fn track_selection_gesture_anchor(
        &mut self,
        pin: Pin,
    ) -> Option<SelectionGestureAnchor> {
        let key = self.screens.active_key();
        let generation = self.screens.active_generation();
        let owner_id = self.screens.active_owner_id();
        let active_epoch = self.screens.active_epoch();
        let pin = self.screens.active_mut().track_pin(pin)?;
        Some(SelectionGestureAnchor {
            pin,
            screen_key: key,
            screen_generation: generation,
            screen_owner_id: owner_id,
            active_epoch,
        })
    }

    pub(crate) fn untrack_selection_gesture_anchor(&mut self, anchor: SelectionGestureAnchor) {
        if self.screens.owner_id(anchor.screen_key) != Some(anchor.screen_owner_id) {
            return;
        }
        if let Some(screen) = self.screens.screen_mut(anchor.screen_key) {
            screen.untrack_pin(anchor.pin);
        }
    }

    pub(crate) fn validated_selection_gesture_anchor(
        &self,
        anchor: &SelectionGestureAnchor,
    ) -> Option<Pin> {
        if self.screens.active_key() != anchor.screen_key
            || self.screens.active_epoch() != anchor.active_epoch
            || self.screens.generation(anchor.screen_key) != anchor.screen_generation
            || self.screens.owner_id(anchor.screen_key) != Some(anchor.screen_owner_id)
        {
            return None;
        }
        Some(unsafe { *anchor.pin.as_ref() })
    }

    pub(crate) fn selection_gesture_anchor_ref(
        &self,
        anchor: &SelectionGestureAnchor,
    ) -> Option<TerminalGridRef> {
        let pin = self.validated_selection_gesture_anchor(anchor)?;
        Some(GridRef::from(pin).into())
    }

    pub(crate) fn scroll_selection_gesture_viewport(&mut self, delta: isize) {
        self.screens.active_mut().scroll_delta_row(delta);
    }

    /// Scroll the viewport by a relative row `delta` (Issue 802 / Exp 23) — the wheel
    /// scrollback-navigation path. Negative scrolls toward history (older rows), matching
    /// `screen.scroll_delta_row`.
    pub(crate) fn scroll_viewport_delta_row(&mut self, delta: isize) {
        self.screens.active_mut().scroll_delta_row(delta);
    }

    /// Whether the alternate screen is active (Issue 802 / Exp 23 — wheel alt-scroll branch).
    pub(crate) fn is_alternate_screen(&self) -> bool {
        matches!(self.screens.active_key(), TerminalScreenKey::Alternate)
    }

    /// Whether DEC mode `1007` (alternate-scroll) is enabled (Issue 802 / Exp 23).
    pub(crate) fn mouse_alternate_scroll_enabled(&self) -> bool {
        self.modes.get(modes::Mode::MouseAlternateScroll)
    }

    /// Whether DECCKM (application cursor keys) is enabled (Issue 802 / Exp 23 — alt-scroll
    /// emits `\x1bO…` vs `\x1b[…`).
    pub(crate) fn cursor_keys_enabled(&self) -> bool {
        self.modes.get(modes::Mode::CursorKeys)
    }

    pub(crate) fn prompt_click_action(
        &self,
        viewport: super::point::Coordinate,
    ) -> Option<PromptClickAction> {
        if self.screens.active_key() == TerminalScreenKey::Alternate {
            return None;
        }
        let screen = self.screens.active();
        let mode = screen.prompt_click_mode();
        if mode == PromptClickMode::None || !screen.cursor_is_at_prompt() || screen.has_selection()
        {
            return None;
        }

        match mode {
            PromptClickMode::None => None,
            PromptClickMode::ClickEvents => Some(PromptClickAction::Bytes(
                format!("\x1b[<0;{};{}M", viewport.x + 1, viewport.y + 1).into_bytes(),
            )),
            PromptClickMode::Line
            | PromptClickMode::Multiple
            | PromptClickMode::ConservativeVertical
            | PromptClickMode::SmartVertical => {
                let movement = screen.prompt_click_move_for_viewport(viewport)?;
                let left = if self.cursor_keys_enabled() {
                    b"\x1bOD".as_slice()
                } else {
                    b"\x1b[D".as_slice()
                };
                let right = if self.cursor_keys_enabled() {
                    b"\x1bOC".as_slice()
                } else {
                    b"\x1b[C".as_slice()
                };
                let mut bytes = Vec::new();
                for _ in 0..movement.left {
                    bytes.extend_from_slice(left);
                }
                for _ in 0..movement.right {
                    bytes.extend_from_slice(right);
                }
                Some(PromptClickAction::Bytes(bytes))
            }
        }
    }

    pub(crate) fn scroll_viewport_to_top(&mut self) {
        self.screens.active_mut().scroll_top();
    }

    pub(crate) fn scroll_viewport_to_bottom(&mut self) {
        self.screens.active_mut().scroll_active();
    }

    pub(crate) fn scroll_viewport_to_row(&mut self, row: usize) {
        self.screens.active_mut().scroll_to_row(row);
    }

    pub(crate) fn scroll_viewport_to_selection(&mut self) -> bool {
        self.screens.active_mut().scroll_to_selection()
    }

    pub(crate) fn scroll_viewport_to_selection_endpoint(
        &mut self,
        endpoint: TerminalGridRef,
    ) -> Result<bool, TerminalGridRefPointError> {
        self.screens
            .active_mut()
            .scroll_to_selection_endpoint(endpoint.into())
            .map_err(Into::into)
    }

    pub(crate) fn scroll_viewport_to_prompt(&mut self, delta: isize) {
        self.screens.active_mut().scroll_delta_prompt(delta);
    }

    pub(crate) fn drag_select_cells(
        &self,
        click_pin: Pin,
        drag_pin: Pin,
        click_x: u32,
        drag_x: u32,
        rectangle: bool,
        geometry: SelectionGestureGeometry,
    ) -> Option<TerminalSelection> {
        Self::selection_from_tuple(self.screens.active().drag_selection(
            click_pin,
            drag_pin,
            click_x,
            drag_x,
            rectangle,
            DragGeometry {
                columns: geometry.columns,
                cell_width: geometry.cell_width,
                padding_left: geometry.padding_left,
                screen_height: geometry.screen_height,
            },
        ))
    }

    pub(crate) fn drag_select_word(
        &self,
        click_pin: Pin,
        drag_pin: Pin,
        boundary_codepoints: Option<&[u32]>,
    ) -> Option<TerminalSelection> {
        let click = GridRef::from(click_pin).into();
        let drag = GridRef::from(drag_pin).into();
        if self
            .screens
            .active()
            .pin_before(drag_pin, click_pin)
            .unwrap_or(false)
        {
            let current = self
                .select_word_between(drag, click, boundary_codepoints)
                .ok()??;
            let start = self
                .select_word_between(click, drag, boundary_codepoints)
                .ok()??;
            Some(TerminalSelection {
                start: current.start,
                end: start.end,
                rectangle: false,
            })
        } else {
            let start = self
                .select_word_between(click, drag, boundary_codepoints)
                .ok()??;
            let current = self
                .select_word_between(drag, click, boundary_codepoints)
                .ok()??;
            Some(TerminalSelection {
                start: start.start,
                end: current.end,
                rectangle: false,
            })
        }
    }

    pub(crate) fn drag_select_line(
        &self,
        click_pin: Pin,
        drag_pin: Pin,
    ) -> Option<TerminalSelection> {
        let click = GridRef::from(click_pin).into();
        let drag = GridRef::from(drag_pin).into();
        let line = self.select_line(drag, None, false).ok()??;
        let mut selection = self
            .select_line(click, None, false)
            .ok()
            .flatten()
            .or_else(|| self.select_line(click, Some(&[]), false).ok().flatten())?;
        if self
            .screens
            .active()
            .pin_before(drag_pin, click_pin)
            .unwrap_or(false)
        {
            selection.start = line.start;
        } else {
            selection.end = line.end;
        }
        Some(selection)
    }

    pub(crate) fn drag_select_output(
        &self,
        click_pin: Pin,
        drag_pin: Pin,
    ) -> Option<TerminalSelection> {
        let click = GridRef::from(click_pin).into();
        let drag = GridRef::from(drag_pin).into();
        let mut selection = self.select_output(click).ok()??;
        if let Some(current) = self.select_output(drag).ok().flatten() {
            if self
                .screens
                .active()
                .pin_before(drag_pin, click_pin)
                .unwrap_or(false)
            {
                selection.start = current.start;
            } else {
                selection.end = current.end;
            }
        }
        Some(selection)
    }

    pub(crate) fn point_from_grid_ref(
        &self,
        grid_ref: TerminalGridRef,
        tag: TerminalPointTag,
    ) -> Result<super::point::Coordinate, TerminalGridRefPointError> {
        self.screens
            .active()
            .point_from_grid_ref(grid_ref.node, grid_ref.x, grid_ref.y, tag.into())
            .map_err(Into::into)
    }

    pub(crate) fn viewport_bounds(&self) -> Option<(TerminalGridRef, TerminalGridRef)> {
        self.screens
            .active()
            .viewport_bounds()
            .map(|(top_left, bottom_right)| (top_left.into(), bottom_right.into()))
    }

    pub(crate) fn grid_ref_before(
        &self,
        a: TerminalGridRef,
        b: TerminalGridRef,
    ) -> Result<bool, TerminalGridRefPointError> {
        self.screens
            .active()
            .grid_ref_before(a.into(), b.into())
            .ok_or(TerminalGridRefPointError::NoValue)
    }

    fn selection_from_tuple(
        selection: Option<(GridRef, GridRef, bool)>,
    ) -> Option<TerminalSelection> {
        let (start, end, rectangle) = selection?;
        Some(TerminalSelection {
            start: start.into(),
            end: end.into(),
            rectangle,
        })
    }

    pub(crate) fn active_selection(&self) -> Option<TerminalSelection> {
        Self::selection_from_tuple(self.screens.active().active_selection_grid_refs())
    }

    pub(crate) fn render_rows_snapshot(&self) -> Vec<RenderRowSnapshot> {
        self.screens.active().render_rows_snapshot()
    }

    /// The renderer-facing entry: assemble the active screen's per-row
    /// [`RunOptions`] for the shaper. Sibling of [`Self::render_rows_snapshot`].
    pub(crate) fn shape_run_options(&self) -> Vec<RunOptions> {
        self.screens.active().shape_run_options()
    }

    pub(crate) fn viewport_string_map(&self) -> super::string_map::ViewportStringMap {
        self.screens.active().viewport_string_map()
    }

    pub(crate) fn kitty_virtual_placements_visible(
        &self,
    ) -> Vec<graphics_unicode::VirtualPlacement> {
        self.screens.active().kitty_virtual_placements_visible()
    }

    pub(crate) fn set_selection(
        &mut self,
        selection: Option<TerminalSelection>,
    ) -> Result<(), TerminalGridRefPointError> {
        let Some(selection) = selection else {
            self.screens.active_mut().clear_selection();
            return Ok(());
        };
        self.screens
            .active_mut()
            .set_selection(
                selection.start.into(),
                selection.end.into(),
                selection.rectangle,
            )
            .map_err(Into::into)
    }

    pub(crate) fn select_word(
        &self,
        ref_: TerminalGridRef,
        boundary_codepoints: Option<&[u32]>,
    ) -> Result<Option<TerminalSelection>, TerminalGridRefPointError> {
        self.screens
            .active()
            .select_word(
                ref_.into(),
                boundary_codepoints.unwrap_or(selection_codepoints::DEFAULT_WORD_BOUNDARIES),
            )
            .map(Self::selection_from_tuple)
            .map_err(Into::into)
    }

    pub(crate) fn select_word_between(
        &self,
        start: TerminalGridRef,
        end: TerminalGridRef,
        boundary_codepoints: Option<&[u32]>,
    ) -> Result<Option<TerminalSelection>, TerminalGridRefPointError> {
        self.screens
            .active()
            .select_word_between(
                start.into(),
                end.into(),
                boundary_codepoints.unwrap_or(selection_codepoints::DEFAULT_WORD_BOUNDARIES),
            )
            .map(Self::selection_from_tuple)
            .map_err(Into::into)
    }

    pub(crate) fn select_line(
        &self,
        ref_: TerminalGridRef,
        whitespace: Option<&[u32]>,
        semantic_prompt_boundary: bool,
    ) -> Result<Option<TerminalSelection>, TerminalGridRefPointError> {
        self.screens
            .active()
            .select_line(ref_.into(), whitespace, semantic_prompt_boundary)
            .map(Self::selection_from_tuple)
            .map_err(Into::into)
    }

    pub(crate) fn select_all(&self) -> Option<TerminalSelection> {
        Self::selection_from_tuple(self.screens.active().select_all())
    }

    pub(crate) fn select_output(
        &self,
        ref_: TerminalGridRef,
    ) -> Result<Option<TerminalSelection>, TerminalGridRefPointError> {
        self.screens
            .active()
            .select_output(ref_.into())
            .map(Self::selection_from_tuple)
            .map_err(Into::into)
    }

    pub(crate) fn selection_adjust(
        &self,
        selection: TerminalSelection,
        adjustment: TerminalSelectionAdjustment,
    ) -> Result<Option<TerminalSelection>, TerminalGridRefPointError> {
        self.screens
            .active()
            .selection_adjust(
                selection.start.into(),
                selection.end.into(),
                selection.rectangle,
                adjustment.into(),
            )
            .map(Self::selection_from_tuple)
            .map_err(Into::into)
    }

    pub(crate) fn adjust_active_selection(
        &mut self,
        adjustment: TerminalSelectionAdjustment,
    ) -> Result<bool, TerminalGridRefPointError> {
        let Some(selection) = self.active_selection() else {
            return Ok(false);
        };
        let adjusted = self
            .selection_adjust(selection, adjustment)?
            .unwrap_or(selection);
        self.set_selection(Some(adjusted))?;
        self.scroll_viewport_to_selection_endpoint(adjusted.end)?;
        Ok(true)
    }

    pub(crate) fn selection_order(
        &self,
        selection: TerminalSelection,
    ) -> Result<Option<TerminalSelectionOrder>, TerminalGridRefPointError> {
        self.screens
            .active()
            .selection_order(
                selection.start.into(),
                selection.end.into(),
                selection.rectangle,
            )
            .map(|order| order.map(Into::into))
            .map_err(Into::into)
    }

    pub(crate) fn selection_ordered(
        &self,
        selection: TerminalSelection,
        desired: TerminalSelectionOrder,
    ) -> Result<Option<TerminalSelection>, TerminalGridRefPointError> {
        self.screens
            .active()
            .selection_ordered(
                selection.start.into(),
                selection.end.into(),
                selection.rectangle,
                desired.into(),
            )
            .map(Self::selection_from_tuple)
            .map_err(Into::into)
    }

    pub(crate) fn selection_contains(
        &self,
        selection: TerminalSelection,
        tag: TerminalPointTag,
        coord: super::point::Coordinate,
    ) -> Result<Option<bool>, TerminalGridRefPointError> {
        let point = match tag {
            TerminalPointTag::Active => super::point::Point::active(coord),
            TerminalPointTag::Viewport => super::point::Point::viewport(coord),
            TerminalPointTag::Screen => super::point::Point::screen(coord),
            TerminalPointTag::History => super::point::Point::history(coord),
        };
        self.screens
            .active()
            .selection_contains(
                selection.start.into(),
                selection.end.into(),
                selection.rectangle,
                point,
            )
            .map_err(Into::into)
    }

    pub(crate) fn selection_equal(
        &self,
        a: TerminalSelection,
        b: TerminalSelection,
    ) -> Result<bool, TerminalGridRefPointError> {
        self.screens
            .active()
            .selection_equal(
                a.start.into(),
                a.end.into(),
                a.rectangle,
                b.start.into(),
                b.end.into(),
                b.rectangle,
            )
            .map_err(Into::into)
    }

    pub(crate) fn selection_format(
        &self,
        format: TerminalSelectionFormat,
        unwrap: bool,
        trim: bool,
        selection: Option<TerminalSelection>,
    ) -> Result<String, TerminalGridRefPointError> {
        self.selection_format_with_codepoint_map(format, unwrap, trim, selection, None)
    }

    pub(crate) fn selection_format_with_codepoint_map(
        &self,
        format: TerminalSelectionFormat,
        unwrap: bool,
        trim: bool,
        selection: Option<TerminalSelection>,
        codepoint_map: Option<&[CodepointMapEntry]>,
    ) -> Result<String, TerminalGridRefPointError> {
        let selection = match selection {
            Some(selection) => selection,
            None => self
                .active_selection()
                .ok_or(TerminalGridRefPointError::NoValue)?,
        };
        let selection = self.screens.active().selection_from_grid_refs(
            selection.start.into(),
            selection.end.into(),
            selection.rectangle,
        )?;
        Ok(TerminalFormatter::init(
            self,
            TerminalFormatterOptions::new(format.into())
                .unwrap(unwrap)
                .trim(trim)
                .codepoint_map(codepoint_map),
        )
        .with_content(ScreenFormatterContent::Selection(Some(selection)))
        .format())
    }

    pub(crate) fn formatter_format(
        &self,
        format: TerminalSelectionFormat,
        unwrap: bool,
        trim: bool,
        extra: TerminalFormatterExtra,
        selection: Option<TerminalSelection>,
    ) -> Result<String, TerminalGridRefPointError> {
        let content = match selection {
            Some(selection) => {
                let selection = self.screens.active().selection_from_grid_refs(
                    selection.start.into(),
                    selection.end.into(),
                    selection.rectangle,
                )?;
                ScreenFormatterContent::Selection(Some(selection))
            }
            None => ScreenFormatterContent::Selection(None),
        };

        Ok(TerminalFormatter::init(
            self,
            TerminalFormatterOptions::new(format.into())
                .unwrap(unwrap)
                .trim(trim),
        )
        .with_content(content)
        .with_extra(extra)
        .format())
    }

    pub(crate) fn scrollback_format(
        &self,
        format: TerminalSelectionFormat,
        unwrap: bool,
        trim: bool,
        extra: TerminalFormatterExtra,
    ) -> Option<String> {
        let selection = self.screens.active().history_selection()?;
        Some(
            TerminalFormatter::init(
                self,
                TerminalFormatterOptions::new(format.into())
                    .unwrap(unwrap)
                    .trim(trim),
            )
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(extra)
            .format(),
        )
    }

    pub(crate) fn mode_get(&self, value: u16, ansi: bool) -> Option<bool> {
        modes::mode_from_int(value, ansi).map(|mode| self.modes.get(mode))
    }

    pub(crate) fn mode_set(&mut self, value: u16, ansi: bool, enabled: bool) -> bool {
        let Some(mode) = modes::mode_from_int(value, ansi) else {
            return false;
        };
        self.modes.set(mode, enabled);
        true
    }

    pub(crate) fn bracketed_paste_enabled(&self) -> bool {
        self.modes.get(modes::Mode::BracketedPaste)
    }

    pub(crate) fn plain_screen(&self, unwrap: bool) -> String {
        TerminalFormatter::init(
            self,
            TerminalFormatterOptions::new(PageOutputFormat::Plain).unwrap(unwrap),
        )
        .format()
    }

    pub(crate) fn pty_response(&self) -> &[u8] {
        &self.pty_response
    }

    pub(crate) fn clear_pty_response(&mut self) {
        self.pty_response.clear();
    }

    #[cfg(test)]
    pub(super) fn pty_response_for_tests(&self) -> &[u8] {
        &self.pty_response
    }

    #[cfg(test)]
    pub(super) fn take_pty_response_for_tests(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pty_response)
    }

    #[cfg(test)]
    pub(super) fn set_palette_entry_for_tests(&mut self, index: usize, rgb: color::Rgb) {
        self.colors.palette.set(index as u8, rgb);
    }

    #[cfg(test)]
    pub(super) fn set_mode_for_tests(&mut self, mode: modes::Mode, value: bool) {
        self.modes.set(mode, value);
    }

    #[cfg(test)]
    pub(super) fn get_mode_for_tests(&self, mode: modes::Mode) -> bool {
        self.modes.get(mode)
    }

    #[cfg(test)]
    pub(super) fn save_mode_for_tests(&mut self, mode: modes::Mode) {
        self.modes.save(mode);
    }

    #[cfg(test)]
    pub(super) fn restore_mode_for_tests(&mut self, mode: modes::Mode) -> bool {
        self.modes.restore(mode)
    }

    #[cfg(test)]
    pub(super) fn set_scrolling_region_for_tests(
        &mut self,
        top: CellCountInt,
        bottom: CellCountInt,
        left: CellCountInt,
        right: CellCountInt,
    ) {
        let region = ScrollingRegion {
            top,
            bottom,
            left,
            right,
        };
        region.assert_valid(self.size);
        self.scrolling_region = region;
    }

    #[cfg(test)]
    fn scrolling_region_for_tests(&self) -> ScrollingRegion {
        self.scrolling_region
    }

    #[cfg(test)]
    pub(super) fn scrolling_region_tuple_for_tests(&self) -> (u16, u16, u16, u16) {
        (
            self.scrolling_region.top,
            self.scrolling_region.bottom,
            self.scrolling_region.left,
            self.scrolling_region.right,
        )
    }

    #[cfg(test)]
    pub(super) fn clear_tabstops_for_tests(&mut self) {
        self.tabstops.reset(0);
    }

    #[cfg(test)]
    pub(super) fn set_tabstop_for_tests(&mut self, col: usize) {
        assert!(col < self.tabstops.cols());
        self.tabstops.set(col);
    }

    #[cfg(test)]
    pub(super) fn clear_tabstop_for_tests(&mut self, col: usize) {
        assert!(col < self.tabstops.cols());
        if self.tabstops.get(col) {
            self.tabstops.unset(col);
        }
    }

    #[cfg(test)]
    pub(super) fn get_tabstop_for_tests(&self, col: usize) -> bool {
        assert!(col < self.tabstops.cols());
        self.tabstops.get(col)
    }

    #[cfg(test)]
    pub(crate) fn set_modify_other_keys_2_for_tests(&mut self, modify_other_keys_2: bool) {
        self.flags.modify_other_keys_2 = modify_other_keys_2;
    }

    #[cfg(test)]
    pub(crate) fn modify_other_keys_2_for_tests(&self) -> bool {
        self.flags.modify_other_keys_2
    }

    #[cfg(test)]
    pub(super) fn mouse_event_for_tests(&self) -> mouse::MouseEventMode {
        self.flags.mouse_event
    }

    #[cfg(test)]
    pub(super) fn mouse_format_for_tests(&self) -> mouse::MouseFormat {
        self.flags.mouse_format
    }

    /// The `XTSHIFTESCAPE` flag (`CSI > 1 s`) — whether shift should be captured by the program vs
    /// available for selection (Issue 802 / Exp 33). `None` = unset (config decides).
    pub(crate) fn mouse_shift_capture_flag(&self) -> Option<bool> {
        self.flags.mouse_shift_capture
    }

    /// Emit an in-band size report (`CSI 48 ; rows ; cols ; height ; width t`) when mode 2048
    /// (`InBandSizeReports`) is enabled — the resize trigger (Issue 802 / Exp 37, upstream emits the
    /// same report on resize). Fetches the size via the `size` callback, like the DSR query path.
    pub(crate) fn report_in_band_size(&mut self) {
        if !self.modes.get(modes::Mode::InBandSizeReports) {
            return;
        }
        let Some(callback) = self.effects.size else {
            return;
        };
        let mut size = size_report::Size::default();
        let ok = unsafe { callback(self.effects.handle, self.effects.userdata, &mut size) };
        if !ok {
            return;
        }
        let response = size_report::encode(size_report::Style::Mode2048, size);
        self.pty_response.extend_from_slice(&response);
        if let Some(cb) = self.effects.write_pty {
            unsafe {
                cb(
                    self.effects.handle,
                    self.effects.userdata,
                    response.as_ptr(),
                    response.len(),
                );
            }
        }
    }

    /// Report a color-scheme CHANGE to the program (write `CSI ? 997 ; 1 n` dark / `; 2 n` light),
    /// but only when mode 2031 (`report_color_scheme`) is enabled — the live-notification (force=false)
    /// path of upstream `Termio.colorSchemeReportLocked` (Issue 802 / Exp 36). The DSR query path
    /// (`TerminalStreamHandler::color_scheme`) always reports (force=true). `scheme`: 0 = light,
    /// 1 = dark.
    pub(crate) fn report_color_scheme_change(&mut self, scheme: i32) {
        if !self.modes.get(modes::Mode::ReportColorScheme) {
            return;
        }
        let Some(bytes) = color_scheme_report_bytes(scheme) else {
            return;
        };
        self.pty_response.extend_from_slice(bytes);
        if let Some(callback) = self.effects.write_pty {
            unsafe {
                callback(
                    self.effects.handle,
                    self.effects.userdata,
                    bytes.as_ptr(),
                    bytes.len(),
                );
            }
        }
    }

    #[cfg(test)]
    pub(super) fn mouse_shift_capture_for_tests(&self) -> Option<bool> {
        self.flags.mouse_shift_capture
    }

    #[cfg(test)]
    pub(super) fn title_for_tests(&self) -> &str {
        self.title.as_str()
    }

    #[cfg(test)]
    pub(super) fn set_pwd_for_tests(&mut self, pwd: &str) {
        self.pwd.set(pwd);
    }

    #[cfg(test)]
    pub(super) fn clear_pwd_for_tests(&mut self) {
        self.pwd.clear();
    }

    #[cfg(test)]
    pub(super) fn pwd_for_tests(&self) -> Option<&str> {
        self.pwd.logical_str()
    }

    #[cfg(test)]
    pub(super) fn mouse_shape_for_tests(&self) -> mouse::MouseShape {
        self.mouse_shape
    }

    #[cfg(test)]
    fn active_screen_key_for_tests(&self) -> TerminalScreenKey {
        self.screens.active_key()
    }

    #[cfg(test)]
    fn alternate_initialized_for_tests(&self) -> bool {
        self.screens.alternate_initialized_for_tests()
    }

    #[cfg(test)]
    pub(super) fn active_screen_for_tests(&self) -> TerminalScreen {
        self.active_screen()
    }

    #[cfg(test)]
    pub(super) fn tmux_screen_initialized_for_tests(&self, screen: TerminalScreen) -> bool {
        match screen {
            TerminalScreen::Primary => true,
            TerminalScreen::Alternate => self.screens.alternate_initialized_for_tests(),
        }
    }

    #[cfg(test)]
    pub(super) fn tmux_cursor_position_for_tests(
        &self,
        screen: TerminalScreen,
    ) -> Option<(CellCountInt, CellCountInt)> {
        let key = match screen {
            TerminalScreen::Primary => TerminalScreenKey::Primary,
            TerminalScreen::Alternate => TerminalScreenKey::Alternate,
        };
        self.screens.screen(key).map(Screen::cursor_position)
    }

    #[cfg(test)]
    pub(super) fn tmux_cursor_visual_style_for_tests(
        &self,
        screen: TerminalScreen,
    ) -> Option<cursor::VisualStyle> {
        let key = match screen {
            TerminalScreen::Primary => TerminalScreenKey::Primary,
            TerminalScreen::Alternate => TerminalScreenKey::Alternate,
        };
        self.screens.screen(key).map(Screen::cursor_visual_style)
    }

    #[cfg(test)]
    pub(super) fn cursor_position_for_tests(&self) -> (CellCountInt, CellCountInt) {
        self.screens.active().cursor_position_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_pending_wrap_for_tests(&self) -> bool {
        self.screens.active().cursor_pending_wrap_for_tests()
    }

    #[cfg(test)]
    pub(super) fn is_dirty_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active().is_dirty_for_tests(x, y)
    }

    #[cfg(test)]
    pub(crate) fn clear_dirty_for_tests(&mut self) {
        self.screens.active_mut().clear_dirty_for_tests();
    }

    #[cfg(test)]
    pub(crate) fn set_cursor_position_for_tests(&mut self, x: CellCountInt, y: CellCountInt) {
        self.screens
            .active_mut()
            .set_cursor_position_for_tests(x, y);
    }

    #[cfg(test)]
    pub(super) fn set_cell_protected_for_tests(
        &mut self,
        x: CellCountInt,
        y: u32,
        protected: bool,
    ) {
        self.screens
            .active_mut()
            .set_cell_protected_for_tests(x, y, protected);
    }

    #[cfg(test)]
    pub(super) fn cell_protected_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active().cell_protected_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn scrollback_rows_for_tests(&self) -> usize {
        self.screens.active().scrollback_rows_for_tests()
    }

    #[cfg(test)]
    pub(super) fn row_wrap_for_tests(&self, y: u32) -> bool {
        self.screens.active().row_wrap_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn row_wrap_continuation_for_tests(&self, y: u32) -> bool {
        self.screens.active().row_wrap_continuation_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn set_row_wrap_for_tests(&mut self, y: u32, wrap: bool) {
        self.screens.active_mut().set_row_wrap_for_tests(y, wrap);
    }

    #[cfg(test)]
    pub(super) fn set_row_wrap_continuation_for_tests(&mut self, y: u32, wrap: bool) {
        self.screens
            .active_mut()
            .set_row_wrap_continuation_for_tests(y, wrap);
    }

    #[cfg(test)]
    pub(super) fn full_screen_plain_for_tests(&self, unwrap: bool) -> String {
        self.screens.active().full_screen_plain_for_tests(unwrap)
    }

    #[cfg(test)]
    pub(super) fn active_area_plain_for_tests(&self) -> String {
        ScreenFormatter::init(
            self.screens.active(),
            ScreenFormatterOptions::new(PageOutputFormat::Plain),
        )
        .format()
    }

    #[cfg(test)]
    pub(super) fn cursor_style_for_tests(&self) -> style::Style {
        self.screens.active().cursor_style_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_visual_style_for_tests(&self) -> cursor::VisualStyle {
        self.screens.active().cursor_visual_style_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_protected_for_tests(&self) -> bool {
        self.screens.active().cursor_protected_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_hyperlink_for_tests(&self) -> Option<(ScreenCursorHyperlinkId, &str)> {
        self.screens.active().cursor_hyperlink_for_tests()
    }

    #[cfg(test)]
    pub(super) fn active_cell_style_for_tests(&self, x: CellCountInt, y: u32) -> style::Style {
        self.screens.active().active_cell_style_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_codepoint_for_tests(&self, x: CellCountInt, y: u32) -> u32 {
        self.screens.active().active_cell_codepoint_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_wide_for_tests(&self, x: CellCountInt, y: u32) -> super::page::Wide {
        self.screens.active().active_cell_wide_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_graphemes_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> Option<Vec<u32>> {
        self.screens.active().active_cell_graphemes_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_style_ref_count_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> style::Id {
        self.screens
            .active()
            .active_cell_style_ref_count_for_tests(x, y)
    }

    #[cfg(test)]
    pub(crate) fn append_grapheme_for_tests(&mut self, x: CellCountInt, y: u32, codepoint: u32) {
        self.screens
            .active_mut()
            .append_grapheme_for_tests(x, y, codepoint);
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active().active_cell_hyperlink_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_snapshot_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> Option<super::page::HyperlinkSnapshot> {
        self.screens
            .active()
            .active_cell_hyperlink_snapshot_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_ref_count_for_tests(&self, x: CellCountInt, y: u32) -> u16 {
        self.screens
            .active()
            .active_cell_hyperlink_ref_count_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_row_hyperlink_for_tests(&self, y: u32) -> bool {
        self.screens.active().active_row_hyperlink_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_row_styled_for_tests(&self, y: u32) -> bool {
        self.screens.active().active_row_styled_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_row_kitty_virtual_placeholder_for_tests(&self, y: u32) -> bool {
        self.screens
            .active()
            .active_row_kitty_virtual_placeholder_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_row_semantic_prompt_for_tests(&self, y: u32) -> SemanticPrompt {
        self.screens
            .active()
            .active_row_semantic_prompt_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_semantic_content_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> super::page::SemanticContent {
        self.screens
            .active()
            .active_cell_semantic_content_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn verify_integrity_for_tests(&self) {
        self.screens.active().verify_integrity_for_tests();
    }
}

impl Handler for TerminalStreamHandler<'_> {
    type Error = TerminalStreamError;

    fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
        match action {
            Action::Print { cp } => self.print(cp),
            Action::PrintRepeat { count } => self.print_repeat(count),
            Action::Bell => {
                self.bell();
                Ok(())
            }
            Action::Enquiry => {
                self.enquiry();
                Ok(())
            }
            Action::LineFeed => self.line_feed(),
            Action::CarriageReturn => {
                self.screens.active_mut().carriage_return_basic();
                Ok(())
            }
            Action::Backspace => {
                self.screens.active_mut().backspace_basic();
                Ok(())
            }
            Action::HorizontalTab { count } => {
                self.screens.active_mut().horizontal_tab_count_basic(
                    self.size.cols,
                    self.tabstops,
                    count,
                );
                Ok(())
            }
            Action::HorizontalTabBack { count } => {
                let left_limit = if self.modes.get(modes::Mode::Origin) {
                    self.scrolling_region.left
                } else {
                    0
                };
                self.screens.active_mut().horizontal_tab_back_count_basic(
                    self.tabstops,
                    count,
                    left_limit,
                );
                Ok(())
            }
            Action::TabSet => {
                self.screens.active_mut().tab_set_basic(self.tabstops);
                Ok(())
            }
            Action::TabClearCurrent => {
                self.screens
                    .active_mut()
                    .tab_clear_current_basic(self.tabstops);
                Ok(())
            }
            Action::TabClearAll => {
                self.tabstops.reset(0);
                Ok(())
            }
            Action::TabReset => {
                self.tabstops.reset(TABSTOP_INTERVAL);
                Ok(())
            }
            Action::Index => self.index(),
            Action::NextLine => {
                self.line_feed()?;
                self.screens.active_mut().carriage_return_basic();
                Ok(())
            }
            Action::CursorUp { count } => {
                self.screens.active_mut().cursor_up_basic(count);
                Ok(())
            }
            Action::CursorDown { count } => {
                self.screens
                    .active_mut()
                    .cursor_down_basic(self.size.rows, count);
                Ok(())
            }
            Action::CursorRight { count } => {
                self.screens
                    .active_mut()
                    .cursor_right_basic(self.size.cols, count);
                Ok(())
            }
            Action::CursorLeft { count } => {
                self.screens.active_mut().cursor_left_basic(count);
                Ok(())
            }
            Action::CursorColumn { col } => {
                self.screens
                    .active_mut()
                    .cursor_column_basic(self.size.cols, col);
                Ok(())
            }
            Action::CursorRow { row } => {
                self.screens
                    .active_mut()
                    .cursor_row_basic(self.size.rows, row);
                Ok(())
            }
            Action::CursorRowRelative { rows } => {
                self.screens
                    .active_mut()
                    .cursor_row_relative_basic(self.size.rows, rows);
                Ok(())
            }
            Action::CursorPosition { row, col } => {
                self.screens.active_mut().cursor_position_basic(
                    row,
                    col,
                    self.size.rows,
                    self.size.cols,
                );
                Ok(())
            }
            Action::EraseDisplay { mode, protected } => self
                .screens
                .active_mut()
                .erase_display_basic(mode, self.size.rows, self.size.cols, protected)
                .map_err(TerminalStreamError::from),
            Action::EraseLine { mode, protected } => self
                .screens
                .active_mut()
                .erase_line_basic(mode, self.size.rows, self.size.cols, protected)
                .map_err(TerminalStreamError::from),
            Action::InsertChars { count } => self
                .screens
                .active_mut()
                .insert_chars_basic(
                    count,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                )
                .map_err(TerminalStreamError::from),
            Action::DeleteChars { count } => self
                .screens
                .active_mut()
                .delete_chars_basic(
                    count,
                    self.size.rows,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                )
                .map_err(TerminalStreamError::from),
            Action::EraseChars { count } => self
                .screens
                .active_mut()
                .erase_chars_basic(count, self.size.rows, self.size.cols)
                .map_err(TerminalStreamError::from),
            Action::InsertLines { count } => self
                .screens
                .active_mut()
                .insert_lines_basic(
                    count,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::DeleteLines { count } => self
                .screens
                .active_mut()
                .delete_lines_basic(
                    count,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::ScrollUp { count } => self
                .screens
                .active_mut()
                .scroll_up_basic(
                    count,
                    self.size.rows,
                    self.size.cols,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::ScrollDown { count } => self
                .screens
                .active_mut()
                .scroll_down_basic(
                    count,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::SetMode { mode } => self.set_mode_basic(mode, true),
            Action::ResetMode { mode } => self.set_mode_basic(mode, false),
            Action::SaveMode { mode } => {
                self.modes.save(mode);
                Ok(())
            }
            Action::RestoreMode { mode } => {
                let enabled = self.modes.restore(mode);
                self.set_mode_basic(mode, enabled)
            }
            Action::MouseShiftCapture { enabled } => {
                self.flags.mouse_shift_capture = Some(enabled);
                Ok(())
            }
            Action::KittyKeyboardQuery => {
                let flags = self.screens.active().kitty_keyboard_flags().int();
                self.write_pty_response(&format!("\x1b[?{flags}u"));
                Ok(())
            }
            Action::KittyKeyboardPush { flags } => {
                self.screens.active_mut().push_kitty_keyboard(flags);
                Ok(())
            }
            Action::KittyKeyboardPop { count } => {
                self.screens
                    .active_mut()
                    .pop_kitty_keyboard(usize::from(count));
                Ok(())
            }
            Action::KittyKeyboardSet { mode, flags } => {
                self.screens.active_mut().set_kitty_keyboard(mode, flags);
                Ok(())
            }
            Action::SaveCursor => {
                self.screens
                    .active_mut()
                    .save_cursor(self.modes.get(modes::Mode::Origin));
                Ok(())
            }
            Action::RestoreCursor => {
                self.restore_cursor_from_active_saved();
                Ok(())
            }
            Action::ReverseIndex => self.reverse_index(),
            Action::FullReset => self.full_reset(),
            Action::ConfigureCharset { slot, charset } => {
                self.configure_charset(slot, charset);
                Ok(())
            }
            Action::InvokeCharset { bank, slot, single } => {
                self.invoke_charset(bank, slot, single);
                Ok(())
            }
            Action::CursorVisualStyle { style, blinking } => {
                let (style, blinking) = match style {
                    Some(style) => (style, blinking),
                    None => (
                        self.default_cursor_visual_style,
                        self.default_cursor_blink.unwrap_or(true),
                    ),
                };
                self.modes.set(modes::Mode::CursorBlinking, blinking);
                self.screens.active_mut().set_cursor_visual_style(style);
                Ok(())
            }
            Action::DcsHook { value } => {
                if let Some(command) = self.dcs.hook(value) {
                    self.dcs_command(command);
                }
                Ok(())
            }
            Action::DcsPut { byte } => {
                if let Some(command) = self.dcs.put(byte) {
                    self.dcs_command(command);
                }
                Ok(())
            }
            Action::DcsUnhook => {
                if let Some(command) = self.dcs.unhook() {
                    self.dcs_command(command);
                }
                Ok(())
            }
            Action::ApcStart => {
                self.kitty_graphics.start();
                Ok(())
            }
            Action::ApcPut { byte } => {
                self.kitty_graphics.put(byte);
                Ok(())
            }
            Action::ApcEnd => {
                if let Some(command) = self.kitty_graphics.end() {
                    self.kitty_graphics_command(command)?;
                }
                Ok(())
            }
            Action::RequestMode { mode } => {
                let report = self.modes.get_report(modes::ModeTag::from_mode(mode));
                self.write_pty_response(&report.encode_vt());
                Ok(())
            }
            Action::RequestModeUnknown { value, ansi } => {
                let report = self.modes.get_report(modes::ModeTag::new(value, ansi));
                self.write_pty_response(&report.encode_vt());
                Ok(())
            }
            Action::DeviceAttributes { request } => {
                let response = self.device_attributes(request).encode_vt(request);
                self.write_pty_response(&response);
                Ok(())
            }
            Action::DeviceStatus { request } => {
                self.device_status(request);
                Ok(())
            }
            Action::SizeReport { request } => {
                self.size_report(request);
                Ok(())
            }
            Action::XtVersion => {
                self.xtversion();
                Ok(())
            }
            Action::SetAttribute { attr } => {
                self.screens.active_mut().set_attribute_basic(attr);
                Ok(())
            }
        }
    }

    fn osc(&mut self, action: stream::OscAction<'_>) -> Result<(), Self::Error> {
        match action {
            stream::OscAction::WindowTitle { title } => {
                self.title.set(title);
                self.title_changed();
            }
            stream::OscAction::ReportPwd { url } => {
                self.pwd.set(url);
            }
            stream::OscAction::ClipboardContents { value } => {
                self.pending_clipboard_events
                    .push(TerminalClipboardEvent::Osc52 {
                        kind: value.kind,
                        data: value.data.to_vec(),
                    });
            }
            stream::OscAction::ContextSignal { .. } => {}
            stream::OscAction::DesktopNotification { .. } => {}
            stream::OscAction::MouseShape { shape } => {
                *self.mouse_shape = shape;
            }
            stream::OscAction::StartHyperlink { id, uri } => {
                let id = match id {
                    Some(id) => ScreenCursorHyperlinkId::Explicit(id.to_string()),
                    None => {
                        let id = *self.next_implicit_hyperlink_id;
                        *self.next_implicit_hyperlink_id = id.wrapping_add(1);
                        ScreenCursorHyperlinkId::Implicit(id)
                    }
                };
                self.screens.active_mut().set_cursor_hyperlink(id, uri);
            }
            stream::OscAction::EndHyperlink => {
                self.screens.active_mut().clear_cursor_hyperlink();
            }
            stream::OscAction::ColorOperation { requests } => {
                self.color_operation(requests);
            }
            stream::OscAction::KittyColor {
                requests,
                terminator,
            } => {
                self.kitty_color_operation(requests, terminator);
            }
            stream::OscAction::KittyTextSizing { .. } => {}
            stream::OscAction::SemanticPrompt { value } => {
                self.semantic_prompt(value)?;
            }
            stream::OscAction::KittyClipboard { value } => {
                self.pending_clipboard_events
                    .push(TerminalClipboardEvent::Kitty {
                        metadata: value.metadata.to_vec(),
                        payload: value.payload.map(Vec::from),
                        terminator: value.terminator,
                    });
            }
        }
        Ok(())
    }
}

impl TerminalStreamHandler<'_> {
    fn print(&mut self, cp: char) -> Result<(), TerminalStreamError> {
        if cp.is_control() {
            return Err(TerminalStreamError::UnsupportedCodepoint(cp));
        }

        let codepoint = cp as u32;
        let props = crate::unicode::get(codepoint);
        let width = crate::unicode::codepoint_width(codepoint);
        let wraparound = self.modes.get(modes::Mode::Wraparound);
        let cursor_x = self.screens.active().cursor_position().0;
        let right_limit = if cursor_x > self.scrolling_region.right {
            self.size.cols
        } else {
            self.scrolling_region.right.saturating_add(1)
        };

        if codepoint > 0xFF && self.modes.get(modes::Mode::GraphemeCluster) {
            let previous = self
                .screens
                .active()
                .previous_print_cell(wraparound, right_limit);
            if let Some((x, y, previous_cell)) = previous {
                if previous_cell.has_text() {
                    let mut previous_codepoint = previous_cell.codepoint();
                    let mut state = crate::unicode::BreakState::default();
                    if let Some(graphemes) = self
                        .screens
                        .active()
                        .active_cell_graphemes(x, y)
                        .map_err(TerminalStreamError::from)?
                    {
                        for grapheme in graphemes {
                            let _ = crate::unicode::grapheme_break(
                                previous_codepoint,
                                grapheme,
                                &mut state,
                            );
                            previous_codepoint = grapheme;
                        }
                    }
                    if !crate::unicode::grapheme_break(previous_codepoint, codepoint, &mut state) {
                        if matches!(codepoint, 0xFE0E | 0xFE0F) {
                            let previous_props = crate::unicode::get(previous_codepoint);
                            if !previous_props.emoji_vs_base {
                                return Ok(());
                            }
                            self.screens
                                .active_mut()
                                .set_previous_cell_wide(x, y, codepoint == 0xFE0F, right_limit)
                                .map_err(TerminalStreamError::from)?;
                        } else if !props.width_zero_in_grapheme {
                            self.screens
                                .active_mut()
                                .set_previous_cell_wide(x, y, true, right_limit)
                                .map_err(TerminalStreamError::from)?;
                        }
                        self.screens
                            .active_mut()
                            .append_grapheme_to_previous_cell(
                                codepoint,
                                wraparound,
                                right_limit,
                                false,
                            )
                            .map_err(TerminalStreamError::from)?;
                        return Ok(());
                    }
                }
            }
        }

        if width == 0 {
            if self.modes.get(modes::Mode::GraphemeCluster) {
                return Ok(());
            }
            if matches!(codepoint, 0xFE0E | 0xFE0F) {
                let Some((_, _, previous_cell)) = self
                    .screens
                    .active()
                    .previous_print_cell(wraparound, right_limit)
                else {
                    return Ok(());
                };
                let previous_props = crate::unicode::get(previous_cell.codepoint());
                if previous_props.grapheme_break
                    != crate::unicode::GraphemeBreak::ExtendedPictographic
                {
                    return Ok(());
                }
            }
            self.screens
                .active_mut()
                .append_grapheme_to_previous_cell(codepoint, wraparound, right_limit, false)
                .map_err(TerminalStreamError::from)?;
            return Ok(());
        }

        let printed = self
            .screens
            .active_mut()
            .print_width_cell(
                self.size.cols,
                self.size.rows,
                cp,
                width,
                self.modes.get(modes::Mode::Insert),
                wraparound,
                self.scrolling_region.left,
                self.scrolling_region.right,
            )
            .map_err(TerminalStreamError::from)?;
        if printed {
            *self.previous_char = Some(cp);
        }
        Ok(())
    }

    fn print_repeat(&mut self, count: u16) -> Result<(), TerminalStreamError> {
        let Some(cp) = *self.previous_char else {
            return Ok(());
        };

        for _ in 0..count.max(1) {
            self.print(cp)?;
        }
        Ok(())
    }

    fn line_feed(&mut self) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .line_feed_basic(self.size.rows, self.size.cols)
            .map_err(TerminalStreamError::from)?;
        if self.modes.get(modes::Mode::Linefeed) {
            self.screens.active_mut().carriage_return_basic();
        }
        Ok(())
    }

    fn index(&mut self) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .line_feed_basic(self.size.rows, self.size.cols)
            .map_err(TerminalStreamError::from)
    }

    fn reverse_index(&mut self) -> Result<(), TerminalStreamError> {
        let (x, y) = self.screens.active_mut().cursor_position();
        if y != self.scrolling_region.top
            || x < self.scrolling_region.left
            || x > self.scrolling_region.right
        {
            self.screens.active_mut().cursor_up_basic(1);
            return Ok(());
        }

        self.screens
            .active_mut()
            .scroll_down_basic(
                1,
                self.scrolling_region.top,
                self.scrolling_region.bottom,
                self.scrolling_region.left,
                self.scrolling_region.right,
                self.scrolling_region.left == 0
                    && self.scrolling_region.right == self.size.cols - 1,
            )
            .map_err(TerminalStreamError::from)
    }

    fn set_mode_basic(
        &mut self,
        mode: modes::Mode,
        enabled: bool,
    ) -> Result<(), TerminalStreamError> {
        match mode {
            modes::Mode::AltScreenLegacy => {
                self.switch_screen_47(enabled)?;
                self.modes.set(mode, enabled);
                return Ok(());
            }
            modes::Mode::AltScreen => {
                self.switch_screen_1047(enabled)?;
                self.modes.set(mode, enabled);
                return Ok(());
            }
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.switch_screen_1049(enabled)?;
                self.modes.set(mode, enabled);
                return Ok(());
            }
            _ => {}
        }

        if mode == modes::Mode::CursorBlinking && self.default_cursor_blink.is_some() {
            return Ok(());
        }

        self.modes.set(mode, enabled);
        self.set_mouse_runtime_mode_flag(mode, enabled);

        match mode {
            modes::Mode::Origin => self.move_cursor_to_origin_home(),
            modes::Mode::EnableLeftAndRightMargin if !enabled => {
                self.scrolling_region.left = 0;
                self.scrolling_region.right = self.size.cols.saturating_sub(1);
            }
            modes::Mode::SaveCursor => {
                if enabled {
                    self.screens
                        .active_mut()
                        .save_cursor(self.modes.get(modes::Mode::Origin));
                } else {
                    self.restore_cursor_from_active_saved();
                }
            }
            // In-band size reports (mode 2048): on enable, send an immediate report (Issue 802 /
            // Exp 37, upstream `stream_handler.zig:751`).
            modes::Mode::InBandSizeReports if enabled => {
                self.emit_size_report(size_report::Style::Mode2048);
            }
            _ => {}
        }
        Ok(())
    }

    fn set_mouse_runtime_mode_flag(&mut self, mode: modes::Mode, enabled: bool) {
        match mode {
            modes::Mode::MouseEventX10 => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::X10
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseEventNormal => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::Normal
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseEventButton => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::Button
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseEventAny => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::Any
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseFormatUtf8 => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::Utf8
                } else {
                    mouse::MouseFormat::X10
                };
            }
            modes::Mode::MouseFormatSgr => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::Sgr
                } else {
                    mouse::MouseFormat::X10
                };
            }
            modes::Mode::MouseFormatUrxvt => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::Urxvt
                } else {
                    mouse::MouseFormat::X10
                };
            }
            modes::Mode::MouseFormatSgrPixels => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::SgrPixels
                } else {
                    mouse::MouseFormat::X10
                };
            }
            _ => {}
        }
    }

    fn write_pty_response(&mut self, bytes: &str) {
        self.write_pty_response_bytes(bytes.as_bytes());
    }

    fn write_pty_response_bytes(&mut self, bytes: &[u8]) {
        self.pty_response.extend_from_slice(bytes);
        if let Some(callback) = self.effects.write_pty {
            unsafe {
                callback(
                    self.effects.handle,
                    self.effects.userdata,
                    bytes.as_ptr(),
                    bytes.len(),
                );
            }
        }
    }

    fn kitty_graphics_command(&mut self, command: Command) -> Result<(), TerminalStreamError> {
        let metrics = self.kitty_graphics.cell_metrics(self.size);
        let execution = graphics_exec::execute_screen(self.screens.active_mut(), &command, metrics);
        if let Some(cursor_after) = execution.cursor_after {
            self.apply_kitty_cursor_after(cursor_after)?;
        }
        self.write_kitty_graphics_response(execution.response);
        Ok(())
    }

    fn apply_kitty_cursor_after(
        &mut self,
        cursor_after: graphics_exec::CursorAfter,
    ) -> Result<(), TerminalStreamError> {
        for _ in 0..cursor_after.rows {
            self.index_for_kitty_cursor_after()?;
        }

        let (_, row) = self.screens.active().cursor_position();
        let col = cursor_after
            .pin_x
            .saturating_add(cursor_after.columns)
            .saturating_add(1);
        self.set_cursor_pos_compat(row.into(), col as usize);
        Ok(())
    }

    fn index_for_kitty_cursor_after(&mut self) -> Result<(), TerminalStreamError> {
        let (x, y) = self.screens.active().cursor_position();
        if y < self.scrolling_region.top || y > self.scrolling_region.bottom {
            self.screens
                .active_mut()
                .cursor_down_basic(self.size.rows, 1);
            return Ok(());
        }

        if y == self.scrolling_region.bottom
            && x >= self.scrolling_region.left
            && x <= self.scrolling_region.right
        {
            self.screens
                .active_mut()
                .scroll_up_basic(
                    1,
                    self.size.rows,
                    self.size.cols,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from)?;
            return Ok(());
        }

        self.screens
            .active_mut()
            .cursor_down_basic(self.size.rows, 1);
        Ok(())
    }

    fn set_cursor_pos_compat(&mut self, row_req: usize, col_req: usize) {
        let (x_offset, y_offset, x_max, y_max) = if self.modes.get(modes::Mode::Origin) {
            (
                self.scrolling_region.left,
                self.scrolling_region.top,
                self.scrolling_region.right + 1,
                self.scrolling_region.bottom + 1,
            )
        } else {
            (0, 0, self.size.cols, self.size.rows)
        };

        let row = CellCountInt::try_from(row_req.max(1)).unwrap_or(CellCountInt::MAX);
        let col = CellCountInt::try_from(col_req.max(1)).unwrap_or(CellCountInt::MAX);
        let x = x_max.min(col.saturating_add(x_offset)).saturating_sub(1);
        let y = y_max.min(row.saturating_add(y_offset)).saturating_sub(1);
        self.screens.active_mut().cursor_position_basic(
            y.saturating_add(1),
            x.saturating_add(1),
            self.size.rows,
            self.size.cols,
        );
    }

    fn write_kitty_graphics_response(&mut self, response: Option<Response<'static>>) {
        let Some(response) = response else {
            return;
        };
        let mut bytes = Vec::new();
        response.encode(&mut bytes);
        if !bytes.is_empty() {
            self.write_pty_response_bytes(&bytes);
        }
    }

    fn bell(&mut self) {
        *self.pending_bell_count = (*self.pending_bell_count).saturating_add(1);
        if let Some(callback) = self.effects.bell {
            unsafe {
                callback(self.effects.handle, self.effects.userdata);
            }
        }
    }

    fn enquiry(&mut self) {
        let Some(callback) = self.effects.enquiry else {
            return;
        };
        let response = unsafe { callback(self.effects.handle, self.effects.userdata) };
        let Some(bytes) = copied_effect_string::<255>(response) else {
            return;
        };
        self.write_pty_response_bytes(&bytes);
    }

    fn xtversion(&mut self) {
        const DEFAULT: &str = "libroastty";

        let value = self
            .effects
            .xtversion
            .and_then(|callback| {
                let response = unsafe { callback(self.effects.handle, self.effects.userdata) };
                copied_effect_string::<256>(response)
            })
            .and_then(|bytes| std::str::from_utf8(&bytes).ok().map(ToOwned::to_owned))
            .unwrap_or_else(|| DEFAULT.to_string());

        self.write_pty_response(&format!("\x1bP>|{value}\x1b\\"));
    }

    fn title_changed(&mut self) {
        if let Some(callback) = self.effects.title_changed {
            unsafe {
                callback(self.effects.handle, self.effects.userdata);
            }
        }
    }

    fn device_attributes(
        &mut self,
        _request: device_attributes::Request,
    ) -> device_attributes::Attributes {
        let Some(callback) = self.effects.device_attributes else {
            return device_attributes::Attributes::default();
        };

        let mut attrs = TerminalDeviceAttributes::default();
        let ok = unsafe { callback(self.effects.handle, self.effects.userdata, &mut attrs) };
        if !ok {
            return device_attributes::Attributes::default();
        }

        let feature_len = attrs.primary.num_features.min(attrs.primary.features.len());
        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: attrs.primary.conformance_level,
                features: attrs.primary.features[..feature_len].to_vec(),
            },
            secondary: device_attributes::Secondary {
                device_type: attrs.secondary.device_type,
                firmware_version: attrs.secondary.firmware_version,
                rom_cartridge: attrs.secondary.rom_cartridge,
            },
            tertiary: device_attributes::Tertiary {
                unit_id: attrs.tertiary.unit_id,
            },
        }
    }

    fn color_scheme(&mut self) {
        let Some(callback) = self.effects.color_scheme else {
            return;
        };
        let mut scheme = 0;
        let ok = unsafe { callback(self.effects.handle, self.effects.userdata, &mut scheme) };
        if !ok {
            return;
        }
        self.write_color_scheme_report(scheme);
    }

    /// Emit the `CSI ? 997` color-scheme report for `scheme` (0 = light, 1 = dark). Shared by the DSR
    /// query (`color_scheme`) and the live change report (Issue 802 / Exp 36).
    fn write_color_scheme_report(&mut self, scheme: i32) {
        if let Some(bytes) = color_scheme_report_bytes(scheme) {
            self.write_pty_response_bytes(bytes);
        }
    }

    fn size_report(&mut self, request: size_report::Request) {
        if request == size_report::Request::Csi21T {
            if !*self.title_report {
                return;
            }
            self.write_pty_response(&format!("\x1b]l{}\x1b\\", self.title.as_str()));
            return;
        }
        let Some(style) = request.report_style() else {
            return;
        };
        self.emit_size_report(style);
    }

    /// Fetch the current size via the `size` callback and write the `style` size report (Issue 802 /
    /// Exp 37). Shared by the CSI 14/16/18 t query path and the mode-2048 enable emit.
    fn emit_size_report(&mut self, style: size_report::Style) {
        let Some(callback) = self.effects.size else {
            return;
        };
        let mut size = size_report::Size::default();
        let ok = unsafe { callback(self.effects.handle, self.effects.userdata, &mut size) };
        if !ok {
            return;
        }
        let response = size_report::encode(style, size);
        self.write_pty_response_bytes(&response);
    }

    fn dcs_command(&mut self, command: dcs::Command) {
        match command {
            dcs::Command::Decrqss(request) => self.decrqss(request),
            dcs::Command::XtGettcap(mut request) => while request.next().is_some() {},
            dcs::Command::Tmux(notification) => self.tmux_command(notification),
        }
    }

    fn tmux_command(&mut self, notification: tmux::ControlNotification) {
        match notification {
            tmux::ControlNotification::Enter => {
                if self.tmux_viewer.is_none() {
                    *self.tmux_viewer = Some(tmux::TmuxViewer::new());
                    self.tmux_windows.clear();
                }
            }
            tmux::ControlNotification::Exit => self.clear_tmux_state(),
            notification => {
                let Some(viewer) = self.tmux_viewer.as_mut() else {
                    return;
                };
                for action in viewer.next(notification) {
                    self.tmux_viewer_action(action);
                }
            }
        }
    }

    fn tmux_viewer_action(&mut self, action: tmux::TmuxViewerAction) {
        match action {
            tmux::TmuxViewerAction::Exit => self.clear_tmux_state(),
            tmux::TmuxViewerAction::Command(command) => {
                self.write_pty_response_bytes(command.as_bytes());
            }
            tmux::TmuxViewerAction::Windows(windows) => {
                *self.tmux_windows = windows;
            }
        }
    }

    fn clear_tmux_state(&mut self) {
        *self.tmux_viewer = None;
        self.tmux_windows.clear();
    }

    fn full_reset(&mut self) -> Result<(), TerminalStreamError> {
        self.screens.reset(*self.kitty_config);
        self.modes.reset();
        *self.scrolling_region = ScrollingRegion::full(self.size);
        self.tabstops.reset(TABSTOP_INTERVAL);
        self.title.clear();
        self.pwd.clear();
        *self.dcs = dcs::Handler::new();
        self.clear_tmux_state();
        self.kitty_graphics.reset();
        *self.flags = TerminalFlags::default();
        *self.previous_char = None;
        self.pending_clipboard_events.clear();
        Ok(())
    }

    fn restore_cursor_from_active_saved(&mut self) {
        let saved = self.screens.active().saved_cursor_or_default();
        self.modes.set(modes::Mode::Origin, saved.origin());
        self.screens
            .active_mut()
            .restore_saved_cursor(saved, self.size.cols, self.size.rows);
    }

    fn switch_screen_47(&mut self, enabled: bool) -> Result<(), TerminalStreamError> {
        let target = if enabled {
            TerminalScreenKey::Alternate
        } else {
            TerminalScreenKey::Primary
        };
        self.switch_screen_copy_cursor(target)
    }

    fn switch_screen_1047(&mut self, enabled: bool) -> Result<(), TerminalStreamError> {
        if !enabled && self.screens.active_key() == TerminalScreenKey::Alternate {
            self.erase_active_display_complete()?;
        }

        let target = if enabled {
            TerminalScreenKey::Alternate
        } else {
            TerminalScreenKey::Primary
        };
        self.switch_screen_copy_cursor(target)
    }

    fn switch_screen_1049(&mut self, enabled: bool) -> Result<(), TerminalStreamError> {
        if enabled {
            self.screens
                .active_mut()
                .save_cursor(self.modes.get(modes::Mode::Origin));
            let old = self.switch_screen(TerminalScreenKey::Alternate)?;
            self.erase_active_display_complete()?;
            if let Some(old) = old {
                self.screens
                    .copy_cursor_from_to(old, TerminalScreenKey::Alternate);
            }
        } else {
            self.switch_screen(TerminalScreenKey::Primary)?;
            self.restore_cursor_from_active_saved();
        }
        Ok(())
    }

    fn switch_screen_copy_cursor(
        &mut self,
        target: TerminalScreenKey,
    ) -> Result<(), TerminalStreamError> {
        if let Some(old) = self.switch_screen(target)? {
            self.screens.copy_cursor_from_to(old, target);
        }
        Ok(())
    }

    fn switch_screen(
        &mut self,
        target: TerminalScreenKey,
    ) -> Result<Option<TerminalScreenKey>, TerminalStreamError> {
        self.screens
            .switch_to(target, self.size.cols, self.size.rows, *self.kitty_config)
            .map_err(|_| TerminalStreamError::PageAlloc)
    }

    fn erase_active_display_complete(&mut self) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .erase_display_basic(
                stream::EraseDisplayMode::Complete,
                self.size.rows,
                self.size.cols,
                false,
            )
            .map_err(TerminalStreamError::from)
    }

    fn configure_charset(&mut self, slot: charsets::CharsetSlot, charset: charsets::Charset) {
        self.screens.active_mut().configure_charset(slot, charset);
    }

    fn invoke_charset(
        &mut self,
        bank: charsets::CharsetBank,
        slot: charsets::CharsetSlot,
        single: bool,
    ) {
        self.screens.active_mut().invoke_charset(bank, slot, single);
    }

    fn decrqss(&mut self, request: dcs::Decrqss) {
        let payload = match request {
            dcs::Decrqss::None => None,
            dcs::Decrqss::Sgr => Some(format!("{}m", self.decrqss_sgr_payload())),
            dcs::Decrqss::Decscusr => Some(format!(
                "{} q",
                self.screens
                    .active_mut()
                    .cursor_visual_style()
                    .decscusr_report(self.modes.get(modes::Mode::CursorBlinking))
            )),
            dcs::Decrqss::Decstbm => Some(format!(
                "{};{}r",
                self.scrolling_region.top + 1,
                self.scrolling_region.bottom + 1
            )),
            dcs::Decrqss::Decslrm => {
                self.modes
                    .get(modes::Mode::EnableLeftAndRightMargin)
                    .then(|| {
                        format!(
                            "{};{}s",
                            self.scrolling_region.left + 1,
                            self.scrolling_region.right + 1
                        )
                    })
            }
        };

        match payload {
            Some(payload) => self.write_pty_response(&format!("\x1bP1$r{payload}\x1b\\")),
            None => self.write_pty_response("\x1bP0$r\x1b\\"),
        }
    }

    fn decrqss_sgr_payload(&self) -> String {
        let style = self.screens.active().cursor_text_style();
        let mut output = String::from("0");

        if style.flags.bold {
            output.push_str(";1");
        }
        if style.flags.faint {
            output.push_str(";2");
        }
        if style.flags.italic {
            output.push_str(";3");
        }
        if style.flags.underline != sgr::Underline::None {
            output.push_str(";4");
        }
        if style.flags.blink {
            output.push_str(";5");
        }
        if style.flags.inverse {
            output.push_str(";7");
        }
        if style.flags.invisible {
            output.push_str(";8");
        }
        if style.flags.strikethrough {
            output.push_str(";9");
        }

        push_decrqss_color(&mut output, 38, 3, 9, style.fg_color);
        push_decrqss_color(&mut output, 48, 4, 10, style.bg_color);

        output
    }

    fn device_status(&mut self, request: device_status::Request) {
        match request {
            device_status::Request::OperatingStatus => self.write_pty_response("\x1b[0n"),
            device_status::Request::CursorPosition => {
                let (cursor_x, cursor_y) = self.screens.active_mut().cursor_position();
                let (x, y) = if self.modes.get(modes::Mode::Origin) {
                    (
                        cursor_x.saturating_sub(self.scrolling_region.left),
                        cursor_y.saturating_sub(self.scrolling_region.top),
                    )
                } else {
                    (cursor_x, cursor_y)
                };
                self.write_pty_response(&format!("\x1b[{};{}R", y + 1, x + 1));
            }
            device_status::Request::ColorScheme => self.color_scheme(),
        }
    }

    fn color_operation(&mut self, requests: osc::ColorRequests) {
        for request in requests.iter() {
            match request {
                osc::ColorRequest::SetPalette { index, rgb } => {
                    self.colors.palette.set(index, rgb);
                }
                osc::ColorRequest::QueryPalette { index, terminator } => {
                    self.write_palette_query_response(index, terminator);
                }
                osc::ColorRequest::ResetPalette { index } => {
                    self.colors.palette.reset(index);
                }
                osc::ColorRequest::ResetAllPalette => {
                    self.colors.palette.reset_all();
                }
                osc::ColorRequest::SetDynamic { target, rgb } => {
                    self.dynamic_color_mut(target).set(rgb);
                }
                osc::ColorRequest::QueryDynamic { target, terminator } => {
                    self.write_dynamic_query_response(target, terminator);
                }
                osc::ColorRequest::ResetDynamic { target } => {
                    self.dynamic_color_mut(target).reset();
                }
            }
        }
    }

    fn dynamic_color_mut(&mut self, target: osc::DynamicColor) -> &mut color::DynamicRgb {
        match target {
            osc::DynamicColor::Foreground => &mut self.colors.foreground,
            osc::DynamicColor::Background => &mut self.colors.background,
            osc::DynamicColor::Cursor => &mut self.colors.cursor,
        }
    }

    fn dynamic_color(&self, target: osc::DynamicColor) -> Option<color::Rgb> {
        match target {
            osc::DynamicColor::Foreground => self.colors.foreground.get(),
            osc::DynamicColor::Background => self.colors.background.get(),
            osc::DynamicColor::Cursor => self
                .colors
                .cursor
                .get()
                .or_else(|| self.colors.foreground.get()),
        }
    }

    fn write_palette_query_response(&mut self, index: u8, terminator: osc::Terminator) {
        let rgb = self.colors.palette.current()[index as usize];
        let response = format!(
            "\x1b]4;{};rgb:{:04x}/{:04x}/{:04x}",
            index,
            u16::from(rgb.r) * 257,
            u16::from(rgb.g) * 257,
            u16::from(rgb.b) * 257
        );
        self.write_pty_response(&response);
        self.write_pty_response_bytes(terminator.bytes());
    }

    fn write_dynamic_query_response(
        &mut self,
        target: osc::DynamicColor,
        terminator: osc::Terminator,
    ) {
        let Some(rgb) = self.dynamic_color(target) else {
            return;
        };
        let response = format!(
            "\x1b]{};rgb:{:04x}/{:04x}/{:04x}",
            target.number(),
            u16::from(rgb.r) * 257,
            u16::from(rgb.g) * 257,
            u16::from(rgb.b) * 257
        );
        self.write_pty_response(&response);
        self.write_pty_response_bytes(terminator.bytes());
    }

    fn kitty_color_operation(
        &mut self,
        requests: super::kitty::ColorRequests,
        terminator: osc::Terminator,
    ) {
        let mut response = String::new();
        for request in requests.iter() {
            match request {
                super::kitty::ColorRequest::Set { key, rgb } => {
                    self.set_kitty_color(key, rgb);
                }
                super::kitty::ColorRequest::Reset(key) => {
                    self.reset_kitty_color(key);
                }
                super::kitty::ColorRequest::Query(key) => {
                    self.write_kitty_color_query(&mut response, key);
                }
            }
        }

        if !response.is_empty() {
            self.write_pty_response(&response);
            self.write_pty_response_bytes(terminator.bytes());
        }
    }

    fn semantic_prompt(
        &mut self,
        prompt: super::semantic_prompt::SemanticPrompt<'_>,
    ) -> Result<(), TerminalStreamError> {
        use super::semantic_prompt::Action;

        if matches!(
            prompt.action,
            Action::FreshLineNewPrompt | Action::NewCommand | Action::PromptStart
        ) {
            self.screens
                .active_mut()
                .set_prompt_click_mode(prompt_click_mode(prompt));
        }

        match prompt.action {
            Action::FreshLine => self.semantic_prompt_fresh_line(),
            Action::FreshLineNewPrompt => {
                self.semantic_prompt_fresh_line()?;
                self.screens
                    .active_mut()
                    .set_cursor_semantic_prompt(
                        prompt
                            .prompt_kind()
                            .unwrap_or(super::semantic_prompt::PromptKind::Initial),
                    )
                    .map_err(TerminalStreamError::from)
            }
            Action::NewCommand => {
                self.semantic_prompt(super::semantic_prompt::SemanticPrompt::new(
                    Action::FreshLineNewPrompt,
                    prompt.options(),
                ))
            }
            Action::PromptStart => self
                .screens
                .active_mut()
                .set_cursor_semantic_prompt(
                    prompt
                        .prompt_kind()
                        .unwrap_or(super::semantic_prompt::PromptKind::Initial),
                )
                .map_err(TerminalStreamError::from),
            Action::EndPromptStartInput => {
                self.screens.active_mut().set_cursor_semantic_input(false);
                Ok(())
            }
            Action::EndPromptStartInputTerminateEol => {
                self.screens.active_mut().set_cursor_semantic_input(true);
                Ok(())
            }
            Action::EndInputStartOutput => {
                self.screens.active_mut().set_cursor_semantic_output();
                if self.screens.active_mut().cursor_position().0 == 0
                    && self.screens.active_mut().current_row_semantic_prompt()
                        != Some(SemanticPrompt::None)
                {
                    self.screens
                        .active_mut()
                        .clear_current_row_semantic_prompt()
                        .map_err(TerminalStreamError::from)?;
                }
                Ok(())
            }
            Action::EndCommand => {
                self.screens.active_mut().set_cursor_semantic_output();
                Ok(())
            }
        }
    }

    fn semantic_prompt_fresh_line(&mut self) -> Result<(), TerminalStreamError> {
        let left_margin =
            if self.screens.active_mut().cursor_position().0 < self.scrolling_region.left {
                0
            } else {
                self.scrolling_region.left
            };
        if self.screens.active_mut().cursor_position().0 == left_margin {
            return Ok(());
        }

        self.screens.active_mut().carriage_return_basic();
        self.index()
    }

    fn set_kitty_color(&mut self, key: super::kitty::ColorKind, rgb: color::Rgb) {
        match key {
            super::kitty::ColorKind::Palette(index) => {
                self.colors.palette.set(index, rgb);
            }
            super::kitty::ColorKind::Special(special) => match special {
                super::kitty::ColorSpecial::Foreground => self.colors.foreground.set(rgb),
                super::kitty::ColorSpecial::Background => self.colors.background.set(rgb),
                super::kitty::ColorSpecial::Cursor => self.colors.cursor.set(rgb),
                super::kitty::ColorSpecial::SelectionForeground
                | super::kitty::ColorSpecial::SelectionBackground
                | super::kitty::ColorSpecial::CursorText
                | super::kitty::ColorSpecial::VisualBell
                | super::kitty::ColorSpecial::SecondTransparentBackground => {}
            },
        }
    }

    fn reset_kitty_color(&mut self, key: super::kitty::ColorKind) {
        match key {
            super::kitty::ColorKind::Palette(index) => {
                self.colors.palette.reset(index);
            }
            super::kitty::ColorKind::Special(special) => match special {
                super::kitty::ColorSpecial::Foreground => self.colors.foreground.reset(),
                super::kitty::ColorSpecial::Background => self.colors.background.reset(),
                super::kitty::ColorSpecial::Cursor => self.colors.cursor.reset(),
                super::kitty::ColorSpecial::SelectionForeground
                | super::kitty::ColorSpecial::SelectionBackground
                | super::kitty::ColorSpecial::CursorText
                | super::kitty::ColorSpecial::VisualBell
                | super::kitty::ColorSpecial::SecondTransparentBackground => {}
            },
        }
    }

    fn write_kitty_color_query(&self, response: &mut String, key: super::kitty::ColorKind) {
        if response.is_empty() {
            response.push_str("\x1b]21");
        }

        match key {
            super::kitty::ColorKind::Palette(index) => {
                append_kitty_color_response(
                    response,
                    key,
                    Some(self.colors.palette.current()[index as usize]),
                );
            }
            super::kitty::ColorKind::Special(special) => match special {
                super::kitty::ColorSpecial::Foreground => {
                    append_kitty_color_response(response, key, self.colors.foreground.get());
                }
                super::kitty::ColorSpecial::Background => {
                    append_kitty_color_response(response, key, self.colors.background.get());
                }
                super::kitty::ColorSpecial::Cursor => {
                    append_kitty_color_response(response, key, self.colors.cursor.get());
                }
                super::kitty::ColorSpecial::SelectionForeground
                | super::kitty::ColorSpecial::SelectionBackground
                | super::kitty::ColorSpecial::CursorText
                | super::kitty::ColorSpecial::VisualBell
                | super::kitty::ColorSpecial::SecondTransparentBackground => {}
            },
        }
    }

    fn move_cursor_to_origin_home(&mut self) {
        let (x, y) = if self.modes.get(modes::Mode::Origin) {
            (self.scrolling_region.left, self.scrolling_region.top)
        } else {
            (0, 0)
        };
        self.screens.active_mut().cursor_position_basic(
            y.saturating_add(1),
            x.saturating_add(1),
            self.size.rows,
            self.size.cols,
        );
    }
}

fn push_decrqss_color(
    output: &mut String,
    extended_prefix: u8,
    normal_prefix: u8,
    bright_prefix: u8,
    color: style::Color,
) {
    match color {
        style::Color::None => {}
        style::Color::Palette(idx) if idx < 8 => {
            output.push_str(&format!(";{}{}", normal_prefix, idx));
        }
        style::Color::Palette(idx) if idx < 16 => {
            output.push_str(&format!(";{}{}", bright_prefix, idx - 8));
        }
        style::Color::Palette(idx) => {
            output.push_str(&format!(";{}:5:{}", extended_prefix, idx));
        }
        style::Color::Rgb(rgb) => {
            output.push_str(&format!(
                ";{}:2::{}:{}:{}",
                extended_prefix, rgb.r, rgb.g, rgb.b
            ));
        }
    }
}

impl From<BasicPrintError> for TerminalStreamError {
    fn from(err: BasicPrintError) -> Self {
        match err {
            BasicPrintError::PageAlloc => Self::PageAlloc,
            BasicPrintError::Cell(err) => match err {
                super::page_list::BasicCellWriteError::InvalidPoint => Self::InvalidPoint,
                super::page_list::BasicCellWriteError::ManagedCell => Self::ManagedCellUnsupported,
            },
        }
    }
}

impl From<super::page_list::BasicCellWriteError> for TerminalStreamError {
    fn from(err: super::page_list::BasicCellWriteError) -> Self {
        match err {
            super::page_list::BasicCellWriteError::InvalidPoint => Self::InvalidPoint,
            super::page_list::BasicCellWriteError::ManagedCell => Self::ManagedCellUnsupported,
        }
    }
}

impl From<EraseDisplayError> for TerminalStreamError {
    fn from(err: EraseDisplayError) -> Self {
        match err {
            EraseDisplayError::PageAlloc => Self::PageAlloc,
            EraseDisplayError::Cell(err) => match err {
                super::page_list::BasicCellWriteError::InvalidPoint => Self::InvalidPoint,
                super::page_list::BasicCellWriteError::ManagedCell => Self::ManagedCellUnsupported,
            },
        }
    }
}

impl ScrollingRegion {
    fn full(size: TerminalSize) -> Self {
        Self {
            top: 0,
            bottom: size.rows - 1,
            left: 0,
            right: size.cols - 1,
        }
    }

    fn assert_valid(self, size: TerminalSize) {
        assert!(self.is_valid(size));
    }

    fn is_valid(self, size: TerminalSize) -> bool {
        self.top <= self.bottom
            && self.left <= self.right
            && self.bottom < size.rows
            && self.right < size.cols
            && (size.rows <= 1 || self.top < self.bottom)
            && (size.cols <= 1 || self.left < self.right)
    }
}

impl TerminalTitle {
    fn set(&mut self, title: &str) {
        self.text.clear();
        self.text.push_str(title);
    }

    fn clear(&mut self) {
        self.text.clear();
    }

    fn as_str(&self) -> &str {
        &self.text
    }
}

impl TerminalPwd {
    fn set(&mut self, pwd: &str) {
        self.text.clear();
        if !pwd.is_empty() {
            self.text.push_str(pwd);
            self.text.push('\0');
        }
    }

    fn clear(&mut self) {
        self.text.clear();
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn stored_str(&self) -> &str {
        &self.text
    }

    fn logical_str(&self) -> Option<&str> {
        if self.text.is_empty() {
            return None;
        }

        Some(&self.text[..self.text.len() - 1])
    }
}

impl<'a> TerminalFormatterOptions<'a> {
    pub(super) const fn new(emit: PageOutputFormat) -> Self {
        Self {
            screen: ScreenFormatterOptions::new(emit),
        }
    }

    pub(super) const fn trim(mut self, trim: bool) -> Self {
        self.screen = self.screen.trim(trim);
        self
    }

    pub(super) const fn unwrap(mut self, unwrap: bool) -> Self {
        self.screen = self.screen.unwrap(unwrap);
        self
    }

    pub(super) const fn palette(mut self, palette: Option<&'a color::Palette>) -> Self {
        self.screen = self.screen.palette(palette);
        self
    }

    pub(super) const fn codepoint_map(
        mut self,
        codepoint_map: Option<&'a [CodepointMapEntry]>,
    ) -> Self {
        self.screen = self.screen.codepoint_map(codepoint_map);
        self
    }
}

impl<'a> TerminalFormatter<'a> {
    pub(super) fn init(terminal: &'a Terminal, options: TerminalFormatterOptions<'a>) -> Self {
        Self {
            terminal,
            options,
            content: ScreenFormatterContent::Selection(None),
            extra: TerminalFormatterExtra::none(),
        }
    }

    pub(super) const fn with_content(mut self, content: ScreenFormatterContent) -> Self {
        self.content = content;
        self
    }

    pub(super) const fn with_extra(mut self, extra: TerminalFormatterExtra) -> Self {
        self.extra = extra;
        self
    }

    pub(super) fn format(self) -> String {
        let mut output = self.terminal_prefix_string();
        output.push_str(
            &ScreenFormatter::init(self.terminal.screens.active(), self.options.screen)
                .with_content(self.content)
                .with_extra(self.extra.screen)
                .format(),
        );
        output.push_str(&self.terminal_suffix_string());
        output
    }

    pub(super) fn format_with_pin_map(self) -> PageStringWithPinMap {
        let prefix = self.terminal_prefix_string();
        let suffix = self.terminal_suffix_string();
        let mut output = ScreenFormatter::init(self.terminal.screens.active(), self.options.screen)
            .with_content(self.content)
            .with_extra(self.extra.screen)
            .format_with_pin_map();

        if !prefix.is_empty() {
            let top_left = self.terminal.screens.active().top_left_pin();
            let mut text = prefix;
            let mut pin_map = vec![top_left; text.len()];
            text.push_str(&output.text);
            pin_map.append(&mut output.pin_map);
            output = PageStringWithPinMap { text, pin_map };
        }

        if !suffix.is_empty() {
            let suffix_pin = output
                .pin_map
                .last()
                .copied()
                .unwrap_or_else(|| self.terminal.screens.active().top_left_pin());
            output
                .pin_map
                .extend(std::iter::repeat_n(suffix_pin, suffix.len()));
            output.text.push_str(&suffix);
        }

        output
    }

    fn terminal_prefix_string(&self) -> String {
        let mut output = self.palette_string();
        output.push_str(&self.modes_string());
        output
    }

    fn terminal_suffix_string(&self) -> String {
        let mut output = self.scrolling_region_string();
        output.push_str(&self.tabstops_string());
        output.push_str(&self.keyboard_string());
        output.push_str(&self.pwd_string());
        output
    }

    fn palette_string(&self) -> String {
        if !self.extra.palette {
            return String::new();
        }

        let palette = self.terminal.colors.palette.current();
        match self.options.screen.emit() {
            PageOutputFormat::Plain => String::new(),
            PageOutputFormat::Vt => palette_vt_string(palette),
            PageOutputFormat::Html => palette_html_string(palette),
        }
    }

    fn modes_string(&self) -> String {
        if !self.extra.modes || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        modes_vt_string(&self.terminal.modes)
    }

    fn scrolling_region_string(&self) -> String {
        if !self.extra.scrolling_region || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        scrolling_region_vt_string(self.terminal.size, self.terminal.scrolling_region)
    }

    fn tabstops_string(&self) -> String {
        if !self.extra.tabstops || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        tabstops_vt_string(&self.terminal.tabstops)
    }

    fn keyboard_string(&self) -> String {
        if !self.extra.keyboard || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        keyboard_vt_string(self.terminal.flags)
    }

    fn pwd_string(&self) -> String {
        if !self.extra.pwd || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        pwd_vt_string(&self.terminal.pwd)
    }
}

impl TerminalFormatterExtra {
    pub(crate) const fn none() -> Self {
        Self {
            palette: false,
            modes: false,
            scrolling_region: false,
            tabstops: false,
            keyboard: false,
            pwd: false,
            screen: ScreenFormatterExtra::none(),
        }
    }

    pub(crate) const fn palette(mut self, palette: bool) -> Self {
        self.palette = palette;
        self
    }

    pub(crate) const fn modes(mut self, modes: bool) -> Self {
        self.modes = modes;
        self
    }

    pub(crate) const fn scrolling_region(mut self, scrolling_region: bool) -> Self {
        self.scrolling_region = scrolling_region;
        self
    }

    pub(crate) const fn tabstops(mut self, tabstops: bool) -> Self {
        self.tabstops = tabstops;
        self
    }

    pub(crate) const fn keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }

    pub(crate) const fn pwd(mut self, pwd: bool) -> Self {
        self.pwd = pwd;
        self
    }

    pub(super) const fn screen(mut self, screen: ScreenFormatterExtra) -> Self {
        self.screen = screen;
        self
    }

    pub(crate) const fn screen_extra(
        self,
        cursor: bool,
        style: bool,
        hyperlink: bool,
        protection: bool,
        kitty_keyboard: bool,
        charsets: bool,
    ) -> Self {
        self.screen(
            ScreenFormatterExtra::none()
                .cursor(cursor)
                .style(style)
                .hyperlink(hyperlink)
                .protection(protection)
                .kitty_keyboard(kitty_keyboard)
                .charsets(charsets),
        )
    }
}

fn palette_vt_string(palette: &color::Palette) -> String {
    let mut output = String::new();
    for (index, rgb) in palette.iter().enumerate() {
        output.push_str(&format!(
            "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
            index, rgb.r, rgb.g, rgb.b
        ));
    }
    output
}

fn palette_html_string(palette: &color::Palette) -> String {
    let mut output = String::from("<style>:root{");
    for (index, rgb) in palette.iter().enumerate() {
        output.push_str(&format!(
            "--vt-palette-{}: #{:02x}{:02x}{:02x};",
            index, rgb.r, rgb.g, rgb.b
        ));
    }
    output.push_str("}</style>");
    output
}

fn rgb_tuple(rgb: color::Rgb) -> (u8, u8, u8) {
    (rgb.r, rgb.g, rgb.b)
}

fn prompt_click_mode(prompt: super::semantic_prompt::SemanticPrompt<'_>) -> PromptClickMode {
    if prompt.click_events() == Some(true) {
        return PromptClickMode::ClickEvents;
    }
    match prompt.click() {
        Some(super::semantic_prompt::Click::Line) => PromptClickMode::Line,
        Some(super::semantic_prompt::Click::Multiple) => PromptClickMode::Multiple,
        Some(super::semantic_prompt::Click::ConservativeVertical) => {
            PromptClickMode::ConservativeVertical
        }
        Some(super::semantic_prompt::Click::SmartVertical) => PromptClickMode::SmartVertical,
        None => PromptClickMode::None,
    }
}

fn rgb_from_tuple(rgb: (u8, u8, u8)) -> color::Rgb {
    color::Rgb::new(rgb.0, rgb.1, rgb.2)
}

fn palette_tuple(palette: color::Palette) -> [(u8, u8, u8); 256] {
    let mut result = [(0, 0, 0); 256];
    for (index, rgb) in palette.into_iter().enumerate() {
        result[index] = rgb_tuple(rgb);
    }
    result
}

fn palette_from_tuple(palette: [(u8, u8, u8); 256]) -> color::Palette {
    let mut result = [color::Rgb::new(0, 0, 0); 256];
    for (index, rgb) in palette.into_iter().enumerate() {
        result[index] = rgb_from_tuple(rgb);
    }
    result
}

fn append_kitty_color_response(
    output: &mut String,
    key: super::kitty::ColorKind,
    rgb: Option<color::Rgb>,
) {
    output.push(';');
    key.append_to_string(output);
    output.push('=');
    if let Some(rgb) = rgb {
        output.push_str(&format!("rgb:{:02x}/{:02x}/{:02x}", rgb.r, rgb.g, rgb.b));
    }
}

fn modes_vt_string(state: &modes::ModeState) -> String {
    let mut output = String::new();
    for entry in modes::entries() {
        let current = state.get(entry.mode);
        if current == state.default_for(entry.mode) {
            continue;
        }

        output.push_str(&format!(
            "\x1b[{}{}{}",
            if entry.ansi { "" } else { "?" },
            entry.value,
            if current { "h" } else { "l" }
        ));
    }
    output
}

fn scrolling_region_vt_string(size: TerminalSize, region: ScrollingRegion) -> String {
    let mut output = String::new();
    if region.top != 0 || region.bottom != size.rows - 1 {
        output.push_str(&format!("\x1b[{};{}r", region.top + 1, region.bottom + 1));
    }
    if region.left != 0 || region.right != size.cols - 1 {
        output.push_str(&format!("\x1b[{};{}s", region.left + 1, region.right + 1));
    }
    output
}

fn tabstops_vt_string(tabstops: &tabstops::Tabstops) -> String {
    let mut output = String::from("\x1b[3g");
    for col in 0..tabstops.cols() {
        if tabstops.get(col) {
            output.push_str(&format!("\x1b[{}G\x1bH", col + 1));
        }
    }
    output
}

fn keyboard_vt_string(flags: TerminalFlags) -> String {
    if flags.modify_other_keys_2 {
        "\x1b[>4;2m".to_string()
    } else {
        String::new()
    }
}

fn pwd_vt_string(pwd: &TerminalPwd) -> String {
    if pwd.is_empty() {
        return String::new();
    }

    let mut output = String::from("\x1b]7;");
    output.push_str(pwd.stored_str());
    output.push_str("\x1b\\");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::charsets;
    use crate::terminal::color;
    use crate::terminal::cursor;
    use crate::terminal::kitty::graphics_command::TransmissionFormat;
    use crate::terminal::kitty::graphics_storage::{
        Placement, PlacementId, PlacementKey, PlacementLocation,
    };
    use crate::terminal::kitty::{KeyFlags, KeySetMode};
    use crate::terminal::modes::Mode;
    use crate::terminal::page::{HyperlinkSnapshotId, SemanticContent, SemanticPrompt, Wide};
    use crate::terminal::page_list::{CodepointReplacement, Pin};
    use crate::terminal::point::{Coordinate, Point};
    use crate::terminal::screen::ScreenCursorHyperlinkId;
    use crate::terminal::selection;
    use crate::terminal::style;
    use std::ffi::c_void;
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TMUX_SIMPLE_LAYOUT: &str = "d962,80x24,0,0,42";
    static TEST_BELL_CALLBACK_COUNT: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn test_bell_callback(_: *mut c_void, _: *mut c_void) {
        TEST_BELL_CALLBACK_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn enter_tmux_dcs(terminal: &mut Terminal) {
        terminal.next_slice(b"\x1bP1000p").unwrap();
    }

    fn start_tmux_command_queue(terminal: &mut Terminal) {
        enter_tmux_dcs(terminal);
        terminal
            .next_slice(b"%begin 1 1 1\n%end 1 1 1\n%session-changed $42 main\n")
            .unwrap();
        terminal.clear_pty_response();
    }

    fn reach_tmux_list_windows_command(terminal: &mut Terminal) {
        start_tmux_command_queue(terminal);
        terminal
            .next_slice(b"%begin 2 1 1\n3.5a\n%end 2 1 1\n")
            .unwrap();
        terminal.clear_pty_response();
    }

    fn cache_tmux_window_with_pending_captures(terminal: &mut Terminal) {
        reach_tmux_list_windows_command(terminal);
        terminal
            .next_slice(
                format!("%begin 3 1 1\n$42 @2 80 24 {TMUX_SIMPLE_LAYOUT}\n%end 3 1 1\n").as_bytes(),
            )
            .unwrap();
        assert_eq!(terminal.tmux_windows.len(), 1);
        terminal.clear_pty_response();
    }

    #[test]
    fn bell_runtime_pending_count_accumulates_without_callback() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x07hello\x07").unwrap();

        assert_eq!(terminal.take_pending_bell_count(), 2);
        assert_eq!(terminal.take_pending_bell_count(), 0);
        assert!(terminal.plain_screen(false).contains("hello"));
    }

    #[test]
    fn bell_runtime_pending_count_preserves_callback_effect() {
        TEST_BELL_CALLBACK_COUNT.store(0, Ordering::SeqCst);
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.set_bell_callback(Some(test_bell_callback));

        terminal.next_slice(b"\x07\x07").unwrap();

        assert_eq!(terminal.take_pending_bell_count(), 2);
        assert_eq!(TEST_BELL_CALLBACK_COUNT.load(Ordering::SeqCst), 2);
    }

    fn terminal_with_lines(lines: &[&str]) -> Terminal {
        let rows = lines.len().max(1);
        let cols = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(1)
            .max(1);
        let mut terminal = Terminal::init(cols.try_into().unwrap(), rows.try_into().unwrap(), None)
            .expect("test terminal must initialize");
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(lines);
        terminal
    }

    fn active_pin(terminal: &Terminal, x: CellCountInt, y: u32) -> Pin {
        terminal.screens.active().pin_for_tests(x, y)
    }

    fn active_selection(
        terminal: &Terminal,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) -> selection::Selection {
        selection::Selection::new(
            active_pin(terminal, start.0, start.1),
            active_pin(terminal, end.0, end.1),
            false,
        )
    }

    fn formatter<'a>(terminal: &'a Terminal, emit: PageOutputFormat) -> TerminalFormatter<'a> {
        TerminalFormatter::init(terminal, TerminalFormatterOptions::new(emit).unwrap(true))
    }

    fn plain_with_unwrap(terminal: &Terminal, unwrap: bool) -> String {
        TerminalFormatter::init(
            terminal,
            TerminalFormatterOptions::new(PageOutputFormat::Plain).unwrap(unwrap),
        )
        .format()
    }

    fn screen_formatter<'a>(terminal: &'a Terminal, emit: PageOutputFormat) -> ScreenFormatter<'a> {
        ScreenFormatter::init(
            terminal.screens.active(),
            ScreenFormatterOptions::new(emit).unwrap(true),
        )
    }

    fn kitty_transmit_apc(image_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=t,f=32,s=1,v=1,i={image_id};AQIDBA==\x1b\\").into_bytes()
    }

    fn kitty_png_transmit_apc(image_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=t,f=100,i={image_id};ZmFrZSBwbmc=\x1b\\").into_bytes()
    }

    struct SysDecodeGuard {
        _guard: MutexGuard<'static, ()>,
    }

    impl SysDecodeGuard {
        fn with_png_decoder(
            callback: unsafe extern "C" fn(
                *mut c_void,
                *const crate::RoasttyAllocator,
                *const u8,
                usize,
                *mut crate::RoasttySysImage,
            ) -> bool,
        ) -> Self {
            let guard = crate::SYS_TEST_LOCK.lock().unwrap();
            assert_eq!(
                crate::roastty_sys_set(
                    crate::ROASTTY_SYS_OPT_DECODE_PNG,
                    callback as *const c_void
                ),
                crate::ROASTTY_SUCCESS
            );
            Self { _guard: guard }
        }
    }

    impl Drop for SysDecodeGuard {
        fn drop(&mut self) {
            let _ = crate::roastty_sys_set(crate::ROASTTY_SYS_OPT_DECODE_PNG, ptr::null());
        }
    }

    unsafe extern "C" fn decode_png_rgba_1x1(
        _userdata: *mut c_void,
        allocator: *const crate::RoasttyAllocator,
        _data: *const u8,
        _data_len: usize,
        out: *mut crate::RoasttySysImage,
    ) -> bool {
        let data = [9, 8, 7, 6];
        let ptr = crate::roastty_alloc(allocator, data.len());
        if ptr.is_null() {
            return false;
        }
        ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        out.write(crate::RoasttySysImage {
            width: 1,
            height: 1,
            data: ptr,
            data_len: data.len(),
        });
        true
    }

    struct KittyFileTestDir {
        path: PathBuf,
    }

    struct KittySharedMemoryObject {
        name: std::ffi::CString,
    }

    impl KittySharedMemoryObject {
        fn new(data: &[u8]) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let name = std::ffi::CString::new(format!(
                "/rt{:x}{:x}",
                std::process::id(),
                (nanos & 0xffff_ffff) as u64
            ))
            .unwrap();
            let fd = unsafe {
                libc::shm_open(
                    name.as_ptr(),
                    libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                    0o600,
                )
            };
            assert!(fd >= 0);
            assert_eq!(unsafe { libc::ftruncate(fd, data.len() as libc::off_t) }, 0);
            if !data.is_empty() {
                let ptr = unsafe {
                    libc::mmap(
                        std::ptr::null_mut(),
                        data.len(),
                        libc::PROT_READ | libc::PROT_WRITE,
                        libc::MAP_SHARED,
                        fd,
                        0,
                    )
                };
                assert_ne!(ptr, libc::MAP_FAILED);
                unsafe {
                    std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.cast::<u8>(), data.len());
                    assert_eq!(libc::munmap(ptr, data.len()), 0);
                }
            }
            assert_eq!(unsafe { libc::close(fd) }, 0);
            Self { name }
        }

        fn name_bytes(&self) -> &[u8] {
            self.name.as_bytes()
        }

        fn exists(&self) -> bool {
            let fd = unsafe { libc::shm_open(self.name.as_ptr(), libc::O_RDONLY, 0) };
            if fd < 0 {
                return false;
            }
            unsafe {
                let _ = libc::close(fd);
            }
            true
        }
    }

    impl Drop for KittySharedMemoryObject {
        fn drop(&mut self) {
            unsafe {
                let _ = libc::shm_unlink(self.name.as_ptr());
            }
        }
    }

    impl KittyFileTestDir {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "roastty-terminal-kitty-file-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn write(&self, name: &str, data: &[u8]) -> PathBuf {
            let path = self.path.join(name);
            fs::write(&path, data).unwrap();
            fs::canonicalize(path).unwrap()
        }
    }

    impl Drop for KittyFileTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn base64(data: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut output = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0];
            let b1 = *chunk.get(1).unwrap_or(&0);
            let b2 = *chunk.get(2).unwrap_or(&0);
            output.push(TABLE[(b0 >> 2) as usize] as char);
            output.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                output.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                output.push('=');
            }
            if chunk.len() > 2 {
                output.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
            } else {
                output.push('=');
            }
        }
        output
    }

    fn kitty_file_transmit_apc(image_id: u32, medium: char, path: &Path) -> Vec<u8> {
        format!(
            "\x1b_Ga=t,t={medium},f=32,s=1,v=1,i={image_id};{}\x1b\\",
            base64(path.as_os_str().as_bytes())
        )
        .into_bytes()
    }

    fn kitty_shared_memory_transmit_apc(image_id: u32, shm: &KittySharedMemoryObject) -> Vec<u8> {
        format!(
            "\x1b_Ga=t,t=s,f=32,s=1,v=1,i={image_id};{}\x1b\\",
            base64(shm.name_bytes())
        )
        .into_bytes()
    }

    fn kitty_numbered_transmit_apc(image_number: u32) -> Vec<u8> {
        format!("\x1b_Ga=t,f=32,s=1,v=1,I={image_number};AQIDBA==\x1b\\").into_bytes()
    }

    fn kitty_implicit_transmit_display_apc(placement_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=T,f=32,s=1,v=1,p={placement_id},C=1;AQIDBA==\x1b\\").into_bytes()
    }

    fn kitty_query_apc(image_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=q,f=32,s=1,v=1,i={image_id};AQIDBA==\x1b\\").into_bytes()
    }

    fn kitty_display_apc(image_id: u32, placement_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=p,i={image_id},p={placement_id},C=1\x1b\\").into_bytes()
    }

    fn kitty_display_sized_apc(
        image_id: u32,
        placement_id: u32,
        columns: u32,
        rows: u32,
        z: i32,
    ) -> Vec<u8> {
        format!("\x1b_Ga=p,i={image_id},p={placement_id},c={columns},r={rows},z={z},C=1\x1b\\")
            .into_bytes()
    }

    fn kitty_display_number_apc(image_number: u32, placement_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=p,I={image_number},p={placement_id},C=1\x1b\\").into_bytes()
    }

    fn kitty_quiet_display_apc(image_id: u32, placement_id: u32, quiet: u32) -> Vec<u8> {
        format!("\x1b_Ga=p,i={image_id},p={placement_id},q={quiet},C=1\x1b\\").into_bytes()
    }

    fn kitty_virtual_display_apc(image_id: u32, placement_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=p,i={image_id},p={placement_id},U=1,C=1\x1b\\").into_bytes()
    }

    fn kitty_cursor_after_display_apc(image_id: u32, placement_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=p,i={image_id},p={placement_id},C=0\x1b\\").into_bytes()
    }

    fn kitty_cursor_after_display_sized_apc(
        image_id: u32,
        placement_id: u32,
        columns: u32,
        rows: u32,
    ) -> Vec<u8> {
        format!("\x1b_Ga=p,i={image_id},p={placement_id},c={columns},r={rows},C=0\x1b\\")
            .into_bytes()
    }

    fn kitty_transmit_display_apc(image_id: u32, placement_id: u32, cursor_after: bool) -> Vec<u8> {
        let cursor = if cursor_after { 0 } else { 1 };
        format!("\x1b_Ga=T,f=32,s=1,v=1,i={image_id},p={placement_id},C={cursor};AQIDBA==\x1b\\")
            .into_bytes()
    }

    fn kitty_transmit_display_sized_apc(
        image_id: u32,
        placement_id: u32,
        columns: u32,
        rows: u32,
        cursor_after: bool,
    ) -> Vec<u8> {
        let cursor = if cursor_after { 0 } else { 1 };
        format!("\x1b_Ga=T,f=32,s=1,v=1,i={image_id},p={placement_id},c={columns},r={rows},C={cursor};AQIDBA==\x1b\\")
            .into_bytes()
    }

    fn kitty_numbered_transmit_display_apc(image_number: u32, placement_id: u32) -> Vec<u8> {
        format!("\x1b_Ga=T,f=32,s=1,v=1,I={image_number},p={placement_id},C=1;AQIDBA==\x1b\\")
            .into_bytes()
    }

    fn kitty_transmit_display_chunk_apc(
        image_id: u32,
        placement_id: u32,
        more_chunks: bool,
        data: &str,
    ) -> Vec<u8> {
        let more = if more_chunks { 1 } else { 0 };
        format!("\x1b_Ga=T,f=32,s=1,v=1,i={image_id},p={placement_id},C=0,m={more};{data}\x1b\\")
            .into_bytes()
    }

    fn kitty_quiet_transmit_display_chunk_apc(
        image_id: u32,
        placement_id: u32,
        more_chunks: bool,
        quiet: u32,
        data: &str,
    ) -> Vec<u8> {
        let more = if more_chunks { 1 } else { 0 };
        format!(
            "\x1b_Ga=T,f=32,s=1,v=1,i={image_id},p={placement_id},C=1,q={quiet},m={more};{data}\x1b\\"
        )
        .into_bytes()
    }

    fn kitty_delete_apc(args: &str) -> Vec<u8> {
        format!("\x1b_Ga=d,{args}\x1b\\").into_bytes()
    }

    fn kitty_placement_key(image_id: u32, placement_id: u32) -> PlacementKey {
        PlacementKey {
            image_id,
            placement_id: PlacementId::External(placement_id),
        }
    }

    fn active_tracked_pin_count(terminal: &Terminal) -> usize {
        terminal.screens.active().count_tracked_pins_for_tests()
    }

    fn active_kitty_placement_pin(
        terminal: &Terminal,
        image_id: u32,
        placement_id: u32,
    ) -> std::ptr::NonNull<Pin> {
        let placement = terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(image_id, placement_id))
            .unwrap();
        match placement.location {
            PlacementLocation::Pin(pin) => pin,
            other => panic!("expected tracked pin placement, got {other:?}"),
        }
    }

    fn active_tracked_pin_value(terminal: &Terminal, pin: std::ptr::NonNull<Pin>) -> Option<Pin> {
        terminal.screens.active().tracked_pin_value(pin)
    }

    fn tracked_placement_at_cursor(terminal: &Terminal, image_id: u32, placement_id: u32) -> Pin {
        let pin = active_kitty_placement_pin(terminal, image_id, placement_id);
        active_tracked_pin_value(terminal, pin).expect("placement pin must still be tracked")
    }

    fn pins(terminal: &Terminal, points: &[(CellCountInt, u32)]) -> Vec<Pin> {
        points
            .iter()
            .map(|&(x, y)| active_pin(terminal, x, y))
            .collect()
    }

    const KITTY_FLAGS_3: KeyFlags = KeyFlags {
        disambiguate: true,
        report_events: true,
        ..KeyFlags::DISABLED
    };

    fn set_active_screen_extras(terminal: &mut Terminal) {
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_cursor_style_for_tests(style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            });
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("idé".to_string()),
                "https://e.test/é",
            );
        terminal
            .screens
            .active_mut()
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
    }

    const fn all_screen_extras() -> ScreenFormatterExtra {
        ScreenFormatterExtra::none()
            .style(true)
            .hyperlink(true)
            .protection(true)
            .kitty_keyboard(true)
            .charsets(true)
            .cursor(true)
    }

    const fn terminal_screen_extras() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().screen(all_screen_extras())
    }

    const fn terminal_palette_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().palette(true)
    }

    const fn terminal_modes_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().modes(true)
    }

    const fn terminal_palette_modes_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().palette(true).modes(true)
    }

    const fn terminal_scrolling_region_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().scrolling_region(true)
    }

    const fn terminal_tabstops_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().tabstops(true)
    }

    const fn terminal_keyboard_pwd_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().keyboard(true).pwd(true)
    }

    fn set_test_palette_entries(terminal: &mut Terminal) {
        terminal.set_palette_entry_for_tests(0, color::Rgb::new(0x12, 0x34, 0x56));
        terminal.set_palette_entry_for_tests(1, color::Rgb::new(0xab, 0xcd, 0xef));
        terminal.set_palette_entry_for_tests(255, color::Rgb::new(0xff, 0x00, 0xff));
    }

    fn palette_vt_prefix_len(terminal: &Terminal) -> usize {
        palette_vt_string(terminal.colors.palette.current()).len()
    }

    fn palette_html_prefix_len(terminal: &Terminal) -> usize {
        palette_html_string(terminal.colors.palette.current()).len()
    }

    fn modes_prefix_len(terminal: &Terminal) -> usize {
        modes_vt_string(&terminal.modes).len()
    }

    fn scrolling_region_suffix_len(terminal: &Terminal) -> usize {
        scrolling_region_vt_string(terminal.size, terminal.scrolling_region).len()
    }

    fn tabstops_suffix_len(terminal: &Terminal) -> usize {
        tabstops_vt_string(&terminal.tabstops).len()
    }

    fn keyboard_pwd_suffix_len(terminal: &Terminal) -> usize {
        keyboard_vt_string(terminal.flags).len() + pwd_vt_string(&terminal.pwd).len()
    }

    #[test]
    fn terminal_stream_ascii_prints_to_active_screen_and_advances_cursor() {
        let mut terminal = Terminal::init(40, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "hello"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_print_marks_written_row_dirty() {
        let mut terminal = Terminal::init(40, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(39, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn shape_run_options_threads_screen_state() {
        let mut terminal = Terminal::init(4, 2, None).unwrap();
        terminal.next_slice(b"AB").unwrap();
        // After printing "AB" the cursor sits at column 2 of row 0.
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));

        let opts = terminal.shape_run_options();

        // One RunOptions per active row.
        assert_eq!(opts.len(), 2);

        // Row 0 decodes the printed cells; the rest are empty.
        let row0 = &opts[0];
        assert_eq!(row0.cells.len(), 4);
        assert_eq!(row0.cells[0].codepoint, u32::from('A'));
        assert_eq!(row0.cells[1].codepoint, u32::from('B'));
        assert!(row0.cells[2].is_empty);
        assert!(row0.cells[3].is_empty);

        // Cursor threaded from the active screen: only on the cursor's row.
        assert_eq!(row0.cursor_x, Some(2));
        assert_eq!(opts[1].cursor_x, None);

        // No selection installed yet.
        assert_eq!(row0.selection, None);
        assert_eq!(opts[1].selection, None);

        // Selection threading: install a whole-screen selection and confirm the
        // wrapper passes `self.selection` (not `None`). `select_all` clamps the
        // end to the last written column (`B` at column 1), so the first row's
        // range is `[0, 1]`. The key fact is that it is `Some`, proving the
        // wrapper threads `self.selection`.
        let sel = terminal.select_all().unwrap();
        terminal.set_selection(Some(sel)).unwrap();
        let opts = terminal.shape_run_options();
        assert_eq!(opts[0].selection, Some([0, 1]));
    }

    /// Issue 802 / Exp 22: after the macOS `clear` sequence (`\033[3J\033[H\033[2J`), text
    /// printed afterward must reach the **render read-path** (`shape_run_options` — the accessor
    /// `present_live` feeds the renderer). The live app showed only a home cursor after `clear`;
    /// this reproduces that headlessly through the same accessor.
    /// Issue 802 / Exp 31: the cursor run-shaping hint (`RunOptions.cursor_x`) must sit on the
    /// cursor's VIEWPORT row, and vanish when the cursor is scrolled off-viewport (not a stray hint
    /// on a history row). Same gating as the Exp-24 cursor draw.
    #[test]
    fn cursor_shaping_hint_gated_by_viewport() {
        let mut term = Terminal::init(20, 6, None).unwrap();
        let mut content = String::new();
        for i in 0..40 {
            content.push_str(&format!("line{i}\r\n"));
        }
        term.next_slice(content.as_bytes()).unwrap();

        // Unscrolled: exactly one row carries the hint, at the cursor's row.
        let (cx, cy) = term.cursor_position();
        let rows = term.shape_run_options();
        let with_hint: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.cursor_x.is_some())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            with_hint.len(),
            1,
            "unscrolled: exactly one row has the cursor hint"
        );
        assert_eq!(with_hint[0] as CellCountInt, cy, "hint at the cursor row");
        assert_eq!(rows[with_hint[0]].cursor_x, Some(cx));

        // Scrolled into history: the cursor is off-viewport → no row carries the hint.
        term.scroll_viewport_delta_row(-100);
        let rows = term.shape_run_options();
        assert!(
            rows.iter().all(|r| r.cursor_x.is_none()),
            "scrolled into history: no stray cursor hint"
        );
    }

    /// Issue 802 / Exp 24: the cursor block must not render in scrollback. Unscrolled, the cursor
    /// maps to its active row; scrolled into history, `cursor_viewport_position` returns `None`
    /// (the value feeding the renderer's cursor overlay).
    #[test]
    fn cursor_viewport_position_hides_when_scrolled_into_history() {
        let mut term = Terminal::init(20, 6, None).unwrap();
        let mut content = String::new();
        for i in 0..40 {
            content.push_str(&format!("line{i}\r\n"));
        }
        term.next_slice(content.as_bytes()).unwrap();

        // Unscrolled (viewport == active): the cursor is visible at its active row.
        let active = term.cursor_position();
        assert_eq!(
            term.cursor_viewport_position(),
            Some(active),
            "unscrolled cursor should map to its active row"
        );

        // Scrolled into history (clamps to top): the cursor (active bottom) is off-viewport.
        term.scroll_viewport_delta_row(-100);
        assert_eq!(
            term.cursor_viewport_position(),
            None,
            "cursor must be hidden when scrolled into scrollback (Exp 24)"
        );

        // Back at the bottom: visible again at the same active position.
        term.scroll_viewport_to_bottom();
        assert_eq!(
            term.cursor_viewport_position(),
            Some(active),
            "cursor visible again when scrolled back to the active viewport"
        );
    }

    #[test]
    fn render_read_path_keeps_post_clear_text() {
        fn rendered_text(term: &Terminal) -> String {
            term.shape_run_options()
                .iter()
                .flat_map(|row| row.cells.iter())
                .filter_map(|cell| char::from_u32(cell.codepoint))
                .filter(|&ch| ch != '\0')
                .collect()
        }
        // Control: plain text reaches the render read-path.
        let mut control = Terminal::init(40, 10, None).unwrap();
        control.next_slice(b"HELLO_CONTROL").unwrap();
        assert!(rendered_text(&control).contains("HELLO_CONTROL"));

        // The exact macOS `clear` (3J, home, 2J) then post-clear text. Pre-fix, the `3J`
        // (erase-scrollback with no history) returned an error that aborted the whole slice,
        // dropping the post-clear text.
        let mut term = Terminal::init(40, 10, None).unwrap();
        term.next_slice(b"\x1b[3J\x1b[H\x1b[2JAFTER_CLEAR").unwrap();
        let after = rendered_text(&term);
        assert!(
            after.contains("AFTER_CLEAR"),
            "render read-path lost post-clear text (Exp 22 repro); got {after:?}"
        );
    }

    #[test]
    fn terminal_stream_invalid_utf8_writes_replacement_character() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(&[0xff]).unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            char::REPLACEMENT_CHARACTER.to_string()
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_controls_and_unsupported_escapes_do_not_write_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x0eB\x1bxC\x1b[?ZD").unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "ABCD"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_c1_control_is_rejected_before_width_fast_path() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        assert_eq!(
            terminal.next_slice("\u{85}".as_bytes()),
            Err(TerminalStreamError::UnsupportedCodepoint('\u{85}'))
        );
        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_crlf_formats_basic_lines() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"hello\r\nworld").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
    }

    #[test]
    fn terminal_stream_lf_preserves_column() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\nB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_vt_preserves_column_like_lf() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x0bB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_ff_preserves_column_like_lf() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x0cB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_escape_d_moves_down_and_preserves_column() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x1bDB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_escape_e_moves_down_and_carriage_returns() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x1bEB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_vt_honors_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x0bB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_ff_honors_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x0cB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_escape_d_bypasses_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x1bDB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_escape_e_bypasses_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x1bEB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_round_trips_position_and_text_state() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\x1b[1;31m").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal.next_slice(b"\x1b7").unwrap();
        terminal.next_slice(b"\x1b[3;32m").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(false);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 2);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
        assert!(terminal.cursor_protected_for_tests());
        assert_eq!(
            terminal.cursor_style_for_tests(),
            style::Style {
                fg_color: style::Color::Palette(1),
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            }
        );
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_round_trips_pending_wrap_origin_and_charset() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal.next_slice(b"\x1b7").unwrap();

        terminal.set_mode_for_tests(Mode::Origin, false);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);
        terminal.next_slice(b"\x1b[2 q").unwrap();
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::Utf8);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G0);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "hello\x1b(0\x0e"
        );
    }

    #[test]
    fn terminal_stream_restore_cursor_without_save_uses_ghostty_defaults() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal
            .next_slice(b"hello\x1b[1;31m\x1b[?6h\x1b[5 q")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert_eq!(terminal.cursor_style_for_tests(), style::Style::default());
        assert!(!terminal.cursor_protected_for_tests());
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Bar
        );
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_does_not_restore_excluded_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5 q").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("saved".to_string()),
                "https://saved.example",
            );
        terminal
            .next_slice(b"\x1b]133;A\x07\x1b7\x1b[4 q\x1b]133;B\x07")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("current".to_string()),
                "https://current.example",
            );
        terminal.next_slice(b"\x1b8X").unwrap();

        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Underline
        );
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("current".to_string()),
                "https://current.example"
            ))
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 0),
            SemanticContent::Input
        );
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_does_not_mutate_cells_dirty_rows_or_responses() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);
        terminal.next_slice(b"\x1b7").unwrap();
        terminal.clear_dirty_for_tests();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 1);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_tmux_dcs_startup_writes_first_command_response() {
        let mut terminal = Terminal::init(80, 24, None).unwrap();

        enter_tmux_dcs(&mut terminal);
        terminal
            .next_slice(b"%begin 1 1 1\n%end 1 1 1\n%session-changed $42 main\n")
            .unwrap();

        assert!(terminal.tmux_viewer.is_some());
        assert!(terminal.tmux_windows.is_empty());
        assert_eq!(
            terminal.pty_response_for_tests(),
            b"display-message -p '#{version}'\n"
        );
    }

    #[test]
    fn terminal_tmux_dcs_command_flow_writes_follow_up_commands() {
        let mut terminal = Terminal::init(80, 24, None).unwrap();

        start_tmux_command_queue(&mut terminal);
        terminal
            .next_slice(b"%begin 2 1 1\n3.5a\n%end 2 1 1\n")
            .unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"list-windows -F '#{session_id} #{window_id} #{window_width} #{window_height} #{window_layout}'\n"
        );

        terminal
            .next_slice(
                format!("%begin 3 1 1\n$42 @2 80 24 {TMUX_SIMPLE_LAYOUT}\n%end 3 1 1\n").as_bytes(),
            )
            .unwrap();

        assert_eq!(terminal.tmux_windows.len(), 1);
        assert_eq!(
            terminal.pty_response_for_tests(),
            b"capture-pane -p -e -q -S - -E -1 -t %42\n"
        );
    }

    #[test]
    fn terminal_tmux_dcs_exit_clears_viewer_and_requires_new_enter() {
        let mut terminal = Terminal::init(80, 24, None).unwrap();
        cache_tmux_window_with_pending_captures(&mut terminal);

        terminal.next_slice(b"\x1b\\").unwrap();
        assert!(terminal.tmux_viewer.is_none());
        assert!(terminal.tmux_windows.is_empty());

        terminal.clear_pty_response();
        terminal.next_slice(b"%sessions-changed\n").unwrap();
        assert!(terminal.tmux_viewer.is_none());
        assert!(terminal.pty_response_for_tests().is_empty());

        enter_tmux_dcs(&mut terminal);
        assert!(terminal.tmux_viewer.is_some());
    }

    #[test]
    fn terminal_tmux_viewer_exit_clears_cached_state() {
        let mut terminal = Terminal::init(80, 24, None).unwrap();

        reach_tmux_list_windows_command(&mut terminal);
        terminal
            .next_slice(b"%begin 3 1 1\nnot a window row\n%end 3 1 1\n")
            .unwrap();

        assert!(terminal.tmux_viewer.is_none());
        assert!(terminal.tmux_windows.is_empty());
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_tmux_windows_cache_update_does_not_write_pty_without_new_panes() {
        let mut terminal = Terminal::init(80, 24, None).unwrap();
        cache_tmux_window_with_pending_captures(&mut terminal);

        terminal
            .next_slice(
                format!("%layout-change @2 {TMUX_SIMPLE_LAYOUT} {TMUX_SIMPLE_LAYOUT} *-\n")
                    .as_bytes(),
            )
            .unwrap();

        assert_eq!(terminal.tmux_windows.len(), 1);
        assert_eq!(terminal.tmux_windows[0].id, 2);
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_tmux_reset_paths_clear_viewer_and_windows() {
        for ris in [false, true] {
            let mut terminal = Terminal::init(80, 24, None).unwrap();
            terminal.tmux_viewer = Some(tmux::TmuxViewer::new());
            terminal.tmux_windows = vec![tmux::TmuxWindow {
                id: 2,
                width: 80,
                height: 24,
                layout: tmux::Layout::parse("80x24,0,0,42").unwrap(),
            }];

            if ris {
                terminal.next_slice(b"\x1bc").unwrap();
            } else {
                terminal.reset();
            }

            assert!(terminal.tmux_viewer.is_none());
            assert!(terminal.tmux_windows.is_empty());
        }
    }

    #[test]
    fn terminal_stream_ris_full_reset_clears_screen_and_cursor_state() {
        let mut terminal = Terminal::init(10, 3, Some(10)).unwrap();

        terminal.next_slice(b"one\ntwo\nthree\nfour").unwrap();
        assert!(terminal.scrollback_rows_for_tests() > 0);
        terminal.next_slice(b"\x1b[1;31m\x1b[5 q").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("link".to_string()),
                "https://example.test",
            );
        terminal
            .next_slice(b"\x1b]133;A\x07prompt\x1b]133;B\x07input")
            .unwrap();
        terminal.next_slice(b"\x1b7").unwrap();

        terminal.next_slice(b"\x1bc").unwrap();
        terminal.next_slice(b"\x1b8X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "X");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert_eq!(terminal.cursor_style_for_tests(), style::Style::default());
        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Block
        );
        assert!(!terminal.cursor_protected_for_tests());
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 0),
            SemanticContent::Output
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::None
        );
    }

    #[test]
    fn terminal_stream_ris_full_reset_clears_terminal_global_state() {
        let mut terminal = Terminal::init(20, 4, None).unwrap();

        terminal.next_slice(b"\x1b]0;title\x07").unwrap();
        terminal.set_pwd_for_tests("file://host/home");
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.save_mode_for_tests(Mode::Insert);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal
            .next_slice(b"\x1b[?1003h\x1b[?1016h\x1b[>1s")
            .unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        terminal.set_scrolling_region_for_tests(1, 2, 3, 10);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1bc").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(!terminal.restore_mode_for_tests(Mode::Insert));
        assert!(!terminal.modify_other_keys_2_for_tests());
        assert_eq!(
            terminal.mouse_event_for_tests(),
            mouse::MouseEventMode::None
        );
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
        assert_eq!(terminal.mouse_shift_capture_for_tests(), None);
        assert!(!terminal.get_tabstop_for_tests(1));
        assert!(terminal.get_tabstop_for_tests(8));
        assert!(terminal.get_tabstop_for_tests(16));
        assert_eq!(terminal.title_for_tests(), "");
        assert_eq!(terminal.pwd_for_tests(), None);
        assert_eq!(
            terminal.scrolling_region_for_tests(),
            ScrollingRegion::full(terminal.size)
        );
        assert!(terminal.pty_response_for_tests().is_empty());
        for row in 0..4 {
            assert!(terminal.is_dirty_for_tests(0, row));
            assert!(terminal.is_dirty_for_tests(19, row));
        }
    }

    #[test]
    fn terminal_stream_ris_full_reset_returns_to_primary_and_drops_alternate() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"pri\x1b[?47halt\x1bc").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert!(!terminal.alternate_initialized_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "");

        terminal.next_slice(b"\x1b[?47h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(!plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_charset_designation_maps_printed_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice("é\x1b(0`\x1b(A#\x1b(Bé".as_bytes())
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "é◆£é");
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_default_and_ascii_charset_preserve_non_ascii() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("😀\x1b(B😀".as_bytes()).unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "😀😀");
    }

    #[test]
    fn terminal_stream_mapped_charset_replaces_non_u8_codepoint_with_space() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("\x1b(0😀".as_bytes()).unwrap();

        assert_eq!(terminal.full_screen_plain_for_tests(false), " ");
    }

    #[test]
    fn terminal_stream_invoke_charset_switches_gl_slots() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b)0`\x0e``\x0f`").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`◆◆`");
    }

    #[test]
    fn terminal_stream_charset_single_shift_affects_one_character() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b*0`\x1bN``").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`◆`");
    }

    #[test]
    fn terminal_stream_charset_gr_invocation_round_trips_without_affecting_print() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b|`").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "`\x1b|"
        );
    }

    #[test]
    fn terminal_stream_charset_save_restore_preserves_designations_and_shifts() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"a\x1b*0\x1bN\x1b7b\x1b8``").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "a◆`");
    }

    #[test]
    fn terminal_stream_charset_save_restore_preserves_gl_and_gr_invocation() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b)0\x0e\x1b|\x1b7\x0f\x1b~\x1b8`")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "◆");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "◆\x1b)0\x0e\x1b|"
        );
    }

    #[test]
    fn terminal_stream_charset_controls_do_not_dirty_rows_or_write_responses() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b(0\x1b)0\x0e\x1b~").unwrap();

        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal.next_slice(b"`").unwrap();
        assert!(terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_print_repeat_without_previous_char_is_noop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 1));
    }

    #[test]
    fn terminal_stream_print_repeat_uses_default_and_explicit_counts() {
        for (input, expected) in [
            (b"A\x1b[b".as_slice(), "AA"),
            (b"A\x1b[0b".as_slice(), "AA"),
            (b"A\x1b[3b".as_slice(), "AAAA"),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), expected);
        }
    }

    #[test]
    fn terminal_stream_print_cjk_uses_wide_cell_and_spacer_tail() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("\u{65E5}A".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 0), 0x65E5);
        assert_eq!(terminal.active_cell_wide_for_tests(0, 0), Wide::Wide);
        assert_eq!(terminal.active_cell_wide_for_tests(1, 0), Wide::SpacerTail);
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(2, 0),
            u32::from(b'A')
        );
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_print_emoji_uses_wide_cell_and_spacer_tail() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("\u{1F642}A".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 0), 0x1F642);
        assert_eq!(terminal.active_cell_wide_for_tests(0, 0), Wide::Wide);
        assert_eq!(terminal.active_cell_wide_for_tests(1, 0), Wide::SpacerTail);
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(2, 0),
            u32::from(b'A')
        );
    }

    #[test]
    fn terminal_stream_print_wide_wraps_from_right_edge() {
        let mut terminal = Terminal::init(4, 2, None).unwrap();

        terminal.next_slice("ABC\u{65E5}".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(3, 0), 0);
        assert_eq!(terminal.active_cell_wide_for_tests(3, 0), Wide::SpacerHead);
        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 1), 0x65E5);
        assert_eq!(terminal.active_cell_wide_for_tests(0, 1), Wide::Wide);
        assert_eq!(terminal.active_cell_wide_for_tests(1, 1), Wide::SpacerTail);
    }

    #[test]
    fn terminal_stream_print_wide_right_edge_without_wraparound_is_ignored() {
        let mut terminal = Terminal::init(4, 2, None).unwrap();

        terminal
            .next_slice("\x1b[?7lABC\u{65E5}".as_bytes())
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_print_insert_mode_uses_wide_width() {
        let mut terminal = Terminal::init(8, 1, None).unwrap();

        terminal.next_slice(b"ABCD\x1b[1G\x1b[4h").unwrap();
        terminal.next_slice("\u{65E5}".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 0), 0x65E5);
        assert_eq!(terminal.active_cell_wide_for_tests(1, 0), Wide::SpacerTail);
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(2, 0),
            u32::from(b'A')
        );
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(5, 0),
            u32::from(b'D')
        );
    }

    #[test]
    fn terminal_stream_print_mode_2027_combining_mark_attaches() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal.next_slice("x\u{0301}Y".as_bytes()).unwrap();

        assert_eq!(
            terminal.active_cell_codepoint_for_tests(0, 0),
            u32::from(b'x')
        );
        assert_eq!(
            terminal.active_cell_graphemes_for_tests(0, 0),
            Some(vec![0x0301])
        );
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(1, 0),
            u32::from(b'Y')
        );
    }

    #[test]
    fn terminal_stream_print_mode_2027_vs16_widens_valid_base() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal.next_slice("\u{2764}\u{FE0F}A".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 0), 0x2764);
        assert_eq!(terminal.active_cell_wide_for_tests(0, 0), Wide::Wide);
        assert_eq!(terminal.active_cell_wide_for_tests(1, 0), Wide::SpacerTail);
        assert_eq!(
            terminal.active_cell_graphemes_for_tests(0, 0),
            Some(vec![0xFE0F])
        );
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(2, 0),
            u32::from(b'A')
        );
    }

    #[test]
    fn terminal_stream_print_mode_2027_replays_emoji_zwj_graphemes() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal
            .next_slice("\u{1F3F4}\u{200D}\u{2620}\u{FE0F}A".as_bytes())
            .unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 0), 0x1F3F4);
        assert_eq!(
            terminal.active_cell_graphemes_for_tests(0, 0),
            Some(vec![0x200D, 0x2620, 0xFE0F])
        );
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(2, 0),
            u32::from(b'A')
        );
    }

    #[test]
    fn terminal_stream_print_mode_2027_vs15_narrows_valid_base() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal.next_slice("\u{2614}\u{FE0E}A".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(0, 0), 0x2614);
        assert_eq!(terminal.active_cell_wide_for_tests(0, 0), Wide::Narrow);
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(1, 0),
            u32::from(b'A')
        );
    }

    #[test]
    fn terminal_stream_print_wide_wraps_from_horizontal_margin_edge() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 2, 2, 5);
        terminal.set_cursor_position_for_tests(5, 0);

        terminal.next_slice("\u{65E5}".as_bytes()).unwrap();

        assert_eq!(terminal.active_cell_codepoint_for_tests(5, 0), 0);
        assert_eq!(terminal.active_cell_wide_for_tests(5, 0), Wide::Narrow);
        assert_eq!(terminal.active_cell_codepoint_for_tests(2, 1), 0x65E5);
        assert_eq!(terminal.active_cell_wide_for_tests(2, 1), Wide::Wide);
        assert_eq!(terminal.active_cell_wide_for_tests(3, 1), Wide::SpacerTail);
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
    }

    #[test]
    fn terminal_stream_print_invalid_variation_selector_is_ignored() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal.next_slice("x\u{FE0F}Y".as_bytes()).unwrap();

        assert_eq!(
            terminal.active_cell_codepoint_for_tests(0, 0),
            u32::from(b'x')
        );
        assert_eq!(terminal.active_cell_graphemes_for_tests(0, 0), None);
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(1, 0),
            u32::from(b'Y')
        );
    }

    #[test]
    fn terminal_stream_print_disabled_wraparound_grapheme_attaches_to_edge_cell() {
        let mut terminal = Terminal::init(4, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal
            .next_slice("ABCD\x1b[?7l\u{0301}".as_bytes())
            .unwrap();

        assert_eq!(
            terminal.active_cell_codepoint_for_tests(3, 0),
            u32::from(b'D')
        );
        assert_eq!(terminal.active_cell_graphemes_for_tests(2, 0), None);
        assert_eq!(
            terminal.active_cell_graphemes_for_tests(3, 0),
            Some(vec![0x0301])
        );
    }

    #[test]
    fn terminal_stream_print_repeat_ignores_zero_width_previous_char() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.modes.set(modes::Mode::GraphemeCluster, true);

        terminal.next_slice("x\u{0301}\x1b[b".as_bytes()).unwrap();

        assert_eq!(
            terminal.active_cell_codepoint_for_tests(0, 0),
            u32::from(b'x')
        );
        assert_eq!(
            terminal.active_cell_graphemes_for_tests(0, 0),
            Some(vec![0x0301])
        );
        assert_eq!(
            terminal.active_cell_codepoint_for_tests(1, 0),
            u32::from(b'x')
        );
    }

    #[test]
    fn terminal_stream_print_repeat_uses_normal_wrap_path() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"    A\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "    A\nA");
    }

    #[test]
    fn terminal_stream_print_repeat_respects_disabled_wraparound() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?7l    A\x1b[3b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "    A");
    }

    #[test]
    fn terminal_stream_print_repeat_uses_current_style_and_hyperlink() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"A\x1b[1m\x1b]8;;https://rep\x1b\\\x1b[b")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AA");
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style::default()
        );
        assert!(terminal.active_cell_style_for_tests(1, 0).flags.bold);
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(1, 0)
                .unwrap()
                .uri,
            b"https://rep"
        );
    }

    #[test]
    fn terminal_stream_print_repeat_maps_unmapped_char_through_current_charset() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b(0`\x1b(B\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "◆`");
    }

    #[test]
    fn terminal_stream_print_repeat_consumes_pending_single_shift_once() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b*0`\x1bN\x1b[b\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`◆`");
    }

    #[test]
    fn terminal_stream_repeat_previous_save_restore_cursor_does_not_restore_previous_char() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x1b7B\x1b8\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
    }

    #[test]
    fn terminal_stream_repeat_previous_ris_resets_previous_char() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x1bc\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_ris_resets_charset_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b(0\x1b*0\x1bN\x1b|\x1bc`")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "`"
        );
    }

    #[test]
    fn terminal_stream_csi_mode_set_and_reset_toggle_basic_mode_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[4h\x1b[20h\x1b[?7l").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert!(terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));

        terminal.next_slice(b"\x1b[4l\x1b[20l\x1b[?7h").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(!terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
    }

    #[test]
    fn terminal_stream_csi_mode_set_updates_formatter_modes_extra() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?2004h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::BracketedPaste));
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(terminal_modes_extra())
                .format(),
            "\x1b[?2004h"
        );

        terminal.next_slice(b"\x1b[?2004l").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::BracketedPaste));
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(terminal_modes_extra())
                .format(),
            ""
        );
    }

    #[test]
    fn terminal_stream_csi_mode_multi_params_skip_unknown_and_toggle_known_modes() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[9999;4;20h\x1b[?9999;7l\x1b[9998;4l")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));
    }

    #[test]
    fn terminal_stream_mouse_event_modes_update_runtime_cache() {
        let cases = [
            (
                b"\x1b[?9h".as_slice(),
                b"\x1b[?9l".as_slice(),
                mouse::MouseEventMode::X10,
            ),
            (
                b"\x1b[?1000h".as_slice(),
                b"\x1b[?1000l".as_slice(),
                mouse::MouseEventMode::Normal,
            ),
            (
                b"\x1b[?1002h".as_slice(),
                b"\x1b[?1002l".as_slice(),
                mouse::MouseEventMode::Button,
            ),
            (
                b"\x1b[?1003h".as_slice(),
                b"\x1b[?1003l".as_slice(),
                mouse::MouseEventMode::Any,
            ),
        ];

        for (set, reset, expected) in cases {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            assert_eq!(
                terminal.mouse_event_for_tests(),
                mouse::MouseEventMode::None
            );

            terminal.next_slice(set).unwrap();
            assert_eq!(terminal.mouse_event_for_tests(), expected);

            terminal.next_slice(reset).unwrap();
            assert_eq!(
                terminal.mouse_event_for_tests(),
                mouse::MouseEventMode::None
            );
        }
    }

    #[test]
    fn terminal_stream_mouse_event_runtime_cache_uses_last_command() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?1000h\x1b[?1003h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert!(terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(terminal.mouse_event_for_tests(), mouse::MouseEventMode::Any);

        terminal.next_slice(b"\x1b[?1003l").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert!(!terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(
            terminal.mouse_event_for_tests(),
            mouse::MouseEventMode::None
        );
    }

    #[test]
    fn terminal_stream_mouse_format_modes_update_runtime_cache() {
        let cases = [
            (
                b"\x1b[?1005h".as_slice(),
                b"\x1b[?1005l".as_slice(),
                mouse::MouseFormat::Utf8,
            ),
            (
                b"\x1b[?1006h".as_slice(),
                b"\x1b[?1006l".as_slice(),
                mouse::MouseFormat::Sgr,
            ),
            (
                b"\x1b[?1015h".as_slice(),
                b"\x1b[?1015l".as_slice(),
                mouse::MouseFormat::Urxvt,
            ),
            (
                b"\x1b[?1016h".as_slice(),
                b"\x1b[?1016l".as_slice(),
                mouse::MouseFormat::SgrPixels,
            ),
        ];

        for (set, reset, expected) in cases {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);

            terminal.next_slice(set).unwrap();
            assert_eq!(terminal.mouse_format_for_tests(), expected);

            terminal.next_slice(reset).unwrap();
            assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
        }
    }

    #[test]
    fn terminal_stream_mouse_format_runtime_cache_uses_last_command() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?1006h\x1b[?1016h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgrPixels));
        assert_eq!(
            terminal.mouse_format_for_tests(),
            mouse::MouseFormat::SgrPixels
        );

        terminal.next_slice(b"\x1b[?1016l").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert!(!terminal.get_mode_for_tests(Mode::MouseFormatSgrPixels));
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
    }

    #[test]
    fn terminal_stream_mouse_runtime_cache_follows_mode_save_restore() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?1003h\x1b[?1003s\x1b[?1003l\x1b[?1003r")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(terminal.mouse_event_for_tests(), mouse::MouseEventMode::Any);

        terminal
            .next_slice(b"\x1b[?1003l\x1b[?1003s\x1b[?1003h\x1b[?1003r")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(
            terminal.mouse_event_for_tests(),
            mouse::MouseEventMode::None
        );

        terminal
            .next_slice(b"\x1b[?1006h\x1b[?1006s\x1b[?1006l\x1b[?1006r")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::Sgr);

        terminal
            .next_slice(b"\x1b[?1006l\x1b[?1006s\x1b[?1006h\x1b[?1006r")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
    }

    #[test]
    fn report_color_scheme_change_gated_on_mode_2031() {
        let mut term = Terminal::init(80, 24, None).unwrap();
        // Mode 2031 off by default → a change reports nothing.
        term.report_color_scheme_change(1);
        assert!(
            term.pty_response().is_empty(),
            "no report when mode 2031 is disabled"
        );

        // Enable mode 2031, then a dark change → `997;1n`, a light change → `997;2n`.
        term.next_slice(b"\x1b[?2031h").unwrap();
        term.clear_pty_response();
        term.report_color_scheme_change(1); // dark
        assert_eq!(term.pty_response(), b"\x1b[?997;1n");
        term.clear_pty_response();
        term.report_color_scheme_change(0); // light
        assert_eq!(term.pty_response(), b"\x1b[?997;2n");

        // An out-of-range scheme is a graceful no-op.
        term.clear_pty_response();
        term.report_color_scheme_change(99);
        assert!(term.pty_response().is_empty(), "unknown scheme is a no-op");
    }

    #[test]
    fn terminal_stream_mouse_shift_capture_updates_runtime_flag() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        assert_eq!(terminal.mouse_shift_capture_for_tests(), None);

        terminal.next_slice(b"\x1b[>1s").unwrap();
        assert_eq!(terminal.mouse_shift_capture_for_tests(), Some(true));

        terminal.next_slice(b"\x1b[>s").unwrap();
        assert_eq!(terminal.mouse_shift_capture_for_tests(), Some(false));

        terminal.next_slice(b"\x1b[>1s\x1b[>2s").unwrap();
        assert_eq!(terminal.mouse_shift_capture_for_tests(), Some(true));
    }

    #[test]
    fn terminal_stream_csi_origin_mode_moves_to_origin_home_and_clears_pending_wrap() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();

        terminal.next_slice(b"0123456789").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?6h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?6l").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_left_right_margin_reset_clears_horizontal_margins() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_mode_for_tests(Mode::EnableLeftAndRightMargin, true);
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?69l").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert!(!terminal.get_mode_for_tests(Mode::EnableLeftAndRightMargin));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 0);
        assert_eq!(region.right, 9);
    }

    #[test]
    fn terminal_stream_alt_screen_legacy_switches_without_clearing() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert!(!terminal.alternate_initialized_for_tests());
        terminal.next_slice(b"pri\x1b[?47halt\x1b[?47l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert!(terminal.alternate_initialized_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "pri");
        assert_eq!(terminal.cursor_position_for_tests(), (6, 0));

        terminal.next_slice(b"\x1b[?47h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(plain_with_unwrap(&terminal, false).contains("alt"));

        terminal.next_slice(b"\x1b[?47h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_alt_screen_has_no_scrollback_and_formatter_reads_active_screen() {
        let mut terminal = Terminal::init(10, 2, Some(10)).unwrap();

        terminal.next_slice(b"pri\nmore\nnext").unwrap();
        assert!(terminal.scrollback_rows_for_tests() > 0);
        terminal.next_slice(b"\x1b[?47h").unwrap();
        terminal.next_slice(b"\x1b[Ha\nb\nc\nd").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert!(plain_with_unwrap(&terminal, false).contains('d'));
        assert!(!plain_with_unwrap(&terminal, false).contains("pri"));
    }

    #[test]
    fn terminal_stream_alt_screen_1047_clears_alternate_on_leave() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"pri").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal.next_slice(b"\x1b[?1047h").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        terminal.next_slice(b"alt").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(7, 2);
        terminal.next_slice(b"\x1b[?1047l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(plain_with_unwrap(&terminal, false), "pri");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 2));

        terminal.next_slice(b"\x1b[?1047h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(!plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_alt_screen_1049_saves_cursor_and_clears_on_entry() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"pri").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.next_slice(b"\x1b[?1049halt").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?1049l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(plain_with_unwrap(&terminal, false), "pri");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));

        terminal.next_slice(b"\x1b[?1049h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(!plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_alt_screen_1049_reset_does_not_corrupt_primary_save() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal.next_slice(b"\x1b[?1049h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?1049halt\x1b[?1049l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
    }

    #[test]
    fn terminal_stream_alt_screen_1048_saves_active_screen_without_switching() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);
        terminal.next_slice(b"\x1b[?1048h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[?1048l").unwrap();
        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));

        terminal.next_slice(b"\x1b[?47h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 1);
        terminal.next_slice(b"\x1b[?1048h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[?1048l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
    }

    #[test]
    fn terminal_stream_alt_screen_switch_clears_hyperlink_and_carries_charset() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("primary".to_string()),
                "https://primary.example",
            );
        terminal.next_slice(b"\x1b(0\x1b[?47h`").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "◆");
        terminal.next_slice(b"\x1b[?47l").unwrap();
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
    }

    #[test]
    fn terminal_stream_kitty_keyboard_query_reports_active_flags() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u");

        terminal.next_slice(b"\x1b[>3u\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?3u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_push_pop_and_oversized_pop_follow_stack() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[>1u\x1b[>2u\x1b[?u\x1b[<u\x1b[?u")
            .unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?2u\x1b[?1u");

        terminal.next_slice(b"\x1b[>4u\x1b[<100u\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_multiple_pop_restores_earlier_value() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[>1u\x1b[>2u\x1b[>4u\x1b[<2u\x1b[?u")
            .unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?1u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_set_or_and_not_mutate_current_flags() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=1u\x1b[?u\x1b[=2;2u\x1b[?u\x1b[=1;3u\x1b[?u")
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b[?1u\x1b[?3u\x1b[?2u"
        );
    }

    #[test]
    fn terminal_stream_kitty_keyboard_primary_and_alternate_states_are_independent() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"\x1b[=1u\x1b[?47h\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u");

        terminal.next_slice(b"\x1b[=3u\x1b[?47l\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?1u");

        terminal.next_slice(b"\x1b[?47h\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?3u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_ris_clears_primary_and_future_alternate_state() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=1u\x1b[?47h\x1b[=3u\x1bc")
            .unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        terminal.next_slice(b"\x1b[?u\x1b[?47h\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u\x1b[?0u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_invalid_forms_do_not_mutate_or_respond() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=3u\x1b[>32u\x1b[=1;4u\x1b[=1:1u\x1b[?u")
            .unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?3u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_lenient_parameter_forms_match_upstream() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=1u\x1b[?123u\x1b[>3;4u\x1b[?u\x1b[<2;3u\x1b[?u\x1b[=3;2;1u\x1b[?u")
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b[?1u\x1b[?0u\x1b[?1u\x1b[?3u"
        );
    }

    #[test]
    fn terminal_stream_csi_u_still_restores_cursor() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);
        terminal.next_slice(b"\x1b7").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[u").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_alt_screen_does_not_fake_unrelated_deferred_modes() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[?3h\x1b[?1000h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Column132));
        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert_eq!(terminal.size.cols, 10);
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));

        terminal.next_slice(b"\x1b[?3l\x1b[?1000l").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Column132));
        assert!(!terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.size.cols, 10);
    }

    #[test]
    fn terminal_stream_unsupported_csi_mode_forms_do_not_mutate_mode_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[>4h\x1b[4:20h\x1b[?6:7h")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(!terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
    }

    #[test]
    fn terminal_stream_csi_mode_commands_do_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[4h\x1b[20h\x1b[?7l").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_reenables_wraparound_behavior() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?7s\x1b[?7l\x1b[?7rabcX")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc\nX");
        assert_eq!(plain_with_unwrap(&terminal, true), "abcX");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_redisables_wraparound_behavior() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?7l\x1b[?7s\x1b[?7h\x1b[?7rabcX")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));
        assert_eq!(plain_with_unwrap(&terminal, false), "abX");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_origin_moves_to_restored_home() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?6s\x1b[?6h").unwrap();
        assert!(terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?6r").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_restore_left_right_margin_false_clears_horizontal_margins() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?69s\x1b[?69h\x1b[?69r").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert!(!terminal.get_mode_for_tests(Mode::EnableLeftAndRightMargin));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 0);
        assert_eq!(region.right, 9);
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_bracketed_paste_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?2004s\x1b[?2004h\x1b[?2004r")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::BracketedPaste));

        terminal
            .next_slice(b"\x1b[?2004h\x1b[?2004s\x1b[?2004l\x1b[?2004r")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::BracketedPaste));
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_multi_params_skip_unknown_and_apply_in_order() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?1;7;2004s").unwrap();
        terminal.next_slice(b"\x1b[?1h\x1b[?7l\x1b[?2004h").unwrap();
        terminal.next_slice(b"\x1b[?9999;1;7;2004r").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::CursorKeys));
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
        assert!(!terminal.get_mode_for_tests(Mode::BracketedPaste));
    }

    #[test]
    fn terminal_stream_csi_mode_save_has_no_side_effect_until_restore() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);

        terminal.next_slice(b"\x1b[?6s\x1b[?69s").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 2);
        assert_eq!(region.right, 8);
    }

    #[test]
    fn terminal_stream_csi_mode_restore_never_saved_uses_saved_false_default() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?7rabcX").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));
        assert_eq!(plain_with_unwrap(&terminal, false), "abX");
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_request_reports_default_and_reset_wraparound() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?7$p").unwrap();
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?7;1$y");

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b[?7;1$y".to_vec()
        );
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal.next_slice(b"\x1b[?7l\x1b[?7$p").unwrap();
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?7;2$y");
    }

    #[test]
    fn terminal_stream_csi_mode_request_reports_bracketed_paste_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?2004$p").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?2004;2$y");

        terminal.next_slice(b"\x1b[?2004h\x1b[?2004$p").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?2004;1$y");
    }

    #[test]
    fn terminal_stream_csi_mode_request_reports_unknown_dec_mode() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?9999$p").unwrap();

        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?9999;0$y");
    }

    #[test]
    fn terminal_stream_csi_mode_request_appends_multiple_responses_in_order() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?7$p\x1b[?2004h\x1b[?2004$p\x1b[?9999$p")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b[?7;1$y\x1b[?2004;1$y\x1b[?9999;0$y"
        );
    }

    #[test]
    fn terminal_stream_csi_mode_request_does_not_mutate_terminal_display_state() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[?7$p").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(terminal_modes_extra())
                .format(),
            "abc"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 2);
        assert_eq!(region.right, 8);
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?7;1$y");
    }

    #[test]
    fn terminal_stream_query_response_enq_and_color_scheme_are_inert() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x05\x1b[?996n").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_query_response_device_attributes_and_decid() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[c").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?62;22c");

        terminal.next_slice(b"\x1b[>c").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[>1;0;0c");

        terminal.next_slice(b"\x1b[=c").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP!|00000000\x1b\\"
        );

        terminal.next_slice(b"\x1bZ").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?62;22c");
    }

    #[test]
    fn terminal_stream_query_response_xtversion_uses_roastty_name() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[>0q").unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1bP>|libroastty\x1b\\"
        );
        assert!(!terminal
            .pty_response_for_tests()
            .windows(b"ghostty".len())
            .any(|window| window == b"ghostty"));
        assert!(!terminal
            .pty_response_for_tests()
            .windows(b"Ghostty".len())
            .any(|window| window == b"Ghostty"));
        assert!(!terminal
            .pty_response_for_tests()
            .windows(b"Roastty".len())
            .any(|window| window == b"Roastty"));
    }

    #[test]
    fn terminal_stream_decrqss_default_sgr_response() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP1$r0m\x1b\\");
    }

    #[test]
    fn terminal_stream_decrqss_sgr_flags_use_ghostty_order() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[1;2;3;4;5;7;8;9m").unwrap();
        terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r0;1;2;3;4;5;7;8;9m\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_decrqss_sgr_palette_colors() {
        for (sgr, expected) in [
            (
                b"\x1b[30;40m".as_slice(),
                b"\x1bP1$r0;30;40m\x1b\\".as_slice(),
            ),
            (b"\x1b[91;103m", b"\x1bP1$r0;91;103m\x1b\\"),
            (
                b"\x1b[38;5;16;48;5;255m",
                b"\x1bP1$r0;38:5:16;48:5:255m\x1b\\",
            ),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(sgr).unwrap();
            terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

            assert_eq!(terminal.take_pty_response_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_decrqss_sgr_rgb_colors() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[38;2;1;2;3;48;2;4;5;6m").unwrap();
        terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r0;38:2::1:2:3;48:2::4:5:6m\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_decrqss_scrolling_region_responses() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);
        terminal.next_slice(b"\x1bP$qr\x1b\\").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r2;4r\x1b\\"
        );

        terminal.next_slice(b"\x1bP$qs\x1b\\").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP0$r\x1b\\");

        terminal.set_mode_for_tests(Mode::EnableLeftAndRightMargin, true);
        terminal.next_slice(b"\x1bP$qs\x1b\\").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r3;9s\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_decrqss_unsupported_requests_return_invalid() {
        for input in [b"\x1bP$qz\x1b\\".as_slice(), b"\x1bP$qx\x1b\\"] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();
            terminal.next_slice(b"abc").unwrap();
            terminal.clear_dirty_for_tests();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP0$r\x1b\\");
            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(9, 0));
        }
    }

    #[test]
    fn terminal_stream_decscusr_sets_cursor_visual_style_and_blinking() {
        for (input, expected_style, expected_blinking) in [
            (b"\x1b[1 q".as_slice(), cursor::VisualStyle::Block, true),
            (b"\x1b[2 q", cursor::VisualStyle::Block, false),
            (b"\x1b[3 q", cursor::VisualStyle::Underline, true),
            (b"\x1b[4 q", cursor::VisualStyle::Underline, false),
            (b"\x1b[5 q", cursor::VisualStyle::Bar, true),
            (b"\x1b[6 q", cursor::VisualStyle::Bar, false),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_visual_style_for_tests(), expected_style);
            assert_eq!(
                terminal.get_mode_for_tests(Mode::CursorBlinking),
                expected_blinking
            );
        }
    }

    #[test]
    fn terminal_stream_decscusr_default_sets_configured_default() {
        for input in [b"\x1b[ q".as_slice(), b"\x1b[0 q"] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(b"\x1b[5 q").unwrap();
            terminal.next_slice(input).unwrap();

            assert_eq!(
                terminal.cursor_visual_style_for_tests(),
                cursor::VisualStyle::Block
            );
            assert!(terminal.get_mode_for_tests(Mode::CursorBlinking));
        }
    }

    #[test]
    fn terminal_cursor_default_config_initializes_style_and_blink() {
        let terminal = Terminal::init_with_options(
            10,
            2,
            None,
            TerminalInitOptions {
                cursor_visual_style: cursor::VisualStyle::Bar,
                cursor_blink: Some(false),
                ..TerminalInitOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Bar
        );
        assert!(!terminal.get_mode_for_tests(Mode::CursorBlinking));
    }

    #[test]
    fn terminal_cursor_default_config_decscusr_resets_to_configured_default() {
        let mut terminal = Terminal::init_with_options(
            10,
            2,
            None,
            TerminalInitOptions {
                cursor_visual_style: cursor::VisualStyle::Underline,
                cursor_blink: Some(false),
                ..TerminalInitOptions::default()
            },
        )
        .unwrap();

        terminal.next_slice(b"\x1b[5 q").unwrap();
        terminal.next_slice(b"\x1b[ q").unwrap();

        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Underline
        );
        assert!(!terminal.get_mode_for_tests(Mode::CursorBlinking));
    }

    #[test]
    fn terminal_cursor_style_blink_config_gates_dec_mode_12() {
        for configured in [Some(true), Some(false)] {
            let mut terminal = Terminal::init_with_options(
                10,
                2,
                None,
                TerminalInitOptions {
                    cursor_visual_style: cursor::VisualStyle::Block,
                    cursor_blink: configured,
                    ..TerminalInitOptions::default()
                },
            )
            .unwrap();
            let initial = terminal.get_mode_for_tests(Mode::CursorBlinking);

            terminal.next_slice(b"\x1b[?12h").unwrap();
            assert_eq!(terminal.get_mode_for_tests(Mode::CursorBlinking), initial);
            terminal.next_slice(b"\x1b[?12l").unwrap();
            assert_eq!(terminal.get_mode_for_tests(Mode::CursorBlinking), initial);
        }

        let mut terminal = Terminal::init_with_options(
            10,
            2,
            None,
            TerminalInitOptions {
                cursor_visual_style: cursor::VisualStyle::Block,
                cursor_blink: None,
                ..TerminalInitOptions::default()
            },
        )
        .unwrap();
        terminal.next_slice(b"\x1b[?12l").unwrap();
        assert!(!terminal.get_mode_for_tests(Mode::CursorBlinking));
        terminal.next_slice(b"\x1b[?12h").unwrap();
        assert!(terminal.get_mode_for_tests(Mode::CursorBlinking));
    }

    #[test]
    fn terminal_stream_decscusr_does_not_mutate_visible_terminal_content() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc\x1b[1;31m").unwrap();
        let text_style = terminal.cursor_style_for_tests();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[5 q").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert_eq!(terminal.cursor_style_for_tests(), text_style);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Bar
        );
        assert!(terminal.get_mode_for_tests(Mode::CursorBlinking));
    }

    #[test]
    fn terminal_stream_decrqss_decscusr_reports_current_cursor_visual_style() {
        for (setup, expected) in [
            (b"".as_slice(), b"\x1bP1$r1 q\x1b\\".as_slice()),
            (b"\x1b[1 q", b"\x1bP1$r1 q\x1b\\"),
            (b"\x1b[4 q", b"\x1bP1$r4 q\x1b\\"),
            (b"\x1b[5 q", b"\x1bP1$r5 q\x1b\\"),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(setup).unwrap();
            terminal.next_slice(b"\x1bP$q q\x1b\\").unwrap();

            assert_eq!(terminal.take_pty_response_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_decscusr_split_feed_then_decrqss_reports_updated_style() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5").unwrap();
        terminal.next_slice(b" q").unwrap();
        terminal.next_slice(b"\x1bP$q q\x1b\\").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP1$r5 q\x1b\\");
    }

    #[test]
    fn terminal_stream_decrqss_over_capacity_is_ignored() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1bP$q\" q\x1b\\").unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_dcs_command_state_survives_split_feed() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1bP$q").unwrap();
        terminal.next_slice(b"m").unwrap();
        terminal.next_slice(b"\x1b").unwrap();
        terminal.next_slice(b"\\").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP1$r0m\x1b\\");
    }

    #[test]
    fn terminal_stream_dcs_command_xtgettcap_runtime_deferred() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1bP+q").unwrap();
        terminal.next_slice(b"536d756C78").unwrap();
        terminal.next_slice(b"\x1b\\").unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_dcs_command_tmux_and_unknown_are_ignored() {
        for input in [
            b"\x1bP1000ppayload\x1b\\".as_slice(),
            b"\x1bPApayload\x1b\\",
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();
            terminal.next_slice(b"abc").unwrap();
            terminal.clear_dirty_for_tests();

            terminal.next_slice(input).unwrap();

            assert!(terminal.pty_response_for_tests().is_empty());
            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position(), (3, 0));
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(9, 0));
        }
    }

    #[test]
    fn terminal_stream_query_response_device_status_reports() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.next_slice(b"\x1b[5n").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[0n");

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal.next_slice(b"\x1b[6n").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[3;5R");
    }

    #[test]
    fn terminal_stream_query_response_cursor_report_respects_origin_mode() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.set_scrolling_region_for_tests(1, 4, 2, 8);
        terminal.next_slice(b"\x1b[?6h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 3);
        terminal.next_slice(b"\x1b[6n").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[3;4R");
    }

    #[test]
    fn terminal_stream_dcs_unknown_and_apc_sequences_are_runtime_noops() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal.clear_dirty_for_tests();
        terminal
            .next_slice(b"\x1bPAignored\x1b\\\x1b_Gpayload\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_kitty_graphics_non_kitty_apc_is_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b_Ha=q,i=7\x1b\\").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_kitty_graphics_malformed_apc_is_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b_Ga=t;%%%%\x1b\\").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_kitty_graphics_over_limit_apc_is_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.kitty_graphics.set_max_bytes_for_tests(2);
        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal
            .next_slice(b"\x1b_Ga=t,f=32,s=1,v=1,i=7;AQIDBA==\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_kitty_graphics_config_applies_to_screens_and_resets() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.set_kitty_image_storage_limit(0);
        terminal.set_kitty_image_medium(KittyImageMedium::File, true);
        assert_eq!(terminal.screens.active().kitty_images().total_limit, 0);
        assert!(terminal.screens.active().kitty_images().image_limits.file);

        terminal.next_slice(b"\x1b[?1049h").unwrap();
        assert_eq!(terminal.screens.active().kitty_images().total_limit, 0);
        assert!(terminal.screens.active().kitty_images().image_limits.file);

        terminal.set_kitty_image_storage_limit(123);
        terminal.set_kitty_image_medium(KittyImageMedium::TemporaryFile, true);
        assert_eq!(terminal.screens.active().kitty_images().total_limit, 123);
        assert!(
            terminal
                .screens
                .active()
                .kitty_images()
                .image_limits
                .temporary_file
        );

        terminal.next_slice(b"\x1b[?1049l").unwrap();
        assert_eq!(terminal.screens.active().kitty_images().total_limit, 123);
        assert!(terminal.screens.active().kitty_images().image_limits.file);
        assert!(
            terminal
                .screens
                .active()
                .kitty_images()
                .image_limits
                .temporary_file
        );

        terminal.set_kitty_image_storage_limit(0);
        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);

        terminal.reset();
        assert_eq!(terminal.screens.active().kitty_images().total_limit, 0);
        assert!(terminal.screens.active().kitty_images().image_limits.file);
        assert!(
            terminal
                .screens
                .active()
                .kitty_images()
                .image_limits
                .temporary_file
        );
        terminal.next_slice(&kitty_transmit_apc(8)).unwrap();
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);

        terminal.next_slice(b"\x1bc").unwrap();
        assert_eq!(terminal.screens.active().kitty_images().total_limit, 0);
        assert!(terminal.screens.active().kitty_images().image_limits.file);
        assert!(
            terminal
                .screens
                .active()
                .kitty_images()
                .image_limits
                .temporary_file
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_file_media_obeys_terminal_options() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        let dir = KittyFileTestDir::new();
        let file_path = dir.write("image.data", &[1, 2, 3, 4]);

        terminal
            .next_slice(&kitty_file_transmit_apc(70, 'f', &file_path))
            .unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(70)
            .is_none());

        terminal.set_kitty_image_medium(KittyImageMedium::File, true);
        terminal
            .next_slice(&kitty_file_transmit_apc(71, 'f', &file_path))
            .unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(71)
            .is_some());
        assert!(file_path.exists());

        terminal.set_kitty_image_medium(KittyImageMedium::File, false);
        terminal
            .next_slice(&kitty_file_transmit_apc(72, 'f', &file_path))
            .unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(72)
            .is_none());

        terminal.next_slice(&kitty_transmit_apc(73)).unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(73)
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_temporary_file_media_deletes_source() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        let dir = KittyFileTestDir::new();
        let blocked_path = dir.write("tty-graphics-protocol-blocked.data", &[1, 2, 3, 4]);

        terminal
            .next_slice(&kitty_file_transmit_apc(80, 't', &blocked_path))
            .unwrap();
        assert!(blocked_path.exists());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(80)
            .is_none());

        terminal.set_kitty_image_medium(KittyImageMedium::TemporaryFile, true);
        let allowed_path = dir.write("tty-graphics-protocol-allowed.data", &[1, 2, 3, 4]);
        terminal
            .next_slice(&kitty_file_transmit_apc(81, 't', &allowed_path))
            .unwrap();
        assert!(!allowed_path.exists());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(81)
            .is_some());

        terminal.set_kitty_image_medium(KittyImageMedium::TemporaryFile, false);
        let disabled_path = dir.write("tty-graphics-protocol-disabled.data", &[1, 2, 3, 4]);
        terminal
            .next_slice(&kitty_file_transmit_apc(82, 't', &disabled_path))
            .unwrap();
        assert!(disabled_path.exists());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(82)
            .is_none());
    }

    #[test]
    fn terminal_stream_kitty_graphics_shared_memory_obeys_terminal_options() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        let blocked = KittySharedMemoryObject::new(&[1, 2, 3, 4]);

        terminal
            .next_slice(&kitty_shared_memory_transmit_apc(90, &blocked))
            .unwrap();
        assert!(blocked.exists());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(90)
            .is_none());

        terminal.set_kitty_image_medium(KittyImageMedium::SharedMemory, true);
        let allowed = KittySharedMemoryObject::new(&[1, 2, 3, 4]);
        terminal
            .next_slice(&kitty_shared_memory_transmit_apc(91, &allowed))
            .unwrap();
        assert!(!allowed.exists());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(91)
            .is_some());

        terminal.set_kitty_image_medium(KittyImageMedium::SharedMemory, false);
        let disabled = KittySharedMemoryObject::new(&[1, 2, 3, 4]);
        terminal
            .next_slice(&kitty_shared_memory_transmit_apc(92, &disabled))
            .unwrap();
        assert!(disabled.exists());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(92)
            .is_none());

        terminal.next_slice(&kitty_transmit_apc(93)).unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(93)
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_png_decodes_through_sys_callback() {
        let _guard = SysDecodeGuard::with_png_decoder(decode_png_rgba_1x1);
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_png_transmit_apc(94)).unwrap();

        let image = terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(94)
            .unwrap();
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 1);
        assert_eq!(image.format, TransmissionFormat::Rgba);
        assert_eq!(image.data, [9, 8, 7, 6]);
    }

    #[test]
    fn terminal_stream_kitty_graphics_query_writes_response() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_query_apc(7)).unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7;OK\x1b\\"
        );
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
    }

    #[test]
    fn terminal_stream_kitty_graphics_transmit_stores_image() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7;OK\x1b\\"
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(7)
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_quiet_transmit_stores_without_response() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b_Ga=t,f=32,s=1,v=1,i=7,q=1;AQIDBA==\x1b\\")
            .unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(7)
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_display_stores_cursor_tracked_pin() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        let tracked_before = active_tracked_pin_count(&terminal);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal.next_slice(&kitty_display_apc(7, 4)).unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;OK\x1b\\"
        );
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before + 1);
        assert_eq!(
            tracked_placement_at_cursor(&terminal, 7, 4),
            active_pin(&terminal, 5, 1)
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_transmit_display_stores_image_and_placement() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_transmit_display_apc(7, 4, false))
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;OK\x1b\\"
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(7)
            .is_some());
        assert_eq!(
            tracked_placement_at_cursor(&terminal, 7, 4),
            active_pin(&terminal, 0, 0)
        );
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_kitty_graphics_transmit_display_maps_display_fields() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_transmit_display_sized_apc(7, 4, 3, 2, false))
            .unwrap();

        let placement = terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 4))
            .unwrap();
        assert_eq!(placement.columns, 3);
        assert_eq!(placement.rows, 2);
    }

    #[test]
    fn terminal_stream_kitty_graphics_numbered_transmit_display_gets_auto_id() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_numbered_transmit_display_apc(5, 4))
            .unwrap();
        let image_id = terminal.screens.active().kitty_images().next_image_id - 1;

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            format!("\x1b_Gi={image_id},I=5,p=4;OK\x1b\\").as_bytes()
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(image_id)
            .is_some());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(image_id, 4))
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_implicit_transmit_display_suppresses_response() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_implicit_transmit_display_apc(4))
            .unwrap();
        let image_id = terminal.screens.active().kitty_images().next_image_id - 1;

        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(image_id)
            .is_some());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(image_id, 4))
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_chunked_transmit_display_displays_on_final_chunk() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_transmit_display_chunk_apc(7, 4, true, "AQID"))
            .unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 4))
            .is_none());

        terminal
            .next_slice(&kitty_transmit_display_chunk_apc(7, 4, false, "BA=="))
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;OK\x1b\\"
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 4))
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_chunked_transmit_display_inherits_quiet() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_quiet_transmit_display_chunk_apc(
                7, 4, true, 2, "AQID",
            ))
            .unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal
            .next_slice(&kitty_quiet_transmit_display_chunk_apc(
                7, 4, false, 0, "BA==",
            ))
            .unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 4))
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_transmit_display_failure_keeps_image() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b_Ga=T,f=32,s=1,v=1,i=7,p=4,U=1,P=9;AQIDBA==\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;EINVAL: virtual placement cannot refer to a parent\x1b\\"
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(7)
            .is_some());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 4))
            .is_none());
    }

    #[test]
    fn terminal_stream_kitty_graphics_external_replacement_untracks_old_pin() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        let tracked_before = active_tracked_pin_count(&terminal);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(&kitty_display_apc(7, 4)).unwrap();
        let old_pin = active_kitty_placement_pin(&terminal, 7, 4);
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before + 1);

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(6, 2);
        terminal.clear_pty_response();
        terminal.next_slice(&kitty_display_apc(7, 4)).unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;OK\x1b\\"
        );
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before + 1);
        assert_eq!(active_tracked_pin_value(&terminal, old_pin), None);
        assert_eq!(
            tracked_placement_at_cursor(&terminal, 7, 4),
            active_pin(&terminal, 6, 2)
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_internal_placements_track_distinct_pins() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        let tracked_before = active_tracked_pin_count(&terminal);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(&kitty_display_apc(7, 0)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 1);
        terminal.next_slice(&kitty_display_apc(7, 0)).unwrap();

        assert_eq!(active_tracked_pin_count(&terminal), tracked_before + 2);
        let first = terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: 7,
                placement_id: PlacementId::Internal(0),
            })
            .unwrap()
            .tracked_pin()
            .unwrap();
        let second = terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: 7,
                placement_id: PlacementId::Internal(1),
            })
            .unwrap()
            .tracked_pin()
            .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn terminal_stream_kitty_graphics_transmit_eviction_untracks_placements() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        terminal.screens.active_mut().set_kitty_image_limit(6);

        terminal.next_slice(&kitty_transmit_apc(1)).unwrap();
        terminal.clear_pty_response();
        terminal.next_slice(&kitty_display_apc(1, 4)).unwrap();
        let tracked_before_eviction = active_tracked_pin_count(&terminal);
        let old_pin = active_kitty_placement_pin(&terminal, 1, 4);

        terminal.next_slice(&kitty_transmit_apc(2)).unwrap();

        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(1)
            .is_none());
        assert_eq!(active_tracked_pin_value(&terminal, old_pin), None);
        assert_eq!(
            active_tracked_pin_count(&terminal),
            tracked_before_eviction - 1
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_display_quiet_filtering() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal
            .next_slice(&kitty_quiet_display_apc(7, 4, 1))
            .unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: 7,
                placement_id: PlacementId::External(4),
            })
            .is_some());

        terminal
            .next_slice(&kitty_quiet_display_apc(99, 1, 2))
            .unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal
            .next_slice(&kitty_quiet_display_apc(7, 5, 2))
            .unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: 7,
                placement_id: PlacementId::External(5),
            })
            .is_some());

        terminal
            .next_slice(&kitty_quiet_display_apc(99, 1, 1))
            .unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=99,p=1;ENOENT: image not found\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_disabled_storage_suppresses_display() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        let tracked_before = active_tracked_pin_count(&terminal);
        terminal.next_slice(&kitty_display_apc(7, 3)).unwrap();
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before + 1);
        terminal.clear_pty_response();

        terminal.screens.active_mut().set_kitty_image_limit(0);
        terminal.next_slice(&kitty_display_apc(7, 4)).unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before);
    }

    #[test]
    fn terminal_stream_kitty_graphics_display_by_number_resolves_newest_image() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_numbered_transmit_apc(5))
            .unwrap();
        let first_id = terminal.screens.active().kitty_images().next_image_id - 1;
        terminal
            .next_slice(&kitty_numbered_transmit_apc(5))
            .unwrap();
        let second_id = terminal.screens.active().kitty_images().next_image_id - 1;
        terminal.clear_pty_response();
        terminal
            .next_slice(&kitty_display_number_apc(5, 8))
            .unwrap();

        assert_ne!(first_id, second_id);
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            format!("\x1b_Gi={second_id},I=5,p=8;OK\x1b\\").as_bytes()
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: second_id,
                placement_id: PlacementId::External(8),
            })
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_virtual_display_stores_virtual_location() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        let tracked_before = active_tracked_pin_count(&terminal);
        terminal
            .next_slice(&kitty_virtual_display_apc(7, 4))
            .unwrap();

        let placement = terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: 7,
                placement_id: PlacementId::External(4),
            })
            .unwrap();
        assert_eq!(placement.location, PlacementLocation::Virtual);
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before);
    }

    #[test]
    fn terminal_stream_kitty_graphics_failed_screen_insert_untracks_new_pin() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        let tracked_before = active_tracked_pin_count(&terminal);
        let screen = terminal.screens.active_mut();
        let pin = screen
            .pin(Point::active(Coordinate::new(2, 1)))
            .expect("test cell must have a pin");
        let tracked = screen.track_pin(pin).expect("test pin must track");
        assert_eq!(screen.count_tracked_pins_for_tests(), tracked_before + 1);

        let result = screen.add_kitty_placement(
            404,
            1,
            Placement {
                location: PlacementLocation::Pin(tracked),
                ..Placement::default()
            },
        );

        assert!(result.is_err());
        assert_eq!(active_tracked_pin_count(&terminal), tracked_before);
        assert_eq!(active_tracked_pin_value(&terminal, tracked), None);
    }

    #[test]
    fn terminal_stream_kitty_graphics_same_image_replacement_preserves_pin() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal.next_slice(&kitty_display_apc(7, 4)).unwrap();
        let tracked_before_replace = active_tracked_pin_count(&terminal);
        let pin = active_kitty_placement_pin(&terminal, 7, 4);

        terminal.clear_pty_response();
        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();

        assert_eq!(active_tracked_pin_count(&terminal), tracked_before_replace);
        assert_eq!(
            active_tracked_pin_value(&terminal, pin),
            Some(active_pin(&terminal, 0, 0))
        );
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 4))
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_cursor_after_moves_right_of_one_row_placement() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal
            .next_slice(&kitty_cursor_after_display_apc(7, 4))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (6, 1));
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(PlacementKey {
                image_id: 7,
                placement_id: PlacementId::External(4),
            })
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_transmit_display_cursor_after_moves_after_display() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal
            .next_slice(&kitty_transmit_display_apc(7, 4, true))
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;OK\x1b\\"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (6, 1));
        assert_eq!(
            tracked_placement_at_cursor(&terminal, 7, 4),
            active_pin(&terminal, 5, 1)
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_chunked_transmit_display_cursor_after_waits_for_final_chunk()
    {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal
            .next_slice(&kitty_transmit_display_chunk_apc(7, 4, true, "AQID"))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal
            .next_slice(&kitty_transmit_display_chunk_apc(7, 4, false, "BA=="))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (6, 1));
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=7,p=4;OK\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_cursor_after_indexes_once_per_row() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal
            .next_slice(&kitty_cursor_after_display_sized_apc(7, 4, 3, 3))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
    }

    #[test]
    fn terminal_stream_kitty_graphics_cursor_after_scrolls_at_bottom_row() {
        let mut terminal = terminal_with_lines(&["aaaa", "bbbb", "cccc"]);

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 2);
        terminal
            .next_slice(&kitty_cursor_after_display_apc(7, 4))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(plain_with_unwrap(&terminal, false).contains("bbbb"));
    }

    #[test]
    fn terminal_stream_kitty_graphics_cursor_after_honors_origin_vertical_margins() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal.set_scrolling_region_for_tests(1, 3, 0, 9);
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 3);
        terminal
            .next_slice(&kitty_cursor_after_display_apc(7, 4))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 3));
    }

    #[test]
    fn terminal_stream_kitty_graphics_cursor_after_honors_origin_horizontal_margins() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal.set_scrolling_region_for_tests(0, 4, 2, 5);
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal
            .next_slice(&kitty_cursor_after_display_sized_apc(7, 4, 4, 1))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
    }

    #[test]
    fn terminal_stream_kitty_graphics_virtual_cursor_after_does_not_move_cursor() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal
            .next_slice(b"\x1b_Ga=p,i=7,p=4,U=1,C=0\x1b\\")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
    }

    #[test]
    fn terminal_stream_kitty_graphics_failed_cursor_after_display_does_not_move_cursor() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal
            .next_slice(&kitty_cursor_after_display_apc(99, 4))
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b_Gi=99,p=4;ENOENT: image not found\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_graphics_alternate_screen_has_separate_storage() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.clear_pty_response();
        terminal.next_slice(&kitty_display_apc(7, 1)).unwrap();
        let primary_count = terminal
            .screens
            .screen(TerminalScreenKey::Primary)
            .unwrap()
            .count_tracked_pins_for_tests();
        terminal.next_slice(b"\x1b[?1049h").unwrap();
        terminal.next_slice(&kitty_transmit_apc(8)).unwrap();
        terminal.clear_pty_response();
        terminal.next_slice(&kitty_display_apc(8, 1)).unwrap();

        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(8)
            .is_some());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(7)
            .is_none());
        assert!(terminal
            .screens
            .screen(TerminalScreenKey::Primary)
            .unwrap()
            .kitty_images()
            .image_by_id(7)
            .is_some());
        assert_eq!(
            terminal
                .screens
                .screen(TerminalScreenKey::Primary)
                .unwrap()
                .count_tracked_pins_for_tests(),
            primary_count
        );
        assert_eq!(terminal.screens.active().count_tracked_pins_for_tests(), 2);
        assert!(terminal
            .screens
            .screen(TerminalScreenKey::Primary)
            .unwrap()
            .kitty_images()
            .placement_by_key(kitty_placement_key(7, 1))
            .is_some());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(8, 1))
            .is_some());
    }

    #[test]
    fn terminal_stream_kitty_graphics_reset_clears_storage_and_partial_apc() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(7)).unwrap();
        terminal.next_slice(&kitty_display_apc(7, 4)).unwrap();
        let tracked_before_reset = active_tracked_pin_count(&terminal);
        assert!(tracked_before_reset > 1);
        terminal.clear_pty_response();
        terminal.next_slice(b"\x1b_Ga=q,i=9").unwrap();
        terminal.reset();
        terminal.next_slice(b";AQIDBA==\x1b\\").unwrap();

        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal.next_slice(&kitty_transmit_apc(8)).unwrap();
        terminal.next_slice(&kitty_display_apc(8, 4)).unwrap();
        assert!(active_tracked_pin_count(&terminal) > 1);
        terminal.clear_pty_response();
        terminal.next_slice(b"\x1bc").unwrap();

        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_kitty_virtual_placeholder_print_sets_row_flag() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.modes.set(Mode::GraphemeCluster, true);

        terminal
            .next_slice("\u{10eeee}\u{0305}\u{0305}".as_bytes())
            .unwrap();

        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));
        let placements = terminal.kitty_virtual_placements_visible();
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].row, 0);
        assert_eq!(placements[0].col, 0);
        assert_eq!(placements[0].width, 1);
    }

    #[test]
    fn terminal_stream_kitty_virtual_placeholder_overwrite_keeps_flag_until_last() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .next_slice("\u{10eeee}\u{10eeee}".as_bytes())
            .unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"A").unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"B").unwrap();
        assert!(!terminal.active_row_kitty_virtual_placeholder_for_tests(0));
        assert!(terminal.kitty_virtual_placements_visible().is_empty());
    }

    #[test]
    fn terminal_stream_kitty_virtual_placeholder_partial_mutations_keep_row_flag_truthful() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();
        terminal
            .next_slice("A\u{10eeee}B\u{10eeee}C".as_bytes())
            .unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[X").unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);
        terminal.next_slice(b"\x1b[X").unwrap();
        assert!(!terminal.active_row_kitty_virtual_placeholder_for_tests(0));

        terminal.next_slice("\rA\u{10eeee}BC".as_bytes()).unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[@").unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[P").unwrap();
        assert!(terminal.active_row_kitty_virtual_placeholder_for_tests(0));
    }

    #[test]
    fn terminal_stream_kitty_virtual_placeholder_scroll_moves_row_flag() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice("\u{10eeee}\nB\nC".as_bytes()).unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), 1);
        assert!(!terminal.active_row_kitty_virtual_placeholder_for_tests(0));
        assert!(!terminal.active_row_kitty_virtual_placeholder_for_tests(1));
        assert!(terminal.kitty_virtual_placements_visible().is_empty());
    }

    #[test]
    fn terminal_stream_kitty_virtual_placeholder_visible_iterator_order_and_values() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();
        let placeholder = char::from_u32(graphics_unicode::PLACEHOLDER).unwrap();
        terminal.screens.active_mut().set_cell_for_tests(0, 0, 'A');
        terminal
            .screens
            .active_mut()
            .set_cell_for_tests(1, 0, placeholder);
        terminal.append_grapheme_for_tests(1, 0, 0x0305);
        terminal.append_grapheme_for_tests(1, 0, 0x0305);
        terminal.screens.active_mut().set_cell_for_tests(0, 1, 'B');
        terminal
            .screens
            .active_mut()
            .set_cell_for_tests(1, 1, placeholder);
        terminal.append_grapheme_for_tests(1, 1, 0x030d);
        terminal.append_grapheme_for_tests(1, 1, 0x030e);
        terminal.append_grapheme_for_tests(1, 1, 0x030e);
        terminal
            .screens
            .active_mut()
            .set_cell_for_tests(2, 1, placeholder);
        terminal.append_grapheme_for_tests(2, 1, 0x030d);
        terminal.append_grapheme_for_tests(2, 1, 0x0310);
        terminal.append_grapheme_for_tests(2, 1, 0x030e);

        let placements = terminal.kitty_virtual_placements_visible();
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[0].pin, active_pin(&terminal, 1, 0));
        assert_eq!(placements[0].row, 0);
        assert_eq!(placements[0].col, 0);
        assert_eq!(placements[0].width, 1);
        assert_eq!(placements[1].pin, active_pin(&terminal, 1, 1));
        assert_eq!(placements[1].row, 1);
        assert_eq!(placements[1].col, 2);
        assert_eq!(placements[1].image_id, 0x0200_0000);
        assert_eq!(placements[1].width, 2);
    }

    #[test]
    fn terminal_stream_kitty_virtual_placeholder_decodes_style_ids() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let placeholder = char::from_u32(graphics_unicode::PLACEHOLDER).unwrap();
        terminal.screens.active_mut().set_styled_cell_for_tests(
            0,
            0,
            placeholder,
            style::Style {
                fg_color: style::Color::Rgb(color::Rgb::new(1, 2, 3)),
                underline_color: style::Color::Palette(21),
                ..style::Style::default()
            },
        );
        terminal.append_grapheme_for_tests(0, 0, 0x0305);
        terminal.append_grapheme_for_tests(0, 0, 0x0305);
        terminal.append_grapheme_for_tests(0, 0, 0x030e);

        let placements = terminal.kitty_virtual_placements_visible();
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].image_id, 0x0201_0203);
        assert_eq!(placements[0].placement_id, 21);
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_all_images_is_silent_and_untracks() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(1)).unwrap();
        terminal.next_slice(&kitty_display_apc(1, 1)).unwrap();
        terminal.next_slice(&kitty_transmit_apc(2)).unwrap();
        terminal.next_slice(&kitty_display_apc(2, 1)).unwrap();
        assert_eq!(active_tracked_pin_count(&terminal), 3);
        terminal.clear_pty_response();

        terminal.next_slice(&kitty_delete_apc("d=A")).unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
        assert!(terminal.screens.active().kitty_images().dirty);
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_all_placements_keeps_images_and_virtuals() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(1)).unwrap();
        terminal.next_slice(&kitty_display_apc(1, 1)).unwrap();
        terminal
            .next_slice(&kitty_virtual_display_apc(1, 2))
            .unwrap();
        terminal.clear_pty_response();

        terminal.next_slice(&kitty_delete_apc("d=a")).unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().len(), 1);
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 1);
        assert_eq!(
            terminal
                .screens
                .active()
                .kitty_images()
                .placement_by_key(kitty_placement_key(1, 2))
                .unwrap()
                .location,
            PlacementLocation::Virtual
        );
        assert_eq!(active_tracked_pin_count(&terminal), 1);
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_by_id_and_unused_image() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(&kitty_transmit_apc(1)).unwrap();
        terminal.next_slice(&kitty_display_apc(1, 1)).unwrap();
        terminal.next_slice(&kitty_display_apc(1, 2)).unwrap();
        terminal.clear_pty_response();

        terminal
            .next_slice(&kitty_delete_apc("d=i,i=1,p=2"))
            .unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(1, 2))
            .is_none());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(1)
            .is_some());
        assert_eq!(active_tracked_pin_count(&terminal), 2);

        terminal
            .next_slice(&kitty_delete_apc("d=I,i=1,p=1"))
            .unwrap();

        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_newest_by_number() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(&kitty_numbered_transmit_apc(5))
            .unwrap();
        let first_id = terminal.screens.active().kitty_images().next_image_id - 1;
        terminal
            .next_slice(&kitty_display_apc(first_id, 1))
            .unwrap();
        terminal
            .next_slice(&kitty_numbered_transmit_apc(5))
            .unwrap();
        let second_id = terminal.screens.active().kitty_images().next_image_id - 1;
        terminal
            .next_slice(&kitty_display_apc(second_id, 1))
            .unwrap();
        terminal.clear_pty_response();

        terminal
            .next_slice(&kitty_delete_apc("d=N,I=5,p=1"))
            .unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(first_id, 1))
            .is_some());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(second_id, 1))
            .is_none());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .image_by_id(second_id)
            .is_none());
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_intersect_cursor_cell_and_z() {
        let mut terminal = Terminal::init(100, 100, None).unwrap();
        terminal.kitty_graphics.set_cell_metrics_for_tests(100, 100);

        terminal.next_slice(&kitty_transmit_apc(1)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal
            .next_slice(&kitty_display_sized_apc(1, 1, 50, 50, 3))
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(25, 25);
        terminal
            .next_slice(&kitty_display_sized_apc(1, 2, 50, 50, 7))
            .unwrap();
        terminal.clear_pty_response();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(12, 12);
        terminal.next_slice(&kitty_delete_apc("d=c")).unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(1, 1))
            .is_none());
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(1, 2))
            .is_some());

        terminal
            .next_slice(&kitty_delete_apc("d=q,x=26,y=26,z=7"))
            .unwrap();
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_intersect_clamps_oversized_placement() {
        let mut terminal = Terminal::init(100, 100, None).unwrap();
        terminal.kitty_graphics.set_cell_metrics_for_tests(100, 100);

        terminal.next_slice(&kitty_transmit_apc(1)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal
            .next_slice(&kitty_display_sized_apc(1, 1, 10, 200, 0))
            .unwrap();
        assert_eq!(active_tracked_pin_count(&terminal), 2);
        terminal.clear_pty_response();

        terminal
            .next_slice(&kitty_delete_apc("d=p,x=1,y=100"))
            .unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
    }

    #[test]
    fn terminal_stream_kitty_graphics_delete_column_row_z_and_range() {
        let mut terminal = Terminal::init(100, 100, None).unwrap();
        terminal.kitty_graphics.set_cell_metrics_for_tests(100, 100);

        for id in 1..=3 {
            terminal.next_slice(&kitty_transmit_apc(id)).unwrap();
        }
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal
            .next_slice(&kitty_display_sized_apc(1, 1, 10, 10, 1))
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(20, 20);
        terminal
            .next_slice(&kitty_display_sized_apc(2, 1, 10, 10, 2))
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(40, 40);
        terminal
            .next_slice(&kitty_display_sized_apc(3, 1, 10, 10, 3))
            .unwrap();
        terminal.clear_pty_response();

        terminal.next_slice(&kitty_delete_apc("d=x,x=25")).unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(2, 1))
            .is_none());

        terminal.next_slice(&kitty_delete_apc("d=y,y=45")).unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(3, 1))
            .is_none());

        terminal
            .next_slice(&kitty_display_sized_apc(2, 1, 10, 10, 2))
            .unwrap();
        terminal.clear_pty_response();
        terminal.next_slice(&kitty_delete_apc("d=z,z=2")).unwrap();
        assert!(terminal
            .screens
            .active()
            .kitty_images()
            .placement_by_key(kitty_placement_key(2, 1))
            .is_none());

        terminal
            .next_slice(&kitty_display_sized_apc(2, 1, 10, 10, 2))
            .unwrap();
        terminal
            .next_slice(&kitty_display_sized_apc(3, 1, 10, 10, 3))
            .unwrap();
        terminal.clear_pty_response();
        terminal
            .next_slice(&kitty_delete_apc("d=R,x=1,y=2"))
            .unwrap();

        assert_eq!(terminal.screens.active().kitty_images().len(), 0);
        assert_eq!(terminal.screens.active().kitty_images().placement_len(), 0);
        assert_eq!(active_tracked_pin_count(&terminal), 1);
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_sgr_mutates_cursor_style_without_visible_output() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"ab").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[1;3;31m").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ab");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(
            terminal.cursor_style_for_tests(),
            style::Style {
                fg_color: style::Color::Palette(1),
                flags: style::Flags {
                    bold: true,
                    italic: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            }
        );
    }

    #[test]
    fn terminal_stream_osc_updates_title_pwd_and_hyperlink_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(
                b"\x1b]0;window title\x07\x1b]7;file://host/home\x1b\\\x1b]8;id=tab;https://e\x07",
            )
            .unwrap();

        assert_eq!(terminal.title_for_tests(), "window title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("tab".to_string()),
                "https://e"
            ))
        );

        terminal.next_slice(b"\x1b]8;;\x1b\\").unwrap();
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
    }

    #[test]
    fn terminal_stream_title_report_disabled_by_default() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]0;secret title\x07\x1b[21t")
            .unwrap();

        assert_eq!(terminal.title_for_tests(), "secret title");
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_title_report_enabled_reports_current_osc_title() {
        let mut terminal = Terminal::init_with_options(
            10,
            2,
            None,
            TerminalInitOptions {
                title_report: true,
                ..TerminalInitOptions::default()
            },
        )
        .unwrap();

        terminal
            .next_slice(b"\x1b]2;visible title\x1b\\\x1b[21t")
            .unwrap();

        assert_eq!(terminal.title_for_tests(), "visible title");
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b]lvisible title\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_title_report_runtime_toggle_preserves_title() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.next_slice(b"\x1b]0;toggle title\x07").unwrap();

        terminal.next_slice(b"\x1b[21t").unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal.set_title_report(true);
        terminal.next_slice(b"\x1b[21t").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b]ltoggle title\x1b\\"
        );

        terminal.set_title_report(false);
        terminal.next_slice(b"\x1b[21t").unwrap();
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_osc_icon_unsupported_and_malformed_leave_state_unchanged() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]0;original\x07\x1b]7;file://host/original\x07")
            .unwrap();
        terminal
            .next_slice(
                b"\x1b]1;icon\x07\x1b]9;notify\x07\x1b]8;bad=value;https://bad\x1b\\\x1b]0;\xff\x07",
            )
            .unwrap();

        assert_eq!(terminal.title_for_tests(), "original");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/original"));
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_osc1337_current_dir_updates_pwd() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]1337;CurrentDir=file://host/osc1337\x07")
            .unwrap();

        assert_eq!(terminal.pwd_for_tests(), Some("file://host/osc1337"));
    }

    #[test]
    fn terminal_stream_osc1337_copy_is_terminal_noop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_pwd_for_tests("file://host/original");
        terminal.clear_dirty_for_tests();
        terminal
            .next_slice(b"\x1b]1337;Copy=:YWJjMTIz\x07")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/original"));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_osc22_updates_mouse_shape() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Text);

        terminal.next_slice(b"\x1b]22;pointer\x07").unwrap();
        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Pointer);

        terminal.next_slice(b"\x1b]22;pointer\x1b\\").unwrap();
        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Pointer);

        terminal.next_slice(b"\x1b]22;left_ptr\x07").unwrap();
        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Default);
    }

    #[test]
    fn terminal_stream_osc22_unknown_shape_does_not_mutate_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b]22;crosshair\x07").unwrap();
        assert_eq!(
            terminal.mouse_shape_for_tests(),
            mouse::MouseShape::Crosshair
        );

        terminal.next_slice(b"\x1b]22;Crosshair\x07").unwrap();
        terminal.next_slice(b"\x1b]22;not-a-shape\x07").unwrap();

        assert_eq!(
            terminal.mouse_shape_for_tests(),
            mouse::MouseShape::Crosshair
        );
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_osc66_text_sizing_is_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]66;s=2:w=7:n=13:d=15:v=1:h=2;wide\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        for row in 0..3 {
            assert!(!terminal.is_dirty_for_tests(0, row));
            assert!(!terminal.is_dirty_for_tests(9, row));
        }
    }

    #[test]
    fn terminal_stream_osc133_marks_prompt_input_and_output() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"abc\x1b]133;A\x07$ \x1b]133;B\x07cmd\x1b]133;C\x07out")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc\n$ cmdout");
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::Prompt
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 1),
            SemanticContent::Prompt
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(2, 1),
            SemanticContent::Input
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(5, 1),
            SemanticContent::Output
        );
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_cursor_is_at_prompt_tracks_semantic_prompt_state() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        assert!(!terminal.cursor_is_at_prompt());

        terminal.next_slice(b"\x1b]133;P\x07$ ").unwrap();
        assert!(terminal.cursor_is_at_prompt());

        terminal.next_slice(b"\x1b]133;B\x07ls").unwrap();
        assert!(terminal.cursor_is_at_prompt());

        terminal.next_slice(b"\x1b]133;C\x07").unwrap();
        assert!(terminal.cursor_is_at_prompt());

        terminal.next_slice(b"\noutput").unwrap();
        assert!(!terminal.cursor_is_at_prompt());

        terminal.next_slice(b"\n\x1b]133;A\x07").unwrap();
        assert!(terminal.cursor_is_at_prompt());
    }

    #[test]
    fn terminal_cursor_is_at_prompt_returns_false_on_alternate_screen() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();
        terminal.next_slice(b"\x1b]133;P\x07$ ").unwrap();
        assert!(terminal.cursor_is_at_prompt());

        terminal.next_slice(b"\x1b[?1049h\x1b]133;P\x07$ ").unwrap();

        assert_eq!(terminal.active_screen(), TerminalScreen::Alternate);
        assert!(!terminal.cursor_is_at_prompt());
    }

    #[test]
    fn terminal_stream_osc133_new_command_and_prompt_kinds() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();

        terminal.next_slice(b"\x1b]133;N\x07one").unwrap();
        terminal.next_slice(b"\n\x1b]133;P;k=c\x07two").unwrap();
        terminal.next_slice(b"\n\x1b]133;P;k=s\x07tri").unwrap();

        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::Prompt
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(2),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 0),
            SemanticContent::Prompt
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(3, 1),
            SemanticContent::Prompt
        );
    }

    #[test]
    fn terminal_stream_osc133_clear_eol_resets_on_newline_without_bulk_marking() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b]133;A\x07> \x1b]133;I\x07cmd\nout")
            .unwrap();

        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::Prompt
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::None
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(2, 0),
            SemanticContent::Input
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(7, 0),
            SemanticContent::Output
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(3, 1),
            SemanticContent::Output
        );
    }

    #[test]
    fn terminal_stream_osc133_marks_linefeed_and_soft_wrap_continuations() {
        let mut newline = Terminal::init(8, 3, None).unwrap();
        newline
            .next_slice(b"\x1b]133;A\x07> \x1b]133;B\x07cmd\nnext")
            .unwrap();
        assert_eq!(
            newline.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            newline.active_cell_semantic_content_for_tests(5, 1),
            SemanticContent::Input
        );

        let mut wrapped = Terminal::init(4, 3, None).unwrap();
        wrapped.next_slice(b"\x1b]133;A\x07abcde").unwrap();
        assert_eq!(
            wrapped.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            wrapped.active_cell_semantic_content_for_tests(0, 1),
            SemanticContent::Prompt
        );
    }

    #[test]
    fn terminal_stream_osc133_fresh_line_uses_ghostty_margin_rule() {
        let mut at_zero = Terminal::init(8, 4, None).unwrap();
        at_zero.set_scrolling_region_for_tests(0, 3, 2, 7);
        at_zero.next_slice(b"\x1b]133;L\x07").unwrap();
        assert_eq!(at_zero.cursor_position_for_tests(), (0, 0));

        let mut at_margin = Terminal::init(8, 4, None).unwrap();
        at_margin.set_scrolling_region_for_tests(0, 3, 2, 7);
        at_margin
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        at_margin.next_slice(b"\x1b]133;L\x07").unwrap();
        assert_eq!(at_margin.cursor_position_for_tests(), (2, 0));

        let mut left_of_margin = Terminal::init(8, 4, None).unwrap();
        left_of_margin.set_scrolling_region_for_tests(0, 3, 2, 7);
        left_of_margin
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        left_of_margin.next_slice(b"\x1b]133;L\x07").unwrap();
        assert_eq!(left_of_margin.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_osc133_c_clears_prompt_row_only_at_column_zero() {
        let mut at_zero = Terminal::init(8, 3, None).unwrap();
        at_zero.next_slice(b"\x1b]133;P\x07\x1b]133;C\x07").unwrap();
        assert_eq!(
            at_zero.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::None
        );

        let mut after_prompt = Terminal::init(8, 3, None).unwrap();
        after_prompt
            .next_slice(b"\x1b]133;P\x07$\x1b]133;C\x07")
            .unwrap();
        assert_eq!(
            after_prompt.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::Prompt
        );
    }

    #[test]
    fn terminal_stream_osc133_d_restores_output_semantic_content() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b]133;A\x07> \x1b]133;B\x07cmd\x1b]133;D\x07out")
            .unwrap();

        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(2, 0),
            SemanticContent::Input
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(5, 0),
            SemanticContent::Output
        );
    }

    #[test]
    fn terminal_stream_notifications_are_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]9;Hello\x07\x1b]777;notify;Title;Body\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_clipboard_protocols_are_retained_without_terminal_side_effects() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]52;s;?\x07\x1b]5522;type=read;payload\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.drain_clipboard_events(),
            vec![
                TerminalClipboardEvent::Osc52 {
                    kind: b's',
                    data: b"?".to_vec(),
                },
                TerminalClipboardEvent::Kitty {
                    metadata: b"type=read".to_vec(),
                    payload: Some(b"payload".to_vec()),
                    terminator: osc::Terminator::St,
                },
            ]
        );
        assert!(terminal.drain_clipboard_events().is_empty());

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_clipboard_events_preserve_parse_order_and_kitty_empty_payload() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b]52;c;raw\x07\x1b]5522;type=read\x07\x1b]52;p;?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.drain_clipboard_events(),
            vec![
                TerminalClipboardEvent::Osc52 {
                    kind: b'c',
                    data: b"raw".to_vec(),
                },
                TerminalClipboardEvent::Kitty {
                    metadata: b"type=read".to_vec(),
                    payload: None,
                    terminator: osc::Terminator::Bel,
                },
                TerminalClipboardEvent::Osc52 {
                    kind: b'p',
                    data: b"?".to_vec(),
                },
            ]
        );
    }

    #[test]
    fn terminal_clipboard_events_clear_on_direct_reset() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"\x1b]52;c;raw\x07").unwrap();
        assert!(!terminal.drain_clipboard_events().is_empty());
        terminal.next_slice(b"\x1b]52;c;raw\x07").unwrap();

        terminal.reset();

        assert!(terminal.drain_clipboard_events().is_empty());
    }

    #[test]
    fn terminal_clipboard_events_clear_on_ris_full_reset() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"\x1b]52;c;raw\x07").unwrap();
        assert!(!terminal.drain_clipboard_events().is_empty());
        terminal.next_slice(b"\x1b]52;c;raw\x07").unwrap();

        terminal.next_slice(b"\x1bc").unwrap();

        assert!(terminal.drain_clipboard_events().is_empty());
    }

    #[test]
    fn terminal_stream_context_signals_are_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(
                b"\x1b]3008;start=myctx;type=shell\x07\x1b]3008;end=myctx;exit=success\x1b\\",
            )
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_osc_actions_do_not_mutate_display_or_responses() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]0;t\x07\x1b]7;file://host/p\x07\x1b]8;;https://e\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_osc_hyperlink_formatter_observes_active_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"hi\x1b]8;;https://implicit\x1b\\")
            .unwrap();

        let actual = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none().screen(ScreenFormatterExtra::none().hyperlink(true)),
            )
            .format();

        assert_eq!(actual, "hi\x1b]8;;https://implicit\x1b\\");
    }

    #[test]
    fn terminal_stream_osc_writes_page_hyperlink_metadata() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\AB\x1b]8;;\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert!(terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(terminal.active_cell_hyperlink_for_tests(1, 0));
        let first = terminal
            .active_cell_hyperlink_snapshot_for_tests(0, 0)
            .unwrap();
        let second = terminal
            .active_cell_hyperlink_snapshot_for_tests(1, 0)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.id, HyperlinkSnapshotId::Implicit(0));
        assert_eq!(first.uri, b"https://e");
        assert_eq!(terminal.active_cell_hyperlink_ref_count_for_tests(0, 0), 2);
        assert!(terminal.active_row_hyperlink_for_tests(0));
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc_after_end_clears_destination_hyperlink() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\AB\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"Z").unwrap();

        assert!(terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(!terminal.active_cell_hyperlink_for_tests(1, 0));
        assert_eq!(terminal.active_cell_hyperlink_ref_count_for_tests(0, 0), 1);
        assert!(terminal.active_row_hyperlink_for_tests(0));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"Y").unwrap();
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(!terminal.active_row_hyperlink_for_tests(0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc_stores_explicit_ids_exactly() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;id=x&<y;https://example.com?a=1&b=<2>\x1b\\A")
            .unwrap();

        let link = terminal
            .active_cell_hyperlink_snapshot_for_tests(0, 0)
            .unwrap();
        assert_eq!(link.id, HyperlinkSnapshotId::Explicit(b"x&<y".to_vec()));
        assert_eq!(link.uri, b"https://example.com?a=1&b=<2>");
    }

    #[test]
    fn terminal_stream_osc_separate_implicit_ranges_get_distinct_ids() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://one\x1b\\A\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .next_slice(b"\x1b]8;;https://two\x1b\\B\x1b]8;;\x1b\\")
            .unwrap();

        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 0)
                .unwrap()
                .id,
            HyperlinkSnapshotId::Implicit(0)
        );
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(1, 0)
                .unwrap()
                .id,
            HyperlinkSnapshotId::Implicit(1)
        );
    }

    #[test]
    fn terminal_stream_osc_and_sgr_compose_on_printed_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\\x1b[1;31mA")
            .unwrap();

        assert!(terminal.active_cell_hyperlink_for_tests(0, 0));
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            }
        );
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_pending_wrap_overwrites_hyperlink_destination() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal
            .next_slice(b"\x1b]8;;https://old\x1b\\X\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\x1b]8;;https://new\x1b\\w").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nw");
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 1)
                .unwrap()
                .uri,
            b"https://new"
        );
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_pending_wrap_without_active_link_clears_hyperlink_destination() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal
            .next_slice(b"\x1b]8;;https://old\x1b\\X\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"hellow").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nw");
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 1));
        assert!(!terminal.active_row_hyperlink_for_tests(1));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_insert_mode_shifts_existing_hyperlinks() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"A\x1b]8;;https://old\x1b\\B\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.next_slice(b"\x1b]8;;https://new\x1b\\Z").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ZAB");
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 0)
                .unwrap()
                .uri,
            b"https://new"
        );
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(2, 0)
                .unwrap()
                .uri,
            b"https://old"
        );
        assert!(!terminal.active_cell_hyperlink_for_tests(1, 0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_scroll_up_preserves_printed_hyperlinks() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal
            .next_slice(b"\x1b]8;;https://scroll\x1b\\X\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "X");
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 0)
                .unwrap()
                .uri,
            b"https://scroll"
        );
        assert!(terminal.active_row_hyperlink_for_tests(0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc4_mutates_palette_entries() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]4;1;rgb:ff/00/80;2;#00ff00\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.palette.current()[1],
            color::Rgb::new(255, 0, 128)
        );
        assert_eq!(
            terminal.colors.palette.current()[2],
            color::Rgb::new(0, 255, 0)
        );
        assert_eq!(terminal.pty_response_for_tests(), b"");
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc4_applies_repeated_palette_index_in_order() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]4;1;#ff0000;1;#0000ff\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.palette.current()[1],
            color::Rgb::new(0, 0, 255)
        );
    }

    #[test]
    fn terminal_stream_osc4_query_reports_palette_with_bel_terminator() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .colors
            .palette
            .set(3, color::Rgb::new(1, 0x80, 0xff));

        terminal.next_slice(b"\x1b]4;3;?\x07").unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]4;3;rgb:0101/8080/ffff\x07"
        );
    }

    #[test]
    fn terminal_stream_osc4_query_reports_palette_with_st_terminator() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .colors
            .palette
            .set(4, color::Rgb::new(0x12, 0x34, 0x56));

        terminal.next_slice(b"\x1b]4;4;?\x1b\\").unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]4;4;rgb:1212/3434/5656\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_osc104_resets_indexed_palette_entry() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.colors.palette.set(1, color::Rgb::new(255, 0, 0));
        terminal.colors.palette.set(2, color::Rgb::new(0, 255, 0));

        terminal.next_slice(b"\x1b]104;1\x1b\\").unwrap();

        assert_eq!(
            terminal.colors.palette.current()[1],
            color::DEFAULT_PALETTE[1]
        );
        assert_eq!(
            terminal.colors.palette.current()[2],
            color::Rgb::new(0, 255, 0)
        );
    }

    #[test]
    fn terminal_stream_osc104_resets_all_palette_entries() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.colors.palette.set(1, color::Rgb::new(255, 0, 0));
        terminal.colors.palette.set(2, color::Rgb::new(0, 255, 0));

        terminal.next_slice(b"\x1b]104\x1b\\").unwrap();

        assert_eq!(terminal.colors.palette.current(), &color::DEFAULT_PALETTE);
    }

    #[test]
    fn terminal_stream_osc_dynamic_colors_set_without_changing_palette() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let original_palette = *terminal.colors.palette.current();

        terminal
            .next_slice(b"\x1b]10;#112233\x1b\\\x1b]11;#445566\x1b\\\x1b]12;#778899\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.foreground.get(),
            Some(color::Rgb::new(0x11, 0x22, 0x33))
        );
        assert_eq!(
            terminal.colors.background.get(),
            Some(color::Rgb::new(0x44, 0x55, 0x66))
        );
        assert_eq!(
            terminal.colors.cursor.get(),
            Some(color::Rgb::new(0x77, 0x88, 0x99))
        );
        assert_eq!(terminal.colors.palette.current(), &original_palette);
    }

    #[test]
    fn terminal_stream_osc_dynamic_sequence_executes_in_order() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]10;#010203;#040506;?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.foreground.get(),
            Some(color::Rgb::new(1, 2, 3))
        );
        assert_eq!(
            terminal.colors.background.get(),
            Some(color::Rgb::new(4, 5, 6))
        );
        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]12;rgb:0101/0202/0303\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_osc_dynamic_resets_restore_defaults() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]10;#010203\x1b\\\x1b]11;#040506\x1b\\\x1b]12;#070809\x1b\\")
            .unwrap();
        terminal
            .next_slice(b"\x1b]110\x1b\\\x1b]111;\x1b\\\x1b]112\x1b\\")
            .unwrap();

        assert_eq!(terminal.colors.foreground.get(), None);
        assert_eq!(terminal.colors.background.get(), None);
        assert_eq!(terminal.colors.cursor.get(), None);
    }

    #[test]
    fn terminal_stream_osc12_query_falls_back_to_foreground_when_cursor_unset() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]10;#123456\x1b\\\x1b]12;?\x07")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]12;rgb:1212/3434/5656\x07"
        );
    }

    #[test]
    fn terminal_stream_osc12_query_reports_cursor_override() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]12;#abcdef\x1b\\\x1b]12;?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]12;rgb:abab/cdcd/efef\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_palette_set_reset_and_query() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]21;2=#010203;2=?;2=\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.palette.current()[2],
            color::DEFAULT_PALETTE[2]
        );
        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;2=rgb:01/02/03\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_dynamic_set_reset_and_query() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(
                b"\x1b]21;foreground=#112233;background=#445566;cursor=#778899;foreground=?;background=?;cursor=?\x07",
            )
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;foreground=rgb:11/22/33;background=rgb:44/55/66;cursor=rgb:77/88/99\x07"
        );

        terminal.take_pty_response_for_tests();
        terminal
            .next_slice(
                b"\x1b]21;foreground=;background=;cursor=;foreground=?;background=?;cursor=?\x1b\\",
            )
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;foreground=;background=;cursor=\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_cursor_query_has_no_foreground_fallback() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]21;foreground=#123456;cursor=?\x1b\\")
            .unwrap();

        assert_eq!(terminal.pty_response_for_tests(), b"\x1b]21;cursor=\x1b\\");
    }

    #[test]
    fn terminal_stream_kitty_osc21_mixed_order_uses_current_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]21;foreground=?;foreground=#010203;foreground=?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;foreground=;foreground=rgb:01/02/03\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_unsupported_specials_are_inert() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let original_palette = *terminal.colors.palette.current();

        terminal
            .next_slice(
                b"\x1b]21;selection_foreground=#010203;cursor_text=;selection_background=?\x1b\\",
            )
            .unwrap();

        assert_eq!(terminal.colors.palette.current(), &original_palette);
        assert_eq!(terminal.colors.foreground.get(), None);
        assert_eq!(terminal.colors.background.get(), None);
        assert_eq!(terminal.colors.cursor.get(), None);
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b]21\x1b\\");
    }

    #[test]
    fn terminal_stream_unsupported_color_osc_does_not_mutate_palette() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let original = *terminal.colors.palette.current();

        for input in [
            b"\x1b]5;0;#ff0000\x1b\\".as_slice(),
            b"\x1b]13;#ff0000\x1b\\".as_slice(),
            b"\x1b]19;?\x1b\\".as_slice(),
            b"\x1b]113\x1b\\".as_slice(),
            b"\x1b]119\x1b\\".as_slice(),
        ] {
            terminal.next_slice(input).unwrap();
        }

        assert_eq!(terminal.colors.palette.current(), &original);
    }

    #[test]
    fn terminal_stream_osc4_palette_change_affects_formatter_palette_output() {
        let mut terminal = terminal_with_lines(&["X"]);

        terminal.next_slice(b"\x1b]4;1;#123456\x1b\\").unwrap();
        let output = formatter(&terminal, PageOutputFormat::Html)
            .with_extra(TerminalFormatterExtra::none().palette(true))
            .format();

        assert!(output.contains("--vt-palette-1: #123456;"));
    }

    #[test]
    fn terminal_stream_osc_split_feed() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b]").unwrap();
        terminal.next_slice(b"0;split").unwrap();
        terminal.next_slice(b"\x1b").unwrap();
        terminal.next_slice(b"\\").unwrap();
        terminal.next_slice(b"\x1b]7;file://host/s").unwrap();
        terminal.next_slice(b"plit\x07").unwrap();

        assert_eq!(terminal.title_for_tests(), "split");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/split"));
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_sgr_prints_styled_cells_and_resets() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[1;31mA\x1b[0mB").unwrap();

        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.active_cell_style_for_tests(0, 0), styled);
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 1);
        assert_eq!(
            terminal.active_cell_style_for_tests(1, 0),
            style::Style::default()
        );
        terminal.verify_integrity_for_tests();

        let vt = formatter(&terminal, PageOutputFormat::Vt).format();
        assert_eq!(vt, "\x1b[0m\x1b[1m\x1b[38;5;1mA\x1b[0mB");
        let html = formatter(&terminal, PageOutputFormat::Html).format();
        assert!(html.contains("font-weight: bold;"));
        assert!(html.contains("color: var(--vt-palette-1);"));
    }

    #[test]
    fn terminal_stream_sgr_overwrites_styled_cells_with_correct_refs() {
        let mut terminal = Terminal::init(2, 1, None).unwrap();

        terminal.next_slice(b"\x1b[31mA").unwrap();
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 1);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[32mB").unwrap();

        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style {
                fg_color: style::Color::Palette(2),
                ..style::Style::default()
            }
        );
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 1);
        terminal.verify_integrity_for_tests();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[0mC").unwrap();

        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style::default()
        );
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 0);
        assert!(!terminal.active_row_styled_for_tests(0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_sgr_styled_printing_keeps_insert_and_wrap_behavior() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal.next_slice(b"AC").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[4h\x1b[31mB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC");
        assert_eq!(
            terminal.active_cell_style_for_tests(1, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"X").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"Y").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert_eq!(
            terminal.active_cell_style_for_tests(2, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 1),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_sgr_pending_wrap_overwrites_styled_target_cell() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal.next_slice(b"\x1b[32mG").unwrap();
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 1),
            style::Style {
                fg_color: style::Color::Palette(2),
                ..style::Style::default()
            }
        );

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1b[31mX").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"Y").unwrap();

        let red = style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        };
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert_eq!(terminal.active_cell_style_for_tests(2, 0), red);
        assert_eq!(terminal.active_cell_style_for_tests(0, 1), red);
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 1), 2);
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_csi_insert_mode_inserts_before_printing() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[1;2H\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXello");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(terminal.get_mode_for_tests(Mode::Insert));

        terminal.next_slice(b"\x1b[4lY").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXYllo");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.get_mode_for_tests(Mode::Insert));
    }

    #[test]
    fn terminal_stream_csi_insert_mode_discards_pushed_edge_content() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[1;2H\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXell");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_insert_mode_at_right_edge_uses_print_wrap_path() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nX");
        assert_eq!(plain_with_unwrap(&terminal, true), "helloX");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_insert_mode_honors_horizontal_margins() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.next_slice(b"0123456789").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 5);

        terminal.next_slice(b"\x1b[1;3H\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "01X2346789");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));

        terminal.next_slice(b"\x1b[1;7HY").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "01X234Y789");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_mode_outside_margin_clears_pending_wrap_without_shift() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 0, 6);

        terminal.next_slice(b"\x1b[1;7HA").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 7, 8);
        terminal.next_slice(b"\x1b[?7l\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "      X");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_split_feed_csi_insert_mode_applies_to_print() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[1;2H\x1b[4").unwrap();
        terminal.next_slice(b"hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXello");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_linefeed_mode_changes_lf_runtime_behavior() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x1b[20h\nB\x1b[20l\nC").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB\n C");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 2));
    }

    #[test]
    fn terminal_stream_csi_linefeed_mode_scrolls_then_carriage_returns() {
        let mut terminal = Terminal::init(4, 2, None).unwrap();

        terminal.next_slice(b"abc\x1b[20h\nX\nY").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "X\nY");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "abc\nX\nY");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_wraparound_mode_controls_pending_wrap_runtime_behavior() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"\x1b[?7lhelloX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellX");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));

        terminal.next_slice(b"YZ").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellZ");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));

        terminal.next_slice(b"\x1b[?7hA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellZ\nA");
        assert_eq!(plain_with_unwrap(&terminal, true), "hellZA");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_csi_wraparound_with_horizontal_margins_wraps_to_left_margin() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 2, 2, 5);

        terminal.next_slice(b"\x1b[1;6HA").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"B").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "     A\n  B");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_csi_disabled_wraparound_dirties_current_row_only() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"\x1b[?7lhello").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellX");
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_disabled_wraparound_bottom_right_does_not_scroll() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"\x1b[2;1H\x1b[?7lworld").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "\nworlX");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "\nworlX");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(1));
    }

    #[test]
    fn terminal_stream_lf_clears_pending_wrap_without_soft_wrap() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\n").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_vt_and_ff_clear_pending_wrap_without_soft_wrap() {
        for input in [b"\x0b".as_slice(), b"\x0c".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"hello").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "hello");
            assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
            assert!(!terminal.cursor_pending_wrap_for_tests());
            assert!(!terminal.row_wrap_for_tests(0));
            assert!(!terminal.row_wrap_continuation_for_tests(1));
        }
    }

    #[test]
    fn terminal_stream_escape_d_and_e_clear_pending_wrap_without_soft_wrap() {
        for (input, expected_cursor) in
            [(b"\x1bD".as_slice(), (4, 1)), (b"\x1bE".as_slice(), (0, 1))]
        {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"hello").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "hello");
            assert_eq!(terminal.cursor_position_for_tests(), expected_cursor);
            assert!(!terminal.cursor_pending_wrap_for_tests());
            assert!(!terminal.row_wrap_for_tests(0));
            assert!(!terminal.row_wrap_continuation_for_tests(1));
        }
    }

    #[test]
    fn terminal_stream_lf_marks_old_and_new_rows_dirty() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"A").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\n").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_escape_d_and_e_mark_same_rows_dirty_as_lf() {
        for input in [b"\x1bD".as_slice(), b"\x1bE".as_slice()] {
            let mut lf = Terminal::init(5, 3, None).unwrap();
            lf.next_slice(b"A").unwrap();
            lf.clear_dirty_for_tests();
            lf.next_slice(b"\n").unwrap();

            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal.next_slice(b"A").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            for point in [(0, 0), (4, 0), (0, 1), (4, 1), (0, 2)] {
                assert_eq!(
                    terminal.is_dirty_for_tests(point.0, point.1),
                    lf.is_dirty_for_tests(point.0, point.1),
                    "dirty state mismatch for {input:?} at {point:?}",
                );
            }
        }
    }

    #[test]
    fn terminal_stream_reverse_index_moves_cursor_up_outside_top_margin_case() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 2);
        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_reverse_index_clamps_at_top_when_not_in_scrolling_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal.set_scrolling_region_for_tests(1, 3, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_reverse_index_scrolls_down_at_top_margin_inside_horizontal_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "AAAAA\nB   B\nCBBBC\nDDDDD"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(terminal.is_dirty_for_tests(1, 1));
        assert!(terminal.is_dirty_for_tests(3, 2));
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_reverse_index_does_not_scroll_top_margin_outside_horizontal_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);

        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "AAAAA\nBBBBB\nCCCCC\nDDDDD"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_cr_clears_pending_wrap_and_does_not_dirty_rows() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\r").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_bottom_row_lf_scrolls_and_preserves_column() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        terminal.next_slice(b"\n").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "cd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_vt_and_ff_scroll_and_preserve_column() {
        for input in [b"\x0b".as_slice(), b"\x0c".as_slice()] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();

            terminal.next_slice(b"ab\r\ncd").unwrap();
            assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "cd");
            assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
            assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_bottom_row_escape_d_scrolls_and_preserves_column() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        terminal.next_slice(b"\x1bD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "cd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_escape_e_scrolls_and_carriage_returns() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        terminal.next_slice(b"\x1bE").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "cd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_lf_marks_visible_rows_dirty() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\n").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
    }

    #[test]
    fn terminal_stream_bottom_row_escape_d_and_e_mark_visible_rows_dirty() {
        for input in [b"\x1bD".as_slice(), b"\x1bE".as_slice()] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();

            terminal.next_slice(b"ab\r\ncd").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert!(terminal.is_dirty_for_tests(0, 0));
            assert!(terminal.is_dirty_for_tests(4, 0));
            assert!(terminal.is_dirty_for_tests(0, 1));
            assert!(terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_crlf_formats_basic_lines() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"hello\r").unwrap();
        terminal.next_slice(b"\nworld").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
    }

    #[test]
    fn terminal_stream_split_feed_vt_and_ff_preserve_column() {
        for input in [b"\x0bworld".as_slice(), b"\x0cworld".as_slice()] {
            let mut terminal = Terminal::init(10, 3, None).unwrap();

            terminal.next_slice(b"hello").unwrap();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "hello\n     world");
            assert_eq!(terminal.cursor_position_for_tests(), (9, 1));
            assert!(terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_split_feed_escape_d_and_e_move_down() {
        for (second, expected_plain, expected_cursor) in [
            (b"Dworld".as_slice(), "hello\n     world", (9, 1)),
            (b"Eworld".as_slice(), "hello\nworld", (5, 1)),
        ] {
            let mut terminal = Terminal::init(10, 3, None).unwrap();

            terminal.next_slice(b"hello\x1b").unwrap();
            terminal.next_slice(second).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), expected_plain);
            assert_eq!(terminal.cursor_position_for_tests(), expected_cursor);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_moves_default_counts() {
        for (input, start, expected) in [
            (b"\x1b[A".as_slice(), (2, 1), (2, 0)),
            (b"\x1b[B".as_slice(), (2, 1), (2, 2)),
            (b"\x1b[C".as_slice(), (2, 1), (3, 1)),
            (b"\x1b[D".as_slice(), (2, 1), (1, 1)),
            (b"\x1b[k".as_slice(), (2, 1), (2, 0)),
            (b"\x1b[a".as_slice(), (2, 1), (3, 1)),
            (b"\x1b[j".as_slice(), (2, 1), (1, 1)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_uses_explicit_and_zero_counts() {
        for (input, start, expected) in [
            (b"\x1b[2A".as_slice(), (2, 3), (2, 1)),
            (b"\x1b[0A".as_slice(), (2, 3), (2, 2)),
            (b"\x1b[2B".as_slice(), (2, 1), (2, 3)),
            (b"\x1b[0B".as_slice(), (2, 1), (2, 2)),
            (b"\x1b[2C".as_slice(), (1, 1), (3, 1)),
            (b"\x1b[0C".as_slice(), (1, 1), (2, 1)),
            (b"\x1b[2D".as_slice(), (3, 1), (1, 1)),
            (b"\x1b[0D".as_slice(), (3, 1), (2, 1)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_clamps_oversized_counts_to_edges() {
        for (input, expected) in [
            (b"\x1b[999999999999999999999999A".as_slice(), (2, 0)),
            (b"\x1b[999999999999999999999999B".as_slice(), (2, 4)),
            (b"\x1b[999999999999999999999999C".as_slice(), (4, 2)),
            (b"\x1b[999999999999999999999999D".as_slice(), (0, 2)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(2, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_clears_pending_wrap() {
        for input in [
            b"\x1b[A".as_slice(),
            b"\x1b[B".as_slice(),
            b"\x1b[C".as_slice(),
            b"\x1b[D".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_does_not_modify_cells_or_dirty_rows() {
        for input in [
            b"\x1b[A".as_slice(),
            b"\x1b[B".as_slice(),
            b"\x1b[C".as_slice(),
            b"\x1b[D".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_clamps_without_scrolling_or_reverse_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.next_slice(b"ab\r\ncd").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[100D").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert_eq!(plain_with_unwrap(&terminal, false), "ab\ncd");

        terminal.next_slice(b"\x1b[100B").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert_eq!(plain_with_unwrap(&terminal, false), "ab\ncd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
    }

    #[test]
    fn terminal_stream_split_feed_csi_cursor_movement_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2C").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_move_and_carriage_return() {
        for (input, start, expected) in [
            (b"\x1b[E".as_slice(), (3, 1), (0, 2)),
            (b"\x1b[F".as_slice(), (3, 1), (0, 0)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_use_explicit_and_zero_counts() {
        for (input, start, expected) in [
            (b"\x1b[2E".as_slice(), (3, 1), (0, 3)),
            (b"\x1b[0E".as_slice(), (3, 1), (0, 2)),
            (b"\x1b[2F".as_slice(), (3, 3), (0, 1)),
            (b"\x1b[0F".as_slice(), (3, 3), (0, 2)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_clamp_without_scrolling() {
        for (input, start, expected) in [
            (b"\x1b[100E".as_slice(), (3, 1), (0, 1)),
            (b"\x1b[100F".as_slice(), (3, 0), (0, 0)),
        ] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();
            terminal.next_slice(b"ab\r\ncd").unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
            assert_eq!(plain_with_unwrap(&terminal, false), "ab\ncd");
            assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_clear_pending_wrap() {
        for input in [b"\x1b[E".as_slice(), b"\x1b[F".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests().0, 0);
            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_do_not_modify_cells_or_dirty_rows() {
        for input in [b"\x1b[E".as_slice(), b"\x1b[F".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
            assert!(!terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_next_line_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2E").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_split_feed_csi_previous_line_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 2);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2F").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_moves_to_default_column() {
        for input in [b"\x1b[G".as_slice(), b"\x1b[`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_uses_one_indexed_columns() {
        for (input, expected) in [
            (b"\x1b[1G".as_slice(), (0, 1)),
            (b"\x1b[2G".as_slice(), (1, 1)),
            (b"\x1b[5G".as_slice(), (4, 1)),
            (b"\x1b[1`".as_slice(), (0, 1)),
            (b"\x1b[3`".as_slice(), (2, 1)),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_zero_and_oversized_columns_clamp() {
        for (input, expected_x) in [
            (b"\x1b[0G".as_slice(), 0),
            (b"\x1b[0`".as_slice(), 0),
            (b"\x1b[999999999999999999999999G".as_slice(), 4),
            (b"\x1b[999999999999999999999999`".as_slice(), 4),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(2, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (expected_x, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_clears_pending_wrap() {
        for input in [b"\x1b[G".as_slice(), b"\x1b[`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_does_not_modify_cells_or_dirty_rows() {
        for input in [b"\x1b[2G".as_slice(), b"\x1b[3`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(4, 1);
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position_for_tests().1, 1);
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
            assert!(!terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_horizontal_absolute_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"3G").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));

        terminal.next_slice(b"\x1b[0").unwrap();
        terminal.next_slice(b"`").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_moves_to_default_rows() {
        for (input, start, expected) in [
            (b"\x1b[d".as_slice(), (3, 2), (3, 0)),
            (b"\x1b[e".as_slice(), (3, 1), (3, 2)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_uses_one_indexed_absolute_rows() {
        for (input, expected) in [
            (b"\x1b[1d".as_slice(), (3, 0)),
            (b"\x1b[2d".as_slice(), (3, 1)),
            (b"\x1b[4d".as_slice(), (3, 3)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_uses_relative_rows() {
        for (input, expected) in [
            (b"\x1b[0e".as_slice(), (3, 1)),
            (b"\x1b[1e".as_slice(), (3, 2)),
            (b"\x1b[2e".as_slice(), (3, 3)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_zero_and_oversized_values_clamp() {
        for (input, start, expected_y) in [
            (b"\x1b[0d".as_slice(), (2, 2), 0),
            (b"\x1b[0e".as_slice(), (2, 2), 2),
            (b"\x1b[999999999999999999999999d".as_slice(), (2, 1), 3),
            (b"\x1b[999999999999999999999999e".as_slice(), (2, 1), 3),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (2, expected_y));
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_clears_pending_wrap() {
        for input in [b"\x1b[d".as_slice(), b"\x1b[0e".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_does_not_modify_cells_or_dirty_rows() {
        for input in [b"\x1b[2d".as_slice(), b"\x1b[1e".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(4, 0);
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position_for_tests().0, 4);
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
            assert!(!terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_vertical_positioning_moves_cursor() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"3d").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 2));

        terminal.next_slice(b"\x1b[1").unwrap();
        terminal.next_slice(b"e").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 3));
    }

    #[test]
    fn terminal_stream_csi_cursor_position_moves_to_default_home() {
        for input in [b"\x1b[H".as_slice(), b"\x1b[f".as_slice()] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_position_uses_one_indexed_coordinates() {
        for (input, expected) in [
            (b"\x1b[2H".as_slice(), (0, 1)),
            (b"\x1b[2;3H".as_slice(), (2, 1)),
            (b"\x1b[4;5f".as_slice(), (4, 3)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_position_empty_and_zero_params_clamp_to_top_left() {
        for input in [
            b"\x1b[0;0H".as_slice(),
            b"\x1b[;H".as_slice(),
            b"\x1b[;;H".as_slice(),
            b"\x1b[3;;H".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            let expected = if input == b"\x1b[3;;H" {
                (0, 2)
            } else {
                (0, 0)
            };
            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_position_oversized_values_clamp_to_edges() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal
            .next_slice(b"\x1b[999999999999999999999999;999999999999999999999999H")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 3));
    }

    #[test]
    fn terminal_stream_csi_cursor_position_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[H").unwrap();

        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_cursor_position_does_not_modify_cells_or_dirty_rows() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[2;4H").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(4, 1));
    }

    #[test]
    fn terminal_stream_split_feed_csi_cursor_position_moves_cursor() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal.next_slice(b"\x1b[2;").unwrap();
        terminal.next_slice(b"4H").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"f").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_backspace_overwrites_previous_cell() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello\x08y").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "helly");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_moves_to_next_default_tabstop() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"1\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1       A");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_uses_custom_tabstops() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(3);

        terminal.next_slice(b"1\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1  A");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_without_later_tabstop_clamps_to_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"1\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1   A");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_repeated_horizontal_tabs_advance_and_clamp() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"1\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (19, 0));

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (19, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_starting_on_tabstop_moves_to_next_tabstop() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"12345678\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "12345678        A");
        assert_eq!(terminal.cursor_position_for_tests(), (17, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_at_right_edge_stays_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCD\t").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_horizontal_tab_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nX");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_horizontal_tab_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\t").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_moves_to_next_default_tabstop() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"1\x1b[IA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1       A");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_zero_count_does_not_move() {
        for input in [b"\x1b[0I".as_slice(), b"\x1b[;I".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_count_moves_multiple_tabstops() {
        for input in [b"\x1b[2I".as_slice(), b"\x1b[2;I".as_slice()] {
            let mut terminal = Terminal::init(24, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_stops_at_right_edge() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[999999999999999999999999I")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (19, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_uses_custom_tabstops() {
        let mut terminal = Terminal::init(12, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(3);
        terminal.set_tabstop_for_tests(7);

        terminal.next_slice(b"\x1b[2I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (7, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_starting_on_tabstop_moves_next() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 0);

        terminal.next_slice(b"\x1b[I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[2I").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_moves_to_previous_default_tabstops() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_starting_on_tabstop_moves_previous() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 0);

        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_zero_count_does_not_move() {
        for input in [b"\x1b[0Z".as_slice(), b"\x1b[;Z".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(16, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_count_moves_multiple_tabstops() {
        for input in [b"\x1b[2Z".as_slice(), b"\x1b[2;Z".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(19, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_large_count_clamps_to_left_edge() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);

        terminal
            .next_slice(b"\x1b[999999999999999999999999Z")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_uses_custom_tabstops() {
        let mut terminal = Terminal::init(12, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(3);
        terminal.set_tabstop_for_tests(7);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(10, 0);

        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_non_origin_ignores_left_margin() {
        let mut terminal = Terminal::init(20, 3, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 2, 10, 19);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(16, 0);

        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_origin_clamps_to_left_margin() {
        let mut terminal = Terminal::init(20, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal.set_scrolling_region_for_tests(0, 2, 5, 19);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(16, 0);

        terminal.next_slice(b"\x1b[9Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_origin_at_or_before_left_margin_does_not_move() {
        for start in [4, 5] {
            let mut terminal = Terminal::init(20, 3, None).unwrap();
            terminal.set_mode_for_tests(Mode::Origin, true);
            terminal.set_scrolling_region_for_tests(0, 2, 5, 19);
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(start, 0);

            terminal.next_slice(b"\x1b[Z").unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (start, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_preserves_pending_wrap_without_moving() {
        let mut terminal = Terminal::init(1, 2, None).unwrap();

        terminal.next_slice(b"A").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_preserves_pending_wrap_after_moving() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"ABCDEFGHIJKLMNOPQRST").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(19, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_split_feed_csi_horizontal_tab_back_moves_cursor() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));

        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_unsupported_csi_horizontal_tab_back_does_not_mutate_state() {
        for input in [
            b"\x1b[?ZA".as_slice(),
            b"\x1b[>ZA".as_slice(),
            b"\x1b[1;2ZA".as_slice(),
            b"\x1b[1:2ZA".as_slice(),
            b"\x1b[ ZA".as_slice(),
        ] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal.next_slice(b"abc").unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(16, 0);
            terminal.clear_dirty_for_tests();

            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc             A");
            assert_eq!(terminal.cursor_position_for_tests(), (17, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_erase_display_below_clears_cursor_to_end() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nFG");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_display_above_clears_start_to_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "\n   IJ\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_display_complete_clears_screen() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[2J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_csi_erase_display_scrollback_preserves_active_screen() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"A\nB\nC").unwrap();
        let active_before = plain_with_unwrap(&terminal, false);
        assert!(terminal.scrollback_rows_for_tests() > 0);

        terminal.next_slice(b"\x1b[3J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), active_before);
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
    }

    #[test]
    fn terminal_stream_csi_erase_display_scroll_complete_clears_active_and_resets_cursor() {
        let mut terminal = Terminal::init(5, 3, Some(10)).unwrap();

        terminal.next_slice(b"A\nB").unwrap();
        terminal.next_slice(b"\x1b[22J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_display_dirty_rows_match_affected_range() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[J").unwrap();

        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_display_full_rows_clear_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(2, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[J").unwrap();

        assert!(!terminal.row_wrap_for_tests(2));
        assert!(!terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_display_protected_cells_survive() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ"]);
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[?J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "  C");
    }

    #[test]
    fn terminal_stream_unsupported_csi_erase_display_does_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[4JCD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_erase_display_clears_screen() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB").unwrap();
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_csi_erase_line_right_clears_cursor_row_suffix() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nFG\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_left_clears_cursor_row_prefix() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n   IJ\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_csi_erase_line_complete_clears_cursor_row_only() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[2K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_does_not_mutate_scrollback() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"A\nB\nC").unwrap();
        let scrollback_before = terminal.scrollback_rows_for_tests();
        let active_before = plain_with_unwrap(&terminal, false);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);
        assert_ne!(plain_with_unwrap(&terminal, false), "");
        assert_ne!(plain_with_unwrap(&terminal, false), active_before);
    }

    #[test]
    fn terminal_stream_csi_erase_line_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_line_right_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[K").unwrap();

        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_left_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_complete_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[2K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n\nKLMNO");
        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_protected_cells_survive() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ"]);
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[?K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A C\nFGHIJ");
    }

    #[test]
    fn terminal_stream_unsupported_csi_erase_line_does_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[4KCD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_erase_line_clears_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB").unwrap();
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_csi_insert_chars_count_one_shifts_suffix_right() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_zero_count_behaves_as_one() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[0@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_clamps_to_remaining_margin() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[99@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_insert_chars_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[@").unwrap();

        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_outside_horizontal_margin_clears_pending_wrap_only() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_honors_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABC123").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC 13");
    }

    #[test]
    fn terminal_stream_csi_insert_chars_moves_protected_bit_with_shifted_cell() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert!(terminal.cell_protected_for_tests(3, 0));
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_count_one_clears_without_shifting() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A CDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_chars_zero_count_behaves_as_one() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[0X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A CDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clamps_to_screen_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[99X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_ignores_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABCDEF").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[2X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A  DEF");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[X").unwrap();

        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clears_stored_protected_cells() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB DE");
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_unsupported_csi_insert_and_erase_chars_do_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[?@CD\x1b[?X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_insert_and_erase_chars_mutates_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A   D");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_count_one_shifts_suffix_left() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ACDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_count_two_shifts_suffix_left() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[2P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ADE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_clamps_to_remaining_margin() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[99P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_delete_chars_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[P").unwrap();

        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_honors_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABC123").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC2 3");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_ignores_vertical_margins() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ACDE\nFGHIJ\nKLMNO");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_outside_horizontal_margin_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_preserves_scrollback_and_other_rows() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"A\nB\nC\nD").unwrap();
        let scrollback_before = terminal.scrollback_rows_for_tests();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);

        let mut terminal = terminal_with_lines(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nGHIJ\nKLMNO");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_moves_protected_bit_with_shifted_cell() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ACDE");
        assert!(terminal.cell_protected_for_tests(1, 0));
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_unsupported_csi_delete_chars_does_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[?PCD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_delete_chars_shifts_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ADE");
    }

    #[test]
    fn terminal_stream_csi_insert_lines_count_one_shifts_rows_down() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC\n\nDEF\nGHI");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(0, 2));
        assert!(terminal.is_dirty_for_tests(0, 3));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_clamps_to_remaining_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 2);

        terminal.next_slice(b"\x1b[99L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_preserves_scrollback_content() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"AAAAA\nBBBBB\nCCCCC\nDDDDD").unwrap();
        let active_before = plain_with_unwrap(&terminal, false);
        let full_before = terminal.full_screen_plain_for_tests(false);
        let scrollback_before = terminal.scrollback_rows_for_tests();
        let history_before = full_before
            .strip_suffix(&active_before)
            .expect("full screen output should end with active output")
            .to_owned();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);
        assert!(terminal
            .full_screen_plain_for_tests(false)
            .starts_with(&history_before));
        assert_ne!(plain_with_unwrap(&terminal, false), active_before);
    }

    #[test]
    fn terminal_stream_csi_insert_lines_outside_vertical_margin_is_noop() {
        let mut terminal = terminal_with_lines(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB\nCCCCC");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_outside_horizontal_margin_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_honors_top_bottom_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "ABC123\nD   56\nGEF489"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_wrap_metadata_depends_on_width() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert!(!terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_continuation_for_tests(2));

        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_unsupported_csi_insert_lines_does_not_mutate_state() {
        let mut terminal = terminal_with_lines(&["abc"]);

        terminal.next_slice(b"\x1b[?L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_split_feed_csi_insert_lines_shifts_rows() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nCCCCC");
    }

    #[test]
    fn terminal_stream_csi_delete_lines_count_one_shifts_rows_up() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC\nGHI");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(0, 2));
        assert!(terminal.is_dirty_for_tests(0, 3));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_clamps_to_remaining_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 2);

        terminal.next_slice(b"\x1b[99M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_preserves_scrollback_content() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"AAAAA\nBBBBB\nCCCCC\nDDDDD").unwrap();
        let active_before = plain_with_unwrap(&terminal, false);
        let full_before = terminal.full_screen_plain_for_tests(false);
        let scrollback_before = terminal.scrollback_rows_for_tests();
        let history_before = full_before
            .strip_suffix(&active_before)
            .expect("full screen output should end with active output")
            .to_owned();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);
        assert!(terminal
            .full_screen_plain_for_tests(false)
            .starts_with(&history_before));
        assert_ne!(plain_with_unwrap(&terminal, false), active_before);
    }

    #[test]
    fn terminal_stream_csi_delete_lines_outside_vertical_margin_is_noop() {
        let mut terminal = terminal_with_lines(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB\nCCCCC");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_outside_horizontal_margin_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_honors_top_bottom_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC\n\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "ABC123\nDHI756\nG   89"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_left_right_high_count_clamps() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[99M").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "ABC123\nD   56\nG   89"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_wrap_metadata_depends_on_width() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert!(!terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_continuation_for_tests(2));

        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_unsupported_csi_delete_lines_does_not_mutate_state() {
        let mut terminal = terminal_with_lines(&["abc"]);

        terminal.next_slice(b"\x1b[?M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_split_feed_csi_delete_lines_shifts_rows() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC");
    }

    #[test]
    fn terminal_stream_csi_scroll_up_creates_scrollback() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD", "EEEEE"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 2);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "BBBBB\nCCCCC\nDDDDD\nEEEEE"
        );
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "AAAAA\nBBBBB\nCCCCC\nDDDDD\nEEEEE"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 1);
        assert_eq!(terminal.cursor_position_for_tests(), (2, 2));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        for y in 0..5 {
            assert!(terminal.is_dirty_for_tests(0, y));
        }
    }

    #[test]
    fn terminal_stream_csi_scroll_up_max_scrollback_zero_scrolls_without_history() {
        let mut terminal = Terminal::init(5, 5, Some(0)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "BBBBB\nCCCCC");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "BBBBB\nCCCCC");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
    }

    #[test]
    fn terminal_stream_csi_scroll_up_clamps_to_region_height() {
        let mut terminal = Terminal::init(5, 3, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[99S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "AAAAA\nBBBBB\nCCCCC"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 3);
    }

    #[test]
    fn terminal_stream_csi_scroll_up_preserves_rows_below_bottom_margin() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD", "EEEEE"]);
        terminal.set_scrolling_region_for_tests(0, 2, 0, 4);

        terminal.next_slice(b"\x1b[2S").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "CCCCC\n\n\nDDDDD\nEEEEE"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 2);
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "AAAAA\nBBBBB\nCCCCC\n\n\nDDDDD\nEEEEE"
        );
    }

    #[test]
    fn terminal_stream_csi_scroll_up_with_top_margin_uses_delete_lines_path() {
        let mut terminal = Terminal::init(5, 4, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 3, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC\nDDDDD");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_up_with_left_right_margin_uses_delete_lines_path() {
        let mut terminal = Terminal::init(6, 3, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "AEF423\nDHI756\nG   89"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
    }

    #[test]
    fn terminal_stream_csi_scroll_up_preserves_pending_wrap() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal.next_slice(b"Z").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 2));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_scroll_up_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();
        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_count_one_shifts_rows_down() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 2);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "\nABC\nDEF\nGHI");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 2));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        for y in 0..4 {
            assert!(terminal.is_dirty_for_tests(0, y));
        }
    }

    #[test]
    fn terminal_stream_csi_scroll_down_honors_top_bottom_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "A   23\nDBC156\nGEF489"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_original_cursor_outside_vertical_margin_still_scrolls() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(2, 3, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 0);
        terminal.next_slice(b"Z").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAZ\nBBBBB\n\nCCCCC");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_scroll_down_original_cursor_outside_horizontal_margin_still_scrolls() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 0);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "A   23\nDBC156\nGEF489"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_preserves_pending_wrap() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal.next_slice(b"Z").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 2));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_scroll_down_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_clamps_to_region_height() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[99T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_unsupported_csi_scroll_up_and_down_does_not_mutate_state() {
        for input in [b"\x1b[?S".as_slice(), b"\x1b[?T".as_slice()] {
            let mut terminal = terminal_with_lines(&["abc"]);

            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_scroll_up_and_down_mutates_rows() {
        let mut terminal = Terminal::init(5, 4, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"S").unwrap();
        assert_eq!(plain_with_unwrap(&terminal, false), "BBBBB\nCCCCC");

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"T").unwrap();
        assert_eq!(plain_with_unwrap(&terminal, false), "\nBBBBB\nCCCCC");
    }

    #[test]
    fn terminal_stream_split_feed_csi_horizontal_tabulation_moves_cursor() {
        let mut terminal = Terminal::init(24, 2, None).unwrap();

        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
    }

    #[test]
    fn terminal_stream_split_feed_horizontal_tab_writes_at_next_tabstop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\tX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello   X");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
    }

    #[test]
    fn terminal_stream_escape_h_sets_tabstop_at_current_column() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1bH").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert!(!terminal.get_tabstop_for_tests(2));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_escape_h_tabstop_is_used_by_horizontal_tab() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1bH\r1\tZ").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert_eq!(plain_with_unwrap(&terminal, false), "1bcZ");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_escape_h_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1bH").unwrap();

        assert!(terminal.get_tabstop_for_tests(4));
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
    }

    #[test]
    fn terminal_stream_escape_h_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1bH").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_split_feed_escape_h_sets_tabstop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b").unwrap();
        terminal.next_slice(b"H").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_w_sets_tabstop_at_current_column() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b[W").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert!(!terminal.get_tabstop_for_tests(2));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_csi_zero_w_sets_tabstop_at_current_column() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b[0W").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert!(!terminal.get_tabstop_for_tests(2));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_csi_w_tabstop_is_used_by_horizontal_tab() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b[W\r1\tZ").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert_eq!(plain_with_unwrap(&terminal, false), "1bcZ");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_csi_w_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[W").unwrap();

        assert!(terminal.get_tabstop_for_tests(4));
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
    }

    #[test]
    fn terminal_stream_csi_w_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[W").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_tab_clear_current_removes_current_tabstop() {
        let mut terminal = Terminal::init(30, 2, None).unwrap();

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        assert!(terminal.get_tabstop_for_tests(8));

        terminal.next_slice(b"\x1b[2W\r\t").unwrap();

        assert!(!terminal.get_tabstop_for_tests(8));
        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
    }

    #[test]
    fn terminal_stream_csi_tab_clear_all_removes_all_tabstops() {
        let mut terminal = Terminal::init(30, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5W\r\t").unwrap();

        assert!(!terminal.get_tabstop_for_tests(8));
        assert!(!terminal.get_tabstop_for_tests(16));
        assert!(!terminal.get_tabstop_for_tests(24));
        assert_eq!(terminal.cursor_position_for_tests(), (29, 0));
    }

    #[test]
    fn terminal_stream_csi_tab_reset_restores_default_tabstops() {
        let mut terminal = Terminal::init(30, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5W\x1b[?5W\r\t").unwrap();

        assert!(terminal.get_tabstop_for_tests(8));
        assert!(terminal.get_tabstop_for_tests(16));
        assert!(terminal.get_tabstop_for_tests(24));
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
    }

    #[test]
    fn terminal_stream_csi_tab_clear_and_reset_preserve_cursor_position() {
        for input in [
            b"abc\x1bH\x1b[2W".as_slice(),
            b"abc\x1b[5W".as_slice(),
            b"abc\x1b[?5W".as_slice(),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        }
    }

    #[test]
    fn terminal_stream_csi_tab_clear_and_reset_preserve_pending_wrap() {
        for input in [
            b"\x1b[2W".as_slice(),
            b"\x1b[5W".as_slice(),
            b"\x1b[?5W".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
            assert!(terminal.cursor_pending_wrap_for_tests());
            assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        }
    }

    #[test]
    fn terminal_stream_csi_tab_clear_and_reset_do_not_dirty_rows_or_modify_cells() {
        for input in [
            b"\x1b[2W".as_slice(),
            b"\x1b[5W".as_slice(),
            b"\x1b[?5W".as_slice(),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(b"abc\x1bH").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(9, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
        }
    }

    #[test]
    fn terminal_stream_backspace_at_column_zero_clamps() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x08A").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_backspace_clears_pending_wrap_without_soft_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x08").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCXE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_backspace_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x08").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_split_feed_backspace_overwrites_previous_cell() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\x08y").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "helly");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_split_utf8_state_survives_feed_calls() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\xf0\x9f").unwrap();
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "");
        terminal.next_slice(b"A").unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            format!("{}A", char::REPLACEMENT_CHARACTER)
        );
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_valid_split_utf8_errors_only_after_completion() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        let bytes = "é".as_bytes();

        terminal.next_slice(&bytes[..1]).unwrap();
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "");
        terminal.next_slice(&bytes[1..]).unwrap();

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "é");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_split_csi_state_survives_feed_calls() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x1b[?").unwrap();
        terminal.next_slice(b"ZB").unwrap();

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_right_edge_writes_cell_and_sets_pending_wrap() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(0));
    }

    #[test]
    fn terminal_stream_pending_wrap_prints_next_cell_on_next_row() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"w").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nw");
        assert_eq!(plain_with_unwrap(&terminal, true), "hellow");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_basic_wraparound_matches_upstream_case() {
        let mut terminal = Terminal::init(5, 40, None).unwrap();

        terminal.next_slice(b"helloworldabc12").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld\nabc12");
        assert_eq!(plain_with_unwrap(&terminal, true), "helloworldabc12");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 2));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_for_tests(2));
        assert!(!terminal.row_wrap_continuation_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_pending_wrap_marks_old_and_new_rows_dirty() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"w").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_bottom_row_pending_wrap_scrolls_and_writes() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworldabc12").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "world\nabc12");
        assert_eq!(plain_with_unwrap(&terminal, true), "worldabc12");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "hello\nworld\nabc12"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_bottom_row_pending_wrap_survives_feed_boundary() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworld").unwrap();
        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"a").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "world\na");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "hello\nworld\na"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_after_scroll_writes_to_active_bottom_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworlda").unwrap();
        terminal.next_slice(b"bc").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "world\nabc");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "hello\nworld\nabc"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_pending_wrap_marks_visible_rows_dirty() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworld").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"a").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
    }

    #[test]
    fn terminal_stream_pending_wrap_managed_destination_errors_without_mutating() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal.next_slice(b"x").unwrap();
        terminal.set_cell_protected_for_tests(0, 1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"hello").unwrap();
        terminal.clear_dirty_for_tests();

        assert_eq!(
            terminal.next_slice(b"w"),
            Err(TerminalStreamError::ManagedCellUnsupported)
        );

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nx");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_non_ascii_prints_as_single_cell() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("é".as_bytes()).unwrap();

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "é");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_managed_cell_overwrite_returns_private_error() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.next_slice(b"x").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.set_cell_protected_for_tests(0, 0, true);

        assert_eq!(
            terminal.next_slice(b"A"),
            Err(TerminalStreamError::ManagedCellUnsupported)
        );
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "x");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_formatter_plain_full_active_screen_single_line() {
        let terminal = terminal_with_lines(&["hello"]);

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "hello"
        );
    }

    #[test]
    fn terminal_formatter_plain_full_active_screen_multiline() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "hello\nworld"
        );
    }

    #[test]
    fn terminal_formatter_plain_selected_line() {
        let terminal = terminal_with_lines(&["line1", "line2", "line3"]);
        let selection = active_selection(&terminal, (0, 1), (4, 1));

        let actual = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .format();

        assert_eq!(actual, "line2");
    }

    #[test]
    fn terminal_formatter_no_content_emits_empty_output_and_pin_map() {
        let terminal = terminal_with_lines(&["hello"]);

        let formatter = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::None);

        assert_eq!(formatter.format(), "");
        assert_eq!(
            formatter.format_with_pin_map(),
            PageStringWithPinMap {
                text: String::new(),
                pin_map: Vec::new(),
            }
        );
    }

    #[test]
    fn terminal_formatter_vt_content_delegates_to_screen_formatter() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt).format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output, "hello\r\nworld");
    }

    #[test]
    fn terminal_formatter_html_content_delegates_to_screen_formatter() {
        let terminal = terminal_with_lines(&["<hi"]);

        let terminal_output = formatter(&terminal, PageOutputFormat::Html).format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Html).format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output,
            "<div style=\"font-family: monospace; white-space: pre;\">&lt;hi</div>"
        );
    }

    #[test]
    fn terminal_formatter_codepoint_map_delegates_output_and_pin_map() {
        let terminal = terminal_with_lines(&["ao"]);
        let map = [CodepointMapEntry::new(
            'o' as u32,
            'o' as u32,
            CodepointReplacement::String("<é".to_string()),
        )
        .unwrap()];
        let options =
            TerminalFormatterOptions::new(PageOutputFormat::Html).codepoint_map(Some(&map));

        let terminal_output = TerminalFormatter::init(&terminal, options).format_with_pin_map();
        let screen_output = ScreenFormatter::init(
            terminal.screens.active(),
            ScreenFormatterOptions::new(PageOutputFormat::Html).codepoint_map(Some(&map)),
        )
        .format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output.text,
            "<div style=\"font-family: monospace; white-space: pre;\">a&lt;&#233;</div>"
        );
        assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
    }

    #[test]
    fn terminal_formatter_codepoint_map_selection_format_uses_replacements() {
        let terminal = terminal_with_lines(&["a─Σ"]);
        let selection = TerminalSelection {
            start: GridRef::from(active_pin(&terminal, 0, 0)).into(),
            end: GridRef::from(active_pin(&terminal, 2, 0)).into(),
            rectangle: false,
        };
        let map = [
            CodepointMapEntry::new('─' as u32, '─' as u32, CodepointReplacement::Codepoint('-'))
                .unwrap(),
            CodepointMapEntry::new(
                'Σ' as u32,
                'Σ' as u32,
                CodepointReplacement::String("SUM".to_string()),
            )
            .unwrap(),
        ];

        let plain = terminal
            .selection_format_with_codepoint_map(
                TerminalSelectionFormat::Plain,
                true,
                true,
                Some(selection),
                Some(&map),
            )
            .unwrap();
        assert_eq!(plain, "a-SUM");

        let html = terminal
            .selection_format_with_codepoint_map(
                TerminalSelectionFormat::Html,
                true,
                true,
                Some(selection),
                Some(&map),
            )
            .unwrap();
        assert!(html.ends_with("a-SUM</div>"), "{html}");
    }

    #[test]
    fn terminal_formatter_trim_and_palette_delegate_to_screen_formatter() {
        let mut terminal = terminal_with_lines(&["X  "]);
        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        };
        terminal
            .screens
            .active_mut()
            .set_styled_cell_for_tests(0, 0, 'X', styled);
        let mut palette = color::DEFAULT_PALETTE;
        palette[1] = color::Rgb::new(1, 2, 3);
        let options = TerminalFormatterOptions::new(PageOutputFormat::Html)
            .trim(false)
            .palette(Some(&palette));

        let terminal_output = TerminalFormatter::init(&terminal, options).format();
        let screen_output = ScreenFormatter::init(
            terminal.screens.active(),
            ScreenFormatterOptions::new(PageOutputFormat::Html)
                .trim(false)
                .palette(Some(&palette)),
        )
        .format();

        assert_eq!(terminal_output, screen_output);
        assert!(terminal_output.contains("rgb(1, 2, 3)"));
        assert!(terminal_output.contains("</div>  </div>"));
    }

    #[test]
    fn terminal_formatter_plain_pin_map_single_line() {
        let terminal = terminal_with_lines(&["hello"]);

        let actual = formatter(&terminal, PageOutputFormat::Plain).format_with_pin_map();

        assert_eq!(actual.text, "hello");
        assert_eq!(
            actual.pin_map,
            pins(&terminal, &[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)])
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn terminal_formatter_plain_pin_map_multiline() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        let actual = formatter(&terminal, PageOutputFormat::Plain).format_with_pin_map();

        assert_eq!(actual.text, "hello\nworld");
        assert_eq!(
            actual.pin_map,
            pins(
                &terminal,
                &[
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (3, 0),
                    (4, 0),
                    (4, 0),
                    (0, 1),
                    (1, 1),
                    (2, 1),
                    (3, 1),
                    (4, 1)
                ]
            )
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn terminal_formatter_selected_plain_pin_map() {
        let terminal = terminal_with_lines(&["line1", "line2", "line3"]);
        let selection = active_selection(&terminal, (0, 1), (4, 1));

        let actual = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .format_with_pin_map();

        assert_eq!(actual.text, "line2");
        assert_eq!(
            actual.pin_map,
            pins(&terminal, &[(0, 1), (1, 1), (2, 1), (3, 1), (4, 1)])
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn terminal_formatter_vt_and_html_pin_maps_delegate_to_screen_formatter() {
        let terminal = terminal_with_lines(&["<é"]);

        for emit in [PageOutputFormat::Vt, PageOutputFormat::Html] {
            let terminal_output = formatter(&terminal, emit).format_with_pin_map();
            let screen_output = screen_formatter(&terminal, emit).format_with_pin_map();

            assert_eq!(terminal_output, screen_output);
            assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
        }
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_style_for_tests(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            });
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal
            .screens
            .active_mut()
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt).format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output, "hi");
        assert!(!terminal_output.contains("\x1b]4;"));
        assert!(!terminal_output.contains("--vt-palette-"));
    }

    #[test]
    fn terminal_formatter_default_pin_map_does_not_emit_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal
            .screens
            .active_mut()
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output.text, "hi");
        assert_eq!(terminal_output.pin_map, pins(&terminal, &[(0, 0), (1, 0)]));
    }

    #[test]
    fn terminal_formatter_vt_palette_extra_emits_before_content() {
        let mut terminal = terminal_with_lines(&["content"]);
        set_test_palette_entries(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_palette_extra())
            .format();

        assert!(output.starts_with("\x1b]4;0;rgb:12/34/56\x1b\\"));
        assert_eq!(output.matches("\x1b]4;").count(), 256);
        assert!(output.contains("\x1b]4;1;rgb:ab/cd/ef\x1b\\"));
        assert!(output.contains("\x1b]4;255;rgb:ff/00/ff\x1b\\"));
        assert!(output.ends_with("content"));
        assert!(output.find("\x1b]4;255;").unwrap() < output.find("content").unwrap());
    }

    #[test]
    fn terminal_formatter_html_palette_extra_emits_before_content() {
        let mut terminal = terminal_with_lines(&["<content"]);
        set_test_palette_entries(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Html)
            .with_extra(terminal_palette_extra())
            .format();

        assert!(output.starts_with("<style>:root{"));
        assert_eq!(output.matches("--vt-palette-").count(), 256);
        assert!(output.contains("--vt-palette-0: #123456;"));
        assert!(output.contains("--vt-palette-1: #abcdef;"));
        assert!(output.contains("--vt-palette-255: #ff00ff;"));
        assert!(output.contains("}</style><div"));
        assert!(output.ends_with("&lt;content</div>"));
    }

    #[test]
    fn terminal_formatter_plain_ignores_palette_extra() {
        let mut terminal = terminal_with_lines(&["plain"]);
        set_test_palette_entries(&mut terminal);

        let default_output = formatter(&terminal, PageOutputFormat::Plain).format();
        let palette_output = formatter(&terminal, PageOutputFormat::Plain)
            .with_extra(terminal_palette_extra())
            .format();
        let palette_pin_map = formatter(&terminal, PageOutputFormat::Plain)
            .with_extra(terminal_palette_extra())
            .format_with_pin_map();

        assert_eq!(palette_output, default_output);
        assert_eq!(palette_output, "plain");
        assert_eq!(palette_pin_map.text, "plain");
        assert_eq!(
            palette_pin_map.pin_map,
            pins(&terminal, &[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)])
        );
    }

    #[test]
    fn terminal_formatter_palette_extra_without_content_emits_for_vt_and_html() {
        let mut terminal = terminal_with_lines(&["ignored"]);
        set_test_palette_entries(&mut terminal);

        let vt = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_palette_extra())
            .format();
        let html = formatter(&terminal, PageOutputFormat::Html)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_palette_extra())
            .format();
        let plain = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_palette_extra())
            .format();

        assert_eq!(vt.matches("\x1b]4;").count(), 256);
        assert!(vt.ends_with("\x1b]4;255;rgb:ff/00/ff\x1b\\"));
        assert_eq!(html.matches("--vt-palette-").count(), 256);
        assert!(html.ends_with("--vt-palette-255: #ff00ff;}</style>"));
        assert_eq!(plain, "");
    }

    #[test]
    fn terminal_formatter_vt_palette_pin_map_uses_top_left_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        set_test_palette_entries(&mut terminal);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_palette_extra())
            .format_with_pin_map();
        let prefix_len = palette_vt_prefix_len(&terminal);

        assert_eq!(output.text.len(), output.pin_map.len());
        assert!(output.text.starts_with("\x1b]4;0;rgb:12/34/56\x1b\\"));
        assert!(output.text.ends_with("éB"));
        assert!(prefix_len < output.text.len());
        for pin in &output.pin_map[..prefix_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        assert_eq!(
            &output.pin_map[prefix_len..],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
    }

    #[test]
    fn terminal_formatter_html_palette_pin_map_uses_top_left_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "<B"]);
        set_test_palette_entries(&mut terminal);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Html)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_palette_extra())
            .format_with_pin_map();
        let prefix_len = palette_html_prefix_len(&terminal);

        assert_eq!(output.text.len(), output.pin_map.len());
        assert!(output.text.starts_with("<style>:root{"));
        assert!(output.text.ends_with("&lt;B</div>"));
        assert!(prefix_len < output.text.len());
        for pin in &output.pin_map[..prefix_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        let content_start = output.text.find("&lt;B").unwrap();
        assert_eq!(output.pin_map[content_start], active_pin(&terminal, 0, 1));
        assert_eq!(
            output.pin_map.last().copied(),
            Some(active_pin(&terminal, 1, 1))
        );
    }

    #[test]
    fn terminal_formatter_palette_pin_map_without_content_uses_top_left() {
        let mut terminal = terminal_with_lines(&["ignored"]);
        set_test_palette_entries(&mut terminal);

        for emit in [PageOutputFormat::Vt, PageOutputFormat::Html] {
            let output = formatter(&terminal, emit)
                .with_content(ScreenFormatterContent::None)
                .with_extra(terminal_palette_extra())
                .format_with_pin_map();

            assert!(!output.text.is_empty());
            assert_eq!(output.text.len(), output.pin_map.len());
            for pin in output.pin_map {
                assert_eq!(pin, active_pin(&terminal, 0, 0));
            }
        }
    }

    #[test]
    fn terminal_formatter_vt_palette_combines_before_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .screen(all_screen_extras()),
            )
            .format();
        let prefix_len = palette_vt_prefix_len(&terminal);

        assert_eq!(output.matches("\x1b]4;").count(), 256);
        assert_eq!(&output[prefix_len..prefix_len + 2], "hi");
        assert!(output[prefix_len + 2..].starts_with("\x1b[0m"));
        assert!(output.ends_with("\x1b[3;5H"));
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_modes() {
        let mut terminal = terminal_with_lines(&["hi"]);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_mode_for_tests(Mode::Wraparound, false);

        let output = formatter(&terminal, PageOutputFormat::Vt).format();

        assert_eq!(output, "hi");
        assert!(!output.contains("\x1b[?2004h"));
        assert!(!output.contains("\x1b[?7l"));
    }

    #[test]
    fn terminal_formatter_vt_modes_extra_emits_only_differences_before_content() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.set_mode_for_tests(Mode::SendReceiveMode, false);
        terminal.set_mode_for_tests(Mode::CursorKeys, true);
        terminal.set_mode_for_tests(Mode::Wraparound, false);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_modes_extra())
            .format();

        assert_eq!(output, "\x1b[4h\x1b[12l\x1b[?1h\x1b[?7l\x1b[?2004hhello");
        assert!(output.find("\x1b[4h").unwrap() < output.find("\x1b[12l").unwrap());
        assert!(output.find("\x1b[12l").unwrap() < output.find("\x1b[?1h").unwrap());
        assert!(output.find("\x1b[?1h").unwrap() < output.find("\x1b[?7l").unwrap());
        assert!(output.find("\x1b[?7l").unwrap() < output.find("\x1b[?2004h").unwrap());
        assert!(output.find("\x1b[?2004h").unwrap() < output.find("hello").unwrap());
    }

    #[test]
    fn terminal_formatter_vt_modes_extra_ignores_default_true_modes_at_default() {
        let terminal = terminal_with_lines(&["hello"]);

        assert!(terminal.get_mode_for_tests(Mode::SendReceiveMode));
        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_modes_extra())
            .format();

        assert_eq!(output, "hello");
        assert!(!output.contains("\x1b[12"));
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_modes_extra() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let modes_output = formatter(&terminal, emit)
                .with_extra(terminal_modes_extra())
                .format();

            assert_eq!(modes_output, default_output);
            assert!(!modes_output.contains("\x1b[4h"));
            assert!(!modes_output.contains("\x1b[?2004h"));
        }
    }

    #[test]
    fn terminal_formatter_modes_extra_without_content_emits_for_vt_only() {
        let mut terminal = terminal_with_lines(&["ignored"]);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);

        let vt = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_modes_extra())
            .format();
        let html = formatter(&terminal, PageOutputFormat::Html)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_modes_extra())
            .format();
        let plain = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_modes_extra())
            .format();

        assert_eq!(vt, "\x1b[?2004h");
        assert_eq!(html, "");
        assert_eq!(plain, "");
    }

    #[test]
    fn terminal_formatter_modes_pin_map_uses_top_left_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_modes_extra())
            .format_with_pin_map();
        let prefix_len = modes_prefix_len(&terminal);

        assert_eq!(output.text, "\x1b[?2004héB");
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in &output.pin_map[..prefix_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        assert_eq!(
            &output.pin_map[prefix_len..],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
    }

    #[test]
    fn terminal_formatter_palette_and_modes_pin_map_order_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_palette_modes_extra())
            .format_with_pin_map();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(output.text.len(), output.pin_map.len());
        assert!(output.text.starts_with("\x1b]4;0;rgb:12/34/56\x1b\\"));
        assert_eq!(
            &output.text[palette_len..palette_len + modes_len],
            "\x1b[?2004h"
        );
        assert!(output.text.ends_with("éB"));
        for pin in &output.pin_map[..palette_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        for pin in &output.pin_map[palette_len..palette_len + modes_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        assert_eq!(
            &output.pin_map[palette_len + modes_len..],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
    }

    #[test]
    fn terminal_formatter_palette_modes_content_and_screen_extras_order() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras()),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 2],
            "hi"
        );
        assert!(output[palette_len + modes_len + 2..].starts_with("\x1b[0m"));
        assert!(output.ends_with("\x1b[3;5H"));
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_scrolling_region_or_change_pin_map() {
        let mut terminal = terminal_with_lines(&["hello", "world", "again"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 4);

        let default_text = formatter(&terminal, PageOutputFormat::Vt).format();
        let default_pin_map = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_text = screen_formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_pin_map =
            screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert_eq!(terminal.scrolling_region_for_tests().top, 1);
        assert_eq!(default_text, screen_text);
        assert_eq!(default_text, "hello\r\nworld\r\nagain");
        assert_eq!(default_pin_map, screen_pin_map);
    }

    #[test]
    fn terminal_formatter_scrolling_region_full_screen_emits_nothing() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert_eq!(output, "hello\r\nworld");
        assert_eq!(scrolling_region_suffix_len(&terminal), 0);
    }

    #[test]
    fn terminal_formatter_scrolling_region_vertical_only_emits_decstbm_after_content() {
        let mut terminal = terminal_with_lines(&["one", "two", "three", "four"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert_eq!(output, "one\r\ntwo\r\nthree\r\nfour\x1b[2;3r");
    }

    #[test]
    fn terminal_formatter_scrolling_region_horizontal_only_emits_decslrm_after_content() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert_eq!(output, "hello\r\nworld\x1b[2;4s");
    }

    #[test]
    fn terminal_formatter_scrolling_region_combined_emits_decstbm_then_decslrm() {
        let mut terminal = terminal_with_lines(&["hello", "world", "again"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 4);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert!(output.ends_with("\x1b[2;3r\x1b[2;5s"));
        assert!(output.find("\x1b[2;3r").unwrap() < output.find("\x1b[2;5s").unwrap());
        assert!(output.find("again").unwrap() < output.find("\x1b[2;3r").unwrap());
    }

    #[test]
    fn terminal_formatter_scrolling_region_emits_after_forwarded_screen_extras() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .screen(all_screen_extras())
                    .scrolling_region(true),
            )
            .format();

        assert!(output.contains("hey\r\nyou\x1b[0m"));
        assert!(output.ends_with("\x1b[1;2s"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
    }

    #[test]
    fn terminal_formatter_palette_modes_screen_extras_and_scrolling_region_order() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras())
                    .scrolling_region(true),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 8],
            "hey\r\nyou"
        );
        assert!(output[palette_len + modes_len + 8..].starts_with("\x1b[0m"));
        assert!(output.ends_with("\x1b[1;2s"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_scrolling_region_extra() {
        let mut terminal = terminal_with_lines(&["<hi", "row"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let region_output = formatter(&terminal, emit)
                .with_extra(terminal_scrolling_region_extra())
                .format();

            assert_eq!(region_output, default_output);
            assert!(!region_output.contains("\x1b["));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_without_content_maps_to_top_left() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_scrolling_region_extra())
            .format_with_pin_map();

        assert_eq!(output.text, "\x1b[2;4s");
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_pin_map_uses_last_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_scrolling_region_extra())
            .format_with_pin_map();
        let suffix_len = scrolling_region_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(output.text, "éB\x1b[2;3s");
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_rejects_invalid_test_regions() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);

        let invalid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            terminal.set_scrolling_region_for_tests(1, 0, 0, 4);
        }));

        assert!(invalid.is_err());
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_tabstops_or_change_pin_map() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);

        let default_text = formatter(&terminal, PageOutputFormat::Vt).format();
        let default_pin_map = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_text = screen_formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_pin_map =
            screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert!(terminal.get_tabstop_for_tests(1));
        assert_eq!(default_text, screen_text);
        assert_eq!(default_text, "hello");
        assert_eq!(default_pin_map, screen_pin_map);
    }

    #[test]
    fn terminal_formatter_tabstops_default_interval_emits_clear_and_ascending_hts() {
        let terminal = terminal_with_lines(&["0123456789abcdefghi"]);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_tabstops_extra())
            .format();

        assert_eq!(
            output,
            "0123456789abcdefghi\x1b[3g\x1b[9G\x1bH\x1b[17G\x1bH"
        );
    }

    #[test]
    fn terminal_formatter_tabstops_custom_columns_emit_one_indexed_columns() {
        let mut terminal = terminal_with_lines(&["0123456789012345678901234567890"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(29);
        terminal.set_tabstop_for_tests(4);
        terminal.set_tabstop_for_tests(14);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_tabstops_extra())
            .format();

        assert_eq!(output, "\x1b[3g\x1b[5G\x1bH\x1b[15G\x1bH\x1b[30G\x1bH");
    }

    #[test]
    fn terminal_formatter_tabstops_empty_state_emits_only_clear_all() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.clear_tabstops_for_tests();
        terminal.clear_tabstop_for_tests(1);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_tabstops_extra())
            .format();

        assert_eq!(output, "\x1b[3g");
        assert!(!terminal.get_tabstop_for_tests(1));
    }

    #[test]
    fn terminal_formatter_tabstops_emit_after_content_and_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .screen(all_screen_extras())
                    .tabstops(true),
            )
            .format();

        assert!(output.contains("hi\x1b[0m"));
        assert!(output.ends_with("\x1b[3g\x1b[2G\x1bH"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[3g").unwrap());
    }

    #[test]
    fn terminal_formatter_tabstops_emit_after_scrolling_region() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(4);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true),
            )
            .format();

        assert_eq!(output, "hello\r\nworld\x1b[2;4s\x1b[3g\x1b[5G\x1bH");
    }

    #[test]
    fn terminal_formatter_all_suffix_extras_keep_upstream_order() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras())
                    .scrolling_region(true)
                    .tabstops(true),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 8],
            "hey\r\nyou"
        );
        assert!(output[palette_len + modes_len + 8..].starts_with("\x1b[0m"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
        assert!(output.find("\x1b[1;2s").unwrap() < output.find("\x1b[3g").unwrap());
        assert!(output.ends_with("\x1b[3g\x1b[2G\x1bH"));
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_tabstops_extra() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let tabstops_output = formatter(&terminal, emit)
                .with_extra(terminal_tabstops_extra())
                .format();

            assert_eq!(tabstops_output, default_output);
            assert!(!tabstops_output.contains("\x1b[3g"));
        }
    }

    #[test]
    fn terminal_formatter_tabstops_without_content_maps_to_top_left() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_tabstops_extra())
            .format_with_pin_map();

        assert_eq!(output.text, "\x1b[3g\x1b[2G\x1bH");
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_tabstops_pin_map_uses_last_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_tabstops_extra())
            .format_with_pin_map();
        let suffix_len = tabstops_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(output.text, "éB\x1b[3g\x1b[2G\x1bH");
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_and_tabstops_pin_map_share_final_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true),
            )
            .format_with_pin_map();
        let suffix_len = scrolling_region_suffix_len(&terminal) + tabstops_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(output.text, "éB\x1b[2;3s\x1b[3g\x1b[2G\x1bH");
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_keyboard_or_pwd_or_change_pin_map() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        let default_text = formatter(&terminal, PageOutputFormat::Vt).format();
        let default_pin_map = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_text = screen_formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_pin_map =
            screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert!(terminal.modify_other_keys_2_for_tests());
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home/user"));
        assert_eq!(default_text, screen_text);
        assert_eq!(default_text, "hello");
        assert_eq!(default_pin_map, screen_pin_map);
    }

    #[test]
    fn terminal_formatter_keyboard_extra_emits_modify_other_keys_2_only_when_enabled() {
        let mut terminal = terminal_with_lines(&["hello"]);

        let disabled = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().keyboard(true))
            .format();
        assert_eq!(disabled, "");

        terminal.set_modify_other_keys_2_for_tests(true);
        let enabled = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().keyboard(true))
            .format();

        assert_eq!(enabled, "\x1b[>4;2m");
    }

    #[test]
    fn terminal_formatter_pwd_extra_emits_raw_stored_bytes_with_nul_and_st() {
        let mut terminal = terminal_with_lines(&["hello"]);

        let empty = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().pwd(true))
            .format();
        assert_eq!(empty, "");

        terminal.set_pwd_for_tests("file://host/home/user");
        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().pwd(true))
            .format();

        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home/user"));
        assert_eq!(output.as_bytes(), b"\x1b]7;file://host/home/user\0\x1b\\");
    }

    #[test]
    fn terminal_formatter_keyboard_and_pwd_emit_after_tabstops() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(4);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true)
                    .keyboard(true)
                    .pwd(true),
            )
            .format();

        assert_eq!(
            output.as_bytes(),
            b"hello\r\nworld\x1b[2;4s\x1b[3g\x1b[5G\x1bH\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
    }

    #[test]
    fn terminal_formatter_all_terminal_extras_keep_upstream_order() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras())
                    .scrolling_region(true)
                    .tabstops(true)
                    .keyboard(true)
                    .pwd(true),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 8],
            "hey\r\nyou"
        );
        assert!(output[palette_len + modes_len + 8..].starts_with("\x1b[0m"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
        assert!(output.find("\x1b[1;2s").unwrap() < output.find("\x1b[3g").unwrap());
        assert!(output.find("\x1b[3g").unwrap() < output.find("\x1b[>4;2m").unwrap());
        assert!(output.find("\x1b[>4;2m").unwrap() < output.find("\x1b]7;").unwrap());
        assert!(output
            .as_bytes()
            .ends_with(b"\x1b[3g\x1b[2G\x1bH\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"));
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_keyboard_and_pwd_extras() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let keyboard_pwd_output = formatter(&terminal, emit)
                .with_extra(terminal_keyboard_pwd_extra())
                .format();

            assert_eq!(keyboard_pwd_output, default_output);
            assert!(!keyboard_pwd_output.contains("\x1b[>4;2m"));
            assert!(!keyboard_pwd_output.contains("\x1b]7;"));
        }
    }

    #[test]
    fn terminal_formatter_keyboard_and_pwd_without_content_maps_to_top_left() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_keyboard_pwd_extra())
            .format_with_pin_map();

        assert_eq!(
            output.text.as_bytes(),
            b"\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_keyboard_and_pwd_pin_map_uses_last_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_keyboard_pwd_extra())
            .format_with_pin_map();
        let suffix_len = keyboard_pwd_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(
            output.text.as_bytes(),
            b"\xc3\xa9B\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_prior_suffixes_keyboard_and_pwd_pin_map_share_final_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true)
                    .keyboard(true)
                    .pwd(true),
            )
            .format_with_pin_map();
        let suffix_len = scrolling_region_suffix_len(&terminal)
            + tabstops_suffix_len(&terminal)
            + keyboard_pwd_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(
            output.text.as_bytes(),
            b"\xc3\xa9B\x1b[2;3s\x1b[3g\x1b[2G\x1bH\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_forwards_screen_extras_to_vt_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_screen_extras())
            .format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(all_screen_extras())
            .format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output,
            "hi\x1b[0m\x1b[38;5;1m\x1b]8;id=idé;https://e.test/é\x1b\\\x1b[1\"q\x1b[=3;1u\x1b(0\x0e\x1b[3;5H"
        );
    }

    #[test]
    fn terminal_formatter_forwards_screen_extras_with_no_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_screen_extras())
            .format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(all_screen_extras())
            .format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output,
            "\x1b[0m\x1b[38;5;1m\x1b]8;id=idé;https://e.test/é\x1b\\\x1b[1\"q\x1b[=3;1u\x1b(0\x0e\x1b[3;5H"
        );
    }

    #[test]
    fn terminal_formatter_forwards_screen_extra_pin_maps_with_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_screen_extras())
            .format_with_pin_map();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(all_screen_extras())
            .format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
        assert!(terminal_output.text.chars().count() < terminal_output.text.len());
        assert_eq!(terminal_output.pin_map[0], active_pin(&terminal, 0, 0));
        assert_eq!(terminal_output.pin_map[1], active_pin(&terminal, 1, 0));
        for pin in &terminal_output.pin_map[2..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 0));
        }
    }

    #[test]
    fn terminal_formatter_forwards_screen_extra_pin_maps_without_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_screen_extras())
            .format_with_pin_map();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(all_screen_extras())
            .format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
        for pin in terminal_output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_forwarded_screen_extras_follow_screen_formatter_for_plain_and_html() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        set_active_screen_extras(&mut terminal);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let terminal_output = formatter(&terminal, emit)
                .with_extra(terminal_screen_extras())
                .format();
            let screen_output = screen_formatter(&terminal, emit)
                .with_extra(all_screen_extras())
                .format();
            let default_output = formatter(&terminal, emit).format();

            assert_eq!(terminal_output, screen_output);
            assert_eq!(terminal_output, default_output);
        }
    }

    #[test]
    fn terminal_formatter_invalid_or_garbage_selection_returns_empty_output_and_map() {
        let terminal = terminal_with_lines(&["hello"]);
        let other = terminal_with_lines(&["other"]);
        let valid = active_pin(&terminal, 0, 0);
        let invalid = active_pin(&other, 0, 0);
        let mut garbage = valid;
        garbage.mark_garbage_for_tests();

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            let actual = formatter(&terminal, PageOutputFormat::Plain)
                .with_content(ScreenFormatterContent::Selection(Some(selection)))
                .format_with_pin_map();
            assert_eq!(
                actual,
                PageStringWithPinMap {
                    text: String::new(),
                    pin_map: Vec::new(),
                }
            );
        }
    }
}
