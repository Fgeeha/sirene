//! Errors retain their underlying causes until a caller presents them.

use std::fmt;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Error>;

/// An operation's context and its original, inspectable cause.
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Arc::new(source)),
        }
    }

    pub fn context(self, message: impl Into<String>) -> Self {
        Self::with_source(message, self)
    }

    pub fn with_boxed_source(
        message: impl Into<String>,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Arc::from(source)),
        }
    }

    /// Formats all operation contexts and causes for a CLI or UI boundary.
    pub fn report(&self) -> String {
        let mut report = self.to_string();
        let mut source = std::error::Error::source(self);
        while let Some(cause) = source {
            report.push_str(": ");
            report.push_str(&cause.to_string());
            source = cause.source();
        }
        report
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as &dyn std::error::Error)
    }
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        // Preserve the I/O error itself, including its kind and raw OS code.
        Self::with_source("I/O operation failed", source)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn contexts_preserve_typed_causes_after_cloning() {
        let error = Error::with_source("read input", std::io::Error::from_raw_os_error(2))
            .context("render diagram");
        let copy = error.clone();
        assert_eq!(copy.to_string(), "render diagram");
        let read = copy.source().unwrap().downcast_ref::<Error>().unwrap();
        let io = read
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(io.raw_os_error(), Some(2));
        assert_eq!(copy.report(), format!("render diagram: read input: {io}"));
    }

    #[test]
    fn root_errors_have_no_source() {
        let error = Error::new("invalid scale");
        assert!(error.source().is_none());
        assert_eq!(error.report(), "invalid scale");
    }
}
