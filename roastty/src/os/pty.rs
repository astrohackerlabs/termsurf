//! POSIX PTY ownership and sizing.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::ptr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PtySize {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) width_px: u16,
    pub(crate) height_px: u16,
}

impl PtySize {
    fn winsize(self) -> libc::winsize {
        libc::winsize {
            ws_row: self.rows,
            ws_col: self.cols,
            ws_xpixel: self.width_px,
            ws_ypixel: self.height_px,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Pty {
    master: OwnedFd,
    slave: Option<OwnedFd>,
}

impl Pty {
    pub(crate) fn open(size: PtySize) -> io::Result<Self> {
        let mut master = 0;
        let mut slave = 0;
        let mut winsize = size.winsize();
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut winsize,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }

        // Take ownership immediately so any post-open error closes both descriptors.
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };

        set_cloexec(master.as_raw_fd())?;
        set_cloexec(slave.as_raw_fd())?;

        Ok(Self {
            master,
            slave: Some(slave),
        })
    }

    pub(crate) fn set_size(&self, size: PtySize) -> io::Result<()> {
        let winsize = size.winsize();
        if unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &winsize) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(crate) fn master_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    pub(crate) fn slave_fd(&self) -> Option<RawFd> {
        self.slave.as_ref().map(AsRawFd::as_raw_fd)
    }

    pub(crate) fn close_slave(&mut self) {
        self.slave = None;
    }
}

#[derive(Debug)]
pub(crate) struct PtyCommand {
    program: OsString,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    cwd: Option<PathBuf>,
    size: PtySize,
}

impl PtyCommand {
    pub(crate) fn new(program: impl Into<OsString>, size: PtySize) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            size,
        }
    }

    pub(crate) fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub(crate) fn cwd(&mut self, cwd: impl AsRef<Path>) -> &mut Self {
        self.cwd = Some(cwd.as_ref().to_path_buf());
        self
    }

    pub(crate) fn env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    pub(crate) fn spawn(&self) -> io::Result<PtyChild> {
        let mut pty = Pty::open(self.size)?;
        let slave_fd = pty
            .slave_fd()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "pty slave is closed"))?;

        let stdin = dup_owned(slave_fd)?;
        let stdout = dup_owned(slave_fd)?;
        let stderr = dup_owned(slave_fd)?;

        let mut command = Command::new(&self.program);
        command.args(&self.args);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command.stdin(Stdio::from(stdin));
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let child = command.spawn()?;
        pty.close_slave();
        Ok(PtyChild { pty, child })
    }
}

#[derive(Debug)]
pub(crate) struct PtyChild {
    pty: Pty,
    child: Child,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PtyReadiness {
    pub(crate) readable: bool,
    pub(crate) hangup: bool,
    pub(crate) error: bool,
    pub(crate) invalid: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PtyRead {
    pub(crate) bytes_read: usize,
    pub(crate) eof: bool,
}

#[cfg(test)]
pub(crate) static PTY_COMMAND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl PtyChild {
    pub(crate) fn master_fd(&self) -> RawFd {
        self.pty.master_fd()
    }

    pub(crate) fn slave_fd(&self) -> Option<RawFd> {
        self.pty.slave_fd()
    }

    pub(crate) fn child_id(&self) -> u32 {
        self.child.id()
    }

    pub(crate) fn set_nonblocking(&self) -> io::Result<()> {
        let fd = self.master_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub(crate) fn write(&self, data: &[u8]) -> io::Result<usize> {
        let written = unsafe {
            libc::write(
                self.master_fd(),
                data.as_ptr() as *const libc::c_void,
                data.len(),
            )
        };
        if written < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(written as usize)
    }

    pub(crate) fn poll(&self, timeout_ms: i32) -> io::Result<PtyReadiness> {
        let mut pollfd = libc::pollfd {
            fd: self.master_fd(),
            events: libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            return Ok(PtyReadiness::default());
        }
        Ok(PtyReadiness {
            readable: pollfd.revents & libc::POLLIN != 0,
            hangup: pollfd.revents & libc::POLLHUP != 0,
            error: pollfd.revents & libc::POLLERR != 0,
            invalid: pollfd.revents & libc::POLLNVAL != 0,
        })
    }

    pub(crate) fn read_available(
        &self,
        out: &mut Vec<u8>,
        max_bytes: usize,
    ) -> io::Result<PtyRead> {
        let mut total = 0;
        let mut buf = [0u8; 1024];
        while total < max_bytes {
            let limit = (max_bytes - total).min(buf.len());
            let got = unsafe {
                libc::read(
                    self.master_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    limit,
                )
            };
            if got > 0 {
                let got = got as usize;
                out.extend_from_slice(&buf[..got]);
                total += got;
                continue;
            }
            if got == 0 {
                return Ok(PtyRead {
                    bytes_read: total,
                    eof: true,
                });
            }

            let err = io::Error::last_os_error();
            match err.raw_os_error() {
                Some(code) if code == libc::EAGAIN || code == libc::EWOULDBLOCK => break,
                Some(code) if code == libc::EIO => {
                    return Ok(PtyRead {
                        bytes_read: total,
                        eof: true,
                    });
                }
                _ => return Err(err),
            }
        }
        Ok(PtyRead {
            bytes_read: total,
            eof: false,
        })
    }

    pub(crate) fn resize(&self, size: PtySize) -> io::Result<()> {
        self.pty.set_size(size)
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            Err(_) => {}
        }
    }
}

fn set_cloexec(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn dup_owned(fd: RawFd) -> io::Result<OwnedFd> {
    let duplicated = unsafe { libc::dup(fd) };
    if duplicated < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(duplicated) })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn fd_cloexec(fd: RawFd) -> bool {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0, "F_GETFD failed");
        flags & libc::FD_CLOEXEC != 0
    }

    fn read_master_with_timeout(fd: RawFd, len: usize) -> io::Result<Vec<u8>> {
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pollfd, 1, 500) };
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        assert_eq!(ready, 1, "pty master did not become readable");
        assert_ne!(pollfd.revents & (libc::POLLIN | libc::POLLHUP), 0);

        let mut buf = vec![0u8; len];
        let got = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if got < 0 {
            return Err(io::Error::last_os_error());
        }
        buf.truncate(got as usize);
        Ok(buf)
    }

    struct RawModeGuard {
        fd: RawFd,
        original: libc::termios,
    }

    impl RawModeGuard {
        fn new(fd: RawFd) -> io::Result<Self> {
            let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
            if unsafe { libc::tcgetattr(fd, &mut original) } < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut raw = original;
            unsafe {
                libc::cfmakeraw(&mut raw);
            }
            if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { fd, original })
        }
    }

    impl Drop for RawModeGuard {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
            }
        }
    }

    #[test]
    fn pty_open_returns_valid_descriptors() {
        let pty = Pty::open(test_size()).expect("open pty");

        assert!(pty.master_fd() >= 0);
        assert!(pty.slave_fd().unwrap() >= 0);
        assert_ne!(pty.master_fd(), pty.slave_fd().unwrap());
    }

    #[test]
    fn pty_open_sets_cloexec_on_both_descriptors() {
        let pty = Pty::open(test_size()).expect("open pty");

        assert!(fd_cloexec(pty.master_fd()));
        assert!(fd_cloexec(pty.slave_fd().unwrap()));
    }

    #[test]
    fn pty_open_applies_initial_size() {
        let pty = Pty::open(test_size()).expect("open pty");

        assert_eq!(
            pty_size(pty.master_fd()).expect("get pty size"),
            test_size()
        );
    }

    #[test]
    fn pty_set_size_updates_reported_size() {
        let pty = Pty::open(test_size()).expect("open pty");
        let resized = PtySize {
            rows: 40,
            cols: 120,
            width_px: 1200,
            height_px: 900,
        };

        pty.set_size(resized).expect("set pty size");

        assert_eq!(pty_size(pty.master_fd()).expect("get pty size"), resized);
    }

    #[test]
    fn pty_transfers_bytes_without_blocking() {
        let pty = Pty::open(test_size()).expect("open pty");
        let _raw_mode = RawModeGuard::new(pty.slave_fd().unwrap()).expect("raw mode");
        let msg = b"hi";

        let written = unsafe {
            libc::write(
                pty.slave_fd().unwrap(),
                msg.as_ptr() as *const libc::c_void,
                msg.len(),
            )
        };
        assert_eq!(written, msg.len() as isize);

        let mut pollfd = libc::pollfd {
            fd: pty.master_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pollfd, 1, 100) };
        assert_eq!(ready, 1, "pty master did not become readable");
        assert_ne!(pollfd.revents & libc::POLLIN, 0);

        let mut buf = [0u8; 2];
        let got = unsafe {
            libc::read(
                pty.master_fd(),
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            )
        };
        assert_eq!(got, 2);
        assert_eq!(&buf, msg);
    }

    #[test]
    fn pty_command_reads_child_output_from_master() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sh", test_size());
        command.arg("-c").arg("printf hello");

        let mut child = command.spawn().expect("spawn child");
        assert!(child.slave_fd().is_none());

        let output = read_master_with_timeout(child.master_fd(), 5).expect("read master");
        assert_eq!(output, b"hello");
        assert!(child.wait().expect("wait child").success());
    }

    #[test]
    fn pty_command_attaches_stdio_to_tty() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sh", test_size());
        command
            .arg("-c")
            .arg("test -t 0 && test -t 1 && test -t 2 && printf tty");

        let mut child = command.spawn().expect("spawn child");

        let output = read_master_with_timeout(child.master_fd(), 3).expect("read master");
        assert_eq!(output, b"tty");
        assert!(child.wait().expect("wait child").success());
    }

    #[test]
    fn pty_command_passes_environment_variables() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sh", test_size());
        command
            .arg("-c")
            .arg("printf '%s' \"$ROASTTY_PTY_ENV_TEST\"")
            .env("ROASTTY_PTY_ENV_TEST", "env-ok");

        let mut child = command.spawn().expect("spawn child");

        let output = read_master_with_timeout(child.master_fd(), 6).expect("read master");
        assert_eq!(output, b"env-ok");
        assert!(child.wait().expect("wait child").success());
    }

    #[test]
    fn pty_child_drop_kills_and_reaps_running_child() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let pid = {
            let mut command = PtyCommand::new("/bin/sleep", test_size());
            command.arg("5");
            let child = command.spawn().expect("spawn child");
            let pid = child.child_id();
            drop(child);
            pid
        };

        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        assert_eq!(result, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    #[test]
    fn pty_child_write_and_read_available_round_trip() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sh", test_size());
        command
            .arg("-c")
            .arg("stty -echo; printf ready; IFS= read line; printf 'out:%s' \"$line\"");
        let mut child = command.spawn().expect("spawn child");
        child.set_nonblocking().expect("set nonblocking");

        let readiness = child.poll(500).expect("poll ready");
        assert!(
            readiness.readable || readiness.hangup,
            "expected ready output: {readiness:?}"
        );
        let mut ready = Vec::new();
        let read = child.read_available(&mut ready, 16).expect("read ready");
        assert_eq!(read.bytes_read, ready.len());
        assert_eq!(ready, b"ready");

        assert_eq!(child.write(b"hello\n").expect("write input"), 6);
        let readiness = child.poll(500).expect("poll output");
        assert!(
            readiness.readable || readiness.hangup,
            "expected output readiness: {readiness:?}"
        );

        let mut output = Vec::new();
        let read = child
            .read_available(&mut output, 32)
            .expect("read available");
        assert_eq!(read.bytes_read, output.len());
        assert_eq!(output, b"out:hello");
        assert!(child.wait().expect("wait child").success());
    }

    #[test]
    fn pty_child_read_available_empty_nonblocking_returns_promptly() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sleep", test_size());
        command.arg("1");
        let child = command.spawn().expect("spawn child");
        child.set_nonblocking().expect("set nonblocking");

        let mut output = Vec::new();
        let read = child
            .read_available(&mut output, 64)
            .expect("read available");

        assert_eq!(read, PtyRead::default());
        assert!(output.is_empty());
    }

    #[test]
    fn pty_child_resize_updates_reported_size_after_spawn() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sleep", test_size());
        command.arg("1");
        let child = command.spawn().expect("spawn child");
        let resized = PtySize {
            rows: 33,
            cols: 101,
            width_px: 1001,
            height_px: 777,
        };

        child.resize(resized).expect("resize pty child");

        assert_eq!(pty_size(child.master_fd()).expect("get pty size"), resized);
    }

    #[test]
    fn pty_child_try_wait_reports_running_then_exited() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sleep", test_size());
        command.arg("1");
        let mut child = command.spawn().expect("spawn child");

        assert!(child.try_wait().expect("try wait running").is_none());
        assert!(child.wait().expect("wait child").success());
        assert!(child.try_wait().expect("try wait exited").is_some());
    }

    #[test]
    fn pty_child_poll_and_read_available_report_eof_for_short_lived_child() {
        let _guard = PTY_COMMAND_LOCK.lock().unwrap();
        let mut command = PtyCommand::new("/bin/sh", test_size());
        command.arg("-c").arg("printf done");
        let mut child = command.spawn().expect("spawn child");
        child.set_nonblocking().expect("set nonblocking");

        let readiness = child.poll(500).expect("poll output");
        assert!(
            readiness.readable || readiness.hangup || readiness.error,
            "expected output or exit readiness: {readiness:?}"
        );

        let mut output = Vec::new();
        let first = child
            .read_available(&mut output, 16)
            .expect("read available");
        assert_eq!(first.bytes_read, output.len());
        assert!(output.starts_with(b"done"));
        assert!(child.wait().expect("wait child").success());

        let mut saw_eof = first.eof;
        for _ in 0..10 {
            if saw_eof {
                break;
            }
            let readiness = child.poll(100).expect("poll eof");
            if readiness.hangup || readiness.error || readiness.readable {
                let read = child
                    .read_available(&mut output, 16)
                    .expect("read remaining");
                saw_eof = read.eof;
            }
        }
        assert!(saw_eof, "expected EOF after short-lived child exit");
    }
}
