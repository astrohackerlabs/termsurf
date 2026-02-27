mod xpc;

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use edtui::actions::{Execute, SelectLine, SwitchMode};
use edtui::clipboard::ClipboardTrait;
use edtui::events::{KeyEventHandler, KeyEventRegister, KeyInput};
use edtui::{
    EditorEventHandler, EditorMode, EditorState, EditorTheme, EditorView, Lines, RowIndex,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

// Tokyo Night palette.
const BG: Color = Color::Rgb(0x1a, 0x1b, 0x26);
const FG: Color = Color::Rgb(0xc0, 0xca, 0xf5);
const COMMENT: Color = Color::Rgb(0x73, 0x7a, 0xa2);
const CYAN: Color = Color::Rgb(0x7d, 0xcf, 0xff);
const BORDER: Color = Color::Rgb(0x56, 0x5f, 0x89);
const DIM: Color = Color::Rgb(0x90, 0x9a, 0xb8);
const SELECTION: Color = Color::Rgb(0x28, 0x34, 0x57);
const PURPLE: Color = Color::Rgb(0xbb, 0x9a, 0xf7);
const YELLOW: Color = Color::Rgb(0xe0, 0xaf, 0x68);

#[derive(PartialEq)]
enum Mode {
    Browse,
    Control,
    Edit,
    Command,
}

// Command dispatch (Issue 659).
enum CommandResult {
    Quit,
    None,
}

struct Command {
    name: &'static str,
    exec: fn(args: &[&str]) -> CommandResult,
}

const COMMANDS: &[Command] = &[Command {
    name: "quit",
    exec: |_| CommandResult::Quit,
}];

fn dispatch(input: &str) -> CommandResult {
    let mut parts = input.trim().splitn(2, ' ');
    let prefix = parts.next().unwrap_or("");
    if prefix.is_empty() {
        return CommandResult::None;
    }
    let args: Vec<&str> = parts
        .next()
        .map(|s| s.split_whitespace().collect())
        .unwrap_or_default();
    let matches: Vec<&Command> = COMMANDS
        .iter()
        .filter(|c| c.name.starts_with(prefix))
        .collect();
    match matches.len() {
        1 => (matches[0].exec)(&args),
        _ => CommandResult::None,
    }
}

/// Clipboard wrapper that strips leading newlines from edtui's line-mode yanks
/// (Issue 658).
struct UrlClipboard(arboard::Clipboard);

impl UrlClipboard {
    fn new() -> Self {
        Self(arboard::Clipboard::new().expect("failed to open system clipboard"))
    }
}

impl ClipboardTrait for UrlClipboard {
    fn set_text(&mut self, text: String) {
        let clean = text.trim_start_matches('\n').to_string();
        let _ = self.0.set_text(clean);
    }

    fn get_text(&mut self) -> String {
        self.0.get_text().unwrap_or_default()
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Parse --profile flag.
    let mut profile = String::from("default");
    let mut url = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--profile" {
            if i + 1 < args.len() {
                profile = args[i + 1].clone();
                i += 2;
                continue;
            }
        } else if url.is_none() {
            url = Some(args[i].clone());
        }
        i += 1;
    }
    // Validate profile name: lowercase alphanumeric, starts with a letter.
    if profile.is_empty()
        || !profile.bytes().next().unwrap().is_ascii_lowercase()
        || !profile
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        eprintln!("Error: profile name must be lowercase alphanumeric, starting with a letter");
        std::process::exit(1);
    }

    let mut url = url.unwrap_or_else(|| {
        eprintln!("Usage: web <url> [--profile <name>]");
        std::process::exit(1);
    });

    // Connect to the TermSurf compositor via XPC (Issue 505).
    let pane_id = std::env::var("TERMSURF_PANE_ID").ok();
    match &pane_id {
        Some(id) => eprintln!("[web] TERMSURF_PANE_ID = {}", id),
        None => eprintln!("[web] TERMSURF_PANE_ID not set (not running inside TermSurf)"),
    }

    let compositor = pane_id
        .as_ref()
        .and_then(|_| xpc::CompositorConnection::connect());
    match &compositor {
        Some(_) => eprintln!("[web] Connected to compositor"),
        None if pane_id.is_some() => {
            eprintln!("[web] XPC service unavailable (is launchd plist loaded?)")
        }
        _ => {}
    }

    // Enter raw mode and alternate screen.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut mode = Mode::Control;
    let mut last_viewport = Rect::default();
    let mut loading_bar_active = false;
    let mut loading_bar_start: Option<Instant> = None;
    const LOADING_TIMEOUT: Duration = Duration::from_secs(30);
    let mut page_title = String::new();

    // edtui state (Issue 637, 658).
    let mut editor_state = EditorState::new(Lines::from(url.as_str()));
    editor_state.set_clipboard(UrlClipboard::new());
    let mut editor_url = url.clone(); // Track which URL the editor has.
    let make_single_line_handler = || {
        let mut kh = KeyEventHandler::vim_mode();
        // Remove newline keybindings for single-line mode.
        kh.remove(&KeyEventRegister::i(vec![KeyInput::new(KeyCode::Enter)]));
        kh.remove(&KeyEventRegister::n(vec![KeyInput::new('o')]));
        kh.remove(&KeyEventRegister::n(vec![KeyInput::shift('O')]));
        EditorEventHandler::new(kh)
    };
    let mut editor_handler = make_single_line_handler();

    // Command mode editor state (Issue 659).
    let mut cmd_state = EditorState::new(Lines::from(""));
    cmd_state.set_clipboard(UrlClipboard::new());
    let mut cmd_handler = make_single_line_handler();

    // Event loop.
    loop {
        let mut viewport_rect = Rect::default();
        terminal.draw(|frame| {
            viewport_rect = ui(
                frame,
                &url,
                &profile,
                &mode,
                &mut editor_state,
                &mut cmd_state,
                &page_title,
            );
        })?;

        // Send overlay coordinates to compositor (only when changed).
        if viewport_rect != last_viewport {
            let first_overlay = last_viewport == Rect::default();
            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                conn.send_set_overlay(
                    pid,
                    viewport_rect.x,
                    viewport_rect.y,
                    viewport_rect.width,
                    viewport_rect.height,
                    &url,
                    &profile,
                    mode == Mode::Browse,
                );
            }
            last_viewport = viewport_rect;

            // Emit indeterminate pulse immediately on first overlay (cold-start coverage).
            if first_overlay {
                let mut stdout = io::stdout();
                let _ = write!(stdout, "\x1b]9;4;3\x1b\\");
                let _ = stdout.flush();
                loading_bar_active = true;
                loading_bar_start = Some(Instant::now());
            }
        }

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                // Ctrl+C quits from any mode.
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }

                match mode {
                    Mode::Browse => {
                        if key.code == KeyCode::Esc {
                            mode = Mode::Control;
                            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                conn.send_mode_changed(pid, false);
                            }
                        }
                    }
                    Mode::Control => {
                        // Sync editor content if URL changed externally (Issue 658).
                        let enter_edit =
                            |editor_state: &mut EditorState,
                             editor_url: &mut String,
                             url: &str,
                             mode: &mut Mode| {
                                if *editor_url != url {
                                    *editor_state = EditorState::new(Lines::from(url));
                                    editor_state.set_clipboard(UrlClipboard::new());
                                    let len = url.len();
                                    editor_state.cursor =
                                        edtui::Index2::new(0, len.saturating_sub(1));
                                    *editor_url = url.to_string();
                                }
                                *mode = Mode::Edit;
                            };
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('i') => {
                                // Insert mode, cursor at last position (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('A') => {
                                // Insert mode, cursor at end of line (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.cursor.col =
                                    editor_state.lines.len_col(0).unwrap_or(0);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('I') => {
                                // Insert mode, cursor at start of line (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.cursor = edtui::Index2::new(0, 0);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('n') => {
                                // Normal mode, cursor at last position (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.mode = EditorMode::Normal;
                                editor_state.selection = None;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('v') => {
                                // Visual mode, cursor at last position (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                SwitchMode(EditorMode::Visual).execute(&mut editor_state);
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('V') => {
                                // Visual mode, entire line selected (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                SelectLine.execute(&mut editor_state);
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char(':') => {
                                // Enter Command mode with fresh editor (Issue 659).
                                cmd_state = EditorState::new(Lines::from(""));
                                cmd_state.set_clipboard(UrlClipboard::new());
                                cmd_state.mode = EditorMode::Insert;
                                mode = Mode::Command;
                            }
                            KeyCode::Enter => {
                                mode = Mode::Browse;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, true);
                                }
                            }
                            _ => {}
                        }
                    }
                    Mode::Edit => {
                        // Ctrl+Esc exits Edit → Control (Issue 658).
                        if key.code == KeyCode::Esc && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            mode = Mode::Control;
                        } else if key.code == KeyCode::Enter
                            && editor_state.mode != EditorMode::Search
                        {
                            // Extract URL from editor, navigate, switch to Browse.
                            let new_url: String = editor_state
                                .lines
                                .get(RowIndex::new(0))
                                .map(|line| line.iter().collect())
                                .unwrap_or_default();
                            url = new_url.clone();
                            editor_url = new_url;
                            mode = Mode::Browse;
                            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                conn.send_navigate(pid, &url);
                                conn.send_mode_changed(pid, true);
                            }
                        } else {
                            // Pass everything else to edtui (including Escape).
                            editor_handler.on_key_event(key, &mut editor_state);
                        }
                    }
                    Mode::Command => {
                        // Ctrl+Esc exits Command → Control (Issue 659).
                        if key.code == KeyCode::Esc && key.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            mode = Mode::Control;
                        } else if key.code == KeyCode::Enter && cmd_state.mode != EditorMode::Search
                        {
                            // Extract command text and dispatch (Issue 659).
                            let cmd_text: String = cmd_state
                                .lines
                                .get(RowIndex::new(0))
                                .map(|line| line.iter().collect())
                                .unwrap_or_default();
                            match dispatch(&cmd_text) {
                                CommandResult::Quit => break,
                                CommandResult::None => {}
                            }
                            mode = Mode::Control;
                        } else {
                            // Pass everything else to command edtui.
                            cmd_handler.on_key_event(key, &mut cmd_state);
                        }
                    }
                }
            }
        }

        // Drain incoming messages from compositor (Issue 513).
        if let Some(ref conn) = compositor {
            while let Some(msg) = conn.try_recv() {
                match msg {
                    xpc::CompositorMessage::ModeChanged { browsing } => {
                        mode = if browsing {
                            Mode::Browse
                        } else {
                            Mode::Control
                        };
                    }
                    xpc::CompositorMessage::UrlChanged { url: new_url } => {
                        url = new_url;
                        // Mark editor_url stale so enter_edit re-syncs (Issue 658).
                        editor_url.clear();
                    }
                    xpc::CompositorMessage::LoadingState {
                        state,
                        _progress: _,
                    } => {
                        let mut stdout = io::stdout();
                        let _ = match state.as_str() {
                            "loading" => {
                                loading_bar_active = true;
                                loading_bar_start = Some(Instant::now());
                                write!(stdout, "\x1b]9;4;3\x1b\\")
                            }
                            "progress" => Ok(()),
                            "done" => {
                                loading_bar_active = false;
                                loading_bar_start = None;
                                write!(stdout, "\x1b]9;4;0\x1b\\")
                            }
                            "error" => {
                                loading_bar_active = false;
                                loading_bar_start = None;
                                write!(stdout, "\x1b]9;4;2\x1b\\")
                            }
                            _ => Ok(()),
                        };
                        let _ = stdout.flush();
                    }
                    xpc::CompositorMessage::TitleChanged { title } => {
                        page_title = title;
                    }
                }
            }
        }

        // Safety timeout: clear stuck loading bar after 30 seconds (Issue 616).
        if loading_bar_active {
            if let Some(start) = loading_bar_start {
                if start.elapsed() >= LOADING_TIMEOUT {
                    let mut stdout = io::stdout();
                    let _ = write!(stdout, "\x1b]9;4;2\x1b\\");
                    let _ = stdout.flush();
                    std::thread::sleep(Duration::from_millis(500));
                    let _ = write!(stdout, "\x1b]9;4;0\x1b\\");
                    let _ = stdout.flush();
                    loading_bar_active = false;
                    loading_bar_start = None;
                }
            }
        }
    }

    // Clear loading bar on exit (Issue 616).
    if loading_bar_active {
        let mut stdout = io::stdout();
        let _ = write!(stdout, "\x1b]9;4;0\x1b\\");
        let _ = stdout.flush();
    }

    // Restore terminal. The compositor connection drops here, which closes
    // the XPC connection and triggers overlay cleanup.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    Ok(())
}

/// Render the UI and return the viewport inner rect (grid coordinates).
fn ui(
    frame: &mut Frame,
    url: &str,
    profile: &str,
    mode: &Mode,
    editor_state: &mut EditorState,
    cmd_state: &mut EditorState,
    page_title: &str,
) -> Rect {
    // Paint full background.
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );

    let layout = Layout::vertical([
        Constraint::Min(1),    // Viewport (fill remaining)
        Constraint::Length(3), // URL bar (1 line + top/bottom border)
        Constraint::Length(1), // Status bar
    ])
    .split(frame.area());

    // Border colors based on mode.
    let (url_border, viewport_border) = match mode {
        Mode::Browse => (BORDER, CYAN),
        Mode::Control => (CYAN, BORDER),
        Mode::Edit => (PURPLE, BORDER),
        Mode::Command => (YELLOW, BORDER),
    };

    // URL bar / Command bar (Issue 659).
    if *mode == Mode::Command {
        // Submode indicator in top-right of command bar.
        let submode_text = match cmd_state.mode {
            EditorMode::Normal => "\u{EA85} NORMAL",
            EditorMode::Insert => "\u{F040} INSERT",
            EditorMode::Visual => "\u{F14A} VISUAL",
            EditorMode::Search => "\u{F002} SEARCH",
        };
        let submode_label = Line::from(vec![
            Span::raw(" ").style(Style::default().fg(YELLOW)),
            Span::raw(submode_text).style(Style::default().fg(YELLOW)),
            Span::raw(" "),
        ]);
        let cmd_title = Line::from(vec![
            Span::raw(" COMMAND ").style(Style::default().fg(url_border))
        ]);
        let cmd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(url_border).bg(BG))
            .title_style(Style::default().fg(url_border))
            .title_top(cmd_title)
            .title_top(submode_label.alignment(Alignment::Right))
            .style(Style::default().bg(BG));
        let cmd_inner = cmd_block.inner(layout[1]);
        frame.render_widget(cmd_block, layout[1]);

        // Split inner area: ":" prefix + editor.
        let cmd_layout =
            Layout::horizontal([Constraint::Length(2), Constraint::Min(0)]).split(cmd_inner);
        frame.render_widget(
            Paragraph::new(": ").style(Style::default().fg(YELLOW).bg(BG)),
            cmd_layout[0],
        );
        let theme = EditorTheme::default()
            .base(Style::default().fg(FG).bg(BG))
            .cursor_style(Style::default().fg(BG).bg(FG))
            .selection_style(Style::default().fg(FG).bg(SELECTION))
            .hide_status_line();
        frame.render_widget(
            EditorView::new(cmd_state).theme(theme).wrap(false),
            cmd_layout[1],
        );
    } else if *mode == Mode::Edit {
        // Submode indicator in top-right of URL bar (Issue 658).
        let submode_text = match editor_state.mode {
            EditorMode::Normal => "\u{EA85} NORMAL",
            EditorMode::Insert => "\u{F040} INSERT",
            EditorMode::Visual => "\u{F14A} VISUAL",
            EditorMode::Search => "\u{F002} SEARCH",
        };
        let submode_label = Line::from(vec![
            Span::raw(" ").style(Style::default().fg(PURPLE)),
            Span::raw(submode_text).style(Style::default().fg(PURPLE)),
            Span::raw(" "),
        ]);
        let url_title = Line::from(vec![
            Span::raw(" URL ").style(Style::default().fg(url_border))
        ]);
        let theme = EditorTheme::default()
            .base(Style::default().fg(FG).bg(BG))
            .cursor_style(Style::default().fg(BG).bg(FG))
            .selection_style(Style::default().fg(FG).bg(SELECTION))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(url_border).bg(BG))
                    .title_style(Style::default().fg(url_border))
                    .title_top(url_title)
                    .title_top(submode_label.alignment(Alignment::Right))
                    .style(Style::default().bg(BG)),
            )
            .hide_status_line();
        frame.render_widget(
            EditorView::new(editor_state).theme(theme).wrap(false),
            layout[1],
        );
    } else {
        let url_title = Line::from(vec![
            Span::raw(" URL ").style(Style::default().fg(url_border))
        ]);
        let url_bar = Paragraph::new(url).style(Style::default().fg(FG)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(url_border).bg(BG))
                .title_style(Style::default().fg(url_border))
                .title_top(url_title)
                .style(Style::default().bg(BG)),
        );
        frame.render_widget(url_bar, layout[1]);
    }

    // Viewport.
    let profile_title = Line::from(vec![
        Span::raw(" \u{F007} ").style(Style::default().fg(COMMENT)),
        Span::raw(profile).style(Style::default().fg(FG)),
        Span::raw(" "),
    ]);
    let viewport_title = if page_title.is_empty() {
        " Viewport ".to_string()
    } else {
        format!(" {} ", page_title)
    };
    let viewport_block = Block::default()
        .borders(Borders::ALL)
        .title(viewport_title)
        .title_top(profile_title.alignment(Alignment::Right))
        .border_style(Style::default().fg(viewport_border).bg(BG))
        .title_style(Style::default().fg(viewport_border))
        .style(Style::default().bg(BG));
    let inner = viewport_block.inner(layout[0]);
    let viewport_text = format!(
        "origin: ({}, {})\nsize: {} x {}",
        inner.x, inner.y, inner.width, inner.height
    );
    let viewport = Paragraph::new(viewport_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(FG).bg(BG))
        .block(viewport_block);
    frame.render_widget(viewport, layout[0]);

    // Status bar.
    let status_layout = Layout::horizontal([
        Constraint::Fill(1),    // Key hints (left)
        Constraint::Length(14), // Mode label (right)
    ])
    .split(layout[2]);

    let d = Style::default().fg(DIM).bg(BG);
    let f = Style::default().fg(FG).bg(BG);

    let hints = match mode {
        Mode::Browse => Line::from(vec![
            Span::styled("<", d),
            Span::styled("cmd+[", f),
            Span::styled("> ", d),
            Span::styled("back  ", f),
            Span::styled("<", d),
            Span::styled("cmd+]", f),
            Span::styled("> ", d),
            Span::styled("fwd  ", f),
            Span::styled("<", d),
            Span::styled("cmd+r", f),
            Span::styled("> ", d),
            Span::styled("reload  ", f),
            Span::styled("<", d),
            Span::styled("ctrl+esc", f),
            Span::styled("> ", d),
            Span::styled("control", f),
        ]),
        Mode::Control => Line::from(vec![
            Span::styled("<", d),
            Span::styled("q", f),
            Span::styled("> ", d),
            Span::styled("quit  ", f),
            Span::styled("<", d),
            Span::styled("i", f),
            Span::styled("> ", d),
            Span::styled("edit url  ", f),
            Span::styled("<", d),
            Span::styled("enter", f),
            Span::styled("> ", d),
            Span::styled("browse", f),
        ]),
        Mode::Edit => Line::from(vec![
            Span::styled("<", d),
            Span::styled("enter", f),
            Span::styled("> ", d),
            Span::styled("navigate  ", f),
            Span::styled("<", d),
            Span::styled("ctrl+esc", f),
            Span::styled("> ", d),
            Span::styled("control", f),
        ]),
        Mode::Command => Line::from(vec![
            Span::styled("<", d),
            Span::styled("enter", f),
            Span::styled("> ", d),
            Span::styled("execute  ", f),
            Span::styled("<", d),
            Span::styled("ctrl+esc", f),
            Span::styled("> ", d),
            Span::styled("control", f),
        ]),
    };

    let label = match mode {
        Mode::Browse => "\u{F059F} BROWSE".to_string(),
        Mode::Control => "\u{F11C} CONTROL".to_string(),
        Mode::Edit => "\u{F044} EDIT".to_string(),
        Mode::Command => "\u{F120} COMMAND".to_string(),
    };

    let hints_widget = Paragraph::new(hints);
    frame.render_widget(hints_widget, status_layout[0]);

    let label_widget = Paragraph::new(label)
        .alignment(Alignment::Right)
        .style(Style::default().fg(FG).bg(BG));
    frame.render_widget(label_widget, status_layout[1]);

    inner
}
