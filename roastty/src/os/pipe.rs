//! A close-on-exec pipe (port of upstream `os/pipe`).

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

/// Create a pipe with `FD_CLOEXEC` set on both ends, returned as `(read, write)` (upstream
/// `os.pipe.pipe`). macOS has no `pipe2`, so close-on-exec is set with `fcntl` after
/// `pipe()` — the same emulation `std.posix.pipe2` uses on macOS.
pub(crate) fn pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }

    // Take ownership immediately so the fds close on any early return below.
    let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };

    set_cloexec(read.as_raw_fd())?;
    set_cloexec(write.as_raw_fd())?;

    Ok((read, write))
}

/// Set the `FD_CLOEXEC` (close-on-exec) flag on a file descriptor.
fn set_cloexec(fd: RawFd) -> std::io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_sets_cloexec_on_both_ends() {
        let (read, write) = pipe().expect("pipe");
        for fd in [read.as_raw_fd(), write.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            assert!(flags >= 0, "F_GETFD failed");
            assert_ne!(flags & libc::FD_CLOEXEC, 0, "FD_CLOEXEC not set");
        }
    }

    #[test]
    fn pipe_transfers_bytes() {
        let (read, write) = pipe().expect("pipe");

        let msg = b"hi";
        let written = unsafe {
            libc::write(
                write.as_raw_fd(),
                msg.as_ptr() as *const libc::c_void,
                msg.len(),
            )
        };
        assert_eq!(written, msg.len() as isize);

        let mut buf = [0u8; 2];
        let got = unsafe {
            libc::read(
                read.as_raw_fd(),
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            )
        };
        assert_eq!(got, 2);
        assert_eq!(&buf, b"hi");
    }
}
