use std::{
    fmt, io,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Error>;

/// Errors are inspectable by launchers and transferable to worker threads.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    InvalidInput {
        field: &'static str,
        message: String,
    },
    NoJar {
        directory: PathBuf,
    },
    AmbiguousJars {
        directory: PathBuf,
        candidates: Vec<PathBuf>,
    },
    EulaNotAccepted {
        path: PathBuf,
    },
    InvalidJar {
        path: PathBuf,
        reason: String,
    },
    Download {
        url: String,
        source: ureq::Error,
    },
    InvalidMetadata {
        url: String,
        source: serde_json::Error,
    },
    NoCompatibleVersion {
        component: &'static str,
        requested: Option<String>,
    },
    DownloadTooLarge {
        limit: u64,
    },
    Cancelled,
}

impl Error {
    pub(crate) fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_owned(),
            source,
        }
    }

    pub(crate) fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidInput {
            field,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(f, "cannot {operation} {}: {source}", path.display()),
            Self::InvalidInput { field, message } => write!(f, "invalid {field}: {message}"),
            Self::NoJar { directory } => write!(f, "no server JAR in {}", directory.display()),
            Self::AmbiguousJars {
                directory,
                candidates,
            } => write!(
                f,
                "multiple JARs in {}: {}; select a JAR explicitly",
                directory.display(),
                candidates
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::EulaNotAccepted { path } => write!(
                f,
                "EULA not accepted in {}; read https://aka.ms/MinecraftEULA before accepting",
                path.display()
            ),
            Self::InvalidJar { path, reason } => {
                write!(f, "invalid JAR {}: {reason}", path.display())
            }
            Self::Download { url, source } => write!(f, "download failed for {url}: {source}"),
            Self::InvalidMetadata { url, source } => {
                write!(f, "invalid metadata from {url}: {source}")
            }
            Self::NoCompatibleVersion {
                component,
                requested,
            } => write!(
                f,
                "no compatible {component} version{}",
                requested
                    .as_ref()
                    .map(|v| format!(" {v}"))
                    .unwrap_or_default()
            ),
            Self::DownloadTooLarge { limit } => {
                write!(f, "launcher exceeds {limit} byte download limit")
            }
            Self::Cancelled => f.write_str("installation cancelled"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Download { source, .. } => Some(source),
            Self::InvalidMetadata { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(source: io::Error) -> Self {
        Self::io("perform I/O on", Path::new("."), source)
    }
}
