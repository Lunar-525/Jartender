use crate::{
    Error, Result,
    config::{require_absolute, validate_name},
    server,
};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Host-supplied launch settings. No environment variables or config files are read.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub server: String,
    /// An absolute path, or a bare executable name to be searched on PATH at spawn time.
    pub java: PathBuf,
    pub jar: Option<PathBuf>,
    /// Optional absolute path to an old inventory, used only to disambiguate JARs.
    pub legacy_list: Option<PathBuf>,
    pub jvm_args: Vec<OsString>,
    pub gui: bool,
    pub server_args: Vec<OsString>,
}

impl LaunchOptions {
    pub fn new(server: impl Into<String>, java: impl Into<PathBuf>) -> Self {
        Self {
            server: server.into(),
            java: java.into(),
            jar: None,
            legacy_list: None,
            jvm_args: Vec::new(),
            gui: false,
            server_args: Vec::new(),
        }
    }
}

/// A launch description. Creating one never starts Java or changes EULA state.
#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub directory: PathBuf,
    pub java: PathBuf,
    pub args: Vec<OsString>,
}

impl LaunchPlan {
    /// Build an unstarted command. The caller controls stdio, spawning, and lifetime.
    /// Check EULA consent with [`check_eula`] before starting the command.
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.java);
        command.args(&self.args).current_dir(&self.directory);
        command
    }
}

/// Inspect the server and prepare arguments without printing or starting a process.
/// `root` and any legacy inventory path must be absolute; Java must be absolute or bare.
pub fn prepare_launch(root: &Path, options: &LaunchOptions) -> Result<LaunchPlan> {
    require_absolute(root, "server root")?;
    validate_name(&options.server)?;
    if let Some(path) = &options.legacy_list {
        require_absolute(path, "legacy inventory")?;
    }
    if options.java.as_os_str().is_empty() {
        return Err(Error::invalid("Java executable", "cannot be empty"));
    }
    let java = if options.java.is_absolute() {
        options
            .java
            .canonicalize()
            .map_err(|e| Error::io("use Java", &options.java, e))?
    } else {
        if options.java.components().count() != 1
            || !matches!(
                options.java.components().next(),
                Some(std::path::Component::Normal(_))
            )
            || options.java.to_string_lossy().contains(['/', '\\', ':'])
        {
            return Err(Error::invalid(
                "Java executable",
                "use an absolute path or a bare executable name",
            ));
        }
        options.java.clone()
    };
    let path = root.join(&options.server);
    let directory = path
        .canonicalize()
        .map_err(|e| Error::io("open server", &path, e))?;
    let jar = server::select_jar(
        &directory,
        &options.server,
        options.jar.as_deref(),
        options.legacy_list.as_deref(),
    )?;
    let mut args = options.jvm_args.clone();
    args.push("-jar".into());
    args.push(jar.into_os_string());
    if !options.gui {
        args.push("nogui".into());
    }
    args.extend(options.server_args.iter().cloned());
    Ok(LaunchPlan {
        directory,
        java,
        args,
    })
}

fn eula_property(line: &str) -> Option<(&str, &str)> {
    line.split_once('=').filter(|(key, _)| key.trim() == "eula")
}

fn read_eula(directory: &Path) -> Result<(PathBuf, String)> {
    require_absolute(directory, "server directory")?;
    let path = directory.join("eula.txt");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::io("read EULA", &path, e)),
    };
    Ok((path, text))
}

/// Check consent without modifying files. Missing consent is a structured error.
pub fn check_eula(directory: &Path) -> Result<()> {
    let (path, text) = read_eula(directory)?;
    if text
        .lines()
        .filter_map(eula_property)
        .next_back()
        .is_some_and(|(_, value)| value.trim() == "true")
    {
        Ok(())
    } else {
        Err(Error::EulaNotAccepted { path })
    }
}

/// Record consent after the host has obtained the user's agreement to the Minecraft EULA.
/// This is an explicit write operation, separate from launch planning and spawning.
pub fn accept_eula(directory: &Path) -> Result<()> {
    let (path, text) = read_eula(directory)?;
    if text
        .lines()
        .filter_map(eula_property)
        .next_back()
        .is_some_and(|(_, value)| value.trim() == "true")
    {
        return Ok(());
    }
    let mut lines: Vec<_> = text
        .lines()
        .filter(|line| eula_property(line).is_none())
        .collect();
    lines.push("eula=true");
    fs::write(&path, format!("{}\n", lines.join("\n")))
        .map_err(|e| Error::io("write EULA", &path, e))
}
