//! Optional, read-only metadata for discovery. Missing data never prevents listing.
use crate::server::EulaState;
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;
use zip::ZipArchive;

const METADATA_LIMIT: u64 = 1024 * 1024;

fn text(reader: impl Read) -> Option<String> {
    let mut result = String::new();
    reader
        .take(METADATA_LIMIT + 1)
        .read_to_string(&mut result)
        .ok()?;
    (result.len() as u64 <= METADATA_LIMIT).then_some(result)
}

fn entry(archive: &mut ZipArchive<File>, name: &str) -> Option<String> {
    text(archive.by_name(name).ok()?)
}

fn properties(text: &str) -> Option<HashMap<String, String>> {
    let mut result = HashMap::new();
    java_properties::PropertiesIter::new_with_encoding(text.as_bytes(), encoding_rs::UTF_8)
        .read_into(|key, value| {
            result.insert(key, value);
        })
        .ok()?;
    Some(result)
}

fn jar_version(path: &Path, kind: &str) -> Option<String> {
    let mut archive = ZipArchive::new(File::open(path).ok()?).ok()?;
    if matches!(kind, "arclight" | "mohist" | "banner" | "catserver")
        && let Some(version) = hybrid_version(&mut archive)
    {
        return Some(version);
    }
    if matches!(kind, "velocity" | "bungeecord") {
        let manifest = entry(&mut archive, "META-INF/MANIFEST.MF")?
            .replace("\r\n ", "")
            .replace("\n ", "");
        if let Some(version) = manifest.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.eq_ignore_ascii_case("Implementation-Version") && !value.trim().is_empty())
                .then(|| value.trim().to_owned())
        }) {
            return Some(version);
        }
        return properties(&entry(
            &mut archive,
            "META-INF/maven/net.md-5/bungeecord-bootstrap/pom.properties",
        )?)?
        .remove("version");
    }
    if let Some(json) = entry(&mut archive, "version.json")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&json)
        && let Some(version) = value
            .get("id")
            .and_then(|id| id.as_str())
            .filter(|v| !v.is_empty())
    {
        return Some(version.to_owned());
    }
    if let Some(contents) = entry(&mut archive, "install.properties")
        && let Some(version) = properties(&contents)?.remove("game-version")
    {
        return Some(version);
    }
    let versions = entry(&mut archive, "META-INF/versions.list")?;
    let ids: BTreeSet<_> = versions
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .filter(|id| !id.is_empty())
        .collect();
    (ids.len() == 1).then(|| ids.into_iter().next().unwrap().to_owned())
}

fn hybrid_version(archive: &mut ZipArchive<File>) -> Option<String> {
    let exact = |value: &str| {
        matches!(value.split('.').count(), 2 | 3)
            && value
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    };
    for name in ["META-INF/installer.json", "installer.json"] {
        if let Some(text) = entry(archive, name)
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(version) = value
                .pointer("/installer/minecraft")
                .and_then(|v| v.as_str())
                .filter(|v| exact(v))
        {
            return Some(version.into());
        }
    }
    if let Some(manifest) = entry(archive, "META-INF/MANIFEST.MF") {
        let manifest = manifest.replace("\r\n ", "").replace("\n ", "");
        // Only main attributes; named sections can describe bundled Forge libraries.
        for line in manifest.lines().take_while(|line| !line.trim().is_empty()) {
            if let Some((key, value)) = line.split_once(':')
                && [
                    "Fabric-Minecraft-Version",
                    "Minecraft-Version",
                    "Git-Branch",
                ]
                .iter()
                .any(|name| key.eq_ignore_ascii_case(name))
                && exact(value.trim())
            {
                return Some(value.trim().into());
            }
        }
    }
    if let Some(text) = entry(archive, "fabric.mod.json")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(version) = value
            .pointer("/depends/minecraft")
            .and_then(|v| v.as_str())
            .filter(|v| exact(v))
    {
        return Some(version.into());
    }
    None
}

pub(crate) fn version(
    directory: &Path,
    jar: Option<&Path>,
    arguments: &[PathBuf],
    kind: &str,
) -> Option<String> {
    if kind == "ambiguous" {
        return None;
    }
    if let Some(jar) = jar
        && let Some(version) = jar_version(&directory.join(jar), kind)
    {
        return Some(normalize_forge_version(version, kind));
    }
    if !matches!(kind, "forge" | "neoforge") {
        return None;
    }
    if arguments.is_empty() {
        return legacy_forge_version(directory);
    }
    let [args] = arguments else {
        return None;
    };
    let args = directory.join(args);
    let contents = text(File::open(&args).ok()?)?;
    let words: Vec<_> = contents
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .collect();
    for pair in words.windows(2) {
        if pair[0] == "--fml.mcVersion" {
            return Some(pair[1].trim_matches(['\'', '"']).to_owned());
        }
    }
    // Modern Forge shims keep the Minecraft version in the installed server payload.
    let mut versions = BTreeSet::new();
    for item in fs::read_dir(args.parent()?).ok()? {
        let item = item.ok()?;
        if item.file_name().to_string_lossy().ends_with("-server.jar")
            && let Some(version) = jar_version(&item.path(), kind)
        {
            versions.insert(version);
        }
    }
    (versions.len() == 1).then(|| versions.into_iter().next().unwrap())
}

/// EULA policy for listing: recognized proxies are exempt, and an unreadable
/// `eula.txt` is reported as not accepted rather than failing the scan.
pub(crate) fn eula(directory: &Path, kind: &str) -> EulaState {
    if matches!(kind, "velocity" | "bungeecord") {
        return EulaState::NotRequired;
    }
    match crate::run::eula_accepted(directory) {
        Ok(true) => EulaState::Accepted,
        _ => EulaState::NotAccepted,
    }
}

/// Legacy Forge `version.json` ids append the Forge build (`1.6.4-Forge9.11.1.1345`).
fn normalize_forge_version(version: String, kind: &str) -> String {
    if kind != "forge" {
        return version;
    }
    let lowercase = version.to_ascii_lowercase();
    if let Some(index) = lowercase.find("-forge") {
        let prefix = &version[..index];
        if !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return prefix.to_owned();
        }
    }
    version
}

/// Legacy Forge installs name the vanilla server or the launcher after the Minecraft version.
fn legacy_forge_version(directory: &Path) -> Option<String> {
    let mut versions = BTreeSet::new();
    for entry in fs::read_dir(directory).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(version) = name
            .strip_prefix("minecraft_server.")
            .and_then(|rest| rest.strip_suffix(".jar"))
            .filter(|version| !version.is_empty())
        {
            versions.insert(version.to_owned());
        }
        for prefix in ["forge-", "minecraftforge-universal-"] {
            if let Some(version) = name
                .strip_prefix(prefix)
                .and_then(|rest| rest.split('-').next())
                .filter(|version| !version.is_empty())
            {
                versions.insert(version.to_owned());
            }
        }
    }
    (versions.len() == 1).then(|| versions.into_iter().next().unwrap())
}

/// Sum regular-file lengths in the configured world and split Bukkit dimensions.
/// Symlinks are not traversed; unreadable or unsupported locations return None.
pub(crate) fn map_size(directory: &Path, kind: &str) -> Option<u64> {
    if matches!(kind, "velocity" | "bungeecord") {
        return None;
    }
    let config = match File::open(directory.join("server.properties")) {
        Ok(file) => properties(&text(file)?)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
        Err(_) => return None,
    };
    let level = config
        .get("level-name")
        .map(String::as_str)
        .unwrap_or("world");
    if level.is_empty()
        || !Path::new(level)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let mut total = 0u64;
    for suffix in ["", "_nether", "_the_end"] {
        let path = directory.join(format!("{level}{suffix}"));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        };
        if !metadata.is_dir() {
            return None;
        }
        for item in WalkDir::new(path)
            .follow_links(false)
            .follow_root_links(false)
        {
            let item = item.ok()?;
            if item.file_type().is_file() {
                total = total.checked_add(item.metadata().ok()?.len())?;
            }
        }
    }
    Some(total)
}
