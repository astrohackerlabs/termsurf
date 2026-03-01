mod xpc;

use std::io::{self, Write};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

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
const BLUE: Color = Color::Rgb(0x7a, 0xa2, 0xf7);
const GREEN: Color = Color::Rgb(0x9e, 0xce, 0x6a);

fn submode_color(mode: &EditorMode) -> Color {
    match mode {
        EditorMode::Normal => BLUE,
        EditorMode::Insert => GREEN,
        EditorMode::Visual => PURPLE,
        EditorMode::Search => YELLOW,
    }
}

#[derive(PartialEq)]
enum Mode {
    Browse,
    Control,
    Edit,
    Command,
}

enum LoopEvent {
    Terminal(Event),
    Xpc(xpc::CompositorMessage),
}

// Command dispatch (Issue 659).
enum CommandResult {
    Quit,
    SetColorScheme(String),
    None,
}

struct Command {
    name: &'static str,
    exec: fn(args: &[&str]) -> CommandResult,
}

const COMMANDS: &[Command] = &[
    Command {
        name: "quit",
        exec: |_| CommandResult::Quit,
    },
    Command {
        name: "quitall",
        exec: |_| CommandResult::Quit,
    },
    Command {
        name: "colorscheme",
        exec: |args| match args.first().map(|s| *s) {
            Some("dark" | "d") => CommandResult::SetColorScheme("dark".into()),
            Some("light" | "l") => CommandResult::SetColorScheme("light".into()),
            Some("system" | "s") => CommandResult::SetColorScheme("system".into()),
            _ => CommandResult::None,
        },
    },
];

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    for c in needle.chars() {
        loop {
            match hay.next() {
                Some(h) if h == c => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

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
        .filter(|c| is_subsequence(prefix, c.name))
        .collect();
    match matches.len() {
        0 => CommandResult::None,
        1 => (matches[0].exec)(&args),
        _ => {
            // Exact match wins, then shortest name wins (Issue 681).
            if let Some(cmd) = matches.iter().find(|c| c.name == prefix) {
                (cmd.exec)(&args)
            } else {
                let shortest = matches.iter().min_by_key(|c| c.name.len()).unwrap();
                (shortest.exec)(&args)
            }
        }
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

#[derive(Parser)]
#[command(name = "web", about = "TermSurf Web")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// URL to open (fallback when no subcommand given)
    url: Option<String>,

    /// Browser profile name
    #[arg(long, global = true)]
    profile: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Open a URL in the browser pane
    Url {
        /// The URL to open
        url: String,
    },
    /// Show the last active browser pane/tab
    Last,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let profile_arg = cli.profile; // Option<String> — None if no --profile given
    let profile = profile_arg.clone().unwrap_or_else(|| "default".to_string());

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

    // Connect to the TermSurf compositor via XPC (Issue 505).
    let pane_id = std::env::var("TERMSURF_PANE_ID").ok();
    match &pane_id {
        Some(id) => eprintln!("[web] TERMSURF_PANE_ID = {}", id),
        None => eprintln!("[web] TERMSURF_PANE_ID not set (not running inside TermSurf)"),
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let compositor = pane_id
        .as_ref()
        .and_then(|_| xpc::CompositorConnection::connect(tx.clone()));
    match &compositor {
        Some(_) => eprintln!("[web] Connected to compositor"),
        None if pane_id.is_some() => {
            eprintln!("[web] XPC service unavailable (is launchd plist loaded?)")
        }
        _ => {}
    }

    // Handle `web last` subcommand — print last active browser pane and exit (Issue 684 Exp 4).
    if let Some(Commands::Last) = cli.command {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            match conn.send_query_last(pid, profile_arg.as_deref().unwrap_or("")) {
                Some((prof, pane, tab)) => {
                    println!("profile: {}", prof);
                    println!("pane_id: {}", pane);
                    println!("tab_id:  {}", tab);
                }
                None => {
                    println!("No active browser tab found.");
                }
            }
        } else {
            println!("Not running inside TermSurf.");
        }
        return Ok(());
    }

    // Send hello to get live config from the GUI (Issue 675).
    let hello_homepage = compositor
        .as_ref()
        .and_then(|conn| pane_id.as_ref().and_then(|pid| conn.send_hello(pid)));

    // Detect devtools://N before normalizing (Issue 684).
    let raw_url = match cli.command {
        Some(Commands::Url { url }) => url,
        Some(Commands::Last) => unreachable!(), // Handled above.
        None => cli.url.unwrap_or_else(|| {
            hello_homepage.unwrap_or_else(|| "https://termsurf.com/welcome".to_string())
        }),
    };
    let inspected_tab_id: i64 = if raw_url.starts_with("devtools://") {
        raw_url["devtools://".len()..].parse::<i64>().unwrap_or(0)
    } else if raw_url == "devtools" {
        0 // Auto-target: GUI resolves to most recent browser tab (Issue 684 Exp 3).
    } else {
        -1 // Not a DevTools request.
    };
    let is_devtools = inspected_tab_id >= 0;
    let mut url = if is_devtools {
        raw_url // Keep devtools://N or bare "devtools" as-is.
    } else {
        normalize_url(&raw_url)
    };

    // Enter raw mode and alternate screen.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    // Crossterm reader thread — forwards relevant terminal events (Issue 668).
    // Key, Resize, and Paste wake the main loop. Mouse and Focus are dropped
    // to avoid redrawing on every pixel of mouse movement.
    let key_tx = tx;
    std::thread::spawn(move || loop {
        match event::read() {
            Ok(ev @ (Event::Key(_) | Event::Resize(_, _) | Event::Paste(_))) => {
                if key_tx.send(LoopEvent::Terminal(ev)).is_err() {
                    break;
                }
            }
            Ok(_) => {} // Mouse, FocusGained, FocusLost — drop silently.
            Err(_) => break,
        }
    });

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
                if is_devtools {
                    // DevTools pane (Issue 684).
                    conn.send_set_devtools_overlay(
                        pid,
                        viewport_rect.x,
                        viewport_rect.y,
                        viewport_rect.width,
                        viewport_rect.height,
                        inspected_tab_id,
                        &profile,
                        mode == Mode::Browse,
                    );
                } else {
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

        // Unified event channel — blocks until a terminal or XPC event arrives (Issue 668).
        match rx.recv() {
            Ok(LoopEvent::Terminal(Event::Key(key))) => {
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
                        // Esc in Normal mode exits Edit → Control (Issue 665).
                        if key.code == KeyCode::Esc && editor_state.mode == EditorMode::Normal {
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
                            url = normalize_url(&new_url);
                            editor_url = url.clone();
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
                        // Esc in Normal mode exits Command → Control (Issue 665).
                        if key.code == KeyCode::Esc && cmd_state.mode == EditorMode::Normal {
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
                                CommandResult::SetColorScheme(scheme) => {
                                    if let (Some(ref conn), Some(ref pid)) =
                                        (&compositor, &pane_id)
                                    {
                                        conn.send_set_color_scheme(pid, &scheme);
                                    }
                                }
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
            Ok(LoopEvent::Terminal(_)) => {
                // Resize, mouse, focus, paste, etc. — just redraw.
            }
            Ok(LoopEvent::Xpc(msg)) => {
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
            Err(_) => break,
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

/// Normalize a URL by prepending a scheme if missing (Issue 676).
fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    // Extract the host portion (before any path/query).
    let host = trimmed.split('/').next().unwrap_or(trimmed);
    if host.ends_with("localhost") || host.contains("localhost:") {
        return format!("http://{trimmed}");
    }
    if trimmed.contains('.') {
        return format!("https://{trimmed}");
    }
    trimmed.to_string()
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
        let sc = submode_color(&cmd_state.mode);
        let submode_label =
            Line::from(vec![Span::raw(submode_text).style(Style::default().fg(sc))]);
        let cmd_title = Line::from(vec![
            Span::raw("COMMAND").style(Style::default().fg(url_border))
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
            Layout::horizontal([Constraint::Length(1), Constraint::Min(0)]).split(cmd_inner);
        frame.render_widget(
            Paragraph::new(":").style(Style::default().fg(YELLOW).bg(BG)),
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
        let sc = submode_color(&editor_state.mode);
        let submode_label =
            Line::from(vec![Span::raw(submode_text).style(Style::default().fg(sc))]);
        let url_title = Line::from(vec![Span::raw("URL").style(Style::default().fg(url_border))]);
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
        let url_title = Line::from(vec![Span::raw("URL").style(Style::default().fg(url_border))]);
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
        Span::raw("\u{F007} ").style(Style::default().fg(COMMENT)),
        Span::raw(profile).style(Style::default().fg(FG)),
    ]);
    let viewport_title = if page_title.is_empty() {
        "Viewport".to_string()
    } else {
        page_title.to_string()
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
            Span::styled("\u{2318}[ ", f),
            Span::styled("back  ", d),
            Span::styled("\u{2318}] ", f),
            Span::styled("fwd  ", d),
            Span::styled("\u{2318}r ", f),
            Span::styled("reload  ", d),
            Span::styled("esc ", f),
            Span::styled("control", d),
        ]),
        Mode::Control => Line::from(vec![
            Span::styled(":q\u{23CE} ", f),
            Span::styled("quit  ", d),
            Span::styled("i ", f),
            Span::styled("edit url  ", d),
            Span::styled("\u{23CE} ", f),
            Span::styled("browse", d),
        ]),
        Mode::Edit => Line::from(vec![
            Span::styled("\u{23CE} ", f),
            Span::styled("navigate  ", d),
            Span::styled("esc ", f),
            Span::styled("control", d),
        ]),
        Mode::Command => Line::from(vec![
            Span::styled("\u{23CE} ", f),
            Span::styled("execute  ", d),
            Span::styled("esc ", f),
            Span::styled("control", d),
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
