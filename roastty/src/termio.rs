//! Synchronous terminal/PTY coordination.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::os::pty::{PtyChild, PtyCommand, PtyReadiness, PtySize};
use crate::terminal::color;
use crate::terminal::cursor;
use crate::terminal::terminal::{
    Terminal, TerminalClipboardEvent, TerminalCommandEvent, TerminalDesktopNotification,
    TerminalInitError, TerminalInitOptions, TerminalStreamError,
};

mod shell_integration;

#[derive(Debug)]
pub(crate) struct Termio {
    terminal: Terminal,
    child: PtyChild,
    child_started_at: Instant,
    child_exit: Option<TermioChildExit>,
    output_buf: Vec<u8>,
    pending_write: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct TermioSpawnOptions {
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) cursor_visual_style: cursor::VisualStyle,
    pub(crate) cursor_blink: Option<bool>,
    pub(crate) shell_integration: crate::config::ShellIntegration,
    pub(crate) shell_integration_features: crate::config::ShellIntegrationFeatures,
    pub(crate) resource_dir: Option<PathBuf>,
    pub(crate) term: String,
    pub(crate) max_scrollback_bytes: Option<usize>,
    pub(crate) palette: color::Palette,
    pub(crate) image_storage_limit: usize,
    pub(crate) grapheme_cluster: bool,
    pub(crate) title_report: bool,
    pub(crate) enquiry_response: Vec<u8>,
    pub(crate) osc_color_report_format: crate::config::OscColorReportFormat,
    pub(crate) clipboard_write: crate::config::ClipboardAccess,
}

impl Default for TermioSpawnOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            env: Vec::new(),
            cursor_visual_style: cursor::VisualStyle::default(),
            cursor_blink: None,
            shell_integration: crate::config::ShellIntegration::Detect,
            shell_integration_features: crate::config::ShellIntegrationFeatures::default(),
            resource_dir: None,
            term: "xterm-roastty".to_string(),
            max_scrollback_bytes: None,
            palette: color::DEFAULT_PALETTE,
            image_storage_limit: crate::terminal::kitty::graphics_storage::DEFAULT_TOTAL_LIMIT,
            grapheme_cluster: crate::config::GraphemeWidthMethod::Unicode.grapheme_cluster(),
            title_report: false,
            enquiry_response: Vec::new(),
            osc_color_report_format: crate::config::OscColorReportFormat::Bits16,
            clipboard_write: crate::config::ClipboardAccess::Allow,
        }
    }
}

// Termio is transferred to the worker thread behind a Mutex. The raw pointers
// inside Terminal are owned terminal data structures, and worker access is
// serialized through the mutex. TermioWorker::spawn rejects terminals with
// installed callbacks because callback userdata may be thread-affine.
unsafe impl Send for Termio {}

#[derive(Debug)]
pub(crate) enum TermioError {
    Io(io::Error),
    TerminalInit(TerminalInitError),
    TerminalStream(TerminalStreamError),
    InvalidPtyReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TermioChildExit {
    pub(crate) exit_code: u32,
    pub(crate) runtime_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TermioPump {
    pub(crate) readiness: PtyReadiness,
    pub(crate) bytes_read: usize,
    pub(crate) bell_count: usize,
    pub(crate) titles: Vec<String>,
    pub(crate) pwd: Vec<String>,
    pub(crate) desktop_notifications: Vec<TerminalDesktopNotification>,
    pub(crate) command_events: Vec<TerminalCommandEvent>,
    pub(crate) eof: bool,
    pub(crate) bytes_written: usize,
    pub(crate) pending_write_bytes: usize,
    pub(crate) child_exited: bool,
    pub(crate) child_exit: Option<TermioChildExit>,
}

#[derive(Debug)]
pub(crate) struct TermioWorker {
    termio: Arc<Mutex<Termio>>,
    commands: Sender<TermioWorkerCommand>,
    events: Receiver<TermioWorkerEvent>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug)]
enum TermioWorkerCommand {
    Write(Vec<u8>),
    ResizePty(PtySize),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TermioWorkerEvent {
    Pump(TermioPump),
    Clipboard(TerminalClipboardEvent),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TermioWorkerError {
    CommandDisconnected,
    TerminalCallbacksInstalled,
    ThreadJoin,
}

impl Termio {
    pub(crate) fn spawn(
        program: impl Into<OsString>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        size: PtySize,
    ) -> Result<Self, TermioError> {
        Self::spawn_with_cwd(program, args, None, size)
    }

    pub(crate) fn spawn_with_cwd(
        program: impl Into<OsString>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        cwd: Option<PathBuf>,
        size: PtySize,
    ) -> Result<Self, TermioError> {
        Self::spawn_with_options(
            program,
            args,
            TermioSpawnOptions {
                cwd,
                env: Vec::new(),
                ..TermioSpawnOptions::default()
            },
            size,
        )
    }

    pub(crate) fn spawn_with_options(
        program: impl Into<OsString>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        options: TermioSpawnOptions,
        size: PtySize,
    ) -> Result<Self, TermioError> {
        let mut program = program.into();
        let mut args: Vec<OsString> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect();
        let env_override = options.env;
        let mut env = inherited_env();
        setup_terminal_identity(&mut env, options.resource_dir.as_deref(), &options.term);
        shell_integration::setup_features(
            &mut env,
            options.shell_integration_features,
            options.cursor_blink.unwrap_or(true),
        );
        if options.shell_integration.enabled() {
            if let Some(resource_dir) = options.resource_dir.as_deref() {
                let command = shell_integration::Command { program, args, env };
                let command =
                    shell_integration::setup(command, resource_dir, options.shell_integration);
                program = command.program;
                args = command.args;
                env = command.env;
            }
        }
        apply_env_overrides(&mut env, env_override);

        let mut terminal = Terminal::init_with_options(
            size.cols,
            size.rows,
            options.max_scrollback_bytes,
            TerminalInitOptions {
                cursor_visual_style: options.cursor_visual_style,
                cursor_blink: options.cursor_blink,
                grapheme_cluster: options.grapheme_cluster,
                title_report: options.title_report,
                enquiry_response: options.enquiry_response,
                osc_color_report_format: options.osc_color_report_format,
                clipboard_write: options.clipboard_write,
            },
        )?;
        terminal.set_palette_default(Some(palette_tuple(options.palette)));
        terminal.set_kitty_image_storage_limit(options.image_storage_limit);
        terminal.set_kitty_image_medium(crate::terminal::terminal::KittyImageMedium::File, true);
        terminal.set_kitty_image_medium(
            crate::terminal::terminal::KittyImageMedium::TemporaryFile,
            true,
        );
        terminal.set_kitty_image_medium(
            crate::terminal::terminal::KittyImageMedium::SharedMemory,
            true,
        );
        let mut command = PtyCommand::new(program, size);
        for arg in &args {
            command.arg(arg);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        if let Some(cwd) = options.cwd {
            command.cwd(cwd);
        }
        let child_started_at = Instant::now();
        let child = command.spawn()?;
        child.set_nonblocking()?;

        Ok(Self {
            terminal,
            child,
            child_started_at,
            child_exit: None,
            output_buf: Vec::new(),
            pending_write: Vec::new(),
        })
    }

    pub(crate) fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    pub(crate) fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }

    pub(crate) fn child_id(&self) -> u32 {
        self.child.child_id()
    }

    pub(crate) fn foreground_pid(&self) -> Option<u64> {
        self.child.foreground_pid()
    }

    pub(crate) fn tty_name(&self) -> Option<&str> {
        self.child.tty_name()
    }

    pub(crate) fn pending_write_bytes(&self) -> usize {
        self.pending_write.len()
    }

    pub(crate) fn queue_write(&mut self, bytes: &[u8]) {
        self.pending_write.extend_from_slice(bytes);
    }

    pub(crate) fn drain_clipboard_events(&mut self) -> Vec<TerminalClipboardEvent> {
        self.terminal.drain_clipboard_events()
    }

    pub(crate) fn pump_once(
        &mut self,
        timeout_ms: i32,
        max_read_bytes: usize,
    ) -> Result<TermioPump, TermioError> {
        let readiness = self.child.poll(timeout_ms)?;
        if readiness.invalid {
            return Err(TermioError::InvalidPtyReadiness);
        }

        let mut bytes_read = 0;
        let mut eof = false;
        if readiness.readable || readiness.hangup || readiness.error {
            self.output_buf.clear();
            let read = self
                .child
                .read_available(&mut self.output_buf, max_read_bytes)?;
            bytes_read = read.bytes_read;
            eof = read.eof;
            if !self.output_buf.is_empty() {
                match self.terminal.next_slice(&self.output_buf) {
                    Ok(()) => {}
                    Err(TerminalStreamError::ManagedCellUnsupported) => {
                        crate::append_ui_key_trace(
                            "rust termio_pump ignored managed-cell render error",
                        );
                    }
                    Err(err) => return Err(err.into()),
                }
                self.collect_terminal_response();
            }
        } else {
            self.collect_terminal_response();
        }
        let bell_count = self.terminal.take_pending_bell_count();
        let titles = self.terminal.take_pending_title_updates();
        let pwd = self.terminal.take_pending_pwd_updates();
        let desktop_notifications = self.terminal.take_pending_desktop_notifications();
        let command_events = self.terminal.take_pending_command_events();

        let bytes_written = self.flush_pending_write()?;
        if self.child_exit.is_none() {
            if let Some(status) = self.child.try_wait()? {
                self.child_exit = Some(TermioChildExit {
                    exit_code: exit_status_code(status),
                    runtime_ms: self.child_started_at.elapsed().as_millis() as u64,
                });
            }
        }
        let child_exit = self.child_exit;
        let child_exited = child_exit.is_some();

        Ok(TermioPump {
            readiness,
            bytes_read,
            bell_count,
            titles,
            pwd,
            desktop_notifications,
            command_events,
            eof,
            bytes_written,
            pending_write_bytes: self.pending_write.len(),
            child_exited,
            child_exit,
        })
    }

    pub(crate) fn resize_pty(&self, size: PtySize) -> Result<(), TermioError> {
        self.child.resize(size)?;
        Ok(())
    }

    pub(crate) fn pty_size(&self) -> Result<PtySize, TermioError> {
        Ok(self.child.size()?)
    }

    fn collect_terminal_response(&mut self) {
        if self.terminal.pty_response().is_empty() {
            return;
        }
        self.pending_write
            .extend_from_slice(self.terminal.pty_response());
        self.terminal.clear_pty_response();
    }

    fn flush_pending_write(&mut self) -> Result<usize, TermioError> {
        let mut total = 0;
        while !self.pending_write.is_empty() {
            match self.child.write(&self.pending_write) {
                Ok(0) => break,
                Ok(written) => {
                    total += written;
                    self.pending_write.drain(..written);
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                Err(err) => return Err(TermioError::Io(err)),
            }
        }
        Ok(total)
    }
}

fn exit_status_code(status: ExitStatus) -> u32 {
    status.code().map(|code| code as u32).unwrap_or(u32::MAX)
}

fn palette_tuple(palette: color::Palette) -> [(u8, u8, u8); 256] {
    let mut result = [(0, 0, 0); 256];
    for (index, rgb) in palette.into_iter().enumerate() {
        result[index] = (rgb.r, rgb.g, rgb.b);
    }
    result
}

fn setup_terminal_identity(
    env: &mut Vec<(String, String)>,
    resource_dir: Option<&Path>,
    term: &str,
) {
    match resource_dir {
        Some(resource_dir) => {
            put_env(
                env,
                "ROASTTY_RESOURCES_DIR",
                resource_dir.display().to_string(),
            );
            put_env(env, "TERM", term.to_string());
            put_env(env, "COLORTERM", "truecolor".to_string());
            if let Some(parent) = resource_dir.parent() {
                put_env(
                    env,
                    "TERMINFO",
                    parent.join("terminfo").display().to_string(),
                );
            }
        }
        None => {
            put_env(env, "TERM", "xterm-256color".to_string());
            put_env(env, "COLORTERM", "truecolor".to_string());
        }
    }
}

fn inherited_env() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

fn apply_env_overrides(env: &mut Vec<(String, String)>, overrides: Vec<(String, String)>) {
    for (key, value) in overrides {
        put_env(env, &key, value);
    }
}

fn put_env(env: &mut Vec<(String, String)>, key: &str, value: String) {
    env.retain(|(existing, _)| existing != key);
    env.push((key.to_string(), value));
}

impl TermioWorker {
    pub(crate) fn spawn(
        termio: Termio,
        pump_timeout_ms: i32,
        max_read_bytes: usize,
    ) -> Result<Self, TermioWorkerError> {
        crate::append_ui_key_trace(format!(
            "rust termio_worker_spawn begin pump_timeout_ms={} max_read_bytes={} has_callbacks={}",
            pump_timeout_ms,
            max_read_bytes,
            termio.terminal.has_effect_callbacks()
        ));
        if termio.terminal.has_effect_callbacks() {
            crate::append_ui_key_trace(
                "rust termio_worker_spawn result=error reason=terminal-callbacks-installed",
            );
            return Err(TermioWorkerError::TerminalCallbacksInstalled);
        }

        let termio = Arc::new(Mutex::new(termio));
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread_termio = Arc::clone(&termio);

        let join = thread::spawn(move || {
            crate::append_ui_key_trace("rust termio_worker_thread begin");
            run_termio_worker(
                thread_termio,
                command_rx,
                event_tx,
                pump_timeout_ms,
                max_read_bytes,
            );
            crate::append_ui_key_trace("rust termio_worker_thread end");
        });

        crate::append_ui_key_trace("rust termio_worker_spawn result=success");
        Ok(Self {
            termio,
            commands: command_tx,
            events: event_rx,
            join: Some(join),
        })
    }

    pub(crate) fn queue_write(&self, bytes: &[u8]) -> Result<(), TermioWorkerError> {
        crate::append_ui_key_trace(format!(
            "rust termio_worker_queue_write len={}",
            bytes.len()
        ));
        self.commands
            .send(TermioWorkerCommand::Write(bytes.to_vec()))
            .map_err(|_| TermioWorkerError::CommandDisconnected)
    }

    pub(crate) fn resize_pty(&self, size: PtySize) -> Result<(), TermioWorkerError> {
        self.commands
            .send(TermioWorkerCommand::ResizePty(size))
            .map_err(|_| TermioWorkerError::CommandDisconnected)
    }

    pub(crate) fn try_recv_event(&self) -> Option<TermioWorkerEvent> {
        self.events.try_recv().ok()
    }

    pub(crate) fn with_termio<R>(&self, f: impl FnOnce(&Termio) -> R) -> R {
        let termio = self.termio.lock().expect("termio worker mutex poisoned");
        f(&termio)
    }

    pub(crate) fn with_termio_mut<R>(&self, f: impl FnOnce(&mut Termio) -> R) -> R {
        let mut termio = self.termio.lock().expect("termio worker mutex poisoned");
        f(&mut termio)
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), TermioWorkerError> {
        if self.join.is_none() {
            return Ok(());
        }

        let _ = self.commands.send(TermioWorkerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            join.join().map_err(|_| TermioWorkerError::ThreadJoin)?;
        }
        Ok(())
    }
}

impl Drop for TermioWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn run_termio_worker(
    termio: Arc<Mutex<Termio>>,
    commands: Receiver<TermioWorkerCommand>,
    events: Sender<TermioWorkerEvent>,
    pump_timeout_ms: i32,
    max_read_bytes: usize,
) {
    loop {
        match drain_worker_commands(&termio, &commands, &events) {
            WorkerCommandState::Continue => {}
            WorkerCommandState::Stop => {
                crate::append_ui_key_trace("rust termio_worker_loop exit reason=command-stop");
                break;
            }
        }

        let pump_started = std::time::Instant::now();
        let pump = {
            let mut termio = termio.lock().expect("termio worker mutex poisoned");
            termio.pump_once(pump_timeout_ms, max_read_bytes)
        };
        let pump_duration_ms = pump_started.elapsed().as_secs_f64() * 1000.0;

        match pump {
            Ok(pump) => {
                let clipboard_events = {
                    let mut termio = termio.lock().expect("termio worker mutex poisoned");
                    termio.drain_clipboard_events()
                };
                for clipboard_event in clipboard_events {
                    if events
                        .send(TermioWorkerEvent::Clipboard(clipboard_event))
                        .is_err()
                    {
                        return;
                    }
                }
                let should_emit = pump.bytes_read > 0
                    || pump.bell_count > 0
                    || !pump.titles.is_empty()
                    || !pump.pwd.is_empty()
                    || !pump.desktop_notifications.is_empty()
                    || !pump.command_events.is_empty()
                    || pump.bytes_written > 0
                    || pump.pending_write_bytes > 0
                    || pump.eof
                    || pump.child_exited;
                let child_exited = pump.child_exited;
                let eof = pump.eof;
                let bytes_read = pump.bytes_read;
                let bytes_written = pump.bytes_written;
                let pending_write_bytes = pump.pending_write_bytes;
                if should_emit {
                    crate::append_ui_key_trace(format!(
                        "rust termio_worker_pump emit bytes_read={} bytes_written={} pending_write={} duration_ms={:.3}",
                        bytes_read,
                        bytes_written,
                        pending_write_bytes,
                        pump_duration_ms
                    ));
                }
                if should_emit && events.send(TermioWorkerEvent::Pump(pump)).is_err() {
                    crate::append_ui_key_trace(
                        "rust termio_worker_loop exit reason=event-receiver-disconnected",
                    );
                    break;
                }
                if child_exited {
                    crate::append_ui_key_trace(format!(
                        "rust termio_worker_loop exit reason=pump-terminal eof={} child_exited={} bytes_read={} bytes_written={} pending_write={}",
                        eof,
                        child_exited,
                        bytes_read,
                        bytes_written,
                        pending_write_bytes
                    ));
                    break;
                }
            }
            Err(err) => {
                crate::append_ui_key_trace(format!(
                    "rust termio_worker_loop exit reason=pump-error duration_ms={pump_duration_ms:.3} error={err:?}"
                ));
                let _ = events.send(TermioWorkerEvent::Error(format!("{err:?}")));
                break;
            }
        }
    }
}

enum WorkerCommandState {
    Continue,
    Stop,
}

fn drain_worker_commands(
    termio: &Arc<Mutex<Termio>>,
    commands: &Receiver<TermioWorkerCommand>,
    events: &Sender<TermioWorkerEvent>,
) -> WorkerCommandState {
    loop {
        match commands.try_recv() {
            Ok(TermioWorkerCommand::Write(bytes)) => {
                crate::append_ui_key_trace(format!(
                    "rust termio_worker_command write len={}",
                    bytes.len()
                ));
                let mut termio = termio.lock().expect("termio worker mutex poisoned");
                termio.queue_write(&bytes);
            }
            Ok(TermioWorkerCommand::ResizePty(size)) => {
                crate::append_ui_key_trace(format!(
                    "rust termio_worker_command resize rows={} cols={} width_px={} height_px={}",
                    size.rows, size.cols, size.width_px, size.height_px
                ));
                let result = {
                    let termio = termio.lock().expect("termio worker mutex poisoned");
                    termio.resize_pty(size)
                };
                if let Err(err) = result {
                    crate::append_ui_key_trace(format!(
                        "rust termio_worker_command resize-error error={err:?}"
                    ));
                    let _ = events.send(TermioWorkerEvent::Error(format!("{err:?}")));
                    return WorkerCommandState::Stop;
                }
            }
            Ok(TermioWorkerCommand::Shutdown) => {
                crate::append_ui_key_trace("rust termio_worker_command shutdown");
                return WorkerCommandState::Stop;
            }
            Err(mpsc::TryRecvError::Empty) => return WorkerCommandState::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                crate::append_ui_key_trace("rust termio_worker_command disconnected");
                return WorkerCommandState::Stop;
            }
        }
    }
}

impl From<io::Error> for TermioError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<TerminalInitError> for TermioError {
    fn from(value: TerminalInitError) -> Self {
        Self::TerminalInit(value)
    }
}

impl From<TerminalStreamError> for TermioError {
    fn from(value: TerminalStreamError) -> Self {
        Self::TerminalStream(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os::pty::pty_command_lock;
    use crate::terminal::osc;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    static TERMIO_TEST_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    unsafe extern "C" fn test_bell_callback(_: *mut c_void, _: *mut c_void) {}

    fn test_size() -> PtySize {
        PtySize {
            rows: 24,
            cols: 80,
            width_px: 800,
            height_px: 600,
        }
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let counter = TERMIO_TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "roastty-termio-{}-{counter}-{label}",
            std::process::id()
        ))
    }

    fn spawn_shell(script: &str) -> Termio {
        Termio::spawn("/bin/sh", ["-c", script], test_size()).expect("spawn termio")
    }

    fn pump_until<F>(termio: &mut Termio, mut done: F) -> TermioPump
    where
        F: FnMut(&Termio, &TermioPump) -> bool,
    {
        let mut last = None;
        for _ in 0..20 {
            let pump = termio.pump_once(500, 4096).expect("pump termio");
            if done(termio, &pump) {
                return pump;
            }
            last = Some(pump);
        }
        panic!("condition not met after pumps: {last:?}");
    }

    fn spawn_worker(script: &str) -> TermioWorker {
        TermioWorker::spawn(spawn_shell(script), 10, 4096).expect("spawn worker")
    }

    fn worker_event_until<F>(worker: &TermioWorker, mut done: F) -> TermioWorkerEvent
    where
        F: FnMut(&TermioWorker, &TermioWorkerEvent) -> bool,
    {
        let mut last = None;
        for _ in 0..100 {
            if let Some(event) = worker.try_recv_event() {
                if done(worker, &event) {
                    return event;
                }
                last = Some(event);
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("worker event condition not met: {last:?}");
    }

    fn worker_events_until<F>(worker: &TermioWorker, mut done: F) -> Vec<TermioWorkerEvent>
    where
        F: FnMut(&[TermioWorkerEvent]) -> bool,
    {
        let mut events = Vec::new();
        for _ in 0..100 {
            while let Some(event) = worker.try_recv_event() {
                events.push(event);
                if done(&events) {
                    return events;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("worker events condition not met: {events:?}");
    }

    fn wait_until<F>(mut done: F)
    where
        F: FnMut() -> bool,
    {
        for _ in 0..100 {
            if done() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("condition not met");
    }

    #[test]
    fn pump_once_delivers_child_output_to_terminal() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("printf hello");

        let pump = pump_until(&mut termio, |termio, pump| {
            pump.bytes_read > 0 && termio.terminal().plain_screen(false).contains("hello")
        });

        assert_eq!(pump.bytes_read, 5);
        assert!(termio.terminal().plain_screen(false).contains("hello"));
    }

    #[test]
    fn termio_bell_pump_reports_child_bel_output() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("printf '\\a\\a'");

        let pump = pump_until(&mut termio, |_, pump| pump.bell_count == 2);

        assert_eq!(pump.bell_count, 2);
        assert_eq!(termio.terminal_mut().take_pending_bell_count(), 0);
    }

    #[test]
    fn termio_desktop_notification_runtime_pump_reports_child_osc() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("printf '\\033]9;Hello from pty\\007'");

        let pump = pump_until(&mut termio, |_, pump| {
            !pump.desktop_notifications.is_empty()
        });

        assert_eq!(
            pump.desktop_notifications,
            vec![TerminalDesktopNotification {
                title: Vec::new(),
                body: b"Hello from pty".to_vec(),
            }]
        );
        assert!(termio
            .terminal_mut()
            .take_pending_desktop_notifications()
            .is_empty());
    }

    #[test]
    fn termio_command_event_runtime_pump_reports_child_osc133() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("printf '\\033]133;C\\007\\033]133;D;7\\007'");

        let pump = pump_until(&mut termio, |_, pump| !pump.command_events.is_empty());

        assert_eq!(
            pump.command_events,
            vec![
                TerminalCommandEvent::Start,
                TerminalCommandEvent::Stop { exit_code: 7 },
            ]
        );
        assert!(termio
            .terminal_mut()
            .take_pending_command_events()
            .is_empty());
    }

    #[test]
    fn queue_write_reaches_child_and_output_returns_to_terminal() {
        let _guard = pty_command_lock();
        let mut termio =
            spawn_shell("stty -echo; printf ready; IFS= read line; printf 'out:%s' \"$line\"");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("ready")
        });
        termio.queue_write(b"hello\n");

        let pump = pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("out:hello")
        });

        assert_eq!(pump.pending_write_bytes, 0);
        assert!(termio.terminal().plain_screen(false).contains("out:hello"));
    }

    #[test]
    fn terminal_response_flushes_to_child_without_dropping_bytes() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell(
            "stty raw -echo min 0 time 10; \
             printf '\\033[c'; \
             response=$(dd bs=1 count=12 2>/dev/null); \
             expected=$(printf '\\033[?62;22;52c'); \
             if [ \"$response\" = \"$expected\" ]; then printf da-ok; else printf da-bad; fi",
        );

        let first = pump_until(&mut termio, |_, pump| pump.bytes_written > 0);
        assert_eq!(first.bytes_written, b"\x1b[?62;22;52c".len());
        assert_eq!(first.pending_write_bytes, 0);
        assert_eq!(termio.pending_write_bytes(), 0);

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("da-ok")
        });
        assert!(termio.terminal().plain_screen(false).contains("da-ok"));
    }

    #[test]
    fn termio_device_attributes_clipboard_write_reaches_child_pty() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            [
                "-c",
                "stty raw -echo min 0 time 10; \
                 printf '\\033[c'; \
                 response=$(dd bs=1 count=9 2>/dev/null); \
                 expected=$(printf '\\033[?62;22c'); \
                 if [ \"$response\" = \"$expected\" ]; then printf deny-ok; else printf 'bad:%s' \"$response\"; fi",
            ],
            TermioSpawnOptions {
                clipboard_write: crate::config::ClipboardAccess::Deny,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with clipboard-write deny");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("deny-ok")
        });

        assert!(termio.terminal().plain_screen(false).contains("deny-ok"));
    }

    #[test]
    fn resize_pty_updates_reported_winsize() {
        let _guard = pty_command_lock();
        let termio = Termio::spawn("/bin/sleep", ["1"], test_size()).expect("spawn termio");
        let resized = PtySize {
            rows: 33,
            cols: 101,
            width_px: 1001,
            height_px: 777,
        };

        termio.resize_pty(resized).expect("resize pty");

        assert_eq!(termio.pty_size().expect("get pty size"), resized);
    }

    #[test]
    fn pump_once_reports_child_exit() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("printf done");

        let pump = pump_until(&mut termio, |_, pump| pump.child_exited || pump.eof);

        assert!(pump.child_exited || pump.eof);
    }

    #[test]
    fn child_exited_payload_runtime_success_exit_code() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("exit 0");

        let pump = pump_until(&mut termio, |_, pump| pump.child_exit.is_some());
        let child_exit = pump.child_exit.expect("child exit payload");

        assert!(pump.child_exited);
        assert_eq!(child_exit.exit_code, 0);
    }

    #[test]
    fn child_exited_payload_runtime_failure_exit_code() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("exit 42");

        let pump = pump_until(&mut termio, |_, pump| pump.child_exit.is_some());
        let child_exit = pump.child_exit.expect("child exit payload");

        assert!(pump.child_exited);
        assert_eq!(child_exit.exit_code, 42);
    }

    #[test]
    fn child_exited_payload_runtime_reports_elapsed_runtime() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("sleep 0.05; exit 7");

        let pump = pump_until(&mut termio, |_, pump| pump.child_exit.is_some());
        let child_exit = pump.child_exit.expect("child exit payload");

        assert!(pump.child_exited);
        assert_eq!(child_exit.exit_code, 7);
        assert!(child_exit.runtime_ms >= 20, "{child_exit:?}");
    }

    #[test]
    fn accessors_expose_terminal_child_id_and_pending_count() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn("/bin/sleep", ["1"], test_size()).expect("spawn termio");

        assert!(termio.child_id() > 0);
        assert_eq!(termio.foreground_pid(), Some(u64::from(termio.child_id())));
        let tty_name = termio.tty_name().expect("tty name");
        assert!(tty_name.starts_with("/dev/"), "{tty_name}");
        assert_eq!(termio.pending_write_bytes(), 0);
        termio.queue_write(b"x");
        assert_eq!(termio.pending_write_bytes(), 1);
        assert_eq!(termio.terminal_mut().pty_response(), b"");
    }

    #[test]
    fn termio_enquiry_response_reaches_child_pty() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/bash",
            [
                "-lc",
                "printf '\\005'; IFS= read -r -n 14 response; printf 'got:%s' \"$response\"",
            ],
            TermioSpawnOptions {
                enquiry_response: b"roastty-enq-42".to_vec(),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with enquiry response");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("got:roastty-enq-42")
        });

        assert!(termio
            .terminal()
            .plain_screen(false)
            .contains("got:roastty-enq-42"));
    }

    #[test]
    fn termio_osc_color_report_format_reaches_child_pty() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/bash",
            [
                "-lc",
                "printf '\\033]4;4;#123456\\033\\\\\\033]4;4;?\\033\\\\'; IFS= read -r -d '\\\\' response; case \"$response\" in *'rgb:12/34/56'*) printf got-8-bit;; *) printf 'bad:%s' \"$response\";; esac",
            ],
            TermioSpawnOptions {
                osc_color_report_format: crate::config::OscColorReportFormat::Bits8,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with OSC color report format");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("got-8-bit")
        });

        assert!(termio.terminal().plain_screen(false).contains("got-8-bit"));
    }

    #[test]
    fn spawn_with_cwd_runs_child_in_requested_directory() {
        let _guard = pty_command_lock();
        let cwd = std::env::current_dir().expect("current dir");
        let mut termio = Termio::spawn_with_cwd(
            "/bin/pwd",
            std::iter::empty::<&str>(),
            Some(cwd.clone()),
            test_size(),
        )
        .expect("spawn termio with cwd");

        pump_until(&mut termio, |termio, pump| {
            pump.bytes_read > 0
                && termio
                    .terminal()
                    .plain_screen(false)
                    .contains(cwd.to_str().unwrap())
        });

        assert!(termio
            .terminal()
            .plain_screen(false)
            .contains(cwd.to_str().unwrap()));
    }

    #[test]
    fn spawn_with_cwd_reports_missing_directory() {
        let _guard = pty_command_lock();
        let missing = std::env::temp_dir().join(format!(
            "roastty-missing-cwd-for-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&missing);

        assert!(Termio::spawn_with_cwd(
            "/bin/pwd",
            std::iter::empty::<&str>(),
            Some(missing),
            test_size()
        )
        .is_err());
    }

    #[test]
    fn termio_env_spawn_with_options_passes_environment_variables() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf '%s' \"$ROASTTY_TERMIO_ENV_TEST\""],
            TermioSpawnOptions {
                cwd: None,
                env: vec![(
                    "ROASTTY_TERMIO_ENV_TEST".to_string(),
                    "termio-env".to_string(),
                )],
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with env");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("termio-env")
        });

        assert!(termio.terminal().plain_screen(false).contains("termio-env"));
    }

    #[test]
    fn termio_env_spawn_with_options_inherits_process_environment() {
        let _guard = pty_command_lock();
        let _env = EnvGuard::set("ROASTTY_TERMIO_INHERITED_ENV_TEST", "inherited-env");
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf '%s' \"$ROASTTY_TERMIO_INHERITED_ENV_TEST\""],
            TermioSpawnOptions::default(),
            test_size(),
        )
        .expect("spawn termio with inherited env");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("inherited-env")
        });

        assert!(termio
            .terminal()
            .plain_screen(false)
            .contains("inherited-env"));
    }

    #[cfg(unix)]
    #[test]
    fn termio_env_spawn_with_options_tolerates_non_unicode_inherited_environment() {
        use std::os::unix::ffi::OsStringExt;

        let _guard = pty_command_lock();
        let value = OsString::from_vec(vec![0xff, b'x']);
        let _env = EnvGuard::set("ROASTTY_TERMIO_NON_UNICODE_ENV_TEST", value);
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf ok"],
            TermioSpawnOptions::default(),
            test_size(),
        )
        .expect("spawn termio with non-unicode inherited env");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("ok")
        });

        assert!(termio.terminal().plain_screen(false).contains("ok"));
    }

    #[test]
    fn termio_env_spawn_with_options_explicit_env_overrides_inherited_environment() {
        let _guard = pty_command_lock();
        let _env = EnvGuard::set("ROASTTY_TERMIO_OVERRIDE_ENV_TEST", "inherited-env");
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf '%s' \"$ROASTTY_TERMIO_OVERRIDE_ENV_TEST\""],
            TermioSpawnOptions {
                env: vec![(
                    "ROASTTY_TERMIO_OVERRIDE_ENV_TEST".to_string(),
                    "explicit-env".to_string(),
                )],
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with explicit env override");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("explicit-env")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("explicit-env"));
        assert!(!screen.contains("inherited-env"));
    }

    #[test]
    fn spawn_with_options_sets_fallback_terminal_identity_without_resources() {
        let _guard = pty_command_lock();
        let _term = EnvGuard::set("TERM", "stale-term");
        let _color = EnvGuard::set("COLORTERM", "stale-color");
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf 'term:%s color:%s resources:%s' \"$TERM\" \"$COLORTERM\" \"$ROASTTY_RESOURCES_DIR\""],
            TermioSpawnOptions {
                shell_integration: crate::config::ShellIntegration::None,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio without resources");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("xterm-256color")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("term:xterm-256color"));
        assert!(screen.contains("color:truecolor"));
        assert!(screen.contains("resources:"));
        assert!(!screen.contains("stale-term"));
        assert!(!screen.contains("stale-color"));
    }

    #[test]
    fn termio_env_explicit_overrides_win_after_terminal_identity() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf 'term:%s color:%s resources:%s' \"$TERM\" \"$COLORTERM\" \"$ROASTTY_RESOURCES_DIR\""],
            TermioSpawnOptions {
                env: vec![
                    ("TERM".to_string(), "explicit-term".to_string()),
                    ("COLORTERM".to_string(), "explicit-color".to_string()),
                    (
                        "ROASTTY_RESOURCES_DIR".to_string(),
                        "/explicit/resources".to_string(),
                    ),
                ],
                shell_integration: crate::config::ShellIntegration::None,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with explicit terminal identity overrides");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("explicit-term")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("term:explicit-term"));
        assert!(screen.contains("color:explicit-color"));
        assert!(screen.contains("resources:/explicit/resources"));
    }

    #[test]
    fn spawn_with_options_sets_resource_terminal_identity() {
        let _guard = pty_command_lock();
        let resources_root = unique_test_dir("termio-resource-env");
        let resources = resources_root.join("roastty");
        std::fs::create_dir_all(&resources).expect("create resources dir");
        let terminfo = resources_root.join("terminfo");

        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "if [ \"$TERM\" = xterm-roastty ] && [ \"$COLORTERM\" = truecolor ] && [ \"$TERMINFO\" = \"$ROASTTY_EXPECT_TERMINFO\" ] && [ \"$ROASTTY_RESOURCES_DIR\" = \"$ROASTTY_EXPECT_RESOURCES\" ]; then printf ok; else printf 'fail:%s:%s:%s:%s' \"$TERM\" \"$COLORTERM\" \"$TERMINFO\" \"$ROASTTY_RESOURCES_DIR\"; fi"],
            TermioSpawnOptions {
                env: vec![
                    (
                        "ROASTTY_EXPECT_TERMINFO".to_string(),
                        terminfo.display().to_string(),
                    ),
                    (
                        "ROASTTY_EXPECT_RESOURCES".to_string(),
                        resources.display().to_string(),
                    ),
                ],
                resource_dir: Some(resources.clone()),
                shell_integration: crate::config::ShellIntegration::None,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with resources");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("ok")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("ok"));

        let _ = std::fs::remove_dir_all(resources_root);
    }

    #[test]
    fn spawn_with_options_resource_identity_overwrites_inherited_env() {
        let _guard = pty_command_lock();
        let _term = EnvGuard::set("TERM", "stale-term");
        let _color = EnvGuard::set("COLORTERM", "stale-color");
        let _terminfo = EnvGuard::set("TERMINFO", "/stale/terminfo");
        let _resources = EnvGuard::set("ROASTTY_RESOURCES_DIR", "/stale/resources");
        let resources_root = unique_test_dir("termio-resource-env-override");
        let resources = resources_root.join("roastty");
        std::fs::create_dir_all(&resources).expect("create resources dir");

        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "if [ \"$TERM\" = screen-256color ] && [ \"$COLORTERM\" = truecolor ] && [ \"$TERMINFO\" = \"$ROASTTY_EXPECT_TERMINFO\" ] && [ \"$ROASTTY_RESOURCES_DIR\" = \"$ROASTTY_EXPECT_RESOURCES\" ]; then printf ok; else printf 'fail:%s:%s:%s:%s' \"$TERM\" \"$COLORTERM\" \"$TERMINFO\" \"$ROASTTY_RESOURCES_DIR\"; fi"],
            TermioSpawnOptions {
                env: vec![
                    (
                        "ROASTTY_EXPECT_TERMINFO".to_string(),
                        resources_root.join("terminfo").display().to_string(),
                    ),
                    (
                        "ROASTTY_EXPECT_RESOURCES".to_string(),
                        resources.display().to_string(),
                    ),
                ],
                resource_dir: Some(resources.clone()),
                term: "screen-256color".to_string(),
                shell_integration: crate::config::ShellIntegration::None,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with resource env overrides");

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("ok")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("ok"));
        assert!(!screen.contains("stale"));

        let _ = std::fs::remove_dir_all(resources_root);
    }

    #[test]
    fn termio_env_spawn_with_options_explicit_env_overrides_shell_integration_env() {
        let _guard = pty_command_lock();
        let resources = unique_test_dir("termio-zsh-explicit-override");
        let zsh_dir = resources.join("shell-integration/zsh");
        std::fs::create_dir_all(&zsh_dir).expect("create zsh resources");
        std::fs::write(zsh_dir.join(".zshenv"), b"").expect("write zshenv");

        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            [
                "-c",
                "printf 'zdot:%s old:%s' \"$ZDOTDIR\" \"$ROASTTY_ZSH_ZDOTDIR\"",
            ],
            TermioSpawnOptions {
                env: vec![("ZDOTDIR".to_string(), "/explicit/zdotdir".to_string())],
                shell_integration: crate::config::ShellIntegration::Zsh,
                resource_dir: Some(resources.clone()),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with explicit ZDOTDIR override");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("zdot:/explicit/zdotdir")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("zdot:/explicit/zdotdir"));
        assert!(!screen.contains("shell-integration/zsh"));

        let _ = std::fs::remove_dir_all(resources);
    }

    #[test]
    fn spawn_with_options_sets_shell_feature_env_even_when_integration_is_none() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            ["-c", "printf '%s' \"$ROASTTY_SHELL_FEATURES\""],
            TermioSpawnOptions {
                shell_integration: crate::config::ShellIntegration::None,
                shell_integration_features: crate::config::ShellIntegrationFeatures {
                    cursor: true,
                    sudo: true,
                    title: false,
                    ssh_env: false,
                    ssh_terminfo: false,
                    path: false,
                },
                cursor_blink: Some(false),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with shell features");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("cursor:steady,sudo")
        });

        assert!(termio
            .terminal()
            .plain_screen(false)
            .contains("cursor:steady,sudo"));
    }

    #[test]
    fn zsh_integration_spawn_with_options_reaches_child_env() {
        let _guard = pty_command_lock();
        let resources = unique_test_dir("termio-zsh-resources");
        let zsh_dir = resources.join("shell-integration/zsh");
        std::fs::create_dir_all(&zsh_dir).expect("create zsh resources");
        std::fs::write(zsh_dir.join(".zshenv"), b"").expect("write zshenv");

        let mut termio = Termio::spawn_with_options(
            "/bin/sh",
            [
                "-c",
                "printf 'zdot:%s features:%s' \"$ZDOTDIR\" \"$ROASTTY_SHELL_FEATURES\"",
            ],
            TermioSpawnOptions {
                shell_integration: crate::config::ShellIntegration::Zsh,
                resource_dir: Some(resources.clone()),
                cursor_blink: Some(true),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with forced zsh integration");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("shell-integration/zsh")
        });

        let screen = termio.terminal().plain_screen(false);
        assert!(screen.contains("zdot:"));
        assert!(screen.contains("shell-integration/zsh"));
        assert!(screen.contains("features:cursor:blink,path,title"));

        let _ = std::fs::remove_dir_all(resources);
    }

    #[test]
    fn zsh_integration_spawn_with_options_sources_inherited_zdotdir() {
        let _guard = pty_command_lock();
        let resources = unique_test_dir("termio-zsh-inherited-bootstrap-resources");
        let zsh_dir = resources.join("shell-integration/zsh");
        std::fs::create_dir_all(&zsh_dir).expect("create zsh resources");
        std::fs::write(
            zsh_dir.join(".zshenv"),
            b"if [[ -n \"${ROASTTY_ZSH_ZDOTDIR+X}\" ]]; then\n  builtin export ZDOTDIR=\"$ROASTTY_ZSH_ZDOTDIR\"\n  builtin unset ROASTTY_ZSH_ZDOTDIR\nelse\n  builtin unset ZDOTDIR\nfi\nbuiltin source -- \"${ZDOTDIR-$HOME}/.zshenv\"\n",
        )
        .expect("write zshenv");
        let bootstrap = unique_test_dir("termio-zsh-inherited-bootstrap");
        std::fs::create_dir_all(&bootstrap).expect("create bootstrap dir");
        std::fs::write(
            bootstrap.join(".zshenv"),
            b"export ROASTTY_TERMIO_BOOTSTRAP_MARKER=bootstrap-sourced\n",
        )
        .expect("write bootstrap zshenv");
        let _zdotdir = EnvGuard::set("ZDOTDIR", &bootstrap);

        let mut termio = Termio::spawn_with_options(
            "/bin/zsh",
            ["-lc", "printf '%s' \"$ROASTTY_TERMIO_BOOTSTRAP_MARKER\""],
            TermioSpawnOptions {
                shell_integration: crate::config::ShellIntegration::Zsh,
                resource_dir: Some(resources.clone()),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn zsh with inherited bootstrap ZDOTDIR");

        pump_until(&mut termio, |termio, _| {
            termio
                .terminal()
                .plain_screen(false)
                .contains("bootstrap-sourced")
        });

        assert!(termio
            .terminal()
            .plain_screen(false)
            .contains("bootstrap-sourced"));

        let _ = std::fs::remove_dir_all(resources);
        let _ = std::fs::remove_dir_all(bootstrap);
    }

    #[test]
    fn spawn_with_options_initializes_cursor_defaults() {
        let _guard = pty_command_lock();
        let termio = Termio::spawn_with_options(
            "/bin/sleep",
            ["1"],
            TermioSpawnOptions {
                cursor_visual_style: cursor::VisualStyle::Underline,
                cursor_blink: Some(false),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with cursor defaults");

        assert_eq!(
            termio.terminal().cursor_visual_style(),
            cursor::VisualStyle::Underline
        );
        assert!(!termio.terminal().cursor_blinking());
    }

    #[test]
    fn termio_cursor_default_runtime_spawn_options_reach_terminal() {
        let _guard = pty_command_lock();
        let mut termio = Termio::spawn_with_options(
            "/bin/sleep",
            ["1"],
            TermioSpawnOptions {
                cursor_visual_style: cursor::VisualStyle::Bar,
                cursor_blink: Some(true),
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with cursor defaults");

        assert_eq!(
            termio.terminal().cursor_visual_style(),
            cursor::VisualStyle::Bar
        );
        assert!(termio.terminal().cursor_blinking());

        termio
            .terminal_mut()
            .set_cursor_defaults(cursor::VisualStyle::Underline, Some(false));
        assert_eq!(
            termio.terminal().cursor_visual_style(),
            cursor::VisualStyle::Underline
        );
        assert!(!termio.terminal().cursor_blinking());
    }

    #[test]
    fn termio_image_storage_limit_runtime_spawn_options_reach_terminal() {
        let _guard = pty_command_lock();
        let termio = Termio::spawn_with_options(
            "/bin/sleep",
            ["1"],
            TermioSpawnOptions {
                image_storage_limit: 12345,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with image storage limit");

        assert_eq!(termio.terminal().kitty_image_storage_limit(), 12345);
        assert!(termio
            .terminal()
            .kitty_image_medium_enabled(crate::terminal::terminal::KittyImageMedium::File));
        assert!(termio.terminal().kitty_image_medium_enabled(
            crate::terminal::terminal::KittyImageMedium::TemporaryFile
        ));
        assert!(termio
            .terminal()
            .kitty_image_medium_enabled(crate::terminal::terminal::KittyImageMedium::SharedMemory));
    }

    #[test]
    fn grapheme_width_method_runtime_spawn_options_reach_terminal() {
        let _guard = pty_command_lock();

        for grapheme_cluster in [true, false] {
            let mut termio = Termio::spawn_with_options(
                "/bin/sleep",
                ["1"],
                TermioSpawnOptions {
                    grapheme_cluster,
                    ..TermioSpawnOptions::default()
                },
                test_size(),
            )
            .expect("spawn termio with grapheme width method");

            assert_eq!(
                termio.terminal().grapheme_cluster_enabled(),
                grapheme_cluster
            );

            let toggle = if grapheme_cluster {
                b"\x1b[?2027l".as_slice()
            } else {
                b"\x1b[?2027h"
            };
            termio.terminal_mut().next_slice(toggle).unwrap();
            assert_ne!(
                termio.terminal().grapheme_cluster_enabled(),
                grapheme_cluster
            );

            termio.terminal_mut().next_slice(b"\x1bc").unwrap();
            assert_eq!(
                termio.terminal().grapheme_cluster_enabled(),
                grapheme_cluster
            );
        }
    }

    #[test]
    fn spawn_with_options_initializes_palette_defaults() {
        let _guard = pty_command_lock();
        let mut palette = color::DEFAULT_PALETTE;
        palette[1] = color::Rgb::new(1, 2, 3);
        palette[240] = color::Rgb::new(4, 5, 6);

        let termio = Termio::spawn_with_options(
            "/bin/sleep",
            ["1"],
            TermioSpawnOptions {
                palette,
                ..TermioSpawnOptions::default()
            },
            test_size(),
        )
        .expect("spawn termio with palette defaults");

        let default = termio.terminal().palette_default();
        assert_eq!(default[1], (1, 2, 3));
        assert_eq!(default[240], (4, 5, 6));
        assert_eq!(termio.terminal().palette_current(), default);
    }

    #[test]
    fn termio_clipboard_osc52_worker_event_preserves_payload() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf '\\033]52;s;?\\007'");

        let event = worker_event_until(&worker, |_, event| {
            matches!(
                event,
                TermioWorkerEvent::Clipboard(TerminalClipboardEvent::Osc52 { .. })
            )
        });

        assert_eq!(
            event,
            TermioWorkerEvent::Clipboard(TerminalClipboardEvent::Osc52 {
                kind: b's',
                data: b"?".to_vec(),
            })
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_clipboard_kitty_worker_event_preserves_payload_and_terminator() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf '\\033]5522;type=read;payload\\033\\\\'");

        let event = worker_event_until(&worker, |_, event| {
            matches!(
                event,
                TermioWorkerEvent::Clipboard(TerminalClipboardEvent::Kitty { .. })
            )
        });

        assert_eq!(
            event,
            TermioWorkerEvent::Clipboard(TerminalClipboardEvent::Kitty {
                metadata: b"type=read".to_vec(),
                payload: Some(b"payload".to_vec()),
                terminator: osc::Terminator::St,
            })
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_clipboard_worker_events_precede_same_read_pump_in_parse_order() {
        let _guard = pty_command_lock();
        let mut worker =
            spawn_worker("printf 'text\\033]52;c;raw\\007\\033]5522;type=read\\007tail'");

        let events = worker_events_until(&worker, |events| {
            matches!(
                events.last(),
                Some(TermioWorkerEvent::Pump(pump)) if pump.bytes_read > 0
            ) && events
                .iter()
                .filter(|event| matches!(event, TermioWorkerEvent::Clipboard(_)))
                .count()
                >= 2
        });

        let pump_index = events
            .iter()
            .position(|event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.bytes_read > 0))
            .expect("pump event");
        let clipboard_events: Vec<_> = events[..pump_index]
            .iter()
            .filter_map(|event| match event {
                TermioWorkerEvent::Clipboard(event) => Some(event.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(
            clipboard_events,
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
            ]
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn worker_delivers_child_output_to_terminal() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf hello");

        let event = worker_event_until(&worker, |worker, event| {
            matches!(event, TermioWorkerEvent::Pump(pump) if pump.bytes_read > 0)
                && worker
                    .with_termio(|termio| termio.terminal().plain_screen(false).contains("hello"))
        });

        assert!(matches!(event, TermioWorkerEvent::Pump(_)));
        assert!(
            worker.with_termio(|termio| termio.terminal().plain_screen(false).contains("hello"))
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_bell_worker_emits_bell_only_pump() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf '\\a'");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.bell_count == 1),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.bell_count == 1
        ));
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_title_worker_emits_non_empty_osc_title_pump() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf '\\033]0;termio title\\007'");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.titles.iter().any(|title| title == "termio title")),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.titles.iter().any(|title| title == "termio title")
        ));
        assert_eq!(
            worker.with_termio(|termio| termio.terminal().title().to_string()),
            "termio title"
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_title_pwd_fallback_worker_emits_empty_title_pump() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf '\\033]0;\\007'");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.titles.iter().any(|title| title.is_empty())),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.titles.iter().any(|title| title.is_empty())
        ));
        assert_eq!(
            worker.with_termio(|termio| termio.terminal().title().to_string()),
            ""
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_title_pwd_fallback_worker_emits_pwd_title_pump() {
        let _guard = pty_command_lock();
        let mut worker =
            spawn_worker("printf '\\033]7;file://localhost/termio-pwd\\007\\033]0;\\007'");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.titles.iter().any(|title| title == "/termio-pwd")),
        );

        assert!(matches!(
            &event,
            TermioWorkerEvent::Pump(pump) if pump.titles.iter().any(|title| title == "/termio-pwd")
                && pump.pwd.iter().any(|pwd| pwd == "/termio-pwd")
        ));
        assert_eq!(
            worker.with_termio(|termio| termio.terminal().title().to_string()),
            "/termio-pwd"
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_osc7_pwd_normalization_worker_emits_normalized_pwd_pump() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf '\\033]7;file://localhost/termio%%20pwd\\007'");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.pwd.iter().any(|pwd| pwd == "/termio pwd")),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.pwd.iter().any(|pwd| pwd == "/termio pwd")
                && pump.titles.iter().any(|title| title == "/termio pwd")
        ));
        assert_eq!(
            worker.with_termio(|termio| termio.terminal().pwd().map(str::to_owned)),
            Some("/termio pwd".to_string())
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_osc7_pwd_edge_worker_emits_raw_kitty_pwd_pump() {
        let _guard = pty_command_lock();
        let mut worker =
            spawn_worker("printf '\\033]7;kitty-shell-cwd://localhost/termio%%2Fraw?x#y\\007'");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.pwd.iter().any(|pwd| pwd == "/termio%2Fraw?x#y")),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.pwd.iter().any(|pwd| pwd == "/termio%2Fraw?x#y")
                && pump.titles.iter().any(|title| title == "/termio%2Fraw?x#y")
        ));
        assert_eq!(
            worker.with_termio(|termio| termio.terminal().pwd().map(str::to_owned)),
            Some("/termio%2Fraw?x#y".to_string())
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn termio_title_pwd_fallback_worker_preserves_multiple_title_events() {
        let _guard = pty_command_lock();
        let mut worker =
            spawn_worker("printf '\\033]7;file://localhost/one\\007\\033]0;two\\007\\033]0;\\007'");

        let expected = vec!["/one".to_string(), "two".to_string(), "/one".to_string()];
        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.titles == expected),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.titles == expected
        ));
        assert_eq!(
            worker.with_termio(|termio| termio.terminal().title().to_string()),
            "/one"
        );
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn worker_rejects_terminal_with_callbacks() {
        let _guard = pty_command_lock();
        let mut termio = spawn_shell("printf ignored");
        termio
            .terminal_mut()
            .set_bell_callback(Some(test_bell_callback));

        let err = TermioWorker::spawn(termio, 10, 4096).expect_err("reject callbacks");

        assert_eq!(err, TermioWorkerError::TerminalCallbacksInstalled);
    }

    #[test]
    fn worker_queue_write_reaches_child() {
        let _guard = pty_command_lock();
        let mut worker =
            spawn_worker("stty -echo; printf ready; IFS= read line; printf 'out:%s' \"$line\"");

        worker_event_until(&worker, |worker, _| {
            worker.with_termio(|termio| termio.terminal().plain_screen(false).contains("ready"))
        });
        worker.queue_write(b"hello\n").expect("queue write");

        worker_event_until(&worker, |worker, _| {
            worker.with_termio(|termio| termio.terminal().plain_screen(false).contains("out:hello"))
        });

        assert!(worker
            .with_termio(|termio| termio.terminal().plain_screen(false).contains("out:hello")));
        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn worker_resize_command_updates_pty_size() {
        let _guard = pty_command_lock();
        let mut worker = TermioWorker::spawn(
            Termio::spawn("/bin/sleep", ["1"], test_size()).expect("spawn termio"),
            10,
            4096,
        )
        .expect("spawn worker");
        let resized = PtySize {
            rows: 43,
            cols: 121,
            width_px: 1210,
            height_px: 860,
        };

        worker.resize_pty(resized).expect("queue resize");
        wait_until(|| {
            worker.with_termio(|termio| termio.pty_size().expect("get pty size") == resized)
        });

        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn worker_emits_final_event_before_exiting() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf done");

        let event = worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.child_exited || pump.eof),
        );

        assert!(matches!(
            event,
            TermioWorkerEvent::Pump(pump) if pump.child_exited || pump.eof
        ));
        worker.shutdown().expect("shutdown exited worker");
        worker.shutdown().expect("shutdown is idempotent");
    }

    #[test]
    fn worker_shutdown_joins_long_lived_child_thread() {
        let _guard = pty_command_lock();
        let mut worker = TermioWorker::spawn(
            Termio::spawn("/bin/sleep", ["5"], test_size()).expect("spawn termio"),
            10,
            4096,
        )
        .expect("spawn worker");

        worker.shutdown().expect("shutdown worker");
        worker.shutdown().expect("shutdown remains idempotent");
    }

    #[test]
    fn worker_drop_cleans_up_long_lived_child() {
        let _guard = pty_command_lock();
        let pid = {
            let worker = TermioWorker::spawn(
                Termio::spawn("/bin/sleep", ["5"], test_size()).expect("spawn termio"),
                10,
                4096,
            )
            .expect("spawn worker");
            worker.with_termio(|termio| termio.child_id())
        };

        wait_until(|| {
            let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
            result == -1 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        });
    }

    #[test]
    fn worker_commands_fail_after_worker_stops() {
        let _guard = pty_command_lock();
        let mut worker = spawn_worker("printf done");

        worker_event_until(
            &worker,
            |_, event| matches!(event, TermioWorkerEvent::Pump(pump) if pump.child_exited || pump.eof),
        );
        worker.shutdown().expect("shutdown exited worker");

        assert_eq!(
            worker.queue_write(b"x"),
            Err(TermioWorkerError::CommandDisconnected)
        );
        assert_eq!(
            worker.resize_pty(test_size()),
            Err(TermioWorkerError::CommandDisconnected)
        );
    }
}
