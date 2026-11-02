use crate::{Error, Result, config::require_absolute};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Whether the server needs the Minecraft EULA and whether consent is already recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EulaState {
    /// Recognized Velocity/BungeeCord proxy: no Minecraft EULA applies.
    NotRequired,
    Accepted,
    NotAccepted,
}

#[derive(Debug, Clone, Serialize)]
pub struct Server {
    pub name: String,
    pub server_type: &'static str,
    /// Minecraft version, or the proxy's own version. None when metadata is unavailable.
    pub version: Option<String>,
    /// Logical bytes in configured world directories; None for proxies or unavailable data.
    pub map_size_bytes: Option<u64>,
    pub eula: EulaState,
    pub jars: Vec<PathBuf>,
    /// Candidate an old inventory or the legacy-Forge rule picks; `run` uses it when several exist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_jar: Option<PathBuf>,
    /// Platform-specific Forge/NeoForge argument files, relative to the server directory.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub argument_files: Vec<PathBuf>,
}

/// A directory that could not be inspected. Reported instead of aborting the scan.
#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

impl Skipped {
    /// Keep the reported reason short: the path is already its own field.
    fn from_error(path: PathBuf, error: &Error) -> Self {
        let reason = match error {
            Error::Io { source, .. } => source.to_string(),
            other => other.to_string(),
        };
        Self { path, reason }
    }
}

/// Result of [`scan`]: the servers that were listed plus what had to be skipped.
#[derive(Debug, Clone)]
pub struct Discovery {
    pub servers: Vec<Server>,
    pub skipped: Vec<Skipped>,
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
///
/// Directories that cannot be inspected are skipped here. Use [`scan`] when the
/// caller needs to report those directories.
pub fn list(root: &Path) -> Result<Vec<Server>> {
    Ok(scan(root, None)?.servers)
}

/// Like [`list`], but also applies an optional legacy inventory and reports
/// directories that could not be inspected instead of failing the whole scan.
///
/// An old inventory that is missing or unreadable is ignored, exactly like
/// `select_target` treats it: it can only disambiguate real candidates.
pub fn scan(root: &Path, legacy: Option<&Path>) -> Result<Discovery> {
    require_absolute(root, "server root")?;
    if let Some(path) = legacy {
        require_absolute(path, "legacy inventory")?;
    }
    let legacy = legacy.map(read_legacy).unwrap_or_default();
    let mut servers = Vec::new();
    let mut skipped = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| Error::io("read server root", root, e))? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                skipped.push(Skipped {
                    path: root.to_owned(),
                    reason: e.to_string(),
                });
                continue;
            }
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(name) => {
                skipped.push(Skipped {
                    path: path.clone(),
                    reason: format!("directory name is not UTF-8: {}", name.to_string_lossy()),
                });
                continue;
            }
        };
        if name.starts_with('.') {
            continue;
        }
        let jars = match jars(&path) {
            Ok(jars) => jars,
            Err(e) => {
                skipped.push(Skipped::from_error(path.clone(), &e));
                continue;
            }
        };
        let argument_files = match argument_files(&path) {
            Ok(files) => files,
            Err(e) => {
                skipped.push(Skipped::from_error(path.clone(), &e));
                continue;
            }
        };
        if jars.is_empty() && argument_files.is_empty() {
            continue;
        }
        let (server_type, selected_jar) = classify(&name, &path, &jars, &argument_files, &legacy);
        let effective = match jars.as_slice() {
            [jar] => Some(jar.as_path()),
            _ => selected_jar.as_deref(),
        };
        servers.push(Server {
            version: crate::details::version(&path, effective, &argument_files, server_type),
            map_size_bytes: crate::details::map_size(&path, server_type),
            eula: crate::details::eula(&path, server_type),
            name,
            server_type,
            jars,
            selected_jar,
            argument_files,
        });
    }
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Discovery { servers, skipped })
}

/// Decide the current launch type and, when an old inventory would resolve it,
/// the JAR that `run` would select. Mirrors [`select_target`] so both agree.
fn classify(
    name: &str,
    directory: &Path,
    jars: &[PathBuf],
    argument_files: &[PathBuf],
    legacy: &[LegacyServer],
) -> (&'static str, Option<PathBuf>) {
    if let [jar] = jars {
        return (identify(&directory.join(jar)), None);
    }
    if jars.is_empty() {
        return match argument_files {
            [file] => (argument_file_type(file), None),
            _ => ("ambiguous", None),
        };
    }
    if let Some(entry) = legacy
        .iter()
        .find(|entry| entry.server_name == name && jars.contains(&entry.jar_name))
    {
        return (
            identify(&directory.join(&entry.jar_name)),
            Some(entry.jar_name.clone()),
        );
    }
    if let Some(jar) = preferred_forge(directory, jars) {
        return (identify(&directory.join(&jar)), Some(jar));
    }
    ("ambiguous", None)
}

/// Legacy Forge installs keep the Minecraft server JAR beside the Forge launcher.
fn preferred_forge(directory: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    let mut found = None;
    for jar in candidates {
        if identify(&directory.join(jar)) != "forge" {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(jar.clone());
    }
    found
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

pub(crate) fn identify(path: &Path) -> &'static str {
    // Read bounded metadata only; directory/JAR filenames do not determine the type.
    let Ok(manifest) = manifest(path) else {
        return "unknown";
    };
    let unfolded = manifest.replace("\r\n ", "").replace("\n ", "");
    let main = unfolded
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("Main-Class")
                .then(|| value.trim().to_ascii_lowercase())
        })
        .unwrap_or_default();
    if main.starts_with("com.velocitypowered.proxy.") {
        return "velocity";
    }
    if main.starts_with("net.md_5.bungee.") {
        return "bungeecord";
    }
    // Hybrid bootstraps must precede generic Forge/Fabric recognition.
    for (prefix, kind) in [
        ("io.izzel.arclight.", "arclight"),
        ("com.mohistmc.banner.", "banner"),
        ("com.mohistmc.", "mohist"),
        ("catserver.", "catserver"),
    ] {
        if main.starts_with(prefix) {
            return kind;
        }
    }
    if let Ok(file) = File::open(path)
        && let Ok(mut archive) = ZipArchive::new(file)
    {
        for (entry, kind) in [
            ("io/izzel/arclight/server/Launcher.class", "arclight"),
            ("io/izzel/arclight/boot/Main.class", "arclight"),
            ("com/mohistmc/banner/BannerLauncher.class", "banner"),
            ("com/mohistmc/MohistMCStart.class", "mohist"),
            ("com/mohistmc/MohistMC.class", "mohist"),
            ("catserver/server/CatServerLaunch.class", "catserver"),
            ("catserver/server/launch/CatServerLaunch.class", "catserver"),
        ] {
            if archive.by_name(entry).is_ok() {
                return kind;
            }
        }
        if let Ok(entry) = archive.by_name("fabric.mod.json") {
            let mut text = String::new();
            if entry.take(64 * 1024).read_to_string(&mut text).is_ok()
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
                && matches!(
                    value.get("id").and_then(|id| id.as_str()),
                    Some("banner" | "banner-launcher")
                )
            {
                return "banner";
            }
        }
    }
    if main.starts_with("org.leavesmc.") || main.contains("leavesclip") {
        return "leaves";
    }
    if main.contains("paperclip") || main.starts_with("io.papermc.") {
        // Paperclip's payload index distinguishes forks sharing the same Main-Class.
        if let Ok(file) = File::open(path)
            && let Ok(mut archive) = ZipArchive::new(file)
            && let Ok(entry) = archive.by_name("META-INF/versions.list")
        {
            let mut text = String::new();
            if entry.take(64 * 1024).read_to_string(&mut text).is_ok() {
                for line in text.lines() {
                    if let Some(payload) =
                        line.split('\t').nth(2).and_then(|p| p.rsplit('/').next())
                    {
                        if payload.starts_with("purpur-") {
                            return "purpur";
                        }
                        if payload.starts_with("leaves-") {
                            return "leaves";
                        }
                        if payload.starts_with("folia-") {
                            return "folia";
                        }
                    }
                }
            }
        }
        return "paper";
    }
    for (needle, kind) in [
        // Legacy Forge launchers (1.6.4-1.12.2) use a launchwrapper main class.
        ("fml.relauncher.serverlaunchwrapper", "forge"),
        ("fabric", "fabric"),
        ("neoforged", "neoforge"),
        ("minecraftforge", "forge"),
        ("mohist", "mohist"),
        ("purpur", "purpur"),
        ("folia", "folia"),
        ("bukkit", "bukkit"),
        ("net.minecraft.server", "vanilla"),
        ("net.minecraft.bundler", "vanilla"),
    ] {
        if main.contains(needle) {
            return kind;
        }
    }
    "unknown"
}

const ARGUMENT_ROOTS: [(&str, &str); 3] = [
    ("libraries/net/neoforged/neoforge", "neoforge"),
    ("libraries/net/minecraftforge/forge", "forge"),
    ("libraries/net/neoforged/forge", "neoforge"),
];

pub(crate) fn argument_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let filename = if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    };
    let mut files = Vec::new();
    for (base, _) in ARGUMENT_ROOTS {
        let parent = dir.join(base);
        let entries = match fs::read_dir(&parent) {
            Ok(entries) => entries,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(e) => return Err(Error::io("read argument file directory", &parent, e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| Error::io("read directory entry", &parent, e))?;
            let relative = Path::new(base).join(entry.file_name()).join(filename);
            if dir.join(&relative).is_file() {
                files.push(relative);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn argument_file_type(path: &Path) -> &'static str {
    ARGUMENT_ROOTS
        .iter()
        .find(|(base, _)| path.starts_with(base))
        .map(|(_, kind)| *kind)
        .unwrap_or("unknown")
}

pub(crate) enum Target {
    Jar(PathBuf),
    Arguments(PathBuf),
}

pub(crate) fn select_target(
    dir: &Path,
    name: &str,
    jar: Option<&Path>,
    args_file: Option<&Path>,
    legacy_list: Option<&Path>,
) -> Result<(Target, &'static str)> {
    if jar.is_some() && args_file.is_some() {
        return Err(Error::invalid(
            "launch target",
            "select either a JAR or an argument file",
        ));
    }
    if let Some(relative) = args_file {
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(Error::invalid(
                "argument file",
                "use a relative path inside the server directory",
            ));
        }
        let path = dir.join(relative);
        if !path.is_file() {
            return Err(Error::invalid(
                "argument file",
                format!("not found: {}", path.display()),
            ));
        }
        return Ok((Target::Arguments(path), argument_file_type(relative)));
    }
    if jar.is_some() || !jars(dir)?.is_empty() {
        let path = select_jar(dir, name, jar, legacy_list)?;
        let kind = identify(&path);
        return Ok((Target::Jar(path), kind));
    }
    let files = argument_files(dir)?;
    match files.as_slice() {
        [relative] => Ok((
            Target::Arguments(dir.join(relative)),
            argument_file_type(relative),
        )),
        [] => Err(Error::NoJar {
            directory: dir.to_owned(),
        }),
        _ => Err(Error::AmbiguousArgumentFiles {
            directory: dir.to_owned(),
            candidates: files,
        }),
    }
}

#[derive(Deserialize)]
struct LegacyServer {
    server_name: String,
    jar_name: PathBuf,
}

/// An old inventory is a preference, never a requirement: unreadable content is ignored.
fn read_legacy(path: &Path) -> Vec<LegacyServer> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
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
        && let Some(entry) = read_legacy(legacy_list)
            .into_iter()
            .find(|s| s.server_name == name && candidates.contains(&s.jar_name))
    {
        return Ok(dir.join(&entry.jar_name));
    }
    // A legacy Forge directory also contains minecraft_server.<version>.jar.
    if let Some(jar) = preferred_forge(dir, &candidates) {
        return Ok(dir.join(jar));
    }
    Err(Error::AmbiguousJars {
        directory: dir.to_owned(),
        candidates,
    })
}
