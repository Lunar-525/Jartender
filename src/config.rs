use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

/// Optional launch defaults in `<server>/jartender.json`. Loading is explicit for library hosts.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub java: Option<PathBuf>,
    pub jvm_args: Vec<String>,
    pub server_args: Vec<String>,
    pub jar: Option<PathBuf>,
    pub args_file: Option<PathBuf>,
    pub gui: Option<bool>,
}

impl ServerConfig {
    /// Read one server's configuration without creating a file; missing files return `None`.
    pub fn load(root: &Path, server: &str) -> Result<Option<Self>> {
        require_absolute(root, "server root")?;
        validate_name(server)?;
        let path = root.join(server).join("jartender.json");
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(Error::io("read server config", &path, e)),
        };
        let mut bytes = Vec::new();
        file.take(64 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| Error::io("read server config", &path, e))?;
        if bytes.len() > 64 * 1024 {
            return Err(Error::invalid(
                "server config",
                format!("{} exceeds 64 KiB", path.display()),
            ));
        }
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|e| Error::invalid("server config", format!("{}: {e}", path.display())))?;
        config.validate()?;
        Ok(Some(config))
    }

    /// Explicitly save defaults atomically. The server directory must already exist.
    pub fn save(&self, root: &Path, server: &str) -> Result<()> {
        require_absolute(root, "server root")?;
        validate_name(server)?;
        self.validate()?;
        let directory = root.join(server);
        let path = directory.join("jartender.json");
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| Error::invalid("server config", e.to_string()))?;
        if bytes.len() + 1 > 64 * 1024 {
            return Err(Error::invalid("server config", "exceeds 64 KiB"));
        }
        let mut file = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|e| Error::io("stage server config", &path, e))?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| Error::io("write server config", &path, e))?;
        file.persist(&path)
            .map_err(|e| Error::io("save server config", &path, e.error))?;
        Ok(())
    }

    /// Apply saved defaults before host/CLI overrides. No files or environment are read.
    /// Relative Java paths with separators are resolved against the server directory.
    pub fn apply(&self, directory: &Path, options: &mut crate::LaunchOptions) -> Result<()> {
        require_absolute(directory, "server directory")?;
        self.validate()?;
        if let Some(java) = &self.java {
            options.java = if java.is_absolute()
                || (java.components().count() == 1 && !java.to_string_lossy().contains(['/', '\\']))
            {
                java.clone()
            } else {
                directory.join(java)
            };
        }
        options.jar = self.jar.clone();
        options.args_file = self.args_file.clone();
        options.jvm_args = self.jvm_args.iter().map(Into::into).collect();
        options.server_args = self.server_args.iter().map(Into::into).collect();
        options.gui = self.gui.unwrap_or(false);
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.jar.is_some() && self.args_file.is_some() {
            return Err(Error::invalid(
                "server config",
                "jar and args_file are mutually exclusive",
            ));
        }
        if self
            .java
            .as_ref()
            .is_some_and(|java| java.as_os_str().is_empty())
        {
            return Err(Error::invalid("server config", "java must not be empty"));
        }
        if self
            .jvm_args
            .iter()
            .chain(&self.server_args)
            .any(|arg| arg.contains('\0'))
        {
            return Err(Error::invalid(
                "server config",
                "arguments must not contain NUL",
            ));
        }
        Ok(())
    }
}

pub(crate) fn require_absolute(path: &Path, field: &'static str) -> Result<()> {
    if !path.is_absolute() {
        return Err(Error::invalid(field, "an absolute path is required"));
    }
    Ok(())
}

pub(crate) fn validate_name(name: &str) -> Result<()> {
    let mut parts = Path::new(name).components();
    if name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
    {
        return Err(Error::invalid(
            "server name",
            "use one directory name, without separators or a leading dot",
        ));
    }
    Ok(())
}
