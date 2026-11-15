//! Fallible terminal output. Diagnostics never panic, even when stderr is closed.

use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

static FAILED: AtomicBool = AtomicBool::new(false);

pub fn failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

pub fn line(out: &mut impl Write, args: fmt::Arguments<'_>) -> std::io::Result<()> {
    out.write_fmt(args)?;
    out.write_all(b"\n")?;
    out.flush()
}

pub fn diagnostic(args: fmt::Arguments<'_>) {
    if line(&mut std::io::stderr().lock(), args).is_err() {
        FAILED.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
    }

    /// ERRORH-3: failed terminal output returns its I/O cause without unwinding.
    #[test]
    fn output_failure_is_returned() {
        assert_eq!(
            line(&mut Broken, format_args!("hello")).unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
        let mut bytes = Vec::new();
        line(&mut bytes, format_args!("{}", 42)).unwrap();
        assert_eq!(bytes, b"42\n");
    }
}
