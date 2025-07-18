// ----------------------------------------------
// macos_redirect_stderr()
// ----------------------------------------------

// Using this to deal with some TTY spam from the OpenGL loader on MacOS.
#[cfg(target_os = "macos")]
pub fn macos_redirect_stderr<F, R>(f: F, filename: &str) -> R where F: FnOnce() -> R {
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;
    use libc::{dup, dup2, close, STDERR_FILENO};

    unsafe {
        let saved_fd = dup(STDERR_FILENO);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(filename)
            .expect("Failed to open stderr log file!");
        dup2(file.as_raw_fd(), STDERR_FILENO);
        let result = f();
        dup2(saved_fd, STDERR_FILENO);
        close(saved_fd);
        result
    }
}

#[cfg(not(target_os = "macos"))]
pub fn macos_redirect_stderr<F, R>(f: F, _filename: &str) -> R where F: FnOnce() -> R {
    f() // No-op on non-MacOS
}
