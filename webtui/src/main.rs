mod ipc;

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
const RED: Color = Color::Rgb(0xf7, 0x76, 0x8e);

fn submode_color(mode: &EditorMode) -> Color {
    match mode {
        EditorMode::Normal => BLUE,
        EditorMode::Insert => GREEN,
        EditorMode::Visual => PURPLE,
        EditorMode::Search => YELLOW,
    }
}

#[derive(Clone, PartialEq)]
enum Mode {
    Browse,
    Control,
    Edit,
    Command,
    Dialog,
}

#[derive(Clone)]
struct PendingJsDialog {
    tab_id: i64,
    request_id: u64,
    dialog_type: String,
    origin_url: String,
    message: String,
    default_prompt_text: String,
    input: String,
    previous_mode: Mode,
}

// Loading screen stages (Issue 773).
#[derive(Clone)]
enum LoadingStage {
    ConnectingToGui,
    StartingBrowser,
    WaitingForChromium,
    LoadingPage,
    Ready,
}

#[derive(Clone)]
enum StageStatus {
    InProgress,
    Done,
    Error(String),
}

impl LoadingStage {
    fn label(&self) -> &'static str {
        match self {
            LoadingStage::ConnectingToGui => "Connected to GUI",
            LoadingStage::StartingBrowser => "Starting browser engine",
            LoadingStage::WaitingForChromium => "Waiting for Chromium",
            LoadingStage::LoadingPage => "Loading page",
            LoadingStage::Ready => "Ready",
        }
    }
}

enum LoopEvent {
    Terminal(Event),
    Ipc(ipc::CompositorMessage),
}

// Command dispatch (Issues 659, 772).
enum DarkAction {
    Toggle,
    On,
    Off,
    System,
}

enum CommandResult {
    Quit,
    Dark(DarkAction),
    DevTools(String), // direction: "right", "down", "left", "up" (Issue 690).
    Error(String),    // error message for command bar (Issue 690).
    None,
}

struct Command {
    names: &'static [&'static str],
    exec: fn(args: &[&str]) -> CommandResult,
}

const COMMANDS: &[Command] = &[
    Command {
        names: &["quit", "q"],
        exec: |_| CommandResult::Quit,
    },
    Command {
        names: &["dark", "da"],
        exec: |args| match args.first().copied() {
            None => CommandResult::Dark(DarkAction::Toggle),
            Some("on" | "yes" | "y") => CommandResult::Dark(DarkAction::On),
            Some("off" | "no" | "n") => CommandResult::Dark(DarkAction::Off),
            Some("system" | "s") => CommandResult::Dark(DarkAction::System),
            Some(other) => CommandResult::Error(format!("Unknown: {}", other)),
        },
    },
    Command {
        names: &["devtools", "de"],
        exec: |args| match args.first().copied() {
            Some("right" | "r") | None => CommandResult::DevTools("right".into()),
            Some("down" | "d") => CommandResult::DevTools("down".into()),
            Some("left" | "l") => CommandResult::DevTools("left".into()),
            Some("up" | "u") => CommandResult::DevTools("up".into()),
            Some(other) => CommandResult::Error(format!("Unknown direction: {}", other)),
        },
    },
];

fn dispatch(input: &str) -> CommandResult {
    let mut parts = input.trim().splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    if cmd.is_empty() {
        return CommandResult::None;
    }
    let args: Vec<&str> = parts
        .next()
        .map(|s| s.split_whitespace().collect())
        .unwrap_or_default();
    for command in COMMANDS {
        if command.names.contains(&cmd) {
            return (command.exec)(&args);
        }
    }
    CommandResult::None
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
#[command(name = "web", about = "TermSurf Webtui")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// URL to open (fallback when no subcommand given)
    url: Option<String>,

    /// Browser profile name
    #[arg(short, long, global = true)]
    profile: Option<String>,

    /// Browser binary to use ("chromium", "plusium", or absolute path)
    #[arg(short, long, global = true)]
    browser: Option<String>,
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
    /// Show Chromium tab inventory for the current profile
    Status,
    /// Open a local file in the browser pane
    File {
        /// Path to the file (relative or absolute)
        path: String,
    },
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let profile_arg = cli.profile; // Option<String> — None if no --profile given
    let mut profile = profile_arg.clone().unwrap_or_else(|| "default".to_string());
    let mut browser = cli.browser.unwrap_or_default();

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

    let (tx, rx) = std::sync::mpsc::channel();
    let compositor = pane_id
        .as_ref()
        .and_then(|_| ipc::CompositorConnection::connect(tx.clone()));

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

    // Handle `web status` — print tab inventory and exit (Issue 689).
    if let Some(Commands::Status) = cli.command {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            match conn.send_query_tabs(pid, &profile) {
                Ok(status) => println!("{}", status),
                Err(e) => eprintln!("Error: {}", e),
            }
        } else {
            println!("Not running inside TermSurf.");
        }
        return Ok(());
    }

    // Send hello to get live config from the GUI (Issue 675).
    // Returns (homepage, browsers) — Issue 712.
    let (hello_homepage, hello_browsers) = compositor
        .as_ref()
        .and_then(|conn| pane_id.as_ref().and_then(|pid| conn.send_hello(pid)))
        .map(|(hp, br)| (Some(hp), br))
        .unwrap_or((None, vec![]));

    // Default browser from hello reply when --browser not specified (Issue 712).
    if browser.is_empty() {
        if let Some(first) = hello_browsers.first() {
            browser = first.clone();
        }
    }

    // Detect devtools://N before normalizing (Issue 684).
    let raw_url = match cli.command {
        Some(Commands::Url { url }) => url,
        Some(Commands::File { path }) => {
            let absolute = std::fs::canonicalize(&path).unwrap_or_else(|e| {
                eprintln!("Error: {}: {}", path, e);
                std::process::exit(1);
            });
            format!("file://{}", absolute.display())
        }
        Some(Commands::Last) | Some(Commands::Status) => unreachable!(), // Handled above.
        None => cli.url.unwrap_or_else(|| {
            hello_homepage
                .filter(|hp| !hp.is_empty())
                .unwrap_or_else(|| "https://termsurf.com/welcome".to_string())
        }),
    };
    let mut inspected_tab_id: i64 = if let Some(id) = raw_url.strip_prefix("devtools://") {
        id.parse::<i64>().unwrap_or(0)
    } else if raw_url == "devtools" {
        eprintln!(
            "Error: DevTools requires opening from a browser pane or an explicit devtools://<tab_id> target with --browser and --profile"
        );
        return Ok(());
    } else {
        -1 // Not a DevTools request.
    };
    let is_devtools = inspected_tab_id >= 0;
    let mut url = if is_devtools {
        raw_url // Keep devtools://N as-is.
    } else {
        match resolve_input(&raw_url) {
            Some(resolved) => resolved,
            None => {
                eprintln!("Error: '{}' is not a URL, file, or command", raw_url);
                std::process::exit(1);
            }
        }
    };

    // Validate DevTools request before entering the UI (Issue 687).
    // The reply includes the inspected tab's browser and profile (Issue 705 Exp 10).
    if is_devtools {
        if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
            match conn.send_query_devtools(pid, inspected_tab_id, &profile, &browser) {
                Ok((resolved_tab_id, resolved_browser, resolved_profile)) => {
                    inspected_tab_id = resolved_tab_id;
                    if !resolved_browser.is_empty() {
                        browser = resolved_browser;
                    }
                    if !resolved_profile.is_empty() {
                        profile = resolved_profile;
                    }
                }
                Err(err) => {
                    eprintln!("Error: {}", err);
                    return Ok(());
                }
            }
        }
    }

    // Enter raw mode and alternate screen.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    // Crossterm reader thread — forwards relevant terminal events (Issue 668).
    // Key, Resize, and Paste wake the main loop. Mouse and Focus are dropped
    // to avoid redrawing on every pixel of mouse movement.
    let browser_tx = tx.clone();
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

    // Capture executable path for `:devtools` split command (Issue 690).
    let current_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "web".to_string());

    let mut mode = Mode::Control;
    let mut is_dark = true;
    let mut command_error: Option<String> = None; // Command bar error (Issue 690).
    let mut browser_ready = false;
    let mut current_tab_id: i64 = 0;
    let mut page_loaded = false;
    let mut page_loaded_at: Option<Instant> = None;
    let mut loading_log: Vec<(LoadingStage, StageStatus)> = Vec::new();
    let mut chromium_wait_start: Option<Instant> = None;

    // Populate initial loading stages (Issue 773).
    if compositor.is_some() {
        loading_log.push((LoadingStage::ConnectingToGui, StageStatus::Done));
    } else if pane_id.is_some() {
        loading_log.push((
            LoadingStage::ConnectingToGui,
            StageStatus::Error("Failed to connect to GUI".into()),
        ));
    }
    let mut last_viewport = Rect::default();
    let mut loading_bar_active = false;
    let mut loading_bar_start: Option<Instant> = None;
    const LOADING_TIMEOUT: Duration = Duration::from_secs(30);
    let mut page_title = String::new();
    let mut target_url = String::new();
    let mut browser_conn: Option<ipc::BrowserConnection> = None;
    let mut pending_dialog: Option<PendingJsDialog> = None;
    let mut handled_dialogs: Vec<(i64, u64)> = Vec::new();
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
        let mut frame_area = Rect::default();
        // Extract display name from browser (last path component for absolute paths).
        let browser_label = browser.rsplit('/').next().unwrap_or(&browser);
        terminal.draw(|frame| {
            frame_area = frame.area();
            viewport_rect = ui(
                frame,
                &url,
                &profile,
                &mode,
                &mut editor_state,
                &mut cmd_state,
                &page_title,
                is_devtools,
                inspected_tab_id,
                current_tab_id,
                &command_error,
                browser_label,
                &target_url,
                &pending_dialog,
                &loading_log,
                browser_ready,
                chromium_wait_start,
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
                        &browser,
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
                        &browser,
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

                // Loading stages (Issue 773).
                loading_log.push((LoadingStage::StartingBrowser, StageStatus::Done));
                loading_log.push((LoadingStage::WaitingForChromium, StageStatus::InProgress));
                chromium_wait_start = Some(Instant::now());
            }
        }

        // Unified event channel.
        // During loading, use a short timeout for smooth spinner animation and
        // to keep the GUI repainting (so the CALayerHost overlay appears).
        // After the page has fully loaded, keep polling for a 2-second grace
        // period so the GUI has time to create and display the overlay.
        // Then switch to blocking recv (Issue 668, 773).
        let needs_polling = if !page_loaded {
            true
        } else if let Some(at) = page_loaded_at {
            at.elapsed() < Duration::from_secs(2)
        } else {
            false
        };
        let event = if needs_polling {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(e) => Ok(e),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            rx.recv()
        };
        match event {
            Ok(LoopEvent::Terminal(Event::Key(key))) => {
                // Ctrl+C quits from any mode.
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }

                if let Some(dialog) = pending_dialog.as_mut() {
                    let mut reply: Option<(bool, String)> = None;
                    match dialog.dialog_type.as_str() {
                        "alert" => {
                            if key.code == KeyCode::Enter {
                                reply = Some((true, String::new()));
                            } else if key.code == KeyCode::Esc {
                                reply = Some((false, String::new()));
                            }
                        }
                        "confirm" | "beforeunload" => match key.code {
                            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                                reply = Some((true, String::new()));
                            }
                            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                reply = Some((false, String::new()));
                            }
                            _ => {}
                        },
                        "prompt" => match key.code {
                            KeyCode::Enter => {
                                reply = Some((true, dialog.input.clone()));
                            }
                            KeyCode::Esc => {
                                reply = Some((false, String::new()));
                            }
                            KeyCode::Backspace => {
                                dialog.input.pop();
                            }
                            KeyCode::Char(ch) => {
                                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT
                                {
                                    dialog.input.push(ch);
                                }
                            }
                            _ => {}
                        },
                        _ => {
                            reply = Some((false, String::new()));
                        }
                    }

                    if let Some((accepted, prompt_text)) = reply {
                        let tab_id = dialog.tab_id;
                        let request_id = dialog.request_id;
                        let previous_mode = dialog.previous_mode.clone();
                        if let Some(ref bc) = browser_conn {
                            bc.send_javascript_dialog_reply(request_id, accepted, &prompt_text);
                        }
                        if let Some(ref conn) = compositor {
                            conn.send_javascript_dialog_reply(
                                tab_id,
                                request_id,
                                accepted,
                                &prompt_text,
                            );
                        }
                        pending_dialog = None;
                        handled_dialogs.push((tab_id, request_id));
                        if handled_dialogs.len() > 32 {
                            handled_dialogs.remove(0);
                        }
                        mode = previous_mode;
                    }
                    continue;
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
                            // Edit mode keys are disabled in DevTools (Issue 687).
                            KeyCode::Char('i') if !is_devtools => {
                                // Insert mode, cursor at last position (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('A') if !is_devtools => {
                                // Insert mode, cursor at end of line (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.cursor.col =
                                    editor_state.lines.len_col(0).unwrap_or(0);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('I') if !is_devtools => {
                                // Insert mode, cursor at start of line (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.cursor = edtui::Index2::new(0, 0);
                                editor_state.mode = EditorMode::Insert;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('n') if !is_devtools => {
                                // Normal mode, cursor at last position (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                editor_state.mode = EditorMode::Normal;
                                editor_state.selection = None;
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('v') if !is_devtools => {
                                // Visual mode, cursor at last position (Issue 658).
                                enter_edit(&mut editor_state, &mut editor_url, &url, &mut mode);
                                SwitchMode(EditorMode::Visual).execute(&mut editor_state);
                                if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                    conn.send_mode_changed(pid, false);
                                }
                            }
                            KeyCode::Char('V') if !is_devtools => {
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
                            && !is_devtools
                        // Safety guard: no navigation in DevTools (Issue 687).
                        {
                            // Extract URL from editor, navigate, switch to Browse.
                            let new_url: String = editor_state
                                .lines
                                .get(RowIndex::new(0))
                                .map(|line| line.iter().collect())
                                .unwrap_or_default();
                            match resolve_input(&new_url) {
                                Some(resolved) => {
                                    url = resolved;
                                    editor_url = url.clone();
                                    mode = Mode::Browse;
                                    if let Some(ref bc) = browser_conn {
                                        bc.send_navigate(&url);
                                    } else if let (Some(ref conn), Some(ref pid)) =
                                        (&compositor, &pane_id)
                                    {
                                        conn.send_navigate(pid, &url);
                                    }
                                    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id)
                                    {
                                        conn.send_mode_changed(pid, true);
                                    }
                                }
                                None => {
                                    command_error =
                                        Some(format!("'{}' is not a URL or file", new_url));
                                    mode = Mode::Command;
                                }
                            }
                        } else {
                            // Pass everything else to edtui (including Escape).
                            editor_handler.on_key_event(key, &mut editor_state);
                        }
                    }
                    Mode::Command => {
                        // Esc in Normal mode exits Command → Control (Issue 665).
                        if key.code == KeyCode::Esc && cmd_state.mode == EditorMode::Normal {
                            command_error = None;
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
                                CommandResult::Dark(action) => {
                                    let dark = match action {
                                        DarkAction::Toggle => !is_dark,
                                        DarkAction::On => true,
                                        DarkAction::Off => false,
                                        DarkAction::System => false,
                                    };
                                    is_dark = dark;
                                    let scheme = if dark { "dark" } else { "light" };
                                    if let Some(ref bc) = browser_conn {
                                        bc.send_set_color_scheme(scheme);
                                    }
                                    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id)
                                    {
                                        conn.send_set_color_scheme(pid, scheme);
                                    }
                                }
                                CommandResult::DevTools(direction) => {
                                    if is_devtools {
                                        command_error = Some(
                                            "Cannot open DevTools from a DevTools pane".into(),
                                        );
                                    } else if let (Some(ref conn), Some(ref pid)) =
                                        (&compositor, &pane_id)
                                    {
                                        if !browser_ready || current_tab_id == 0 {
                                            command_error = Some(
                                                "Browser is still loading; try again in a moment"
                                                    .into(),
                                            );
                                            continue;
                                        }
                                        match conn.send_query_devtools(
                                            pid,
                                            current_tab_id,
                                            &profile,
                                            &browser,
                                        ) {
                                            Err(msg) => {
                                                command_error = Some(msg);
                                            }
                                            Ok(_) => {
                                                let cmd = format!(
                                                    "{} --browser {} --profile {} devtools://{}",
                                                    current_exe,
                                                    shell_quote_arg(&browser),
                                                    shell_quote_arg(&profile),
                                                    current_tab_id
                                                );
                                                conn.send_open_split(pid, &direction, &cmd);
                                            }
                                        }
                                    }
                                }
                                CommandResult::Error(msg) => {
                                    command_error = Some(msg);
                                }
                                CommandResult::None => {}
                            }
                            if command_error.is_none() {
                                mode = Mode::Control;
                            }
                        } else {
                            // Clear command error on any non-Enter keystroke (Issue 690).
                            command_error = None;
                            // Pass everything else to command edtui.
                            cmd_handler.on_key_event(key, &mut cmd_state);
                        }
                    }
                    Mode::Dialog => {}
                }
            }
            Ok(LoopEvent::Terminal(_)) => {
                // Resize, mouse, focus, paste, etc. — just redraw.
            }
            Ok(LoopEvent::Ipc(msg)) => {
                match msg {
                    ipc::CompositorMessage::ModeChanged { browsing } => {
                        mode = if browsing {
                            Mode::Browse
                        } else {
                            Mode::Control
                        };
                    }
                    ipc::CompositorMessage::UrlChanged { url: new_url } => {
                        url = new_url;
                        // Mark editor_url stale so enter_edit re-syncs (Issue 658).
                        editor_url.clear();
                    }
                    ipc::CompositorMessage::LoadingState {
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
                                // Loading stages (Issue 773).
                                for entry in loading_log.iter_mut() {
                                    if matches!(entry.0, LoadingStage::LoadingPage)
                                        && matches!(entry.1, StageStatus::InProgress)
                                    {
                                        entry.1 = StageStatus::Done;
                                    }
                                }
                                loading_log.push((LoadingStage::Ready, StageStatus::Done));
                                page_loaded = true;
                                page_loaded_at = Some(Instant::now());
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
                    ipc::CompositorMessage::TitleChanged { title } => {
                        page_title = title;
                    }
                    ipc::CompositorMessage::TargetUrlChanged { url: new_target } => {
                        target_url = new_target;
                    }
                    ipc::CompositorMessage::BrowserReady {
                        tab_id,
                        browser_socket,
                        browser: resolved_browser,
                    } => {
                        current_tab_id = tab_id;
                        if !resolved_browser.is_empty() {
                            browser = resolved_browser;
                        }
                        // Connect directly to the browser engine.
                        if let Some(conn) = ipc::BrowserConnection::connect(
                            &browser_socket,
                            tab_id,
                            browser_tx.clone(),
                        ) {
                            browser_conn = Some(conn);
                        }

                        // Loading stages (Issue 773).
                        browser_ready = true;
                        // Mark WaitingForChromium as done.
                        for entry in loading_log.iter_mut() {
                            if matches!(entry.0, LoadingStage::WaitingForChromium)
                                && matches!(entry.1, StageStatus::InProgress)
                            {
                                entry.1 = StageStatus::Done;
                            }
                        }
                        loading_log.push((LoadingStage::LoadingPage, StageStatus::InProgress));
                    }
                    ipc::CompositorMessage::JavaScriptDialogRequest {
                        tab_id,
                        request_id,
                        dialog_type,
                        origin_url,
                        message,
                        default_prompt_text,
                    } => {
                        let duplicate = pending_dialog
                            .as_ref()
                            .map(|dialog| {
                                dialog.tab_id == tab_id && dialog.request_id == request_id
                            })
                            .unwrap_or(false)
                            || handled_dialogs.contains(&(tab_id, request_id));
                        if !duplicate {
                            let previous_mode = mode.clone();
                            mode = Mode::Dialog;
                            pending_dialog = Some(PendingJsDialog {
                                tab_id,
                                request_id,
                                dialog_type,
                                origin_url,
                                message,
                                input: default_prompt_text.clone(),
                                default_prompt_text,
                                previous_mode,
                            });
                        }
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

        // Loading timeout: mark error if Chromium never connects (Issue 773).
        if !browser_ready {
            if let Some(start) = chromium_wait_start {
                if start.elapsed() >= Duration::from_secs(120) {
                    for entry in loading_log.iter_mut() {
                        if matches!(entry.0, LoadingStage::WaitingForChromium)
                            && matches!(entry.1, StageStatus::InProgress)
                        {
                            entry.1 = StageStatus::Error("Timeout — is Roamium installed?".into());
                        }
                    }
                    chromium_wait_start = None; // Don't keep re-triggering.
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

/// Resolve bare input to a URL or file:// path (Issue 693).
///
/// Returns `None` if the input is not recognizable as a URL, file, or command.
/// Callers should show an error for `None`.
fn resolve_input(input: &str) -> Option<String> {
    let trimmed = input.trim();

    // Step 1: Has a scheme — use as-is.
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }

    // Step 3: Explicit file paths (/, ./, ../).
    if trimmed.starts_with('/') || trimmed.starts_with("./") || trimmed.starts_with("../") {
        if let Ok(absolute) = std::fs::canonicalize(trimmed) {
            return Some(format!("file://{}", absolute.display()));
        }
    }

    // Step 4: Contains ":" — treat as host:port URL.
    if trimmed.contains(':') {
        let host = trimmed.split(':').next().unwrap_or(trimmed);
        if host.ends_with("localhost") || host.contains("localhost") {
            return Some(format!("http://{trimmed}"));
        }
        return Some(format!("https://{trimmed}"));
    }

    // Step 5: File exists — open as file.
    if let Ok(absolute) = std::fs::canonicalize(trimmed) {
        return Some(format!("file://{}", absolute.display()));
    }

    // Step 6: URL fallback (has a dot — looks like a domain).
    if trimmed.contains('.') {
        let host = trimmed.split('/').next().unwrap_or(trimmed);
        if host.ends_with("localhost") {
            return Some(format!("http://{trimmed}"));
        }
        return Some(format!("https://{trimmed}"));
    }

    // Step 7: Nothing matched.
    None
}

fn shell_quote_arg(value: &str) -> String {
    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
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
    is_devtools: bool,
    inspected_tab_id: i64,
    current_tab_id: i64,
    command_error: &Option<String>,
    browser_label: &str,
    target_url: &str,
    pending_dialog: &Option<PendingJsDialog>,
    loading_log: &[(LoadingStage, StageStatus)],
    browser_ready: bool,
    chromium_wait_start: Option<Instant>,
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
        Mode::Dialog => (YELLOW, YELLOW),
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
        // Red border on error, yellow otherwise (Issue 690).
        let border_color = if command_error.is_some() {
            RED
        } else {
            url_border
        };
        let cmd_title = Line::from(vec![
            Span::raw("COMMAND").style(Style::default().fg(border_color))
        ]);
        let mut cmd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color).bg(BG))
            .title_style(Style::default().fg(border_color))
            .title_top(cmd_title)
            .title_top(submode_label.alignment(Alignment::Right))
            .style(Style::default().bg(BG));
        if let Some(ref err) = command_error {
            cmd_block =
                cmd_block.title_bottom(Line::from(err.as_str()).style(Style::default().fg(RED)));
        }
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
    let identity_label = if is_devtools {
        format!("{}/{}#{}", browser_label, profile, inspected_tab_id)
    } else if current_tab_id > 0 {
        format!("{}/{}#{}", browser_label, profile, current_tab_id)
    } else {
        format!("{}/{}#loading", browser_label, profile)
    };

    let viewport_title = if is_devtools {
        format!("DevTools \u{00B7} {}", identity_label)
    } else if page_title.is_empty() {
        "Viewport".to_string()
    } else {
        page_title.to_string()
    };
    let engine_label = Line::from(vec![
        Span::raw("\u{F007} ").style(Style::default().fg(COMMENT)),
        Span::raw(identity_label).style(Style::default().fg(DIM)),
    ]);
    let mut viewport_block = Block::default()
        .borders(Borders::ALL)
        .title(viewport_title)
        .title_bottom(engine_label.alignment(Alignment::Right))
        .border_style(Style::default().fg(viewport_border).bg(BG))
        .title_style(Style::default().fg(viewport_border))
        .style(Style::default().bg(BG));
    if !target_url.is_empty() {
        let hover_label = Line::from(Span::raw(target_url).style(Style::default().fg(DIM)));
        viewport_block = viewport_block.title_bottom(hover_label);
    }
    let inner = viewport_block.inner(layout[0]);

    if let Some(dialog) = pending_dialog {
        let prompt_line = match dialog.dialog_type.as_str() {
            "alert" => "Enter accepts, Esc cancels".to_string(),
            "confirm" => "Enter/y accepts, n/Esc cancels".to_string(),
            "prompt" => format!(
                "Input: {}{}",
                dialog.input,
                if dialog.default_prompt_text.is_empty() {
                    ""
                } else {
                    " "
                }
            ),
            "beforeunload" => "Enter/y proceeds, n/Esc stays".to_string(),
            _ => "Enter accepts, Esc cancels".to_string(),
        };
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  JavaScript ").style(Style::default().fg(YELLOW).bg(BG)),
                Span::raw(dialog.dialog_type.as_str()).style(Style::default().fg(CYAN).bg(BG)),
            ]),
            Line::from(
                Span::raw(format!("  {}", dialog.origin_url)).style(Style::default().fg(DIM)),
            ),
            Line::from(""),
            Line::from(Span::raw(format!("  {}", dialog.message)).style(Style::default().fg(FG))),
            Line::from(""),
            Line::from(Span::raw(format!("  {}", prompt_line)).style(Style::default().fg(COMMENT))),
        ];
        let dialog_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(dialog_widget, layout[0]);
    } else if !browser_ready && !loading_log.is_empty() {
        // Render loading log (Issue 773).
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_frame = chromium_wait_start
            .map(|s| (s.elapsed().as_millis() / 100) as usize % SPINNER.len())
            .unwrap_or(0);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from("")); // top padding

        for (stage, status) in loading_log {
            let (icon, color) = match status {
                StageStatus::Done => ("✓", GREEN),
                StageStatus::InProgress => (SPINNER[spinner_frame], CYAN),
                StageStatus::Error(_) => ("✗", RED),
            };
            let mut spans = vec![
                Span::raw("  ").style(Style::default()),
                Span::raw(icon).style(Style::default().fg(color)),
                Span::raw(" ").style(Style::default()),
            ];
            match status {
                StageStatus::Error(msg) => {
                    spans.push(Span::raw(msg.clone()).style(Style::default().fg(color)));
                }
                _ => {
                    let mut label = stage.label().to_string();
                    // Show elapsed time for WaitingForChromium.
                    if matches!(stage, LoadingStage::WaitingForChromium)
                        && matches!(status, StageStatus::InProgress)
                    {
                        if let Some(start) = chromium_wait_start {
                            let elapsed = start.elapsed().as_secs();
                            label = format!("{} ({}s)", label, elapsed);
                        }
                    }
                    spans.push(Span::raw(label).style(Style::default().fg(color)));
                }
            }
            lines.push(Line::from(spans));
        }

        // Warnings based on elapsed time.
        if let Some(start) = chromium_wait_start {
            let elapsed = start.elapsed().as_secs();
            if elapsed >= 120 {
                // This is handled in the timeout below, but show inline too.
            } else if elapsed >= 30 {
                lines.push(Line::from(""));
                lines.push(Line::from(
                    Span::raw("    First launch is slow — Chromium is initializing")
                        .style(Style::default().fg(COMMENT)),
                ));
            }
        }

        let loading_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(loading_widget, layout[0]);
    } else {
        let viewport = Paragraph::new("")
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(viewport, layout[0]);
    }

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
        Mode::Control => {
            if is_devtools {
                // DevTools: no edit keys (Issue 687).
                Line::from(vec![
                    Span::styled(":q\u{23CE} ", f),
                    Span::styled("quit  ", d),
                    Span::styled("\u{23CE} ", f),
                    Span::styled("browse", d),
                ])
            } else {
                Line::from(vec![
                    Span::styled(":q\u{23CE} ", f),
                    Span::styled("quit  ", d),
                    Span::styled("i ", f),
                    Span::styled("edit url  ", d),
                    Span::styled("\u{23CE} ", f),
                    Span::styled("browse", d),
                ])
            }
        }
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
        Mode::Dialog => Line::from(vec![
            Span::styled("\u{23CE}/y ", f),
            Span::styled("accept  ", d),
            Span::styled("n/esc ", f),
            Span::styled("cancel", d),
        ]),
    };

    let label = match mode {
        Mode::Browse => "\u{F059F} BROWSE".to_string(),
        Mode::Control => "\u{F11C} CONTROL".to_string(),
        Mode::Edit => "\u{F044} EDIT".to_string(),
        Mode::Command => "\u{F120} COMMAND".to_string(),
        Mode::Dialog => "\u{F27A} DIALOG".to_string(),
    };

    let hints_widget = Paragraph::new(hints);
    frame.render_widget(hints_widget, status_layout[0]);

    let label_widget = Paragraph::new(label)
        .alignment(Alignment::Right)
        .style(Style::default().fg(FG).bg(BG));
    frame.render_widget(label_widget, status_layout[1]);

    inner
}
