//! Tmux protocol helpers.

use std::collections::VecDeque;

use super::terminal::Terminal;

const DEFAULT_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControlParser {
    state: State,
    buffer: Vec<u8>,
    max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ControlNotification {
    Enter,
    Exit,
    BlockEnd(Vec<u8>),
    BlockErr(Vec<u8>),
    Output {
        pane_id: usize,
        data: String,
    },
    SessionChanged {
        id: usize,
        name: String,
    },
    SessionsChanged,
    LayoutChange {
        window_id: usize,
        layout: String,
        visible_layout: String,
        raw_flags: String,
    },
    WindowAdd {
        id: usize,
    },
    WindowRenamed {
        id: usize,
        name: String,
    },
    WindowPaneChanged {
        window_id: usize,
        pane_id: usize,
    },
    ClientDetached {
        client: String,
    },
    ClientSessionChanged {
        client: String,
        session_id: usize,
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlParseError {
    OutOfMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) content: LayoutContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LayoutContent {
    Pane(usize),
    Horizontal(Vec<Layout>),
    Vertical(Vec<Layout>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutParseError {
    SyntaxError,
    ChecksumMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputVariable {
    AlternateOn,
    AlternateSavedX,
    AlternateSavedY,
    BracketedPaste,
    CursorBlinking,
    CursorColour,
    CursorFlag,
    CursorShape,
    CursorX,
    CursorY,
    FocusFlag,
    InsertFlag,
    KeypadCursorFlag,
    KeypadFlag,
    MouseAllFlag,
    MouseAnyFlag,
    MouseButtonFlag,
    MouseSgrFlag,
    MouseStandardFlag,
    MouseUtf8Flag,
    OriginFlag,
    PaneId,
    PaneTabs,
    ScrollRegionLower,
    ScrollRegionUpper,
    SessionId,
    Version,
    WindowId,
    WindowWidth,
    WindowHeight,
    WindowLayout,
    WrapFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutputValue {
    Bool(bool),
    Number(usize),
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputParseError {
    MissingEntry,
    ExtraEntry,
    FormatError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TmuxScreenKey {
    Primary,
    Alternate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TmuxCapturePane {
    pub(crate) id: usize,
    pub(crate) screen_key: TmuxScreenKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TmuxCommand {
    ListWindows,
    PaneHistory(TmuxCapturePane),
    PaneVisible(TmuxCapturePane),
    PaneState,
    TmuxVersion,
    User(String),
}

#[derive(Debug)]
pub(crate) struct TmuxViewer {
    state: TmuxViewerState,
    session_id: usize,
    tmux_version: String,
    command_queue: VecDeque<TmuxCommand>,
    windows: Vec<TmuxWindow>,
    panes: Vec<TmuxPane>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TmuxWindow {
    pub(crate) id: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) layout: Layout,
}

#[derive(Debug)]
pub(crate) struct TmuxPane {
    id: usize,
    terminal: Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TmuxPaneSpec {
    id: usize,
    width: usize,
    height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TmuxViewerState {
    StartupBlock,
    StartupSession,
    CommandQueue,
    Defunct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TmuxViewerAction {
    Exit,
    Command(String),
    Windows(Vec<TmuxWindow>),
}

pub(crate) const LIST_PANES_DELIMITER: u8 = b';';
pub(crate) const LIST_WINDOWS_DELIMITER: u8 = b' ';
pub(crate) const TMUX_VERSION_DELIMITER: u8 = b' ';

pub(crate) const LIST_PANES_VARIABLES: &[OutputVariable] = &[
    OutputVariable::PaneId,
    OutputVariable::CursorX,
    OutputVariable::CursorY,
    OutputVariable::CursorFlag,
    OutputVariable::CursorShape,
    OutputVariable::CursorColour,
    OutputVariable::CursorBlinking,
    OutputVariable::AlternateOn,
    OutputVariable::AlternateSavedX,
    OutputVariable::AlternateSavedY,
    OutputVariable::InsertFlag,
    OutputVariable::WrapFlag,
    OutputVariable::KeypadFlag,
    OutputVariable::KeypadCursorFlag,
    OutputVariable::OriginFlag,
    OutputVariable::MouseAllFlag,
    OutputVariable::MouseAnyFlag,
    OutputVariable::MouseButtonFlag,
    OutputVariable::MouseStandardFlag,
    OutputVariable::MouseUtf8Flag,
    OutputVariable::MouseSgrFlag,
    OutputVariable::FocusFlag,
    OutputVariable::BracketedPaste,
    OutputVariable::ScrollRegionUpper,
    OutputVariable::ScrollRegionLower,
    OutputVariable::PaneTabs,
];

pub(crate) const LIST_WINDOWS_VARIABLES: &[OutputVariable] = &[
    OutputVariable::SessionId,
    OutputVariable::WindowId,
    OutputVariable::WindowWidth,
    OutputVariable::WindowHeight,
    OutputVariable::WindowLayout,
];

pub(crate) const TMUX_VERSION_VARIABLES: &[OutputVariable] = &[OutputVariable::Version];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayoutChecksum(u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Broken,
    Notification,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockTerminator {
    End,
    Err,
}

impl ControlParser {
    pub(crate) const fn new() -> Self {
        Self {
            state: State::Idle,
            buffer: Vec::new(),
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub(crate) const fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            state: State::Idle,
            buffer: Vec::new(),
            max_bytes,
        }
    }

    pub(crate) fn put(
        &mut self,
        byte: u8,
    ) -> Result<Option<ControlNotification>, ControlParseError> {
        if self.state == State::Broken {
            return Ok(None);
        }

        if self.buffer.len() >= self.max_bytes {
            self.broken();
            return Err(ControlParseError::OutOfMemory);
        }

        match self.state {
            State::Broken => unreachable!("broken state returns before match"),
            State::Idle => {
                if byte != b'%' {
                    self.broken();
                    return Ok(Some(ControlNotification::Exit));
                }

                self.buffer.clear();
                self.state = State::Notification;
            }
            State::Notification => {
                if byte == b'\n' {
                    return Ok(self.parse_notification());
                }
            }
            State::Block => {
                if byte == b'\n' {
                    let line_start = self
                        .buffer
                        .iter()
                        .rposition(|b| *b == b'\n')
                        .map_or(0, |index| index + 1);
                    let line = &self.buffer[line_start..];

                    if let Some(terminator) = parse_block_terminator(line) {
                        let output = trim_ascii_right(&self.buffer[..line_start], b"\r\n").to_vec();
                        self.state = State::Idle;
                        return Ok(Some(match terminator {
                            BlockTerminator::End => ControlNotification::BlockEnd(output),
                            BlockTerminator::Err => ControlNotification::BlockErr(output),
                        }));
                    }
                }
            }
        }

        self.buffer.push(byte);
        Ok(None)
    }

    fn parse_notification(&mut self) -> Option<ControlNotification> {
        let mut line = self.buffer.as_slice();
        if let Some(stripped) = line.strip_suffix(b"\r") {
            line = stripped;
        }

        if line == b"%begin" || line.starts_with(b"%begin ") {
            self.buffer.clear();
            self.state = State::Block;
            return None;
        }

        let notification = std::str::from_utf8(line)
            .ok()
            .and_then(parse_notification_line);

        self.buffer.clear();
        self.state = State::Idle;
        notification
    }

    fn broken(&mut self) {
        self.state = State::Broken;
        self.buffer.clear();
    }
}

impl Default for ControlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub(crate) fn parse_with_checksum(value: &str) -> Result<Self, LayoutParseError> {
        if value.len() < 5 {
            return Err(LayoutParseError::SyntaxError);
        }
        if value.as_bytes()[4] != b',' {
            return Err(LayoutParseError::SyntaxError);
        }

        let layout = &value[5..];
        if value.as_bytes()[..4] != LayoutChecksum::calculate(layout.as_bytes()).as_string() {
            return Err(LayoutParseError::ChecksumMismatch);
        }

        Self::parse(layout)
    }

    pub(crate) fn parse(value: &str) -> Result<Self, LayoutParseError> {
        let mut offset = 0;
        let layout = Self::parse_next(value, &mut offset)?;
        if offset != value.len() {
            return Err(LayoutParseError::SyntaxError);
        }
        Ok(layout)
    }

    fn parse_next(value: &str, offset: &mut usize) -> Result<Self, LayoutParseError> {
        let bytes = value.as_bytes();
        let width = parse_number_until(value, offset, b"x")?;
        let height = parse_number_until(value, offset, b",")?;
        let x = parse_number_until(value, offset, b",")?;
        let y = parse_number_until_without_consuming(value, offset, b",{[")?;

        let content = match bytes.get(*offset).copied() {
            Some(b',') => {
                *offset += 1;
                let start = *offset;
                while *offset < value.len() && !matches!(bytes[*offset], b',' | b'}' | b']') {
                    *offset += 1;
                }
                let pane_id = parse_usize_exact(&value[start..*offset])
                    .ok_or(LayoutParseError::SyntaxError)?;
                LayoutContent::Pane(pane_id)
            }
            Some(opening @ (b'{' | b'[')) => {
                *offset += 1;
                let mut children = Vec::new();

                loop {
                    children.push(Self::parse_next(value, offset)?);

                    if *offset >= value.len() {
                        return Err(LayoutParseError::SyntaxError);
                    }

                    if bytes[*offset] == b',' {
                        *offset += 1;
                        continue;
                    }

                    match opening {
                        b'{' if bytes[*offset] != b'}' => {
                            return Err(LayoutParseError::SyntaxError);
                        }
                        b'[' if bytes[*offset] != b']' => {
                            return Err(LayoutParseError::SyntaxError);
                        }
                        _ => {}
                    }

                    *offset += 1;
                    break match opening {
                        b'{' => LayoutContent::Horizontal(children),
                        b'[' => LayoutContent::Vertical(children),
                        _ => unreachable!("opening is constrained above"),
                    };
                }
            }
            _ => return Err(LayoutParseError::SyntaxError),
        };

        Ok(Self {
            width,
            height,
            x,
            y,
            content,
        })
    }
}

impl LayoutChecksum {
    pub(crate) fn calculate(value: &[u8]) -> Self {
        let mut result = 0u16;
        for byte in value {
            result = result.rotate_right(1);
            result = result.wrapping_add(u16::from(*byte));
        }
        Self(result)
    }

    pub(crate) fn as_string(self) -> [u8; 4] {
        const CHARSET: &[u8; 16] = b"0123456789abcdef";
        [
            CHARSET[usize::from((self.0 >> 12) & 0xf)],
            CHARSET[usize::from((self.0 >> 8) & 0xf)],
            CHARSET[usize::from((self.0 >> 4) & 0xf)],
            CHARSET[usize::from(self.0 & 0xf)],
        ]
    }
}

impl OutputVariable {
    pub(crate) fn parse_value(self, value: &str) -> Result<OutputValue, OutputParseError> {
        match self {
            Self::AlternateOn
            | Self::BracketedPaste
            | Self::CursorBlinking
            | Self::CursorFlag
            | Self::FocusFlag
            | Self::InsertFlag
            | Self::KeypadCursorFlag
            | Self::KeypadFlag
            | Self::MouseAllFlag
            | Self::MouseAnyFlag
            | Self::MouseButtonFlag
            | Self::MouseSgrFlag
            | Self::MouseStandardFlag
            | Self::MouseUtf8Flag
            | Self::OriginFlag
            | Self::WrapFlag => Ok(OutputValue::Bool(value == "1")),
            Self::AlternateSavedX
            | Self::AlternateSavedY
            | Self::CursorX
            | Self::CursorY
            | Self::ScrollRegionLower
            | Self::ScrollRegionUpper
            | Self::WindowWidth
            | Self::WindowHeight => parse_output_number(value),
            Self::SessionId => parse_prefixed_output_number(value, b'$'),
            Self::WindowId => parse_prefixed_output_number(value, b'@'),
            Self::PaneId => parse_prefixed_output_number(value, b'%'),
            Self::CursorColour
            | Self::CursorShape
            | Self::PaneTabs
            | Self::Version
            | Self::WindowLayout => Ok(OutputValue::Text(value.to_owned())),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::AlternateOn => "alternate_on",
            Self::AlternateSavedX => "alternate_saved_x",
            Self::AlternateSavedY => "alternate_saved_y",
            Self::BracketedPaste => "bracketed_paste",
            Self::CursorBlinking => "cursor_blinking",
            Self::CursorColour => "cursor_colour",
            Self::CursorFlag => "cursor_flag",
            Self::CursorShape => "cursor_shape",
            Self::CursorX => "cursor_x",
            Self::CursorY => "cursor_y",
            Self::FocusFlag => "focus_flag",
            Self::InsertFlag => "insert_flag",
            Self::KeypadCursorFlag => "keypad_cursor_flag",
            Self::KeypadFlag => "keypad_flag",
            Self::MouseAllFlag => "mouse_all_flag",
            Self::MouseAnyFlag => "mouse_any_flag",
            Self::MouseButtonFlag => "mouse_button_flag",
            Self::MouseSgrFlag => "mouse_sgr_flag",
            Self::MouseStandardFlag => "mouse_standard_flag",
            Self::MouseUtf8Flag => "mouse_utf8_flag",
            Self::OriginFlag => "origin_flag",
            Self::PaneId => "pane_id",
            Self::PaneTabs => "pane_tabs",
            Self::ScrollRegionLower => "scroll_region_lower",
            Self::ScrollRegionUpper => "scroll_region_upper",
            Self::SessionId => "session_id",
            Self::Version => "version",
            Self::WindowId => "window_id",
            Self::WindowWidth => "window_width",
            Self::WindowHeight => "window_height",
            Self::WindowLayout => "window_layout",
            Self::WrapFlag => "wrap_flag",
        }
    }
}

pub(crate) fn parse_output_values(
    variables: &[OutputVariable],
    text: &str,
    delimiter: u8,
) -> Result<Vec<OutputValue>, OutputParseError> {
    let mut parts = text.as_bytes().split(|byte| *byte == delimiter);
    let mut values = Vec::with_capacity(variables.len());

    for variable in variables {
        let part = parts.next().ok_or(OutputParseError::MissingEntry)?;
        let part = std::str::from_utf8(part).map_err(|_| OutputParseError::FormatError)?;
        values.push(variable.parse_value(part)?);
    }

    if parts.next().is_some() {
        return Err(OutputParseError::ExtraEntry);
    }

    Ok(values)
}

pub(crate) fn format_output_variables(variables: &[OutputVariable], delimiter: u8) -> String {
    assert!(delimiter.is_ascii(), "tmux output delimiters must be ASCII");

    let mut output = String::new();
    for (index, variable) in variables.iter().enumerate() {
        if index != 0 {
            output.push(char::from(delimiter));
        }
        output.push_str("#{");
        output.push_str(variable.name());
        output.push('}');
    }
    output
}

impl TmuxCommand {
    pub(crate) fn format_command(&self) -> String {
        match self {
            Self::ListWindows => format!(
                "list-windows -F '{}'\n",
                format_output_variables(LIST_WINDOWS_VARIABLES, LIST_WINDOWS_DELIMITER)
            ),
            Self::PaneHistory(capture) => format!(
                "capture-pane -p -e -q {}-S - -E -1 -t %{}\n",
                capture.alternate_flag(),
                capture.id
            ),
            Self::PaneVisible(capture) => format!(
                "capture-pane -p -e -q {}-t %{}\n",
                capture.alternate_flag(),
                capture.id
            ),
            Self::PaneState => format!(
                "list-panes -F '{}'\n",
                format_output_variables(LIST_PANES_VARIABLES, LIST_PANES_DELIMITER)
            ),
            Self::TmuxVersion => format!(
                "display-message -p '{}'\n",
                format_output_variables(TMUX_VERSION_VARIABLES, TMUX_VERSION_DELIMITER)
            ),
            Self::User(command) => command.clone(),
        }
    }
}

impl TmuxCapturePane {
    fn alternate_flag(self) -> &'static str {
        match self.screen_key {
            TmuxScreenKey::Primary => "",
            TmuxScreenKey::Alternate => "-a ",
        }
    }
}

impl TmuxViewer {
    pub(crate) fn new() -> Self {
        Self {
            state: TmuxViewerState::StartupBlock,
            session_id: 0,
            tmux_version: String::new(),
            command_queue: VecDeque::new(),
            windows: Vec::new(),
            panes: Vec::new(),
        }
    }

    pub(crate) fn next(&mut self, notification: ControlNotification) -> Vec<TmuxViewerAction> {
        match self.state {
            TmuxViewerState::Defunct => Vec::new(),
            TmuxViewerState::StartupBlock => self.next_startup_block(notification),
            TmuxViewerState::StartupSession => self.next_startup_session(notification),
            TmuxViewerState::CommandQueue => self.next_command_queue(notification),
        }
    }

    fn next_startup_block(&mut self, notification: ControlNotification) -> Vec<TmuxViewerAction> {
        match notification {
            ControlNotification::Exit => self.defunct(),
            ControlNotification::BlockEnd(_) | ControlNotification::BlockErr(_) => {
                self.state = TmuxViewerState::StartupSession;
                Vec::new()
            }
            ControlNotification::Enter => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn next_startup_session(&mut self, notification: ControlNotification) -> Vec<TmuxViewerAction> {
        match notification {
            ControlNotification::Exit => self.defunct(),
            ControlNotification::SessionChanged { id, .. } => {
                self.session_id = id;
                self.enter_command_queue([TmuxCommand::TmuxVersion, TmuxCommand::ListWindows])
            }
            ControlNotification::Enter => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn next_command_queue(&mut self, notification: ControlNotification) -> Vec<TmuxViewerAction> {
        match notification {
            ControlNotification::Exit => self.defunct(),
            ControlNotification::BlockEnd(content) => self.received_command_output(content),
            ControlNotification::BlockErr(content) => self.received_command_output(content),
            ControlNotification::SessionChanged { id, .. } => self.session_changed(id),
            ControlNotification::WindowAdd { .. } => {
                self.queue_commands([TmuxCommand::ListWindows])
            }
            ControlNotification::LayoutChange {
                window_id, layout, ..
            } => self.layout_changed(window_id, &layout),
            ControlNotification::Enter => Vec::new(),
            ControlNotification::Output { .. }
            | ControlNotification::WindowRenamed { .. }
            | ControlNotification::WindowPaneChanged { .. }
            | ControlNotification::SessionsChanged
            | ControlNotification::ClientDetached { .. }
            | ControlNotification::ClientSessionChanged { .. } => Vec::new(),
        }
    }

    fn enter_command_queue<const N: usize>(
        &mut self,
        commands: [TmuxCommand; N],
    ) -> Vec<TmuxViewerAction> {
        debug_assert!(self.state != TmuxViewerState::CommandQueue);
        debug_assert!(N > 0);

        let first = commands[0].format_command();
        self.command_queue.extend(commands);
        self.state = TmuxViewerState::CommandQueue;
        vec![TmuxViewerAction::Command(first)]
    }

    fn queue_commands<const N: usize>(
        &mut self,
        commands: [TmuxCommand; N],
    ) -> Vec<TmuxViewerAction> {
        debug_assert!(self.state == TmuxViewerState::CommandQueue);

        let was_empty = self.command_queue.is_empty();
        self.command_queue.extend(commands);
        if was_empty {
            self.emit_next_queued_command()
        } else {
            Vec::new()
        }
    }

    fn session_changed(&mut self, session_id: usize) -> Vec<TmuxViewerAction> {
        self.session_id = session_id;
        self.windows.clear();
        self.panes.clear();
        self.command_queue.clear();

        let mut actions = vec![TmuxViewerAction::Windows(Vec::new())];
        actions.extend(self.queue_commands([TmuxCommand::ListWindows]));
        actions
    }

    fn layout_changed(&mut self, window_id: usize, layout: &str) -> Vec<TmuxViewerAction> {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.id == window_id)
        else {
            return Vec::new();
        };

        let Ok(layout) = Layout::parse_with_checksum(layout) else {
            return self.defunct();
        };

        self.windows[index].layout = layout;
        let mut actions = vec![TmuxViewerAction::Windows(self.windows.clone())];
        let was_empty = self.command_queue.is_empty();
        if !self.sync_layouts() {
            return self.defunct();
        }
        if was_empty {
            actions.extend(self.emit_next_queued_command());
        }
        actions
    }

    fn received_command_output(&mut self, content: Vec<u8>) -> Vec<TmuxViewerAction> {
        let Some(command) = self.command_queue.pop_front() else {
            return Vec::new();
        };

        let mut actions = match command {
            TmuxCommand::TmuxVersion => {
                if !self.received_tmux_version(&content) {
                    return self.defunct();
                }
                Vec::new()
            }
            TmuxCommand::ListWindows => {
                let Some(windows) = self.received_list_windows(&content) else {
                    return self.defunct();
                };
                if !self.sync_layouts() {
                    return self.defunct();
                }
                vec![TmuxViewerAction::Windows(windows)]
            }
            TmuxCommand::PaneHistory(_)
            | TmuxCommand::PaneVisible(_)
            | TmuxCommand::PaneState
            | TmuxCommand::User(_) => Vec::new(),
        };

        actions.extend(self.emit_next_queued_command());
        actions
    }

    fn received_list_windows(&mut self, content: &[u8]) -> Option<Vec<TmuxWindow>> {
        let content = std::str::from_utf8(content).ok()?;
        let mut windows = Vec::new();

        for line in content.split('\n') {
            let line = line.trim_matches([' ', '\t', '\r']);
            if line.is_empty() {
                continue;
            }
            windows.push(parse_list_window(line)?);
        }

        self.windows = windows;
        Some(self.windows.clone())
    }

    fn sync_layouts(&mut self) -> bool {
        let mut pane_specs = Vec::new();
        for window in &self.windows {
            collect_layout_panes(&window.layout, &mut pane_specs);
        }

        let added: Vec<TmuxPaneSpec> = pane_specs
            .iter()
            .copied()
            .filter(|pane| !self.panes.iter().any(|existing| existing.id == pane.id))
            .collect();

        let mut old_panes = std::mem::take(&mut self.panes);
        let mut panes = Vec::with_capacity(pane_specs.len());
        for pane_spec in pane_specs {
            if let Some(index) = old_panes.iter().position(|pane| pane.id == pane_spec.id) {
                panes.push(old_panes.remove(index));
            } else {
                let Some(pane) = TmuxPane::new(pane_spec) else {
                    return false;
                };
                panes.push(pane);
            }
        }
        self.panes = panes;

        for pane_id in &added {
            self.command_queue
                .push_back(TmuxCommand::PaneHistory(TmuxCapturePane {
                    id: pane_id.id,
                    screen_key: TmuxScreenKey::Primary,
                }));
            self.command_queue
                .push_back(TmuxCommand::PaneVisible(TmuxCapturePane {
                    id: pane_id.id,
                    screen_key: TmuxScreenKey::Primary,
                }));
            self.command_queue
                .push_back(TmuxCommand::PaneHistory(TmuxCapturePane {
                    id: pane_id.id,
                    screen_key: TmuxScreenKey::Alternate,
                }));
            self.command_queue
                .push_back(TmuxCommand::PaneVisible(TmuxCapturePane {
                    id: pane_id.id,
                    screen_key: TmuxScreenKey::Alternate,
                }));
        }

        if !added.is_empty() {
            self.command_queue.push_back(TmuxCommand::PaneState);
        }

        true
    }

    fn received_tmux_version(&mut self, content: &[u8]) -> bool {
        let Ok(content) = std::str::from_utf8(content) else {
            return false;
        };
        let line = content.trim_matches([' ', '\t', '\r', '\n']);
        if line.is_empty() {
            return true;
        }

        let Ok(mut values) =
            parse_output_values(TMUX_VERSION_VARIABLES, line, TMUX_VERSION_DELIMITER)
        else {
            return false;
        };
        let Some(OutputValue::Text(version)) = values.pop() else {
            return false;
        };
        self.tmux_version = version;
        true
    }

    fn emit_next_queued_command(&self) -> Vec<TmuxViewerAction> {
        self.command_queue
            .front()
            .map(|command| vec![TmuxViewerAction::Command(command.format_command())])
            .unwrap_or_default()
    }

    fn defunct(&mut self) -> Vec<TmuxViewerAction> {
        self.state = TmuxViewerState::Defunct;
        vec![TmuxViewerAction::Exit]
    }

    #[cfg(test)]
    fn state(&self) -> TmuxViewerState {
        self.state
    }

    #[cfg(test)]
    fn session_id(&self) -> usize {
        self.session_id
    }

    #[cfg(test)]
    fn tmux_version(&self) -> &str {
        &self.tmux_version
    }

    #[cfg(test)]
    fn queue_len(&self) -> usize {
        self.command_queue.len()
    }

    #[cfg(test)]
    fn windows(&self) -> &[TmuxWindow] {
        &self.windows
    }

    #[cfg(test)]
    fn pane_ids(&self) -> Vec<usize> {
        self.panes.iter().map(|pane| pane.id).collect()
    }

    #[cfg(test)]
    fn pane_dimensions(&self, id: usize) -> Option<(u16, u16)> {
        self.panes
            .iter()
            .find(|pane| pane.id == id)
            .map(|pane| (pane.terminal.columns(), pane.terminal.rows()))
    }

    #[cfg(test)]
    fn set_panes_for_tests(&mut self, pane_specs: &[(usize, u16, u16)]) {
        self.panes = pane_specs
            .iter()
            .map(|(id, cols, rows)| TmuxPane {
                id: *id,
                terminal: Terminal::init(*cols, *rows, None).unwrap(),
            })
            .collect();
    }

    #[cfg(test)]
    fn queue_command_for_tests(&mut self, command: TmuxCommand) {
        self.state = TmuxViewerState::CommandQueue;
        self.command_queue.push_back(command);
    }
}

impl Default for TmuxViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl TmuxPane {
    fn new(spec: TmuxPaneSpec) -> Option<Self> {
        let cols = u16::try_from(spec.width).ok()?;
        let rows = u16::try_from(spec.height).ok()?;
        let terminal = Terminal::init(cols, rows, None).ok()?;
        Some(Self {
            id: spec.id,
            terminal,
        })
    }
}

fn parse_notification_line(line: &str) -> Option<ControlNotification> {
    let cmd = line.split_once(' ').map_or(line, |(cmd, _)| cmd);

    match cmd {
        "%output" => {
            let rest = line.strip_prefix("%output %")?;
            let (pane_id, data) = parse_usize_field(rest)?;
            non_empty(data).map(|data| ControlNotification::Output {
                pane_id,
                data: data.to_owned(),
            })
        }
        "%session-changed" => {
            let rest = line.strip_prefix("%session-changed $")?;
            let (id, name) = parse_usize_field(rest)?;
            non_empty(name).map(|name| ControlNotification::SessionChanged {
                id,
                name: name.to_owned(),
            })
        }
        "%sessions-changed" => {
            (line == "%sessions-changed").then_some(ControlNotification::SessionsChanged)
        }
        "%layout-change" => {
            let rest = line.strip_prefix("%layout-change @")?;
            let (window_id, rest) = parse_usize_field(rest)?;
            let (layout, rest) = parse_non_empty_field(rest)?;
            let (visible_layout, raw_flags) = parse_non_empty_field(rest)?;
            Some(ControlNotification::LayoutChange {
                window_id,
                layout: layout.to_owned(),
                visible_layout: visible_layout.to_owned(),
                raw_flags: raw_flags.to_owned(),
            })
        }
        "%window-add" => {
            let rest = line.strip_prefix("%window-add @")?;
            parse_usize_exact(rest).map(|id| ControlNotification::WindowAdd { id })
        }
        "%window-renamed" => {
            let rest = line.strip_prefix("%window-renamed @")?;
            let (id, name) = parse_usize_field(rest)?;
            non_empty(name).map(|name| ControlNotification::WindowRenamed {
                id,
                name: name.to_owned(),
            })
        }
        "%window-pane-changed" => {
            let rest = line.strip_prefix("%window-pane-changed @")?;
            let (window_id, rest) = parse_usize_field(rest)?;
            let pane_id = rest.strip_prefix('%').and_then(parse_usize_exact)?;
            Some(ControlNotification::WindowPaneChanged { window_id, pane_id })
        }
        "%client-detached" => {
            let client = line.strip_prefix("%client-detached ")?;
            non_empty(client).map(|client| ControlNotification::ClientDetached {
                client: client.to_owned(),
            })
        }
        "%client-session-changed" => {
            let rest = line.strip_prefix("%client-session-changed ")?;
            let (client, rest) = rest.rsplit_once(" $")?;
            let (session_id, name) = parse_usize_field(rest)?;
            non_empty(name).map(|name| ControlNotification::ClientSessionChanged {
                client: client.to_owned(),
                session_id,
                name: name.to_owned(),
            })
        }
        _ => None,
    }
}

fn parse_list_window(line: &str) -> Option<TmuxWindow> {
    let mut values =
        parse_output_values(LIST_WINDOWS_VARIABLES, line, LIST_WINDOWS_DELIMITER).ok()?;

    if values.len() != LIST_WINDOWS_VARIABLES.len() {
        return None;
    }

    let OutputValue::Text(layout) = values.pop()? else {
        return None;
    };
    let OutputValue::Number(height) = values.pop()? else {
        return None;
    };
    let OutputValue::Number(width) = values.pop()? else {
        return None;
    };
    let OutputValue::Number(id) = values.pop()? else {
        return None;
    };
    let OutputValue::Number(_session_id) = values.pop()? else {
        return None;
    };

    Some(TmuxWindow {
        id,
        width,
        height,
        layout: Layout::parse_with_checksum(&layout).ok()?,
    })
}

fn collect_layout_panes(layout: &Layout, panes: &mut Vec<TmuxPaneSpec>) {
    match &layout.content {
        LayoutContent::Pane(id) => {
            if !panes.iter().any(|pane| pane.id == *id) {
                panes.push(TmuxPaneSpec {
                    id: *id,
                    width: layout.width,
                    height: layout.height,
                });
            }
        }
        LayoutContent::Horizontal(children) | LayoutContent::Vertical(children) => {
            for child in children {
                collect_layout_panes(child, panes);
            }
        }
    }
}

fn parse_block_terminator(mut line: &[u8]) -> Option<BlockTerminator> {
    if let Some(stripped) = line.strip_suffix(b"\r") {
        line = stripped;
    }

    let line = std::str::from_utf8(line).ok()?;
    let mut fields = line.split(' ').filter(|field| !field.is_empty());
    let terminator = match fields.next()? {
        "%end" => BlockTerminator::End,
        "%error" => BlockTerminator::Err,
        _ => return None,
    };

    parse_usize_exact(fields.next()?)?;
    parse_usize_exact(fields.next()?)?;
    parse_usize_exact(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }

    Some(terminator)
}

fn parse_usize_exact(value: &str) -> Option<usize> {
    if value.is_empty() || !value.as_bytes().iter().all(u8::is_ascii_digit) {
        return None;
    }
    value.parse().ok()
}

fn parse_output_number(value: &str) -> Result<OutputValue, OutputParseError> {
    parse_usize_exact(value)
        .map(OutputValue::Number)
        .ok_or(OutputParseError::FormatError)
}

fn parse_prefixed_output_number(value: &str, prefix: u8) -> Result<OutputValue, OutputParseError> {
    let bytes = value.as_bytes();
    if bytes.len() < 2 || bytes[0] != prefix {
        return Err(OutputParseError::FormatError);
    }
    parse_output_number(&value[1..])
}

fn parse_usize_field(value: &str) -> Option<(usize, &str)> {
    let (digits, rest) = value.split_once(' ')?;
    Some((parse_usize_exact(digits)?, rest))
}

fn parse_number_until(
    value: &str,
    offset: &mut usize,
    delimiters: &[u8],
) -> Result<usize, LayoutParseError> {
    let index = find_any(value, *offset, delimiters).ok_or(LayoutParseError::SyntaxError)?;
    let result = parse_usize_exact(&value[*offset..index]).ok_or(LayoutParseError::SyntaxError)?;
    *offset = index + 1;
    Ok(result)
}

fn parse_number_until_without_consuming(
    value: &str,
    offset: &mut usize,
    delimiters: &[u8],
) -> Result<usize, LayoutParseError> {
    let index = find_any(value, *offset, delimiters).ok_or(LayoutParseError::SyntaxError)?;
    let result = parse_usize_exact(&value[*offset..index]).ok_or(LayoutParseError::SyntaxError)?;
    *offset = index;
    Ok(result)
}

fn find_any(value: &str, offset: usize, bytes: &[u8]) -> Option<usize> {
    value.as_bytes()[offset..]
        .iter()
        .position(|byte| bytes.contains(byte))
        .map(|index| offset + index)
}

fn parse_non_empty_field(value: &str) -> Option<(&str, &str)> {
    let (field, rest) = value.split_once(' ')?;
    non_empty(field).map(|field| (field, rest))
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn trim_ascii_right<'a>(mut value: &'a [u8], bytes: &[u8]) -> &'a [u8] {
    while value.last().is_some_and(|byte| bytes.contains(byte)) {
        value = &value[..value.len() - 1];
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(parser: &mut ControlParser, data: &[u8]) {
        for byte in data {
            assert_eq!(parser.put(*byte), Ok(None));
        }
    }

    fn pane(layout: &Layout) -> usize {
        let LayoutContent::Pane(pane_id) = layout.content else {
            panic!("expected pane layout");
        };
        pane_id
    }

    fn horizontal(layout: &Layout) -> &[Layout] {
        let LayoutContent::Horizontal(children) = &layout.content else {
            panic!("expected horizontal layout");
        };
        children
    }

    fn vertical(layout: &Layout) -> &[Layout] {
        let LayoutContent::Vertical(children) = &layout.content else {
            panic!("expected vertical layout");
        };
        children
    }

    #[test]
    fn tmux_layout_simple_single_pane() {
        let layout = Layout::parse("80x24,0,0,42").unwrap();
        assert_eq!(layout.width, 80);
        assert_eq!(layout.height, 24);
        assert_eq!(layout.x, 0);
        assert_eq!(layout.y, 0);
        assert_eq!(pane(&layout), 42);
    }

    #[test]
    fn tmux_layout_single_pane_with_offset() {
        let layout = Layout::parse("40x12,10,5,7").unwrap();
        assert_eq!(layout.width, 40);
        assert_eq!(layout.height, 12);
        assert_eq!(layout.x, 10);
        assert_eq!(layout.y, 5);
        assert_eq!(pane(&layout), 7);
    }

    #[test]
    fn tmux_layout_single_pane_large_values() {
        let layout = Layout::parse("1920x1080,100,200,999").unwrap();
        assert_eq!(layout.width, 1920);
        assert_eq!(layout.height, 1080);
        assert_eq!(layout.x, 100);
        assert_eq!(layout.y, 200);
        assert_eq!(pane(&layout), 999);
    }

    #[test]
    fn tmux_layout_horizontal_split_two_panes() {
        let layout = Layout::parse("80x24,0,0{40x24,0,0,1,40x24,40,0,2}").unwrap();
        assert_eq!(layout.width, 80);
        assert_eq!(layout.height, 24);

        let children = horizontal(&layout);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].width, 40);
        assert_eq!(children[0].height, 24);
        assert_eq!(children[0].x, 0);
        assert_eq!(children[0].y, 0);
        assert_eq!(pane(&children[0]), 1);
        assert_eq!(children[1].width, 40);
        assert_eq!(children[1].height, 24);
        assert_eq!(children[1].x, 40);
        assert_eq!(children[1].y, 0);
        assert_eq!(pane(&children[1]), 2);
    }

    #[test]
    fn tmux_layout_vertical_split_two_panes() {
        let layout = Layout::parse("80x24,0,0[80x12,0,0,1,80x12,0,12,2]").unwrap();
        assert_eq!(layout.width, 80);
        assert_eq!(layout.height, 24);

        let children = vertical(&layout);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].width, 80);
        assert_eq!(children[0].height, 12);
        assert_eq!(children[0].x, 0);
        assert_eq!(children[0].y, 0);
        assert_eq!(pane(&children[0]), 1);
        assert_eq!(children[1].width, 80);
        assert_eq!(children[1].height, 12);
        assert_eq!(children[1].x, 0);
        assert_eq!(children[1].y, 12);
        assert_eq!(pane(&children[1]), 2);
    }

    #[test]
    fn tmux_layout_horizontal_split_three_panes() {
        let layout = Layout::parse("120x24,0,0{40x24,0,0,1,40x24,40,0,2,40x24,80,0,3}").unwrap();
        let children = horizontal(&layout);
        assert_eq!(children.len(), 3);
        assert_eq!(pane(&children[0]), 1);
        assert_eq!(pane(&children[1]), 2);
        assert_eq!(pane(&children[2]), 3);
    }

    #[test]
    fn tmux_layout_nested_horizontal_in_vertical() {
        let layout =
            Layout::parse("80x24,0,0[80x12,0,0,1,80x12,0,12{40x12,0,12,2,40x12,40,12,3}]").unwrap();
        let children = vertical(&layout);
        assert_eq!(children.len(), 2);
        assert_eq!(pane(&children[0]), 1);

        let nested = horizontal(&children[1]);
        assert_eq!(nested.len(), 2);
        assert_eq!(pane(&nested[0]), 2);
        assert_eq!(pane(&nested[1]), 3);
    }

    #[test]
    fn tmux_layout_nested_vertical_in_horizontal() {
        let layout =
            Layout::parse("80x24,0,0{40x24,0,0,1,40x24,40,0[40x12,40,0,2,40x12,40,12,3]}").unwrap();
        let children = horizontal(&layout);
        assert_eq!(children.len(), 2);
        assert_eq!(pane(&children[0]), 1);

        let nested = vertical(&children[1]);
        assert_eq!(nested.len(), 2);
        assert_eq!(pane(&nested[0]), 2);
        assert_eq!(pane(&nested[1]), 3);
    }

    #[test]
    fn tmux_layout_deeply_nested_layout() {
        let layout =
            Layout::parse("80x24,0,0{40x24,0,0[40x12,0,0,1,40x12,0,12,2],40x24,40,0,3}").unwrap();
        let children = horizontal(&layout);
        assert_eq!(children.len(), 2);
        let nested = vertical(&children[0]);
        assert_eq!(nested.len(), 2);
        assert_eq!(pane(&nested[0]), 1);
        assert_eq!(pane(&nested[1]), 2);
        assert_eq!(pane(&children[1]), 3);
    }

    #[test]
    fn tmux_layout_syntax_errors() {
        for value in [
            "",
            "x24,0,0,1",
            "80x,0,0,1",
            "80x24,,0,1",
            "80x24,0,,1",
            "80x24,0,0,",
            "abcx24,0,0,1",
            "80x24,0,0,abc",
            "80x24,0,0{40x24,0,0,1",
            "80x24,0,0[40x24,0,0,1",
            "80x24,0,0{40x24,0,0,1]",
            "80x24,0,0[40x24,0,0,1}",
            "80x24,0,0,1extra",
            "8024,0,0,1",
            "80x24,0,0",
        ] {
            assert_eq!(Layout::parse(value), Err(LayoutParseError::SyntaxError));
        }
    }

    #[test]
    fn tmux_layout_parse_with_checksum_valid() {
        let layout =
            Layout::parse_with_checksum("f8f9,80x24,0,0{40x24,0,0,1,40x24,40,0,2}").unwrap();
        assert_eq!(layout.width, 80);
        assert_eq!(layout.height, 24);
    }

    #[test]
    fn tmux_layout_parse_with_checksum_mismatch() {
        assert_eq!(
            Layout::parse_with_checksum("0000,80x24,0,0{40x24,0,0,1,40x24,40,0,2}"),
            Err(LayoutParseError::ChecksumMismatch)
        );
        assert_eq!(
            Layout::parse_with_checksum("F8F9,80x24,0,0{40x24,0,0,1,40x24,40,0,2}"),
            Err(LayoutParseError::ChecksumMismatch)
        );
        assert_eq!(
            Layout::parse_with_checksum("zzzz,80x24,0,0{40x24,0,0,1,40x24,40,0,2}"),
            Err(LayoutParseError::ChecksumMismatch)
        );
    }

    #[test]
    fn tmux_layout_parse_with_checksum_syntax_errors() {
        assert_eq!(
            Layout::parse_with_checksum("bb62"),
            Err(LayoutParseError::SyntaxError)
        );
        assert_eq!(
            Layout::parse_with_checksum(""),
            Err(LayoutParseError::SyntaxError)
        );
        assert_eq!(
            Layout::parse_with_checksum("bb62x159x48,0,0"),
            Err(LayoutParseError::SyntaxError)
        );
    }

    #[test]
    fn tmux_layout_checksum_empty_string() {
        let checksum = LayoutChecksum::calculate(b"");
        assert_eq!(checksum.0, 0);
        assert_eq!(checksum.as_string(), *b"0000");
    }

    #[test]
    fn tmux_layout_checksum_single_character() {
        let checksum = LayoutChecksum::calculate(b"A");
        assert_eq!(checksum.0, 65);
        assert_eq!(checksum.as_string(), *b"0041");
    }

    #[test]
    fn tmux_layout_checksum_two_characters() {
        let checksum = LayoutChecksum::calculate(b"AB");
        assert_eq!(checksum.0, 32866);
        assert_eq!(checksum.as_string(), *b"8062");
    }

    #[test]
    fn tmux_layout_checksum_known_layouts() {
        assert_eq!(
            LayoutChecksum::calculate(b"80x24,0,0,42").as_string(),
            *b"d962"
        );
        assert_eq!(
            LayoutChecksum::calculate(b"80x24,0,0{40x24,0,0,1,40x24,40,0,2}").as_string(),
            *b"f8f9"
        );
    }

    #[test]
    fn tmux_layout_checksum_as_string_values() {
        assert_eq!(LayoutChecksum(0x000f).as_string(), *b"000f");
        assert_eq!(LayoutChecksum(0x1234).as_string(), *b"1234");
        assert_eq!(LayoutChecksum(0xabcd).as_string(), *b"abcd");
        assert_eq!(LayoutChecksum(0xffff).as_string(), *b"ffff");
    }

    #[test]
    fn tmux_layout_checksum_wraparound() {
        let checksum = LayoutChecksum::calculate(b"\xff\xff\xff\xff\xff\xff\xff\xff");
        assert_eq!(checksum.as_string(), *b"03fc");
    }

    #[test]
    fn tmux_layout_checksum_deterministic() {
        let value = b"80x24,0,0{40x24,0,0,1,40x24,40,0,2}";
        assert_eq!(
            LayoutChecksum::calculate(value),
            LayoutChecksum::calculate(value)
        );
    }

    #[test]
    fn tmux_layout_checksum_different_inputs_different_outputs() {
        assert_ne!(
            LayoutChecksum::calculate(b"80x24,0,0,1"),
            LayoutChecksum::calculate(b"80x24,0,0,2")
        );
    }

    #[test]
    fn tmux_layout_checksum_known_tmux_layout_bb62() {
        assert_eq!(
            LayoutChecksum::calculate(b"159x48,0,0{79x48,0,0,79x48,80,0}").as_string(),
            *b"bb62"
        );
    }

    #[test]
    fn tmux_output_parse_bool_variables() {
        for variable in [
            OutputVariable::AlternateOn,
            OutputVariable::BracketedPaste,
            OutputVariable::CursorBlinking,
            OutputVariable::CursorFlag,
            OutputVariable::FocusFlag,
            OutputVariable::InsertFlag,
            OutputVariable::KeypadCursorFlag,
            OutputVariable::KeypadFlag,
            OutputVariable::MouseAllFlag,
            OutputVariable::MouseAnyFlag,
            OutputVariable::MouseButtonFlag,
            OutputVariable::MouseSgrFlag,
            OutputVariable::MouseStandardFlag,
            OutputVariable::MouseUtf8Flag,
            OutputVariable::OriginFlag,
            OutputVariable::WrapFlag,
        ] {
            assert_eq!(variable.parse_value("1"), Ok(OutputValue::Bool(true)));
            assert_eq!(variable.parse_value("0"), Ok(OutputValue::Bool(false)));
            assert_eq!(variable.parse_value(""), Ok(OutputValue::Bool(false)));
            assert_eq!(variable.parse_value("true"), Ok(OutputValue::Bool(false)));
        }
    }

    #[test]
    fn tmux_output_parse_number_variables() {
        for variable in [
            OutputVariable::AlternateSavedX,
            OutputVariable::AlternateSavedY,
            OutputVariable::CursorX,
            OutputVariable::CursorY,
            OutputVariable::ScrollRegionLower,
            OutputVariable::ScrollRegionUpper,
            OutputVariable::WindowWidth,
            OutputVariable::WindowHeight,
        ] {
            assert_eq!(variable.parse_value("0"), Ok(OutputValue::Number(0)));
            assert_eq!(variable.parse_value("42"), Ok(OutputValue::Number(42)));
            assert_eq!(
                variable.parse_value("abc"),
                Err(OutputParseError::FormatError)
            );
            assert_eq!(
                variable.parse_value("-1"),
                Err(OutputParseError::FormatError)
            );
        }
    }

    #[test]
    fn tmux_output_parse_prefixed_ids() {
        assert_eq!(
            OutputVariable::SessionId.parse_value("$42"),
            Ok(OutputValue::Number(42))
        );
        assert_eq!(
            OutputVariable::SessionId.parse_value("42"),
            Err(OutputParseError::FormatError)
        );
        assert_eq!(
            OutputVariable::SessionId.parse_value("$"),
            Err(OutputParseError::FormatError)
        );
        assert_eq!(
            OutputVariable::SessionId.parse_value("$abc"),
            Err(OutputParseError::FormatError)
        );

        assert_eq!(
            OutputVariable::WindowId.parse_value("@12345"),
            Ok(OutputValue::Number(12345))
        );
        assert_eq!(
            OutputVariable::WindowId.parse_value("$0"),
            Err(OutputParseError::FormatError)
        );

        assert_eq!(
            OutputVariable::PaneId.parse_value("%0"),
            Ok(OutputValue::Number(0))
        );
        assert_eq!(
            OutputVariable::PaneId.parse_value("@0"),
            Err(OutputParseError::FormatError)
        );
    }

    #[test]
    fn tmux_output_parse_text_variables() {
        for (variable, value) in [
            (OutputVariable::CursorColour, "red"),
            (OutputVariable::CursorShape, "underline"),
            (OutputVariable::PaneTabs, "0,8,16,24"),
            (OutputVariable::Version, "3.5a"),
            (OutputVariable::WindowLayout, "a]b,c{d}e(f)"),
        ] {
            assert_eq!(
                variable.parse_value(value),
                Ok(OutputValue::Text(value.to_owned()))
            );
            assert_eq!(
                variable.parse_value(""),
                Ok(OutputValue::Text(String::new()))
            );
        }
    }

    #[test]
    fn tmux_output_parse_values_single_field() {
        assert_eq!(
            parse_output_values(&[OutputVariable::SessionId], "$42", b' '),
            Ok(vec![OutputValue::Number(42)])
        );
    }

    #[test]
    fn tmux_output_parse_values_multiple_fields() {
        assert_eq!(
            parse_output_values(
                &[
                    OutputVariable::SessionId,
                    OutputVariable::WindowId,
                    OutputVariable::WindowWidth,
                    OutputVariable::WindowHeight,
                ],
                "$1 @2 80 24",
                b' ',
            ),
            Ok(vec![
                OutputValue::Number(1),
                OutputValue::Number(2),
                OutputValue::Number(80),
                OutputValue::Number(24),
            ])
        );
    }

    #[test]
    fn tmux_output_parse_values_with_string_field() {
        assert_eq!(
            parse_output_values(
                &[OutputVariable::WindowId, OutputVariable::WindowLayout],
                "@5,abc123",
                b',',
            ),
            Ok(vec![
                OutputValue::Number(5),
                OutputValue::Text("abc123".to_owned()),
            ])
        );
    }

    #[test]
    fn tmux_output_parse_values_different_delimiter() {
        assert_eq!(
            parse_output_values(
                &[OutputVariable::WindowWidth, OutputVariable::WindowHeight],
                "120\t40",
                b'\t',
            ),
            Ok(vec![OutputValue::Number(120), OutputValue::Number(40)])
        );
    }

    #[test]
    fn tmux_output_parse_values_missing_entry() {
        assert_eq!(
            parse_output_values(
                &[OutputVariable::SessionId, OutputVariable::WindowId],
                "$1",
                b' ',
            ),
            Err(OutputParseError::MissingEntry)
        );
    }

    #[test]
    fn tmux_output_parse_values_extra_entry() {
        assert_eq!(
            parse_output_values(&[OutputVariable::SessionId], "$1 @2", b' '),
            Err(OutputParseError::ExtraEntry)
        );
        assert_eq!(
            parse_output_values(&[OutputVariable::SessionId], "$1 ", b' '),
            Err(OutputParseError::ExtraEntry)
        );
    }

    #[test]
    fn tmux_output_parse_values_format_error() {
        for value in ["42", "@42", "$abc", ""] {
            assert_eq!(
                parse_output_values(&[OutputVariable::SessionId], value, b' '),
                Err(OutputParseError::FormatError)
            );
        }
    }

    #[test]
    fn tmux_output_parse_values_preserves_empty_layout_field() {
        assert_eq!(
            parse_output_values(
                &[OutputVariable::SessionId, OutputVariable::WindowLayout],
                "$1,",
                b',',
            ),
            Ok(vec![
                OutputValue::Number(1),
                OutputValue::Text(String::new())
            ])
        );
    }

    #[test]
    fn tmux_output_format_variables() {
        assert_eq!(format_output_variables(&[], b' '), "");
        assert_eq!(
            format_output_variables(&[OutputVariable::SessionId], b' '),
            "#{session_id}"
        );
        assert_eq!(
            format_output_variables(
                &[
                    OutputVariable::SessionId,
                    OutputVariable::WindowId,
                    OutputVariable::WindowWidth,
                    OutputVariable::WindowHeight,
                ],
                b' ',
            ),
            "#{session_id} #{window_id} #{window_width} #{window_height}"
        );
        assert_eq!(
            format_output_variables(
                &[OutputVariable::WindowId, OutputVariable::WindowLayout],
                b',',
            ),
            "#{window_id},#{window_layout}"
        );
        assert_eq!(
            format_output_variables(
                &[OutputVariable::WindowWidth, OutputVariable::WindowHeight],
                b'\t',
            ),
            "#{window_width}\t#{window_height}"
        );
    }

    #[test]
    fn tmux_output_format_representative_all_variables() {
        assert_eq!(
            format_output_variables(
                &[
                    OutputVariable::SessionId,
                    OutputVariable::WindowId,
                    OutputVariable::WindowWidth,
                    OutputVariable::WindowHeight,
                    OutputVariable::WindowLayout,
                ],
                b' ',
            ),
            "#{session_id} #{window_id} #{window_width} #{window_height} #{window_layout}"
        );
    }

    #[test]
    fn tmux_command_format_constants_match_upstream_lists() {
        assert_eq!(
            LIST_WINDOWS_VARIABLES,
            &[
                OutputVariable::SessionId,
                OutputVariable::WindowId,
                OutputVariable::WindowWidth,
                OutputVariable::WindowHeight,
                OutputVariable::WindowLayout,
            ]
        );
        assert_eq!(TMUX_VERSION_VARIABLES, &[OutputVariable::Version]);
        assert_eq!(
            LIST_PANES_VARIABLES,
            &[
                OutputVariable::PaneId,
                OutputVariable::CursorX,
                OutputVariable::CursorY,
                OutputVariable::CursorFlag,
                OutputVariable::CursorShape,
                OutputVariable::CursorColour,
                OutputVariable::CursorBlinking,
                OutputVariable::AlternateOn,
                OutputVariable::AlternateSavedX,
                OutputVariable::AlternateSavedY,
                OutputVariable::InsertFlag,
                OutputVariable::WrapFlag,
                OutputVariable::KeypadFlag,
                OutputVariable::KeypadCursorFlag,
                OutputVariable::OriginFlag,
                OutputVariable::MouseAllFlag,
                OutputVariable::MouseAnyFlag,
                OutputVariable::MouseButtonFlag,
                OutputVariable::MouseStandardFlag,
                OutputVariable::MouseUtf8Flag,
                OutputVariable::MouseSgrFlag,
                OutputVariable::FocusFlag,
                OutputVariable::BracketedPaste,
                OutputVariable::ScrollRegionUpper,
                OutputVariable::ScrollRegionLower,
                OutputVariable::PaneTabs,
            ]
        );
    }

    #[test]
    fn tmux_command_format_list_windows() {
        assert_eq!(
            TmuxCommand::ListWindows.format_command(),
            "list-windows -F '#{session_id} #{window_id} #{window_width} #{window_height} #{window_layout}'\n"
        );
    }

    #[test]
    fn tmux_command_format_pane_history_primary_and_alternate() {
        assert_eq!(
            TmuxCommand::PaneHistory(TmuxCapturePane {
                id: 42,
                screen_key: TmuxScreenKey::Primary,
            })
            .format_command(),
            "capture-pane -p -e -q -S - -E -1 -t %42\n"
        );
        assert_eq!(
            TmuxCommand::PaneHistory(TmuxCapturePane {
                id: 42,
                screen_key: TmuxScreenKey::Alternate,
            })
            .format_command(),
            "capture-pane -p -e -q -a -S - -E -1 -t %42\n"
        );
    }

    #[test]
    fn tmux_command_format_pane_visible_primary_and_alternate() {
        assert_eq!(
            TmuxCommand::PaneVisible(TmuxCapturePane {
                id: 42,
                screen_key: TmuxScreenKey::Primary,
            })
            .format_command(),
            "capture-pane -p -e -q -t %42\n"
        );
        assert_eq!(
            TmuxCommand::PaneVisible(TmuxCapturePane {
                id: 42,
                screen_key: TmuxScreenKey::Alternate,
            })
            .format_command(),
            "capture-pane -p -e -q -a -t %42\n"
        );
    }

    #[test]
    fn tmux_command_format_pane_state() {
        assert_eq!(
            TmuxCommand::PaneState.format_command(),
            "list-panes -F '#{pane_id};#{cursor_x};#{cursor_y};#{cursor_flag};#{cursor_shape};#{cursor_colour};#{cursor_blinking};#{alternate_on};#{alternate_saved_x};#{alternate_saved_y};#{insert_flag};#{wrap_flag};#{keypad_flag};#{keypad_cursor_flag};#{origin_flag};#{mouse_all_flag};#{mouse_any_flag};#{mouse_button_flag};#{mouse_standard_flag};#{mouse_utf8_flag};#{mouse_sgr_flag};#{focus_flag};#{bracketed_paste};#{scroll_region_upper};#{scroll_region_lower};#{pane_tabs}'\n"
        );
    }

    #[test]
    fn tmux_command_format_tmux_version() {
        assert_eq!(
            TmuxCommand::TmuxVersion.format_command(),
            "display-message -p '#{version}'\n"
        );
    }

    #[test]
    fn tmux_command_format_user_passthrough() {
        assert_eq!(
            TmuxCommand::User("display-message hello".to_owned()).format_command(),
            "display-message hello"
        );
        assert_eq!(
            TmuxCommand::User("display-message hello\n".to_owned()).format_command(),
            "display-message hello\n"
        );
    }

    #[test]
    fn tmux_viewer_immediate_exit_and_defunct_ignores_later_input() {
        let mut viewer = TmuxViewer::new();

        assert_eq!(
            viewer.next(ControlNotification::Exit),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
        assert_eq!(viewer.next(ControlNotification::Exit), Vec::new());
    }

    #[test]
    fn tmux_viewer_startup_block_end_and_err_enter_startup_session() {
        for notification in [
            ControlNotification::BlockEnd(Vec::new()),
            ControlNotification::BlockErr(Vec::new()),
        ] {
            let mut viewer = TmuxViewer::new();

            assert_eq!(viewer.next(notification), Vec::new());
            assert_eq!(viewer.state(), TmuxViewerState::StartupSession);
        }
    }

    #[test]
    fn tmux_viewer_ignores_enter_and_non_startup_notifications() {
        let mut viewer = TmuxViewer::new();

        assert_eq!(viewer.next(ControlNotification::Enter), Vec::new());
        assert_eq!(
            viewer.next(ControlNotification::SessionsChanged),
            Vec::new()
        );
        assert_eq!(viewer.state(), TmuxViewerState::StartupBlock);
    }

    #[test]
    fn tmux_viewer_exit_from_startup_session() {
        let mut viewer = TmuxViewer::new();

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(Vec::new())),
            Vec::new()
        );
        assert_eq!(
            viewer.next(ControlNotification::Exit),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
    }

    #[test]
    fn tmux_viewer_session_changed_emits_tmux_version_command() {
        let mut viewer = TmuxViewer::new();

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(Vec::new())),
            Vec::new()
        );
        assert_eq!(
            viewer.next(ControlNotification::SessionChanged {
                id: 42,
                name: "main".to_owned(),
            }),
            vec![TmuxViewerAction::Command(
                "display-message -p '#{version}'\n".to_owned()
            )]
        );
        assert_eq!(viewer.state(), TmuxViewerState::CommandQueue);
        assert_eq!(viewer.session_id(), 42);
        assert_eq!(viewer.queue_len(), 2);
    }

    #[test]
    fn tmux_viewer_version_output_stores_version_and_emits_list_windows() {
        let mut viewer = TmuxViewer::new();

        viewer.next(ControlNotification::BlockEnd(Vec::new()));
        viewer.next(ControlNotification::SessionChanged {
            id: 42,
            name: "main".to_owned(),
        });

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b" 3.5a\r\n".to_vec())),
            vec![TmuxViewerAction::Command(
                "list-windows -F '#{session_id} #{window_id} #{window_width} #{window_height} #{window_layout}'\n"
                    .to_owned()
            )]
        );
        assert_eq!(viewer.tmux_version(), "3.5a");
        assert_eq!(viewer.queue_len(), 1);
    }

    #[test]
    fn tmux_viewer_empty_version_output_keeps_existing_version() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::TmuxVersion);
        viewer.tmux_version = "3.5a".to_owned();

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b" \t\r\n".to_vec())),
            Vec::new()
        );
        assert_eq!(viewer.tmux_version(), "3.5a");
        assert_eq!(viewer.queue_len(), 0);
    }

    #[test]
    fn tmux_viewer_startup_flow_parses_single_window_output() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("80x24,0,0{40x24,0,0,1,40x24,40,0,2}");
        let expected = vec![TmuxWindow {
            id: 2,
            width: 80,
            height: 24,
            layout: Layout::parse_with_checksum(&layout).unwrap(),
        }];

        viewer.next(ControlNotification::BlockEnd(Vec::new()));
        viewer.next(ControlNotification::SessionChanged {
            id: 42,
            name: "main".to_owned(),
        });
        viewer.next(ControlNotification::BlockEnd(b"3.5a".to_vec()));

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(
                format!("$1 @2 80 24 {layout}").into_bytes()
            )),
            vec![
                TmuxViewerAction::Windows(expected.clone()),
                TmuxViewerAction::Command(
                    TmuxCommand::PaneHistory(TmuxCapturePane {
                        id: 1,
                        screen_key: TmuxScreenKey::Primary,
                    })
                    .format_command()
                ),
            ]
        );
        assert_eq!(viewer.queue_len(), 9);
        assert_eq!(viewer.windows(), expected.as_slice());
        assert_eq!(viewer.pane_ids(), &[1, 2]);
        assert_eq!(viewer.pane_dimensions(1), Some((40, 24)));
        assert_eq!(viewer.pane_dimensions(2), Some((40, 24)));
        assert_eq!(viewer.state(), TmuxViewerState::CommandQueue);
    }

    #[test]
    fn tmux_viewer_command_queue_block_err_consumes_and_emits_next() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::TmuxVersion);
        viewer.command_queue.push_back(TmuxCommand::ListWindows);

        assert_eq!(
            viewer.next(ControlNotification::BlockErr(b"3.5a".to_vec())),
            vec![TmuxViewerAction::Command(
                "list-windows -F '#{session_id} #{window_id} #{window_width} #{window_height} #{window_layout}'\n"
                    .to_owned()
            )]
        );
        assert_eq!(viewer.tmux_version(), "3.5a");
        assert_eq!(viewer.queue_len(), 1);
    }

    #[test]
    fn tmux_viewer_malformed_version_output_defuncts() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::TmuxVersion);
        viewer.command_queue.push_back(TmuxCommand::ListWindows);

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b"bad version".to_vec())),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
        assert_eq!(viewer.tmux_version(), "");
    }

    #[test]
    fn tmux_viewer_list_windows_parses_multiple_windows_and_blank_lines() {
        let mut viewer = TmuxViewer::new();
        let layout_one = checked_layout("80x24,0,0,42");
        let layout_two = checked_layout("100x30,0,0,43");
        let expected = vec![
            TmuxWindow {
                id: 2,
                width: 80,
                height: 24,
                layout: Layout::parse_with_checksum(&layout_one).unwrap(),
            },
            TmuxWindow {
                id: 3,
                width: 100,
                height: 30,
                layout: Layout::parse_with_checksum(&layout_two).unwrap(),
            },
        ];

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        let actions = viewer.next(ControlNotification::BlockEnd(
            format!("$1 @2 80 24 {layout_one}\n\t$1 @3 100 30 {layout_two}\r\n\n").into_bytes(),
        ));

        assert_eq!(
            actions,
            vec![
                TmuxViewerAction::Windows(expected.clone()),
                TmuxViewerAction::Command(
                    TmuxCommand::PaneHistory(TmuxCapturePane {
                        id: 42,
                        screen_key: TmuxScreenKey::Primary,
                    })
                    .format_command()
                ),
            ]
        );
        assert_eq!(viewer.windows(), expected.as_slice());
        assert_eq!(viewer.pane_ids(), &[42, 43]);
        assert_eq!(viewer.pane_dimensions(42), Some((80, 24)));
        assert_eq!(viewer.pane_dimensions(43), Some((100, 30)));
    }

    #[test]
    fn tmux_viewer_empty_list_windows_output_clears_windows() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("80x24,0,0,42");
        viewer.windows = vec![TmuxWindow {
            id: 2,
            width: 80,
            height: 24,
            layout: Layout::parse_with_checksum(&layout).unwrap(),
        }];

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b" \t\r\n\n".to_vec())),
            vec![TmuxViewerAction::Windows(Vec::new())]
        );
        assert!(viewer.windows().is_empty());
        assert!(viewer.pane_ids().is_empty());
    }

    #[test]
    fn tmux_viewer_malformed_list_windows_output_defuncts() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b"$1 @2 80".to_vec())),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
    }

    #[test]
    fn tmux_viewer_invalid_list_windows_layout_defuncts() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(
                b"$1 @2 80 24 0000,80x24,0,0,42".to_vec()
            )),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
    }

    #[test]
    fn tmux_viewer_oversized_pane_dimensions_defunct() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("70000x24,0,0,42");

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(
                format!("$1 @2 70000 24 {layout}").into_bytes()
            )),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
    }

    #[test]
    fn tmux_viewer_list_windows_output_emits_next_queued_command() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("80x24,0,0,42");
        let expected_window = TmuxWindow {
            id: 2,
            width: 80,
            height: 24,
            layout: Layout::parse_with_checksum(&layout).unwrap(),
        };

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        viewer
            .command_queue
            .push_back(TmuxCommand::User("send-prefix\n".to_owned()));

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(
                format!("$1 @2 80 24 {layout}").into_bytes()
            )),
            vec![
                TmuxViewerAction::Windows(vec![expected_window]),
                TmuxViewerAction::Command("send-prefix\n".to_owned()),
            ]
        );
    }

    #[test]
    fn tmux_viewer_new_panes_queue_capture_commands_in_layout_order() {
        let mut viewer = TmuxViewer::new();
        let layout =
            checked_layout("120x24,0,0{40x24,0,0,10,80x24,40,0[80x12,40,0,11,80x12,40,12,12]}");

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        viewer.next(ControlNotification::BlockEnd(
            format!("$1 @2 120 24 {layout}").into_bytes(),
        ));

        assert_eq!(viewer.pane_ids(), &[10, 11, 12]);
        assert_eq!(viewer.pane_dimensions(10), Some((40, 24)));
        assert_eq!(viewer.pane_dimensions(11), Some((80, 12)));
        assert_eq!(viewer.pane_dimensions(12), Some((80, 12)));
        assert_eq!(
            viewer.command_queue.iter().collect::<Vec<_>>(),
            expected_new_pane_commands(&[10, 11, 12])
                .iter()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn tmux_viewer_existing_panes_do_not_queue_duplicate_captures() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("80x24,0,0{40x24,0,0,42,40x24,40,0,43}");
        viewer.set_panes_for_tests(&[(42, 40, 24), (43, 40, 24)]);

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(
                format!("$1 @2 80 24 {layout}").into_bytes()
            )),
            vec![TmuxViewerAction::Windows(vec![test_window(
                2,
                80,
                24,
                "80x24,0,0{40x24,0,0,42,40x24,40,0,43}"
            )])]
        );
        assert_eq!(viewer.pane_ids(), &[42, 43]);
        assert_eq!(viewer.pane_dimensions(42), Some((40, 24)));
        assert_eq!(viewer.pane_dimensions(43), Some((40, 24)));
        assert!(viewer.command_queue.is_empty());
    }

    #[test]
    fn tmux_viewer_removed_panes_are_pruned() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("80x24,0,0,42");
        viewer.set_panes_for_tests(&[(42, 80, 24), (43, 40, 24)]);

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        viewer.next(ControlNotification::BlockEnd(
            format!("$1 @2 80 24 {layout}").into_bytes(),
        ));

        assert_eq!(viewer.pane_ids(), &[42]);
        assert_eq!(viewer.pane_dimensions(42), Some((80, 24)));
        assert!(viewer.command_queue.is_empty());
    }

    #[test]
    fn tmux_viewer_duplicate_layout_pane_ids_are_tracked_once() {
        let mut viewer = TmuxViewer::new();
        let layout = checked_layout("80x24,0,0{40x24,0,0,42,40x24,40,0,42}");

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        viewer.next(ControlNotification::BlockEnd(
            format!("$1 @2 80 24 {layout}").into_bytes(),
        ));

        assert_eq!(viewer.pane_ids(), &[42]);
        assert_eq!(viewer.pane_dimensions(42), Some((40, 24)));
        assert_eq!(
            viewer.command_queue.iter().collect::<Vec<_>>(),
            expected_new_pane_commands(&[42]).iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn tmux_viewer_session_changed_refreshes_windows_and_preserves_version() {
        let mut viewer = TmuxViewer::new();
        viewer.state = TmuxViewerState::CommandQueue;
        viewer.session_id = 7;
        viewer.tmux_version = "3.5a".to_owned();
        viewer.windows = vec![test_window(2, 80, 24, "80x24,0,0,42")];
        viewer.set_panes_for_tests(&[(42, 80, 24)]);

        assert_eq!(
            viewer.next(ControlNotification::SessionChanged {
                id: 9,
                name: "other".to_owned(),
            }),
            vec![
                TmuxViewerAction::Windows(Vec::new()),
                TmuxViewerAction::Command(TmuxCommand::ListWindows.format_command()),
            ]
        );
        assert_eq!(viewer.session_id(), 9);
        assert_eq!(viewer.tmux_version(), "3.5a");
        assert!(viewer.windows().is_empty());
        assert!(viewer.pane_ids().is_empty());
        assert_eq!(viewer.queue_len(), 1);
    }

    #[test]
    fn tmux_viewer_session_changed_clears_pending_commands() {
        let mut viewer = TmuxViewer::new();
        viewer.queue_command_for_tests(TmuxCommand::TmuxVersion);
        viewer
            .command_queue
            .push_back(TmuxCommand::User("send-prefix\n".to_owned()));

        assert_eq!(
            viewer.next(ControlNotification::SessionChanged {
                id: 9,
                name: "other".to_owned(),
            }),
            vec![
                TmuxViewerAction::Windows(Vec::new()),
                TmuxViewerAction::Command(TmuxCommand::ListWindows.format_command()),
            ]
        );
        assert_eq!(viewer.queue_len(), 1);
        assert_eq!(
            viewer.command_queue.front(),
            Some(&TmuxCommand::ListWindows)
        );
    }

    #[test]
    fn tmux_viewer_window_add_with_empty_queue_emits_list_windows() {
        let mut viewer = TmuxViewer::new();
        viewer.state = TmuxViewerState::CommandQueue;

        assert_eq!(
            viewer.next(ControlNotification::WindowAdd { id: 2 }),
            vec![TmuxViewerAction::Command(
                TmuxCommand::ListWindows.format_command()
            )]
        );
        assert_eq!(viewer.queue_len(), 1);
    }

    #[test]
    fn tmux_viewer_window_add_with_in_flight_command_waits_to_emit() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::TmuxVersion);
        assert_eq!(
            viewer.next(ControlNotification::WindowAdd { id: 2 }),
            Vec::new()
        );
        assert_eq!(viewer.queue_len(), 2);

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b"3.5a".to_vec())),
            vec![TmuxViewerAction::Command(
                TmuxCommand::ListWindows.format_command()
            )]
        );
        assert_eq!(viewer.queue_len(), 1);
    }

    #[test]
    fn tmux_viewer_ignored_command_queue_notifications_do_not_change_state() {
        let ignored = vec![
            ControlNotification::WindowRenamed {
                id: 2,
                name: "renamed".to_owned(),
            },
            ControlNotification::WindowPaneChanged {
                window_id: 2,
                pane_id: 42,
            },
            ControlNotification::SessionsChanged,
            ControlNotification::ClientDetached {
                client: "client".to_owned(),
            },
            ControlNotification::ClientSessionChanged {
                client: "client".to_owned(),
                session_id: 9,
                name: "other".to_owned(),
            },
            ControlNotification::Output {
                pane_id: 42,
                data: "output".to_owned(),
            },
        ];

        for notification in ignored {
            let mut viewer = TmuxViewer::new();
            let windows = vec![test_window(2, 80, 24, "80x24,0,0,42")];
            viewer.state = TmuxViewerState::CommandQueue;
            viewer.session_id = 7;
            viewer.tmux_version = "3.5a".to_owned();
            viewer.windows = windows.clone();
            viewer.command_queue.push_back(TmuxCommand::ListWindows);

            assert_eq!(viewer.next(notification), Vec::new());
            assert_eq!(viewer.state(), TmuxViewerState::CommandQueue);
            assert_eq!(viewer.session_id(), 7);
            assert_eq!(viewer.tmux_version(), "3.5a");
            assert_eq!(viewer.windows(), windows.as_slice());
            assert_eq!(viewer.queue_len(), 1);
        }
    }

    #[test]
    fn tmux_viewer_layout_change_updates_known_window_and_ignores_visible_fields() {
        let mut viewer = TmuxViewer::new();
        let old_window = test_window(2, 80, 24, "80x24,0,0,42");
        let unchanged_window = test_window(3, 100, 30, "100x30,0,0,43");
        let new_layout = checked_layout("80x24,0,0{40x24,0,0,42,40x24,40,0,44}");
        let expected = vec![
            TmuxWindow {
                layout: Layout::parse_with_checksum(&new_layout).unwrap(),
                ..old_window.clone()
            },
            unchanged_window.clone(),
        ];
        viewer.state = TmuxViewerState::CommandQueue;
        viewer.windows = vec![old_window, unchanged_window];
        viewer.set_panes_for_tests(&[(42, 80, 24), (43, 100, 30), (44, 40, 24)]);

        assert_eq!(
            viewer.next(ControlNotification::LayoutChange {
                window_id: 2,
                layout: new_layout,
                visible_layout: "not-a-layout".to_owned(),
                raw_flags: "ignored".to_owned(),
            }),
            vec![TmuxViewerAction::Windows(expected.clone())]
        );
        assert_eq!(viewer.windows(), expected.as_slice());
        assert_eq!(viewer.pane_ids(), &[42, 44, 43]);
        assert_eq!(viewer.pane_dimensions(42), Some((80, 24)));
        assert_eq!(viewer.pane_dimensions(43), Some((100, 30)));
        assert_eq!(viewer.pane_dimensions(44), Some((40, 24)));
    }

    #[test]
    fn tmux_viewer_layout_change_unknown_window_is_ignored() {
        let mut viewer = TmuxViewer::new();
        let windows = vec![test_window(2, 80, 24, "80x24,0,0,42")];
        viewer.state = TmuxViewerState::CommandQueue;
        viewer.windows = windows.clone();

        assert_eq!(
            viewer.next(ControlNotification::LayoutChange {
                window_id: 99,
                layout: checked_layout("80x24,0,0,99"),
                visible_layout: checked_layout("80x24,0,0,99"),
                raw_flags: "ignored".to_owned(),
            }),
            Vec::new()
        );
        assert_eq!(viewer.windows(), windows.as_slice());
    }

    #[test]
    fn tmux_viewer_layout_change_invalid_layout_defuncts() {
        let mut viewer = TmuxViewer::new();
        viewer.state = TmuxViewerState::CommandQueue;
        viewer.windows = vec![test_window(2, 80, 24, "80x24,0,0,42")];

        assert_eq!(
            viewer.next(ControlNotification::LayoutChange {
                window_id: 2,
                layout: "0000,80x24,0,0,42".to_owned(),
                visible_layout: checked_layout("80x24,0,0,42"),
                raw_flags: "ignored".to_owned(),
            }),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
    }

    #[test]
    fn tmux_viewer_layout_change_does_not_consume_pending_command() {
        let mut viewer = TmuxViewer::new();
        let new_layout = checked_layout("80x24,0,0,43");
        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        viewer.windows = vec![test_window(2, 80, 24, "80x24,0,0,42")];
        viewer.set_panes_for_tests(&[(42, 80, 24)]);

        assert!(matches!(
            viewer
                .next(ControlNotification::LayoutChange {
                    window_id: 2,
                    layout: new_layout,
                    visible_layout: checked_layout("80x24,0,0,43"),
                    raw_flags: "ignored".to_owned(),
                })
                .as_slice(),
            [TmuxViewerAction::Windows(_)]
        ));
        assert_eq!(viewer.queue_len(), 6);
        assert_eq!(
            viewer.command_queue.front(),
            Some(&TmuxCommand::ListWindows)
        );
        assert_eq!(
            viewer.command_queue.iter().skip(1).collect::<Vec<_>>(),
            expected_new_pane_commands(&[43]).iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn tmux_viewer_layout_change_with_empty_queue_emits_first_new_pane_command() {
        let mut viewer = TmuxViewer::new();
        let new_layout = checked_layout("80x24,0,0{40x24,0,0,42,40x24,40,0,43}");
        let expected_window = TmuxWindow {
            layout: Layout::parse_with_checksum(&new_layout).unwrap(),
            ..test_window(2, 80, 24, "80x24,0,0,42")
        };
        viewer.state = TmuxViewerState::CommandQueue;
        viewer.windows = vec![test_window(2, 80, 24, "80x24,0,0,42")];
        viewer.set_panes_for_tests(&[(42, 80, 24)]);

        assert_eq!(
            viewer.next(ControlNotification::LayoutChange {
                window_id: 2,
                layout: new_layout,
                visible_layout: "ignored".to_owned(),
                raw_flags: "ignored".to_owned(),
            }),
            vec![
                TmuxViewerAction::Windows(vec![expected_window]),
                TmuxViewerAction::Command(
                    TmuxCommand::PaneHistory(TmuxCapturePane {
                        id: 43,
                        screen_key: TmuxScreenKey::Primary,
                    })
                    .format_command()
                ),
            ]
        );
        assert_eq!(viewer.pane_ids(), &[42, 43]);
        assert_eq!(viewer.pane_dimensions(43), Some((40, 24)));
        assert_eq!(viewer.queue_len(), 5);
    }

    #[test]
    fn tmux_viewer_command_output_with_empty_queue_is_ignored() {
        let mut viewer = TmuxViewer::new();

        viewer.next(ControlNotification::BlockEnd(Vec::new()));
        viewer.next(ControlNotification::SessionChanged {
            id: 42,
            name: "main".to_owned(),
        });
        viewer.command_queue.clear();

        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b"unexpected".to_vec())),
            Vec::new()
        );
        assert_eq!(viewer.state(), TmuxViewerState::CommandQueue);
    }

    #[test]
    fn tmux_viewer_exit_from_command_queue() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::ListWindows);
        assert_eq!(
            viewer.next(ControlNotification::Exit),
            vec![TmuxViewerAction::Exit]
        );
        assert_eq!(viewer.state(), TmuxViewerState::Defunct);
    }

    #[test]
    fn tmux_viewer_user_command_output_consumes_without_side_effects() {
        let mut viewer = TmuxViewer::new();

        viewer.queue_command_for_tests(TmuxCommand::User("send-prefix\n".to_owned()));
        assert_eq!(
            viewer.next(ControlNotification::BlockEnd(b"ignored".to_vec())),
            Vec::new()
        );
        assert_eq!(viewer.queue_len(), 0);
        assert_eq!(viewer.tmux_version(), "");
    }

    fn test_window(id: usize, width: usize, height: usize, layout: &str) -> TmuxWindow {
        let layout = checked_layout(layout);
        TmuxWindow {
            id,
            width,
            height,
            layout: Layout::parse_with_checksum(&layout).unwrap(),
        }
    }

    fn expected_new_pane_commands(pane_ids: &[usize]) -> Vec<TmuxCommand> {
        let mut commands = Vec::new();
        for pane_id in pane_ids {
            commands.push(TmuxCommand::PaneHistory(TmuxCapturePane {
                id: *pane_id,
                screen_key: TmuxScreenKey::Primary,
            }));
            commands.push(TmuxCommand::PaneVisible(TmuxCapturePane {
                id: *pane_id,
                screen_key: TmuxScreenKey::Primary,
            }));
            commands.push(TmuxCommand::PaneHistory(TmuxCapturePane {
                id: *pane_id,
                screen_key: TmuxScreenKey::Alternate,
            }));
            commands.push(TmuxCommand::PaneVisible(TmuxCapturePane {
                id: *pane_id,
                screen_key: TmuxScreenKey::Alternate,
            }));
        }
        if !pane_ids.is_empty() {
            commands.push(TmuxCommand::PaneState);
        }
        commands
    }

    fn checked_layout(layout: &str) -> String {
        let checksum = LayoutChecksum::calculate(layout.as_bytes()).as_string();
        let checksum = std::str::from_utf8(&checksum).unwrap();
        format!("{checksum},{layout}")
    }

    #[test]
    fn tmux_begin_end_empty() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1578922740 269 1\n");
        feed(&mut parser, b"%end 1578922740 269 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(Vec::new())))
        );
    }

    #[test]
    fn tmux_begin_error_empty() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1578922740 269 1\n");
        feed(&mut parser, b"%error 1578922740 269 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockErr(Vec::new())))
        );
    }

    #[test]
    fn tmux_begin_end_data() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1578922740 269 1\n");
        feed(&mut parser, b"hello\nworld\n");
        feed(&mut parser, b"%end 1578922740 269 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(
                b"hello\nworld".to_vec()
            )))
        );
    }

    #[test]
    fn tmux_block_payload_may_start_with_end() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1 1 1\n");
        feed(&mut parser, b"%end not really\n");
        feed(&mut parser, b"hello\n");
        feed(&mut parser, b"%end 1 1 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(
                b"%end not really\nhello".to_vec()
            )))
        );
    }

    #[test]
    fn tmux_block_payload_may_start_with_error() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1 1 1\n");
        feed(&mut parser, b"%error not really\n");
        feed(&mut parser, b"hello\n");
        feed(&mut parser, b"%end 1 1 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(
                b"%error not really\nhello".to_vec()
            )))
        );
    }

    #[test]
    fn tmux_block_may_terminate_with_real_error_after_misleading_payload() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1 1 1\n");
        feed(&mut parser, b"%error not really\n");
        feed(&mut parser, b"hello\n");
        feed(&mut parser, b"%error 1 1 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockErr(
                b"%error not really\nhello".to_vec()
            )))
        );
    }

    #[test]
    fn tmux_block_terminator_requires_exact_token_count() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1 1 1\n");
        feed(&mut parser, b"%end 1 1 1 trailing\n");
        feed(&mut parser, b"hello\n");
        feed(&mut parser, b"%end 1 1 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(
                b"%end 1 1 1 trailing\nhello".to_vec()
            )))
        );
    }

    #[test]
    fn tmux_block_terminator_requires_numeric_metadata() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1 1 1\n");
        feed(&mut parser, b"%end foo bar baz\n");
        feed(&mut parser, b"hello\n");
        feed(&mut parser, b"%end 1 1 1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(
                b"%end foo bar baz\nhello".to_vec()
            )))
        );
    }

    #[test]
    fn tmux_block_terminator_allows_repeated_spaces() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%begin 1 1 1\n");
        feed(&mut parser, b"%end  1  1  1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::BlockEnd(Vec::new())))
        );
    }

    #[test]
    fn tmux_output() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%output %42 foo bar baz");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::Output {
                pane_id: 42,
                data: "foo bar baz".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_session_changed() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%session-changed $42 foo");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::SessionChanged {
                id: 42,
                name: "foo".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_sessions_changed() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%sessions-changed");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::SessionsChanged))
        );
    }

    #[test]
    fn tmux_sessions_changed_carriage_return() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%sessions-changed\r");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::SessionsChanged))
        );
    }

    #[test]
    fn tmux_layout_change() {
        let mut parser = ControlParser::new();

        feed(
            &mut parser,
            b"%layout-change @2 1234x791,0,0{617x791,0,0,0,617x791,618,0,1} 1234x791,0,0{617x791,0,0,0,617x791,618,0,1} *-",
        );
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::LayoutChange {
                window_id: 2,
                layout: "1234x791,0,0{617x791,0,0,0,617x791,618,0,1}".to_owned(),
                visible_layout: "1234x791,0,0{617x791,0,0,0,617x791,618,0,1}".to_owned(),
                raw_flags: "*-".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_window_add() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%window-add @14");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::WindowAdd { id: 14 }))
        );
    }

    #[test]
    fn tmux_window_renamed() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%window-renamed @42 bar");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::WindowRenamed {
                id: 42,
                name: "bar".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_window_pane_changed() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%window-pane-changed @42 %2");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::WindowPaneChanged {
                window_id: 42,
                pane_id: 2,
            }))
        );
    }

    #[test]
    fn tmux_client_detached() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%client-detached /dev/pts/1");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::ClientDetached {
                client: "/dev/pts/1".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_client_session_changed() {
        let mut parser = ControlParser::new();

        feed(
            &mut parser,
            b"%client-session-changed /dev/pts/1 $2 mysession",
        );
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::ClientSessionChanged {
                client: "/dev/pts/1".to_owned(),
                session_id: 2,
                name: "mysession".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_client_session_changed_allows_spaces_in_client() {
        let mut parser = ControlParser::new();

        feed(
            &mut parser,
            b"%client-session-changed /dev/pts/client one $2 mysession",
        );
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::ClientSessionChanged {
                client: "/dev/pts/client one".to_owned(),
                session_id: 2,
                name: "mysession".to_owned(),
            }))
        );
    }

    #[test]
    fn tmux_unknown_notification_returns_to_idle() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%unknown value");
        assert_eq!(parser.put(b'\n'), Ok(None));
        feed(&mut parser, b"%sessions-changed");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::SessionsChanged))
        );
    }

    #[test]
    fn tmux_malformed_notification_returns_to_idle() {
        let mut parser = ControlParser::new();

        feed(&mut parser, b"%window-add @wat");
        assert_eq!(parser.put(b'\n'), Ok(None));
        feed(&mut parser, b"%window-add @14");
        assert_eq!(
            parser.put(b'\n'),
            Ok(Some(ControlNotification::WindowAdd { id: 14 }))
        );
    }

    #[test]
    fn tmux_idle_non_percent_enters_broken_state_and_emits_exit() {
        let mut parser = ControlParser::new();

        assert_eq!(parser.put(b'x'), Ok(Some(ControlNotification::Exit)));
        assert_eq!(parser.put(b'%'), Ok(None));
        assert_eq!(parser.put(b'\n'), Ok(None));
    }

    #[test]
    fn tmux_over_capacity_enters_broken_state() {
        let mut parser = ControlParser::with_max_bytes(2);

        assert_eq!(parser.put(b'%'), Ok(None));
        assert_eq!(parser.put(b'x'), Ok(None));
        assert_eq!(parser.put(b'y'), Err(ControlParseError::OutOfMemory));
        assert_eq!(parser.put(b'z'), Ok(None));
    }

    #[test]
    fn tmux_enter_notification_is_reserved_for_dcs_entry() {
        assert_eq!(ControlNotification::Enter, ControlNotification::Enter);
    }
}
