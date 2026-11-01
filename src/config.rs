use crate::{Error, Result};
use std::path::{Component, Path};

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
