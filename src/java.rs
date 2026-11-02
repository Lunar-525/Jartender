//! Explicit local Java discovery. Only `ScanOptions::system` reads environment settings.
//! Probes run `java -XshowSettings:properties -version`, never a server or a compiler.
//!
//! ```no_run
//! let options = jartender::java::ScanOptions::system();
//! let discovered = jartender::java::scan(&options)?;
//! for runtime in discovered.runtimes {
//!     println!("Java {}: {}", runtime.major, runtime.path.display());
//! }
//! # Ok::<(), jartender::Error>(())
//! ```
pub mod install;
mod system;

use crate::{Error, Result, Skipped, config::require_absolute};
use process_control::{ChildExt, Control};
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct Runtime {
    pub path: PathBuf,
    pub version: String,
    pub major: u32,
    pub vendor: Option<String>,
    pub vendor_version: Option<String>,
    pub arch: Option<String>,
    pub is_64bit: Option<bool>,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Absolute executables, JDK homes, or installation directories to inspect.
    pub paths: Vec<PathBuf>,
    pub timeout: Duration,
    pub max_depth: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            timeout: Duration::from_secs(3),
            max_depth: 6,
        }
    }
}

impl ScanOptions {
    /// Opt in to reading PATH, JAVA_HOME, user locations, and platform installation sources.
    /// No processes are started until `scan` is called.
    pub fn system() -> Self {
        Self {
            paths: system::paths(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Discovery {
    pub runtimes: Vec<Runtime>,
    pub skipped: Vec<Skipped>,
}

/// Discover and probe Java executables with bounded directory traversal and output capture.
/// Failures are reported per path; they never hide working runtimes. Nothing is persisted.
pub fn scan(options: &ScanOptions) -> Result<Discovery> {
    if options.timeout.is_zero() || options.max_depth == 0 || options.max_depth > 12 {
        return Err(Error::invalid(
            "Java scan",
            "timeout must be positive and max_depth must be 1..=12",
        ));
    }
    for path in &options.paths {
        require_absolute(path, "Java search path")?;
    }
    let mut candidates = BTreeSet::new();
    let mut result = Discovery {
        runtimes: Vec::new(),
        skipped: Vec::new(),
    };
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    let mut roots = BTreeSet::new();
    for path in &options.paths {
        match path.try_exists() {
            Ok(true) => (),
            Ok(false) => continue,
            Err(error) => {
                result.skipped.push(Skipped {
                    path: path.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
        }
        if path.is_file() {
            match path.canonicalize() {
                Ok(path) => {
                    candidates.insert(path);
                }
                Err(error) => result.skipped.push(Skipped {
                    path: path.clone(),
                    reason: error.to_string(),
                }),
            }
            continue;
        }
        let root = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                result.skipped.push(Skipped {
                    path: path.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
        };
        if !roots.insert(root.clone()) {
            continue;
        }
        for entry in WalkDir::new(root)
            .follow_links(true)
            .max_depth(options.max_depth)
            .sort_by_file_name()
        {
            match entry {
                Ok(entry) if entry.file_type().is_file() && entry.file_name() == name => {
                    if let Ok(path) = entry.path().canonicalize() {
                        candidates.insert(path);
                    }
                }
                Err(error) => result.skipped.push(Skipped {
                    path: error.path().unwrap_or(path).to_owned(),
                    reason: error.to_string(),
                }),
                _ => (),
            }
        }
    }
    let mut seen = BTreeSet::new();
    for path in candidates {
        match probe(&path, options.timeout) {
            Ok(runtime) if seen.insert(runtime.path.clone()) => result.runtimes.push(runtime),
            Ok(_) => (),
            Err(error) => result.skipped.push(Skipped {
                path,
                reason: match error {
                    Error::JavaProbe { reason, .. } => reason,
                    Error::Io { source, .. } => source.to_string(),
                    other => other.to_string(),
                },
            }),
        }
    }
    result.runtimes.sort_by(|a, b| {
        b.major
            .cmp(&a.major)
            .then_with(|| {
                version_compare::compare(b.version.replace('_', "."), a.version.replace('_', "."))
                    .ok()
                    .and_then(|v| v.ord())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(result)
}

/// Probe an absolute Java executable. Timeout and output are bounded; no environment is changed.
pub fn probe(path: &Path, timeout: Duration) -> Result<Runtime> {
    require_absolute(path, "Java executable")?;
    if timeout.is_zero() {
        return Err(Error::invalid("Java timeout", "must be positive"));
    }
    let path = path
        .canonicalize()
        .map_err(|e| Error::io("open Java", path, e))?;
    let failure = |reason: String| Error::JavaProbe {
        path: path.clone(),
        reason,
    };
    let overflow = Arc::new(AtomicBool::new(false));
    let filter = |overflow: Arc<AtomicBool>| {
        let mut count = 0usize;
        move |bytes: &[u8]| -> std::io::Result<bool> {
            count = count.saturating_add(bytes.len());
            if count > 64 * 1024 {
                overflow.store(true, Ordering::Relaxed);
                Ok(false)
            } else {
                Ok(true)
            }
        }
    };
    let mut command = Command::new(&path);
    command
        .args(["-XshowSettings:properties", "-version"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // User-injected VM options must not load agents or change a version-only probe.
    for key in [
        "JAVA_TOOL_OPTIONS",
        "JDK_JAVA_OPTIONS",
        "_JAVA_OPTIONS",
        "CLASSPATH",
    ] {
        command.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = command
        .spawn()
        .map_err(|e| failure(e.to_string()))?
        .controlled_with_output()
        .time_limit(timeout)
        .terminate_for_timeout()
        .stdout_filter(filter(overflow.clone()))
        .stderr_filter(filter(overflow.clone()))
        .wait()
        .map_err(|e| failure(e.to_string()))?
        .ok_or_else(|| failure(format!("timed out after {} ms", timeout.as_millis())))?;
    if overflow.load(Ordering::Relaxed) {
        return Err(failure("probe output exceeds 64 KiB per stream".into()));
    }
    if !output.status.success() {
        return Err(failure(format!(
            "version command failed: {:?}",
            output.status
        )));
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse(&path, &text).ok_or_else(|| failure("missing or invalid java.version".into()))
}

fn parse(path: &Path, text: &str) -> Option<Runtime> {
    let values: HashMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();
    let version = values.get("java.version")?.to_string();
    let major = major_version(&version)?;
    let home = values
        .get("java.home")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    let actual = home
        .as_ref()
        .and_then(|home| {
            home.join("bin")
                .join(if cfg!(windows) { "java.exe" } else { "java" })
                .canonicalize()
                .ok()
        })
        .unwrap_or_else(|| path.to_owned());
    let arch = values.get("os.arch").map(|v| v.to_string());
    let is_64bit = values
        .get("sun.arch.data.model")
        .and_then(|v| match *v {
            "64" => Some(true),
            "32" => Some(false),
            _ => None,
        })
        .or_else(|| {
            arch.as_deref().and_then(|v| match v {
                "amd64" | "x86_64" | "aarch64" | "arm64" | "riscv64" | "ppc64" | "ppc64le"
                | "s390x" => Some(true),
                "x86" | "i386" | "i686" | "arm" => Some(false),
                _ => None,
            })
        });
    Some(Runtime {
        path: actual,
        version,
        major,
        vendor: values.get("java.vendor").map(|v| v.to_string()),
        vendor_version: values.get("java.vendor.version").map(|v| v.to_string()),
        arch,
        is_64bit,
        home,
    })
}

pub(crate) fn major_version(version: &str) -> Option<u32> {
    let mut numbers = version.split(['.', '_', '-', '+']);
    let first = numbers.next()?.parse::<u32>().ok()?;
    let major = if first == 1 {
        numbers.next()?.parse().ok()?
    } else {
        first
    };
    (major > 0).then_some(major)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_architectures_and_exact_property_keys_are_parsed() {
        for (version, major, arch, wide) in [
            ("1.8.0_442", 8, "amd64", Some(true)),
            ("21.0.9+7", 21, "aarch64", Some(true)),
            ("25-ea", 25, "x86", Some(false)),
            ("17.0.1", 17, "unknown", None),
        ] {
            let row=parse(Path::new("/missing/java"),&format!("java.version = {version}\njava.vendor = Example\nos.arch = {arch}\nother.java.version = 999\n")).unwrap();
            assert_eq!(row.major, major);
            assert_eq!(row.is_64bit, wide);
            assert_eq!(row.version, version);
        }
        assert!(
            parse(
                Path::new("/missing/java"),
                "java.vendor = Example\nos.arch = amd64"
            )
            .is_none()
        );
        assert!(parse(Path::new("/missing/java"), "java.version = invalid").is_none());
        assert!(
            scan(&ScanOptions {
                paths: vec![PathBuf::from("relative")],
                ..ScanOptions::default()
            })
            .is_err()
        );
    }

    #[cfg(unix)]
    fn fake(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn scan_deduplicates_symlinks_and_reports_failed_probes_without_hiding_runtimes() {
        use std::os::unix::fs::symlink;
        let root = tempfile::TempDir::new().unwrap();
        let java = root.path().join("jdk/bin/java");
        fake(
            &java,
            "[ \"$*\" = '-XshowSettings:properties -version' ] || exit 9\nprintf 'java.version = 21.0.9\\njava.vendor = Example\\nos.arch = aarch64\\n' >&2",
        );
        fake(&root.path().join("broken/bin/java"), "exit 7");
        symlink(root.path().join("jdk"), root.path().join("current")).unwrap();
        let result = scan(&ScanOptions {
            paths: vec![root.path().into(), java.clone()],
            ..ScanOptions::default()
        })
        .unwrap();
        assert_eq!(result.runtimes.len(), 1);
        assert_eq!(result.runtimes[0].path, java.canonicalize().unwrap());
        assert_eq!(result.runtimes[0].major, 21);
        assert_eq!(result.skipped.len(), 1);
        let _: Box<dyn Send + Sync> = Box::new(result);
    }

    #[cfg(unix)]
    #[test]
    fn hanging_closed_pipe_and_noisy_probes_have_a_deadline() {
        let root = tempfile::TempDir::new().unwrap();
        for body in [
            "exec /bin/sleep 30",
            "exec 1>&- 2>&-; exec /bin/sleep 30",
            "while :; do printf 'output output output output output output\\n'; done",
        ] {
            let java = root.path().join("java");
            fake(&java, body);
            let start = std::time::Instant::now();
            assert!(matches!(
                probe(&java, Duration::from_millis(150)),
                Err(Error::JavaProbe { .. })
            ));
            assert!(start.elapsed() < Duration::from_secs(3));
        }
    }

    #[cfg(unix)]
    #[test]
    fn excessive_output_is_rejected_and_vm_option_environment_is_removed() {
        let root = tempfile::TempDir::new().unwrap();
        let java = root.path().join("java");
        fake(
            &java,
            "i=0; while [ $i -lt 4096 ]; do printf 'XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX'; i=$((i+1)); done\nprintf '\\njava.version = 21.0.1\\n'",
        );
        let error = probe(&java, Duration::from_secs(3)).unwrap_err();
        assert!(error.to_string().contains("64 KiB"));
        // A subprocess-level environment fixture avoids global environment mutation in tests.
        fake(
            &java,
            "[ -z \"$JAVA_TOOL_OPTIONS$JDK_JAVA_OPTIONS$_JAVA_OPTIONS$CLASSPATH\" ] || exit 9\nprintf 'java.version = 21.0.1\\n'",
        );
        assert_eq!(probe(&java, Duration::from_secs(1)).unwrap().major, 21);
    }
}
