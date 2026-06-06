//! Synchronous terminal/PTY coordination.

use std::ffi::{OsStr, OsString};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::os::pty::{PtyChild, PtyCommand, PtyReadiness, PtySize};
use crate::terminal::terminal::{Terminal, TerminalInitError, TerminalStreamError};

#[derive(Debug)]
pub(crate) struct Termio {
    terminal: Terminal,
    child: PtyChild,
    output_buf: Vec<u8>,
    pending_write: Vec<u8>,
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
pub(crate) struct TermioPump {
    pub(crate) readiness: PtyReadiness,
    pub(crate) bytes_read: usize,
    pub(crate) eof: bool,
    pub(crate) bytes_written: usize,
    pub(crate) pending_write_bytes: usize,
    pub(crate) child_exited: bool,
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
        let terminal = Terminal::init(size.cols, size.rows, None)?;
        let mut command = PtyCommand::new(program, size);
        for arg in args {
            command.arg(arg);
        }
        let child = command.spawn()?;
        child.set_nonblocking()?;

        Ok(Self {
            terminal,
            child,
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

    pub(crate) fn pending_write_bytes(&self) -> usize {
        self.pending_write.len()
    }

    pub(crate) fn queue_write(&mut self, bytes: &[u8]) {
        self.pending_write.extend_from_slice(bytes);
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
                self.terminal.next_slice(&self.output_buf)?;
                self.collect_terminal_response();
            }
        } else {
            self.collect_terminal_response();
        }

        let bytes_written = self.flush_pending_write()?;
        let child_exited = self.child.try_wait()?.is_some();

        Ok(TermioPump {
            readiness,
            bytes_read,
            eof,
            bytes_written,
            pending_write_bytes: self.pending_write.len(),
            child_exited,
        })
    }

    pub(crate) fn resize_pty(&self, size: PtySize) -> Result<(), TermioError> {
        self.child.resize(size)?;
        Ok(())
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

impl TermioWorker {
    pub(crate) fn spawn(
        termio: Termio,
        pump_timeout_ms: i32,
        max_read_bytes: usize,
    ) -> Result<Self, TermioWorkerError> {
        if termio.terminal.has_effect_callbacks() {
            return Err(TermioWorkerError::TerminalCallbacksInstalled);
        }

        let termio = Arc::new(Mutex::new(termio));
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread_termio = Arc::clone(&termio);

        let join = thread::spawn(move || {
            run_termio_worker(
                thread_termio,
                command_rx,
                event_tx,
                pump_timeout_ms,
                max_read_bytes,
            );
        });

        Ok(Self {
            termio,
            commands: command_tx,
            events: event_rx,
            join: Some(join),
        })
    }

    pub(crate) fn queue_write(&self, bytes: &[u8]) -> Result<(), TermioWorkerError> {
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
            WorkerCommandState::Stop => break,
        }

        let pump = {
            let mut termio = termio.lock().expect("termio worker mutex poisoned");
            termio.pump_once(pump_timeout_ms, max_read_bytes)
        };

        match pump {
            Ok(pump) => {
                let should_emit = pump.bytes_read > 0
                    || pump.bytes_written > 0
                    || pump.pending_write_bytes > 0
                    || pump.eof
                    || pump.child_exited;
                if should_emit && events.send(TermioWorkerEvent::Pump(pump)).is_err() {
                    break;
                }
                if pump.eof || pump.child_exited {
                    break;
                }
            }
            Err(err) => {
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
                let mut termio = termio.lock().expect("termio worker mutex poisoned");
                termio.queue_write(&bytes);
            }
            Ok(TermioWorkerCommand::ResizePty(size)) => {
                let result = {
                    let termio = termio.lock().expect("termio worker mutex poisoned");
                    termio.resize_pty(size)
                };
                if let Err(err) = result {
                    let _ = events.send(TermioWorkerEvent::Error(format!("{err:?}")));
                    return WorkerCommandState::Stop;
                }
            }
            Ok(TermioWorkerCommand::Shutdown) => return WorkerCommandState::Stop,
            Err(mpsc::TryRecvError::Empty) => return WorkerCommandState::Continue,
            Err(mpsc::TryRecvError::Disconnected) => return WorkerCommandState::Stop,
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
    use crate::os::pty::PTY_COMMAND_LOCK;
    use std::ffi::c_void;
    use std::os::fd::RawFd;
    use std::thread;
    use std::time::Duration;

    unsafe extern "C" fn test_bell_callback(_: *mut c_void, _: *mut c_void) {}

    fn test_size() -> PtySize {
        PtySize {
            rows: 24,
            cols: 80,
            width_px: 800,
            height_px: 600,
        }
    }

    fn pty_size(fd: RawFd) -> io::Result<PtySize> {
        let mut winsize = libc::winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut winsize) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(PtySize {
            rows: winsize.ws_row,
            cols: winsize.ws_col,
            width_px: winsize.ws_xpixel,
            height_px: winsize.ws_ypixel,
        })
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
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut termio = spawn_shell("printf hello");

        let pump = pump_until(&mut termio, |termio, pump| {
            pump.bytes_read > 0 && termio.terminal().plain_screen(false).contains("hello")
        });

        assert_eq!(pump.bytes_read, 5);
        assert!(termio.terminal().plain_screen(false).contains("hello"));
    }

    #[test]
    fn queue_write_reaches_child_and_output_returns_to_terminal() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut termio = spawn_shell(
            "stty raw -echo min 0 time 10; \
             printf '\\033[c'; \
             response=$(dd bs=1 count=9 2>/dev/null); \
             expected=$(printf '\\033[?62;22c'); \
             if [ \"$response\" = \"$expected\" ]; then printf da-ok; else printf da-bad; fi",
        );

        let first = pump_until(&mut termio, |_, pump| pump.bytes_written > 0);
        assert_eq!(first.bytes_written, b"\x1b[?62;22c".len());
        assert_eq!(first.pending_write_bytes, 0);
        assert_eq!(termio.pending_write_bytes(), 0);

        pump_until(&mut termio, |termio, _| {
            termio.terminal().plain_screen(false).contains("da-ok")
        });
        assert!(termio.terminal().plain_screen(false).contains("da-ok"));
    }

    #[test]
    fn resize_pty_updates_reported_winsize() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let termio = Termio::spawn("/bin/sleep", ["1"], test_size()).expect("spawn termio");
        let resized = PtySize {
            rows: 33,
            cols: 101,
            width_px: 1001,
            height_px: 777,
        };

        termio.resize_pty(resized).expect("resize pty");

        assert_eq!(
            pty_size(termio.child.master_fd()).expect("get pty size"),
            resized
        );
    }

    #[test]
    fn pump_once_reports_child_exit() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut termio = spawn_shell("printf done");

        let pump = pump_until(&mut termio, |_, pump| pump.child_exited || pump.eof);

        assert!(pump.child_exited || pump.eof);
    }

    #[test]
    fn accessors_expose_terminal_child_id_and_pending_count() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut termio = Termio::spawn("/bin/sleep", ["1"], test_size()).expect("spawn termio");

        assert!(termio.child_id() > 0);
        assert_eq!(termio.pending_write_bytes(), 0);
        termio.queue_write(b"x");
        assert_eq!(termio.pending_write_bytes(), 1);
        assert_eq!(termio.terminal_mut().pty_response(), b"");
    }

    #[test]
    fn worker_delivers_child_output_to_terminal() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
    fn worker_rejects_terminal_with_callbacks() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut termio = spawn_shell("printf ignored");
        termio
            .terminal_mut()
            .set_bell_callback(Some(test_bell_callback));

        let err = TermioWorker::spawn(termio, 10, 4096).expect_err("reject callbacks");

        assert_eq!(err, TermioWorkerError::TerminalCallbacksInstalled);
    }

    #[test]
    fn worker_queue_write_reaches_child() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
            worker.with_termio(|termio| {
                pty_size(termio.child.master_fd()).expect("get pty size") == resized
            })
        });

        worker.shutdown().expect("shutdown worker");
    }

    #[test]
    fn worker_emits_final_event_before_exiting() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
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
