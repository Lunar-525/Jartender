use crate::{Error, Result, config::require_absolute};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize)]
pub struct Server {
    pub name: String,
    pub server_type: &'static str,
    pub jars: Vec<PathBuf>,
}

pub(crate) fn jars(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut jars = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| Error::io("read directory", dir, e))? {
        let entry = entry.map_err(|e| Error::io("read directory entry", dir, e))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
            && !entry
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains("installer")
        {
            jars.push(PathBuf::from(entry.file_name()));
        }
    }
    jars.sort();
    Ok(jars)
}

/// Discover immediate server directories without executing JARs or writing files.
/// The root must be absolute; no configuration or environment is read.
pub fn list(root: &Path) -> Result<Vec<Server>> {
    require_absolute(root, "server root")?;
    let mut servers = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| Error::io("read server root", root, e))? {
        let entry = entry.map_err(|e| Error::io("read directory entry", root, e))?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().into_string().map_err(|_| {
            Error::invalid(
                "server name",
                format!("not UTF-8: {}", entry.path().display()),
            )
        })?;
        if name.starts_with('.') {
            continue;
        }
        let jars = jars(&entry.path())?;
        if jars.is_empty() {
            continue;
        }
        let server_type = if jars.len() == 1 {
            identify(&entry.path().join(&jars[0]))
        } else {
            "ambiguous"
        };
        servers.push(Server {
            name,
            server_type,
            jars,
        });
    }
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(servers)
}

pub(crate) fn manifest(path: &Path) -> Result<String> {
    let read = || -> std::result::Result<String, Box<dyn std::error::Error>> {
        let mut archive = ZipArchive::new(File::open(path)?)?;
        let mut text = String::new();
        archive
            .by_name("META-INF/MANIFEST.MF")?
            .take(64 * 1024)
            .read_to_string(&mut text)?;
        Ok(text)
    };
    read().map_err(|e| Error::InvalidJar {
        path: path.to_owned(),
        reason: e.to_string(),
    })
}

fn identify(path: &Path) -> &'static str {
    // Unknown metadata stays unknown. Inspection never executes a JAR.
    let Ok(manifest) = manifest(path) else {
        return "unknown";
    };
    let text = manifest.to_ascii_lowercase();
    for (needle, kind) in [
        ("fabric", "fabric"),
        ("neoforged", "neoforge"),
        ("minecraftforge", "forge"),
        ("mohist", "mohist"),
        ("purpur", "purpur"),
        ("paperclip", "paper"),
        ("papermc", "paper"),
        ("bukkit", "bukkit"),
        ("net.minecraft.server", "vanilla"),
        ("net.minecraft.bundler", "vanilla"),
    ] {
        if text.contains(needle) {
            return kind;
        }
    }
    "unknown"
}

#[derive(Deserialize)]
struct LegacyServer {
    server_name: String,
    jar_name: PathBuf,
}

pub(crate) fn select_jar(
    dir: &Path,
    name: &str,
    explicit: Option<&Path>,
    legacy_list: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(jar) = explicit {
        if jar.is_absolute()
            || jar.components().count() != 1
            || !matches!(
                jar.components().next(),
                Some(std::path::Component::Normal(_))
            )
            || jar.to_string_lossy().contains(['/', '\\'])
        {
            return Err(Error::invalid(
                "JAR",
                "use a filename inside the server directory",
            ));
        }
        let path = dir.join(jar);
        if !path.is_file() {
            return Err(Error::invalid(
                "JAR",
                format!("not found: {}", path.display()),
            ));
        }
        return Ok(path);
    }
    let candidates = jars(dir)?;
    if candidates.len() == 1 {
        return Ok(dir.join(&candidates[0]));
    }
    if candidates.is_empty() {
        return Err(Error::NoJar {
            directory: dir.to_owned(),
        });
    }

    // A legacy selection can disambiguate, but its saved absolute path is never trusted.
    if let Some(legacy_list) = legacy_list
        && let Ok(bytes) = fs::read(legacy_list)
        && let Ok(entries) = serde_json::from_slice::<Vec<LegacyServer>>(&bytes)
        && let Some(entry) = entries
            .iter()
            .find(|s| s.server_name == name && candidates.contains(&s.jar_name))
    {
        return Ok(dir.join(&entry.jar_name));
    }
    Err(Error::AmbiguousJars {
        directory: dir.to_owned(),
        candidates,
    })
}
