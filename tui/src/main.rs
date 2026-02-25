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
use edtui::events::{KeyEventHandler, KeyEventRegister, KeyInput};
use edtui::{EditorEventHandler, EditorState, EditorTheme, EditorView, Lines, RowIndex};
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

#[derive(PartialEq)]
enum Mode {
    Browse,
    Control,
    UrlEdit,
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

    let mut mode = Mode::Browse;
    let mut last_viewport = Rect::default();
    let mut loading_bar_active = false;
    let mut loading_bar_start: Option<Instant> = None;
    const LOADING_TIMEOUT: Duration = Duration::from_secs(30);

    // edtui state (Issue 637).
    let mut editor_state = EditorState::new(Lines::from(url.as_str()));
    let mut editor_handler = {
        let mut kh = KeyEventHandler::vim_mode();
        // Remove newline keybindings for single-line mode.
        kh.remove(&KeyEventRegister::i(vec![KeyInput::new(KeyCode::Enter)]));
        kh.remove(&KeyEventRegister::n(vec![KeyInput::new('o')]));
        kh.remove(&KeyEventRegister::n(vec![KeyInput::shift('O')]));
        EditorEventHandler::new(kh)
    };

    // Event loop.
    loop {
        let mut viewport_rect = Rect::default();
        terminal.draw(|frame| {
            viewport_rect = ui(frame, &url, &profile, &mode, &mut editor_state);
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
                    Mode::Control => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('e') => {
                            // Initialize editor with current URL, cursor at end (Issue 637).
                            editor_state = EditorState::new(Lines::from(url.as_str()));
                            let len = url.len();
                            editor_state.cursor = edtui::Index2::new(0, len.saturating_sub(1));
                            mode = Mode::UrlEdit;
                            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                conn.send_mode_changed(pid, false);
                            }
                        }
                        KeyCode::Enter => {
                            mode = Mode::Browse;
                            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                conn.send_mode_changed(pid, true);
                            }
                        }
                        _ => {}
                    },
                    Mode::UrlEdit => match key.code {
                        KeyCode::Enter => {
                            // Extract URL from editor, navigate, switch to Browse.
                            let new_url: String = editor_state
                                .lines
                                .get(RowIndex::new(0))
                                .map(|line| line.iter().collect())
                                .unwrap_or_default();
                            url = new_url;
                            mode = Mode::Browse;
                            if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
                                conn.send_navigate(pid, &url);
                                conn.send_mode_changed(pid, true);
                            }
                        }
                        _ => {
                            // Pass everything else to edtui (including Escape).
                            editor_handler.on_key_event(key, &mut editor_state);
                        }
                    },
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
) -> Rect {
    // Paint full background.
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );

    let layout = Layout::vertical([
        Constraint::Length(3), // URL bar (1 line + top/bottom border)
        Constraint::Min(1),    // Viewport (fill remaining)
        Constraint::Length(1), // Status bar
    ])
    .split(frame.area());

    // Border colors based on mode.
    let (url_border, viewport_border) = match mode {
        Mode::Browse => (BORDER, CYAN),
        Mode::Control | Mode::UrlEdit => (CYAN, BORDER),
    };

    // URL bar.
    let profile_title = Line::from(vec![
        Span::raw(" \u{F007} ").style(Style::default().fg(COMMENT)),
        Span::raw(profile).style(Style::default().fg(FG)),
        Span::raw(" "),
    ]);

    if *mode == Mode::UrlEdit {
        let theme = EditorTheme::default()
            .base(Style::default().fg(FG).bg(BG))
            .cursor_style(Style::default().fg(BG).bg(FG))
            .selection_style(Style::default().fg(FG).bg(SELECTION))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title_top(profile_title.alignment(Alignment::Right))
                    .border_style(Style::default().fg(url_border).bg(BG))
                    .title_style(Style::default().fg(url_border))
                    .style(Style::default().bg(BG)),
            )
            .hide_status_line();
        frame.render_widget(
            EditorView::new(editor_state).theme(theme).wrap(false),
            layout[0],
        );
    } else {
        let url_bar = Paragraph::new(url).style(Style::default().fg(FG)).block(
            Block::default()
                .borders(Borders::ALL)
                .title_top(profile_title.alignment(Alignment::Right))
                .border_style(Style::default().fg(url_border).bg(BG))
                .title_style(Style::default().fg(url_border))
                .style(Style::default().bg(BG)),
        );
        frame.render_widget(url_bar, layout[0]);
    }

    // Viewport.
    let viewport_block = Block::default()
        .borders(Borders::ALL)
        .title(" Viewport ")
        .border_style(Style::default().fg(viewport_border).bg(BG))
        .title_style(Style::default().fg(viewport_border))
        .style(Style::default().bg(BG));
    let inner = viewport_block.inner(layout[1]);
    let viewport_text = format!(
        "origin: ({}, {})\nsize: {} x {}",
        inner.x, inner.y, inner.width, inner.height
    );
    let viewport = Paragraph::new(viewport_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(FG).bg(BG))
        .block(viewport_block);
    frame.render_widget(viewport, layout[1]);

    // Status bar.
    let status_layout = Layout::horizontal([
        Constraint::Fill(1),    // Key hints (left)
        Constraint::Length(12), // Mode label (right)
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
            Span::styled("e", f),
            Span::styled("> ", d),
            Span::styled("edit url  ", f),
            Span::styled("<", d),
            Span::styled("enter", f),
            Span::styled("> ", d),
            Span::styled("browse", f),
        ]),
        Mode::UrlEdit => Line::from(vec![
            Span::styled("<", d),
            Span::styled("enter", f),
            Span::styled("> ", d),
            Span::styled("navigate  ", f),
            Span::styled("<", d),
            Span::styled("ctrl+esc", f),
            Span::styled("> ", d),
            Span::styled("control", f),
        ]),
    };

    let label = match mode {
        Mode::Browse => "\u{F059F} BROWSE",
        Mode::Control => "\u{F11C} CONTROL",
        Mode::UrlEdit => "\u{F040} EDIT",
    };

    let hints_widget = Paragraph::new(hints);
    frame.render_widget(hints_widget, status_layout[0]);

    let label_widget = Paragraph::new(label)
        .alignment(Alignment::Right)
        .style(Style::default().fg(FG).bg(BG));
    frame.render_widget(label_widget, status_layout[1]);

    inner
}
