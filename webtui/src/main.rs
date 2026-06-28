mod ipc;

use std::fs::{File, OpenOptions};
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
    Auth,
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

#[derive(Clone, PartialEq)]
enum AuthField {
    Username,
    Password,
}

#[derive(Clone)]
struct PendingHttpAuth {
    tab_id: i64,
    request_id: u64,
    url: String,
    auth_scheme: String,
    challenger: String,
    realm: String,
    is_proxy: bool,
    first_auth_attempt: bool,
    username: String,
    password: String,
    field: AuthField,
    previous_mode: Mode,
}

struct StateTrace {
    file: File,
}

impl StateTrace {
    fn from_env() -> Option<Self> {
        let path = std::env::var_os("TERMSURF_WEBTUI_STATE_TRACE_FILE")?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        Some(Self { file })
    }

    fn write(&mut self, event: &str, fields: &[(&str, String)]) {
        let _ = write!(self.file, "event={}", trace_field(event));
        for (key, value) in fields {
            let _ = write!(self.file, "\t{}={}", trace_field(key), trace_field(value));
        }
        let _ = writeln!(self.file);
        let _ = self.file.flush();
    }
}

fn trace_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[derive(Clone)]
struct ConsoleLogEntry {
    tab_id: i64,
    level: String,
    message: String,
    line_no: i32,
    source_id: String,
}

#[derive(Clone)]
struct RendererCrashState {
    tab_id: i64,
    termination_status: String,
    termination_status_code: i32,
    url: String,
    can_reload: bool,
}

// Loading screen stages (Issue 773).
#[derive(Clone)]
enum LoadingStage {
    ConnectingToGui,
    StartingBrowser,
    WaitingForBrowser,
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
            LoadingStage::WaitingForBrowser => "Waiting for browser",
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DarkAction {
    Toggle,
    On,
    Off,
    System,
}

#[derive(Debug, PartialEq, Eq)]
struct ResolvedDarkAction {
    dark: bool,
    source: &'static str,
}

fn parse_macos_interface_style_dark(output: &str) -> Option<bool> {
    match output.trim().to_ascii_lowercase().as_str() {
        "dark" => Some(true),
        "light" => Some(false),
        "" => None,
        _ => None,
    }
}

fn macos_defaults_color_scheme(status_success: bool, stdout: &[u8]) -> Option<bool> {
    if !status_success {
        return Some(false);
    }

    parse_macos_interface_style_dark(&String::from_utf8_lossy(stdout))
}

#[cfg(target_os = "macos")]
fn current_system_dark_mode() -> Option<(bool, &'static str)> {
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .ok()?;

    macos_defaults_color_scheme(output.status.success(), &output.stdout)
        .map(|dark| (dark, "macos-defaults"))
}

#[cfg(not(target_os = "macos"))]
fn current_system_dark_mode() -> Option<(bool, &'static str)> {
    None
}

fn resolve_dark_action(
    action: DarkAction,
    current_is_dark: bool,
    system_resolver: impl FnOnce() -> Option<(bool, &'static str)>,
) -> ResolvedDarkAction {
    match action {
        DarkAction::Toggle => ResolvedDarkAction {
            dark: !current_is_dark,
            source: "toggle",
        },
        DarkAction::On => ResolvedDarkAction {
            dark: true,
            source: "explicit-on",
        },
        DarkAction::Off => ResolvedDarkAction {
            dark: false,
            source: "explicit-off",
        },
        DarkAction::System => {
            if let Some((dark, source)) = system_resolver() {
                ResolvedDarkAction { dark, source }
            } else {
                ResolvedDarkAction {
                    dark: current_is_dark,
                    source: "current-state-fallback",
                }
            }
        }
    }
}

enum ViewportCommand {
    Height(u16),
    Reset,
}

enum CommandResult {
    Quit,
    Dark(DarkAction),
    Viewport(ViewportCommand),
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
        names: &["dark"],
        exec: |args| match args.first().copied() {
            None => CommandResult::Dark(DarkAction::Toggle),
            Some("on" | "yes" | "y") => CommandResult::Dark(DarkAction::On),
            Some("off" | "no" | "n") => CommandResult::Dark(DarkAction::Off),
            Some("system" | "s") => CommandResult::Dark(DarkAction::System),
            Some(other) => CommandResult::Error(format!("Unknown: {}", other)),
        },
    },
    Command {
        names: &["viewport", "vp"],
        exec: |args| match args.first().copied() {
            Some("height" | "h") => match args.get(1).copied() {
                Some(rows) => match rows.parse::<u16>() {
                    Ok(0) => CommandResult::Error("Viewport height must be greater than 0".into()),
                    Ok(rows) => CommandResult::Viewport(ViewportCommand::Height(rows)),
                    Err(_) => CommandResult::Error(format!("Invalid viewport height: {}", rows)),
                },
                None => CommandResult::Error("Usage: viewport height <rows>".into()),
            },
            Some("reset" | "r") => CommandResult::Viewport(ViewportCommand::Reset),
            Some(other) => CommandResult::Error(format!("Unknown viewport command: {}", other)),
            None => CommandResult::Error("Usage: viewport height <rows> | viewport reset".into()),
        },
    },
    Command {
        names: &["devtools", "dev"],
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

    /// Open an ephemeral private browser profile
    #[arg(long, global = true)]
    incognito: bool,

    /// Browser binary to use ("chromium", "plusium", or absolute path)
    #[arg(short, long, global = true)]
    browser: Option<String>,

    /// Render in the primary terminal screen instead of the alternate screen
    #[arg(long, global = true)]
    primary_screen: bool,
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
    if cli.incognito && profile_arg.as_deref().is_some_and(|p| p != "incognito") {
        eprintln!(
            "Error: --incognito cannot be combined with --profile unless the profile is incognito"
        );
        std::process::exit(1);
    }
    let mut profile = if cli.incognito {
        "incognito".to_string()
    } else {
        profile_arg.clone().unwrap_or_else(|| "default".to_string())
    };
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
            let query_profile = if cli.incognito {
                "incognito"
            } else {
                profile_arg.as_deref().unwrap_or("")
            };
            match conn.send_query_last(pid, query_profile) {
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

    let use_alternate_screen = !cli.primary_screen;

    // Enter raw mode and the requested terminal screen.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if use_alternate_screen {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    } else {
        execute!(stdout, EnableMouseCapture)?;
    }
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
    let mut console_log: Vec<ConsoleLogEntry> = Vec::new();
    let mut renderer_crash: Option<RendererCrashState> = None;
    let mut renderer_crash_recovery_load_started = false;
    let mut browser_wait_start: Option<Instant> = None;

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
    let mut pending_auth: Option<PendingHttpAuth> = None;
    let mut handled_dialogs: Vec<(i64, u64)> = Vec::new();
    let mut handled_auth: Vec<(i64, u64)> = Vec::new();
    let mut copy_url_feedback_until: Option<Instant> = None;
    let mut state_trace = StateTrace::from_env();
    let mut last_render_trace = String::new();
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
    let mut viewport_height_override: Option<u16> = None;

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
                &pending_auth,
                copy_url_feedback_until,
                &loading_log,
                &renderer_crash,
                browser_ready,
                browser_wait_start,
                viewport_height_override,
            );
        })?;
        if let Some(trace) = state_trace.as_mut() {
            let latest_console = console_log.last();
            let latest_console_summary = latest_console
                .map(|entry| {
                    format!(
                        "{}:{} #{} {} {}",
                        entry
                            .source_id
                            .rsplit('/')
                            .next()
                            .unwrap_or(&entry.source_id),
                        entry.line_no,
                        entry.tab_id,
                        entry.level,
                        entry.message
                    )
                })
                .unwrap_or_default();
            let (
                renderer_crash_active,
                renderer_crash_tab_id,
                renderer_crash_status,
                renderer_crash_can_reload,
            ) = renderer_crash
                .as_ref()
                .map(|crash| {
                    (
                        true,
                        crash.tab_id.to_string(),
                        crash.termination_status.clone(),
                        crash.can_reload.to_string(),
                    )
                })
                .unwrap_or_else(|| (false, String::new(), String::new(), String::new()));
            let identity_label = viewport_identity_label(
                browser_label,
                &profile,
                is_devtools,
                inspected_tab_id,
                current_tab_id,
            );
            let render_trace = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                url,
                page_title,
                target_url,
                page_loaded,
                browser_ready,
                loading_bar_active,
                renderer_crash_active,
                renderer_crash_tab_id,
                renderer_crash_status,
                renderer_crash_can_reload,
                latest_console_summary,
                identity_label,
                browser_label,
                profile,
                is_devtools,
                current_tab_id,
                inspected_tab_id
            );
            if render_trace != last_render_trace {
                trace.write(
                    "render_state",
                    &[
                        ("url", url.clone()),
                        ("title", page_title.clone()),
                        ("target_url", target_url.clone()),
                        ("page_loaded", page_loaded.to_string()),
                        ("browser_ready", browser_ready.to_string()),
                        ("loading_bar_active", loading_bar_active.to_string()),
                        ("renderer_crash_active", renderer_crash_active.to_string()),
                        ("renderer_crash_tab_id", renderer_crash_tab_id),
                        ("renderer_crash_status", renderer_crash_status),
                        ("renderer_crash_can_reload", renderer_crash_can_reload),
                        ("latest_console", latest_console_summary),
                        ("identity_label", identity_label),
                        ("browser_label", browser_label.to_string()),
                        ("profile", profile.clone()),
                        ("is_devtools", is_devtools.to_string()),
                        ("current_tab_id", current_tab_id.to_string()),
                        ("inspected_tab_id", inspected_tab_id.to_string()),
                    ],
                );
                last_render_trace = render_trace;
            }
        }

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
                loading_log.push((LoadingStage::WaitingForBrowser, StageStatus::InProgress));
                browser_wait_start = Some(Instant::now());
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
        } else if copy_url_feedback_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
        {
            true
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
                        let dialog_type = dialog.dialog_type.clone();
                        let message = dialog.message.clone();
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
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "javascript_dialog_reply",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("request_id", request_id.to_string()),
                                    ("dialog_type", dialog_type),
                                    ("message", message),
                                    ("accepted", accepted.to_string()),
                                    ("prompt_text", prompt_text.clone()),
                                ],
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

                if let Some(auth) = pending_auth.as_mut() {
                    let mut reply: Option<bool> = None;
                    match key.code {
                        KeyCode::Esc => reply = Some(false),
                        KeyCode::Enter => {
                            if auth.field == AuthField::Username {
                                auth.field = AuthField::Password;
                            } else {
                                reply = Some(true);
                            }
                        }
                        KeyCode::Tab => {
                            auth.field = if auth.field == AuthField::Username {
                                AuthField::Password
                            } else {
                                AuthField::Username
                            };
                        }
                        KeyCode::Backspace => {
                            if auth.field == AuthField::Username {
                                auth.username.pop();
                            } else {
                                auth.password.pop();
                            }
                        }
                        KeyCode::Char(ch) => {
                            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                                if auth.field == AuthField::Username {
                                    auth.username.push(ch);
                                } else {
                                    auth.password.push(ch);
                                }
                            }
                        }
                        _ => {}
                    }

                    if let Some(accepted) = reply {
                        let tab_id = auth.tab_id;
                        let request_id = auth.request_id;
                        let url = auth.url.clone();
                        let auth_scheme = auth.auth_scheme.clone();
                        let realm = auth.realm.clone();
                        let previous_mode = auth.previous_mode.clone();
                        let username = if accepted {
                            auth.username.clone()
                        } else {
                            String::new()
                        };
                        let password = if accepted {
                            auth.password.clone()
                        } else {
                            String::new()
                        };
                        if let Some(ref bc) = browser_conn {
                            bc.send_http_auth_reply(request_id, accepted, &username, &password);
                        }
                        if let Some(ref conn) = compositor {
                            conn.send_http_auth_reply(
                                tab_id, request_id, accepted, &username, &password,
                            );
                        }
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "http_auth_reply",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("request_id", request_id.to_string()),
                                    ("url", url),
                                    ("auth_scheme", auth_scheme),
                                    ("realm", realm),
                                    ("accepted", accepted.to_string()),
                                    ("username", username.clone()),
                                    ("password_len", password.chars().count().to_string()),
                                ],
                            );
                        }
                        pending_auth = None;
                        handled_auth.push((tab_id, request_id));
                        if handled_auth.len() > 32 {
                            handled_auth.remove(0);
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
                            KeyCode::Char('c') | KeyCode::Char('C')
                                if key.modifiers.contains(KeyModifiers::SUPER) && !is_devtools =>
                            {
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    let _ = clipboard.set_text(url.clone());
                                }
                                copy_url_feedback_until =
                                    Some(Instant::now() + Duration::from_millis(1500));
                                if let Some(trace) = state_trace.as_mut() {
                                    trace.write(
                                        "copy_current_url",
                                        &[
                                            ("tab_id", current_tab_id.to_string()),
                                            ("url", url.clone()),
                                            ("mode", "control".to_string()),
                                        ],
                                    );
                                }
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
                                    let resolved = resolve_dark_action(
                                        action,
                                        is_dark,
                                        current_system_dark_mode,
                                    );
                                    let action_label = match action {
                                        DarkAction::Toggle => "toggle",
                                        DarkAction::On => "on",
                                        DarkAction::Off => "off",
                                        DarkAction::System => "system",
                                    };
                                    let dark = resolved.dark;
                                    is_dark = dark;
                                    let scheme = if dark { "dark" } else { "light" };
                                    if let Some(trace) = state_trace.as_mut() {
                                        trace.write(
                                            "color_scheme_command",
                                            &[
                                                ("action", action_label.to_string()),
                                                ("scheme", scheme.to_string()),
                                                ("dark", dark.to_string()),
                                                ("source", resolved.source.to_string()),
                                                ("tab_id", current_tab_id.to_string()),
                                            ],
                                        );
                                    }
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
                                CommandResult::Viewport(command) => match command {
                                    ViewportCommand::Height(rows) => {
                                        viewport_height_override = Some(rows);
                                    }
                                    ViewportCommand::Reset => {
                                        viewport_height_override = None;
                                    }
                                },
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
                    Mode::Dialog | Mode::Auth => {}
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
                        if let Some(trace) = state_trace.as_mut() {
                            let mode_name = match mode {
                                Mode::Browse => "browse",
                                Mode::Control => "control",
                                Mode::Edit => "edit",
                                Mode::Command => "command",
                                Mode::Dialog => "dialog",
                                Mode::Auth => "auth",
                            };
                            trace.write(
                                "mode_changed",
                                &[
                                    ("source", "gui".to_string()),
                                    ("browsing", browsing.to_string()),
                                    ("mode", mode_name.to_string()),
                                ],
                            );
                        }
                    }
                    ipc::CompositorMessage::UrlChanged { url: new_url } => {
                        url = new_url;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write("url_changed", &[("url", url.clone())]);
                        }
                        // Mark editor_url stale so enter_edit re-syncs (Issue 658).
                        editor_url.clear();
                    }
                    ipc::CompositorMessage::LoadingState {
                        state,
                        _progress: progress,
                    } => {
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "loading_state",
                                &[("state", state.clone()), ("progress", progress.to_string())],
                            );
                        }
                        let mut stdout = io::stdout();
                        let _ = match state.as_str() {
                            "loading" => {
                                if renderer_crash.is_some() {
                                    renderer_crash = None;
                                    renderer_crash_recovery_load_started = true;
                                }
                                loading_bar_active = true;
                                loading_bar_start = Some(Instant::now());
                                write!(stdout, "\x1b]9;4;3\x1b\\")
                            }
                            "progress" => Ok(()),
                            "done" => {
                                if renderer_crash_recovery_load_started {
                                    renderer_crash = None;
                                    renderer_crash_recovery_load_started = false;
                                }
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
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write("title_changed", &[("title", page_title.clone())]);
                        }
                    }
                    ipc::CompositorMessage::TargetUrlChanged { url: new_target } => {
                        target_url = new_target;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write("target_url_changed", &[("url", target_url.clone())]);
                        }
                    }
                    ipc::CompositorMessage::ConsoleMessage {
                        tab_id,
                        level,
                        message,
                        line_no,
                        source_id,
                    } => {
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "console_message",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("level", level.clone()),
                                    ("message", message.clone()),
                                    ("line_no", line_no.to_string()),
                                    ("source_id", source_id.clone()),
                                ],
                            );
                        }
                        console_log.push(ConsoleLogEntry {
                            tab_id,
                            level,
                            message,
                            line_no,
                            source_id,
                        });
                        if console_log.len() > 100 {
                            let drain_count = console_log.len() - 100;
                            console_log.drain(0..drain_count);
                        }
                    }
                    ipc::CompositorMessage::RendererCrashed {
                        tab_id,
                        termination_status,
                        termination_status_code,
                        url,
                        can_reload,
                    } => {
                        loading_bar_active = false;
                        loading_bar_start = None;
                        renderer_crash_recovery_load_started = false;
                        if let Some(trace) = state_trace.as_mut() {
                            trace.write(
                                "renderer_crashed",
                                &[
                                    ("tab_id", tab_id.to_string()),
                                    ("status", termination_status.clone()),
                                    ("code", termination_status_code.to_string()),
                                    ("url", url.clone()),
                                    ("can_reload", can_reload.to_string()),
                                ],
                            );
                        }
                        renderer_crash = Some(RendererCrashState {
                            tab_id,
                            termination_status,
                            termination_status_code,
                            url,
                            can_reload,
                        });
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
                        // Mark WaitingForBrowser as done.
                        for entry in loading_log.iter_mut() {
                            if matches!(entry.0, LoadingStage::WaitingForBrowser)
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
                            if let Some(trace) = state_trace.as_mut() {
                                trace.write(
                                    "javascript_dialog_request",
                                    &[
                                        ("tab_id", tab_id.to_string()),
                                        ("request_id", request_id.to_string()),
                                        ("dialog_type", dialog_type.clone()),
                                        ("origin_url", origin_url.clone()),
                                        ("message", message.clone()),
                                        ("default_prompt_text", default_prompt_text.clone()),
                                    ],
                                );
                            }
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
                    ipc::CompositorMessage::HttpAuthRequest {
                        tab_id,
                        request_id,
                        url,
                        auth_scheme,
                        challenger,
                        realm,
                        is_proxy,
                        first_auth_attempt,
                    } => {
                        let duplicate = pending_auth
                            .as_ref()
                            .map(|auth| auth.tab_id == tab_id && auth.request_id == request_id)
                            .unwrap_or(false)
                            || handled_auth.contains(&(tab_id, request_id));
                        if !duplicate {
                            let previous_mode = mode.clone();
                            mode = Mode::Auth;
                            if let Some(trace) = state_trace.as_mut() {
                                trace.write(
                                    "http_auth_request",
                                    &[
                                        ("tab_id", tab_id.to_string()),
                                        ("request_id", request_id.to_string()),
                                        ("url", url.clone()),
                                        ("auth_scheme", auth_scheme.clone()),
                                        ("challenger", challenger.clone()),
                                        ("realm", realm.clone()),
                                        ("is_proxy", is_proxy.to_string()),
                                        ("first_auth_attempt", first_auth_attempt.to_string()),
                                    ],
                                );
                            }
                            pending_auth = Some(PendingHttpAuth {
                                tab_id,
                                request_id,
                                url,
                                auth_scheme,
                                challenger,
                                realm,
                                is_proxy,
                                first_auth_attempt,
                                username: String::new(),
                                password: String::new(),
                                field: AuthField::Username,
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

        // Loading timeout: mark error if browser never connects (Issue 773).
        if !browser_ready {
            if let Some(start) = browser_wait_start {
                if start.elapsed() >= Duration::from_secs(120) {
                    for entry in loading_log.iter_mut() {
                        if matches!(entry.0, LoadingStage::WaitingForBrowser)
                            && matches!(entry.1, StageStatus::InProgress)
                        {
                            entry.1 =
                                StageStatus::Error("Timeout — is the browser installed?".into());
                        }
                    }
                    browser_wait_start = None; // Don't keep re-triggering.
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
    if use_alternate_screen {
        execute!(
            terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        )?;
    } else {
        execute!(terminal.backend_mut(), DisableMouseCapture)?;
    }
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

fn viewport_identity_label(
    browser_label: &str,
    profile: &str,
    is_devtools: bool,
    inspected_tab_id: i64,
    current_tab_id: i64,
) -> String {
    if is_devtools {
        format!("{}/{}#{}", browser_label, profile, inspected_tab_id)
    } else if current_tab_id > 0 {
        format!("{}/{}#{}", browser_label, profile, current_tab_id)
    } else {
        format!("{}/{}#loading", browser_label, profile)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BrowserLayout {
    viewport_area: Rect,
    url_area: Rect,
    status_area: Rect,
}

fn browser_layout(area: Rect, viewport_height_override: Option<u16>) -> BrowserLayout {
    let layout = if let Some(rows) = viewport_height_override {
        let available = area.height.saturating_sub(4);
        let viewport_height = rows.saturating_add(2).clamp(1, available.max(1));
        Layout::vertical([
            Constraint::Length(3),               // URL bar
            Constraint::Length(1),               // Status bar
            Constraint::Length(viewport_height), // Viewport override
            Constraint::Min(0),                  // Filler
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(3), // URL bar (1 line + top/bottom border)
            Constraint::Length(1), // Status bar
            Constraint::Min(1),    // Viewport (fill remaining)
        ])
        .split(area)
    };

    BrowserLayout {
        url_area: layout[0],
        status_area: layout[1],
        viewport_area: layout[2],
    }
}

fn viewport_inner_rect(viewport_area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(viewport_area)
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
    pending_auth: &Option<PendingHttpAuth>,
    copy_url_feedback_until: Option<Instant>,
    loading_log: &[(LoadingStage, StageStatus)],
    renderer_crash: &Option<RendererCrashState>,
    browser_ready: bool,
    browser_wait_start: Option<Instant>,
    viewport_height_override: Option<u16>,
) -> Rect {
    // Paint full background.
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );

    let layout = browser_layout(frame.area(), viewport_height_override);
    let viewport_area = layout.viewport_area;
    let url_area = layout.url_area;
    let status_area = layout.status_area;

    // Border colors based on mode.
    let (url_border, viewport_border) = match mode {
        Mode::Browse => (BORDER, CYAN),
        Mode::Control => (CYAN, BORDER),
        Mode::Edit => (PURPLE, BORDER),
        Mode::Command => (YELLOW, BORDER),
        Mode::Dialog => (YELLOW, YELLOW),
        Mode::Auth => (YELLOW, YELLOW),
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
        let cmd_inner = cmd_block.inner(url_area);
        frame.render_widget(cmd_block, url_area);

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
            url_area,
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
        frame.render_widget(url_bar, url_area);
    }

    // Viewport.
    let identity_label = viewport_identity_label(
        browser_label,
        profile,
        is_devtools,
        inspected_tab_id,
        current_tab_id,
    );

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
    let inner = viewport_inner_rect(viewport_area);

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
        frame.render_widget(dialog_widget, viewport_area);
    } else if let Some(auth) = pending_auth {
        let password_mask = "*".repeat(auth.password.chars().count());
        let username_style = if auth.field == AuthField::Username {
            Style::default().fg(CYAN).bg(BG)
        } else {
            Style::default().fg(FG).bg(BG)
        };
        let password_style = if auth.field == AuthField::Password {
            Style::default().fg(CYAN).bg(BG)
        } else {
            Style::default().fg(FG).bg(BG)
        };
        let retry = if auth.first_auth_attempt {
            ""
        } else {
            " retry"
        };
        let target = if auth.is_proxy { "proxy" } else { "origin" };
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  HTTP Auth").style(Style::default().fg(YELLOW).bg(BG)),
                Span::raw(retry).style(Style::default().fg(RED).bg(BG)),
            ]),
            Line::from(Span::raw(format!("  {}", auth.url)).style(Style::default().fg(DIM).bg(BG))),
            Line::from(Span::raw(format!(
                "  {} {} {} realm={}",
                target, auth.auth_scheme, auth.challenger, auth.realm
            ))),
            Line::from(""),
            Line::from(vec![
                Span::raw("  Username: ").style(Style::default().fg(COMMENT).bg(BG)),
                Span::raw(auth.username.as_str()).style(username_style),
            ]),
            Line::from(vec![
                Span::raw("  Password: ").style(Style::default().fg(COMMENT).bg(BG)),
                Span::raw(password_mask).style(password_style),
            ]),
            Line::from(""),
            Line::from(
                Span::raw("  Enter advances/submits, Tab switches fields, Esc cancels")
                    .style(Style::default().fg(COMMENT).bg(BG)),
            ),
        ];
        let auth_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(auth_widget, viewport_area);
    } else if let Some(crash) = renderer_crash {
        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  ").style(Style::default()),
            Span::raw("Renderer crashed").style(Style::default().fg(RED)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(
            Span::raw("  Press Cmd+R to reload, or enter a new URL.")
                .style(Style::default().fg(FG)),
        ));
        let detail = if crash.url.is_empty() {
            format!(
                "  status={} code={} tab={}",
                crash.termination_status, crash.termination_status_code, crash.tab_id
            )
        } else {
            format!(
                "  status={} code={} tab={} url={}",
                crash.termination_status, crash.termination_status_code, crash.tab_id, crash.url
            )
        };
        lines.push(Line::from(
            Span::raw(detail).style(Style::default().fg(COMMENT)),
        ));
        if !crash.can_reload {
            lines.push(Line::from(
                Span::raw("  Reload is not available for this tab.")
                    .style(Style::default().fg(YELLOW)),
            ));
        }
        let crash_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(crash_widget, viewport_area);
    } else if !browser_ready && !loading_log.is_empty() {
        // Render loading log (Issue 773).
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_frame = browser_wait_start
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
                    // Show elapsed time for WaitingForBrowser.
                    if matches!(stage, LoadingStage::WaitingForBrowser)
                        && matches!(status, StageStatus::InProgress)
                    {
                        label = format!("Waiting for {browser_label}");
                        if let Some(start) = browser_wait_start {
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
        if let Some(start) = browser_wait_start {
            let elapsed = start.elapsed().as_secs();
            if elapsed < 120 {
                lines.push(Line::from(""));
                lines.push(Line::from(
                    Span::raw(
                        "    The first time you load a web browser it may take up to two minutes.",
                    )
                    .style(Style::default().fg(COMMENT)),
                ));
            } else {
                // This is handled in the timeout below, but show inline too.
            }
        }

        let loading_widget = Paragraph::new(lines)
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(loading_widget, viewport_area);
    } else {
        let viewport = Paragraph::new("")
            .style(Style::default().fg(FG).bg(BG))
            .block(viewport_block);
        frame.render_widget(viewport, viewport_area);
    }

    // Status bar.
    let status_layout = Layout::horizontal([
        Constraint::Fill(1),    // Key hints (left)
        Constraint::Length(14), // Mode label (right)
    ])
    .split(status_area);

    let d = Style::default().fg(DIM).bg(BG);
    let f = Style::default().fg(FG).bg(BG);

    let hints = if let Some(crash) = renderer_crash.as_ref() {
        Line::from(vec![
            Span::styled("renderer crashed ", Style::default().fg(RED).bg(BG)),
            Span::styled(
                format!(
                    "{} code={} #{}",
                    crash.termination_status, crash.termination_status_code, crash.tab_id
                ),
                d,
            ),
        ])
    } else {
        match mode {
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
                if copy_url_feedback_until
                    .map(|until| Instant::now() < until)
                    .unwrap_or(false)
                {
                    Line::from(vec![
                        Span::styled("url copied ", Style::default().fg(GREEN).bg(BG)),
                        Span::styled("\u{2318}c", d),
                    ])
                } else if is_devtools {
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
            Mode::Auth => Line::from(vec![
                Span::styled("\u{23CE} ", f),
                Span::styled("next/submit  ", d),
                Span::styled("tab ", f),
                Span::styled("field  ", d),
                Span::styled("esc ", f),
                Span::styled("cancel", d),
            ]),
        }
    };

    let label = match mode {
        Mode::Browse => "\u{F059F} BROWSE".to_string(),
        Mode::Control => "\u{F11C} CONTROL".to_string(),
        Mode::Edit => "\u{F044} EDIT".to_string(),
        Mode::Command => "\u{F120} COMMAND".to_string(),
        Mode::Dialog => "\u{F27A} DIALOG".to_string(),
        Mode::Auth => "\u{F023} AUTH".to_string(),
    };

    let hints_widget = Paragraph::new(hints);
    frame.render_widget(hints_widget, status_layout[0]);

    let label_widget = Paragraph::new(label)
        .alignment(Alignment::Right)
        .style(Style::default().fg(FG).bg(BG));
    frame.render_widget(label_widget, status_layout[1]);

    inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    struct RenderProbe {
        viewport: Rect,
        capture: String,
    }

    fn render_probe(
        mode: Mode,
        width: u16,
        height: u16,
        override_rows: Option<u16>,
    ) -> RenderProbe {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut editor_state = EditorState::new(Lines::from("https://example.test"));
        let mut cmd_state = EditorState::new(Lines::from("open https://example.test"));
        let mut viewport = Rect::default();

        terminal
            .draw(|frame| {
                viewport = ui(
                    frame,
                    "https://example.test",
                    "default",
                    &mode,
                    &mut editor_state,
                    &mut cmd_state,
                    "Viewport",
                    false,
                    -1,
                    1,
                    &None,
                    "roamium",
                    "",
                    &None,
                    &None,
                    None,
                    &[],
                    &None,
                    true,
                    None,
                    override_rows,
                );
            })
            .unwrap();

        RenderProbe {
            viewport,
            capture: numbered_buffer_capture(terminal.backend().buffer()),
        }
    }

    fn render_loading_probe(browser_label: &str) -> RenderProbe {
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut editor_state = EditorState::new(Lines::from("https://example.test"));
        let mut cmd_state = EditorState::new(Lines::from("open https://example.test"));
        let mut viewport = Rect::default();
        let loading_log = vec![
            (LoadingStage::ConnectingToGui, StageStatus::Done),
            (LoadingStage::StartingBrowser, StageStatus::Done),
            (LoadingStage::WaitingForBrowser, StageStatus::InProgress),
        ];

        terminal
            .draw(|frame| {
                viewport = ui(
                    frame,
                    "https://example.test",
                    "default",
                    &Mode::Control,
                    &mut editor_state,
                    &mut cmd_state,
                    "Viewport",
                    false,
                    -1,
                    0,
                    &None,
                    browser_label,
                    "",
                    &None,
                    &None,
                    None,
                    &loading_log,
                    &None,
                    false,
                    Some(Instant::now()),
                    None,
                );
            })
            .unwrap();

        RenderProbe {
            viewport,
            capture: numbered_buffer_capture(terminal.backend().buffer()),
        }
    }

    fn numbered_buffer_capture(buffer: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buffer.area.height {
            let mut row = String::new();
            for x in 0..buffer.area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            out.push_str(&format!("{y:02}: {row}\n"));
        }
        out
    }

    fn row_containing(capture: &str, needle: &str) -> u16 {
        capture
            .lines()
            .find_map(|line| {
                line.contains(needle)
                    .then(|| line[..2].parse::<u16>().unwrap())
            })
            .unwrap_or_else(|| panic!("missing {needle:?} in capture:\n{capture}"))
    }

    fn assert_controls_before_viewport(capture: &str, chrome_marker: &str, status_marker: &str) {
        let chrome_row = row_containing(capture, chrome_marker);
        let status_row = row_containing(capture, status_marker);
        let viewport_row = row_containing(capture, "Viewport");
        assert!(
            chrome_row < status_row,
            "chrome row should precede status row\n{capture}"
        );
        assert!(
            status_row < viewport_row,
            "status row should precede viewport row\n{capture}"
        );
    }

    fn assert_layout_invariants(mode: Mode, area: Rect, override_rows: Option<u16>) {
        let layout = browser_layout(area, override_rows);
        let inner = viewport_inner_rect(layout.viewport_area);
        assert!(
            layout.url_area.y < layout.status_area.y,
            "URL area should be above status area: {layout:?}"
        );
        assert!(
            layout.status_area.y < layout.viewport_area.y,
            "status area should be above viewport area: {layout:?}"
        );
        assert!(
            inner.y > layout.status_area.y,
            "inner viewport should start below controls: inner={inner:?} layout={layout:?}"
        );
        assert!(
            inner.width > 0 && inner.height > 0,
            "inner viewport should not collapse: inner={inner:?} layout={layout:?}"
        );

        let rendered = render_probe(mode, area.width, area.height, override_rows);
        assert_eq!(
            rendered.viewport, inner,
            "ui() return value must be the rect sent as overlay geometry"
        );
    }

    fn assert_devtools_command(input: &str, expected_direction: &str) {
        match dispatch(input) {
            CommandResult::DevTools(direction) => assert_eq!(direction, expected_direction),
            _ => panic!("{input:?} did not dispatch DevTools"),
        }
    }

    fn assert_dark_command(input: &str, expected: DarkAction) {
        match (dispatch(input), expected) {
            (CommandResult::Dark(DarkAction::Toggle), DarkAction::Toggle)
            | (CommandResult::Dark(DarkAction::On), DarkAction::On)
            | (CommandResult::Dark(DarkAction::Off), DarkAction::Off)
            | (CommandResult::Dark(DarkAction::System), DarkAction::System) => {}
            _ => panic!("{input:?} did not dispatch expected dark command"),
        }
    }

    #[test]
    fn command_aliases_follow_current_policy() {
        assert_devtools_command("dev", "right");
        assert_devtools_command("devtools", "right");

        assert!(matches!(dispatch("de"), CommandResult::None));
        assert!(matches!(dispatch("da"), CommandResult::None));

        assert_dark_command("dark", DarkAction::Toggle);
    }

    #[test]
    fn devtools_preserves_full_and_shorthand_directions() {
        for (input, expected) in [
            ("devtools right", "right"),
            ("devtools down", "down"),
            ("devtools left", "left"),
            ("devtools up", "up"),
            ("dev r", "right"),
            ("dev d", "down"),
            ("dev l", "left"),
            ("dev u", "up"),
        ] {
            assert_devtools_command(input, expected);
        }
    }

    #[test]
    fn dark_preserves_subcommand_shorthands() {
        assert_dark_command("dark on", DarkAction::On);
        assert_dark_command("dark y", DarkAction::On);
        assert_dark_command("dark off", DarkAction::Off);
        assert_dark_command("dark n", DarkAction::Off);
        assert_dark_command("dark system", DarkAction::System);
        assert_dark_command("dark s", DarkAction::System);
    }

    #[test]
    fn parses_macos_interface_style() {
        assert_eq!(parse_macos_interface_style_dark("Dark\n"), Some(true));
        assert_eq!(parse_macos_interface_style_dark("dark"), Some(true));
        assert_eq!(parse_macos_interface_style_dark("Light\n"), Some(false));
        assert_eq!(parse_macos_interface_style_dark(""), None);
        assert_eq!(parse_macos_interface_style_dark("Graphite"), None);
    }

    #[test]
    fn maps_missing_macos_interface_style_to_light() {
        assert_eq!(macos_defaults_color_scheme(false, b""), Some(false));
        assert_eq!(macos_defaults_color_scheme(true, b"Dark\n"), Some(true));
        assert_eq!(macos_defaults_color_scheme(true, b"Light\n"), Some(false));
        assert_eq!(macos_defaults_color_scheme(true, b"Graphite\n"), None);
    }

    #[test]
    fn resolves_system_dark_action_from_injected_resolver() {
        assert_eq!(
            resolve_dark_action(DarkAction::System, false, || {
                Some((true, "test-system"))
            }),
            ResolvedDarkAction {
                dark: true,
                source: "test-system",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::System, true, || {
                Some((false, "test-system"))
            }),
            ResolvedDarkAction {
                dark: false,
                source: "test-system",
            }
        );
    }

    #[test]
    fn system_dark_action_falls_back_to_current_state() {
        assert_eq!(
            resolve_dark_action(DarkAction::System, true, || None),
            ResolvedDarkAction {
                dark: true,
                source: "current-state-fallback",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::System, false, || None),
            ResolvedDarkAction {
                dark: false,
                source: "current-state-fallback",
            }
        );
    }

    #[test]
    fn loading_screen_uses_browser_label_and_immediate_warning() {
        for browser_label in ["surfari", "roamium"] {
            let rendered = render_loading_probe(browser_label);
            assert!(
                rendered
                    .capture
                    .contains(&format!("Waiting for {browser_label}")),
                "loading screen should name {browser_label}\n{}",
                rendered.capture
            );
            assert!(
                rendered
                    .capture
                    .contains("The first time you load a web browser"),
                "loading screen should show immediate engine-neutral warning\n{}",
                rendered.capture
            );
            assert!(
                !rendered.capture.contains("Chromium"),
                "loading screen should not mention Chromium\n{}",
                rendered.capture
            );
        }
    }

    #[test]
    fn explicit_dark_actions_do_not_call_system_resolver() {
        let resolver = || -> Option<(bool, &'static str)> {
            panic!("explicit dark actions should not query system appearance")
        };

        assert_eq!(
            resolve_dark_action(DarkAction::On, false, resolver),
            ResolvedDarkAction {
                dark: true,
                source: "explicit-on",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::Off, true, resolver),
            ResolvedDarkAction {
                dark: false,
                source: "explicit-off",
            }
        );
        assert_eq!(
            resolve_dark_action(DarkAction::Toggle, false, resolver),
            ResolvedDarkAction {
                dark: true,
                source: "toggle",
            }
        );
    }

    #[test]
    fn default_control_layout_places_controls_above_viewport() {
        let rendered = render_probe(Mode::Control, 80, 18, None);
        assert_controls_before_viewport(&rendered.capture, "URL", "edit url");
        assert_layout_invariants(Mode::Control, Rect::new(0, 0, 80, 18), None);
    }

    #[test]
    fn default_browse_layout_places_controls_above_viewport() {
        let rendered = render_probe(Mode::Browse, 80, 18, None);
        assert_controls_before_viewport(&rendered.capture, "URL", "back");
        assert_layout_invariants(Mode::Browse, Rect::new(0, 0, 80, 18), None);
    }

    #[test]
    fn issue_836_capture_documents_top_controls() {
        let control = render_probe(Mode::Control, 80, 18, None);
        let browse = render_probe(Mode::Browse, 80, 18, None);

        assert_controls_before_viewport(&control.capture, "URL", "edit url");
        assert_controls_before_viewport(&browse.capture, "URL", "back");

        println!("CONTROL MODE\n{}", control.capture);
        println!("BROWSE MODE\n{}", browse.capture);
    }

    #[test]
    fn edit_and_command_layouts_keep_chrome_above_viewport() {
        let edit = render_probe(Mode::Edit, 80, 18, None);
        assert_controls_before_viewport(&edit.capture, "URL", "navigate");
        assert_layout_invariants(Mode::Edit, Rect::new(0, 0, 80, 18), None);

        let command = render_probe(Mode::Command, 80, 18, None);
        assert_controls_before_viewport(&command.capture, "COMMAND", "execute");
        assert_layout_invariants(Mode::Command, Rect::new(0, 0, 80, 18), None);
    }

    #[test]
    fn viewport_override_keeps_controls_above_viewport() {
        let rendered = render_probe(Mode::Control, 80, 20, Some(5));
        assert_controls_before_viewport(&rendered.capture, "URL", "edit url");
        assert_layout_invariants(Mode::Control, Rect::new(0, 0, 80, 20), Some(5));
    }

    #[test]
    fn small_and_large_panes_keep_non_collapsed_viewport_below_controls() {
        assert_layout_invariants(Mode::Control, Rect::new(0, 0, 24, 7), None);
        assert_layout_invariants(Mode::Browse, Rect::new(0, 0, 120, 40), None);
    }
}
