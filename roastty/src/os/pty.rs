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
    cwd: Option<PathBuf>,
    size: PtySize,
}

impl PtyCommand {
    pub(crate) fn new(program: impl Into<OsString>, size: PtySize) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
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
    fn pty_child_drop_kills_and_reaps_running_child() {
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
}
