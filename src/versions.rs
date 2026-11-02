//! Read-only version queries. No local configuration, files, or Java are needed.
//!
//! ```no_run
//! use jartender::{install::InstallKind, versions::{query, VersionQuery}};
//! # fn main() -> jartender::Result<()> {
//! let releases = query(&VersionQuery::default())?;
//! let fabric = query(&VersionQuery::loader("26.3", InstallKind::Fabric))?;
//! # let _ = (releases, fabric);
//! # Ok(())
//! # }
//! ```
use crate::{
    Error, Result,
    install::{self, InstallKind},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MINECRAFT: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC: &str = "https://meta.fabricmc.net/v2/versions";
const FORGE: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
const MOHIST_PROJECTS: &str = "https://mohistmc.com/api/v2/projects";

/// A version the caller can pass to `InstallRequest`. Newer versions appear first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Version {
    pub version: String,
    /// Purpur/Mohist/Banner have no stability channels: their published catalog entries are `true`.
    pub stable: bool,
    /// Latest stable for Minecraft/Fabric/NeoForge/Paper/Folia/Leaves; official recommended for Forge,
    /// official latest build for Purpur/Mohist/Banner. Arclight marks all platform assets
    /// in its newest stable release; CatServer marks the newest matching release.
    pub recommended: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VersionQuery {
    /// Set both `minecraft` and `kind` to query compatible loader builds.
    /// Leave both unset to query Minecraft releases.
    pub minecraft: Option<String>,
    pub kind: Option<InstallKind>,
    pub include_unstable: bool,
    /// GitHub token for Arclight/CatServer queries; anonymous requests are rate-limited.
    pub github_token: Option<String>,
    pub proxy: crate::network::ProxyConfig,
}

impl VersionQuery {
    pub fn loader(minecraft: impl Into<String>, kind: InstallKind) -> Self {
        Self {
            minecraft: Some(minecraft.into()),
            kind: Some(kind),
            include_unstable: false,
            github_token: None,
            proxy: Default::default(),
        }
    }
}

/// Query official version services synchronously, without printing or changing files.
/// GUI hosts should call this on a worker thread. HTTP requests have a 180-second timeout.
pub fn query(request: &VersionQuery) -> Result<Vec<Version>> {
    query_with(
        request,
        &request.proxy.agent(std::time::Duration::from_secs(180)),
        &Sources::default(),
    )
}

/// Minecraft versions a core or loader can install, newest first.
///
/// This answers "which Minecraft versions does KIND support?" without naming one.
/// `github_token` raises the GitHub limit for Arclight/CatServer.
pub fn supported_minecraft(
    kind: InstallKind,
    include_unstable: bool,
    github_token: Option<&str>,
) -> Result<Vec<Version>> {
    supported_minecraft_with_proxy(kind, include_unstable, github_token, &Default::default())
}

/// Query supported Minecraft versions with the same proxy policy as downloads.
pub fn supported_minecraft_with_proxy(
    kind: InstallKind,
    include_unstable: bool,
    github_token: Option<&str>,
    proxy: &crate::network::ProxyConfig,
) -> Result<Vec<Version>> {
    supported_with(
        kind,
        include_unstable,
        &proxy.agent(std::time::Duration::from_secs(180)),
        &Sources::default(),
        github_token,
    )
}

/// A core or loader that can install a given Minecraft version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindSupport {
    pub kind: InstallKind,
    /// At least one stable build exists for that Minecraft version.
    pub stable: bool,
    /// Newest stable (or newest available) build or loader version.
    pub latest: Option<String>,
}

/// Cores and loaders that can install `minecraft`, in [`InstallKind::ALL`] order.
pub fn kinds_for(minecraft: &str, github_token: Option<&str>) -> Result<Vec<KindSupport>> {
    kinds_for_with_proxy(minecraft, github_token, &Default::default())
}

/// Like [`kinds_for`], using the same proxy policy as downloads.
pub fn kinds_for_with_proxy(
    minecraft: &str,
    github_token: Option<&str>,
    proxy: &crate::network::ProxyConfig,
) -> Result<Vec<KindSupport>> {
    kinds_for_with(
        minecraft,
        &proxy.agent(std::time::Duration::from_secs(180)),
        &Sources::default(),
        github_token,
    )
}

fn kinds_for_with(
    minecraft: &str,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
    token: Option<&str>,
) -> Result<Vec<KindSupport>> {
    install::validate_version(minecraft)?;
    let mut supported = Vec::new();
    for &kind in InstallKind::ALL {
        let mut request = VersionQuery {
            minecraft: Some(minecraft.to_owned()),
            kind: Some(kind),
            include_unstable: false,
            github_token: token.map(str::to_owned),
            proxy: Default::default(),
        };
        let stable = match query_with(&request, agent, sources) {
            Ok(versions) => versions,
            // Fill/Purpur/Leaves answer 404 for versions they never published.
            Err(error) if not_found(&error) => continue,
            Err(error) => return Err(error),
        };
        if let Some(first) = stable.first() {
            supported.push(KindSupport {
                kind,
                stable: true,
                latest: Some(first.version.clone()),
            });
            continue;
        }
        // Keep cores that only publish alpha/beta builds, marked as unstable.
        request.include_unstable = true;
        let any = match query_with(&request, agent, sources) {
            Ok(versions) => versions,
            Err(error) if not_found(&error) => Vec::new(),
            Err(error) => return Err(error),
        };
        if let Some(first) = any.first() {
            supported.push(KindSupport {
                kind,
                stable: false,
                latest: Some(first.version.clone()),
            });
        }
    }
    Ok(supported)
}

fn not_found(error: &Error) -> bool {
    matches!(
        error,
        Error::Download {
            // Fabric answers 400 for unknown game versions; Fill/Purpur/Leaves use 404.
            source: ureq::Error::StatusCode(400 | 404),
            ..
        }
    )
}

fn supported_with(
    kind: InstallKind,
    all: bool,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
    token: Option<&str>,
) -> Result<Vec<Version>> {
    let mut versions = match kind {
        InstallKind::Fabric => fabric_games(agent, sources.fabric)?,
        InstallKind::Paper => fill_games(agent, sources.paper)?,
        InstallKind::Folia => fill_games(agent, sources.folia)?,
        InstallKind::Purpur => version_catalog(agent, sources.purpur)?,
        InstallKind::Leaves => version_catalog(agent, sources.leaves)?,
        InstallKind::Forge => forge_games(agent, sources.forge)?,
        InstallKind::NeoForge => neoforge_games(agent, sources.neoforge)?,
        InstallKind::Mohist | InstallKind::Banner => {
            mohist_games(agent, sources.mohist_projects, kind)?
        }
        InstallKind::Arclight | InstallKind::CatServer => {
            let base = if kind == InstallKind::Arclight {
                sources.arclight
            } else {
                sources.catserver
            };
            github_games(agent, base, kind, token)?
        }
    };
    finish(&mut versions, all);
    recommend_latest(&mut versions);
    Ok(versions)
}

/// Release versions are plain dotted numbers; `-pre`/`-rc`/snapshot names are not.
fn release_version(version: &str) -> bool {
    !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

#[derive(Deserialize)]
struct GameRow {
    version: String,
    #[serde(default)]
    stable: bool,
}

fn fabric_games(agent: &ureq::Agent, base: &str) -> Result<Vec<Version>> {
    let rows: Vec<GameRow> = install::metadata(agent, &format!("{base}/game"))?;
    Ok(rows
        .into_iter()
        .map(|row| Version {
            version: row.version,
            stable: row.stable,
            recommended: false,
        })
        .collect())
}

fn fill_games(agent: &ureq::Agent, base: &str) -> Result<Vec<Version>> {
    #[derive(Deserialize)]
    struct Project {
        #[serde(default)]
        versions: HashMap<String, Vec<String>>,
    }
    let project: Project = install::metadata(agent, base)?;
    let mut versions = Vec::new();
    for value in project.versions.into_values().flatten() {
        versions.push(Version {
            stable: release_version(&value),
            version: value,
            recommended: false,
        });
    }
    Ok(versions)
}

fn version_catalog(agent: &ureq::Agent, base: &str) -> Result<Vec<Version>> {
    #[derive(Deserialize)]
    struct Catalog {
        #[serde(default)]
        versions: Vec<String>,
    }
    let catalog: Catalog = install::metadata(agent, base)?;
    Ok(catalog
        .versions
        .into_iter()
        .map(|version| Version {
            stable: release_version(&version),
            version,
            recommended: false,
        })
        .collect())
}

fn forge_games(agent: &ureq::Agent, url: &str) -> Result<Vec<Version>> {
    Ok(maven(agent, url)?
        .into_iter()
        .filter_map(|version| {
            version
                .split_once('-')
                .map(|(minecraft, _)| minecraft.to_owned())
        })
        .map(|version| Version {
            stable: release_version(&version),
            version,
            recommended: false,
        })
        .collect())
}

/// NeoForge encodes Minecraft in its own version: 20.4.237 -> 1.20.4, 26.3.0.100 -> 26.3.
fn neoforge_minecraft(version: &str) -> Option<String> {
    let lower = version.to_ascii_lowercase();
    if lower.contains("alpha") || lower.contains("beta") || lower.contains("snapshot") {
        return None;
    }
    let parts: Vec<&str> = version.split('.').collect();
    if !parts
        .iter()
        .all(|part| part.bytes().all(|b| b.is_ascii_digit()) && !part.is_empty())
    {
        return None;
    }
    match parts.len() {
        3 => Some(format!("1.{}.{}", parts[0], parts[1])),
        _ if parts.len() >= 4 => Some(if parts[2] == "0" {
            format!("{}.{}", parts[0], parts[1])
        } else {
            format!("{}.{}.{}", parts[0], parts[1], parts[2])
        }),
        _ => None,
    }
}

fn neoforge_games(agent: &ureq::Agent, url: &str) -> Result<Vec<Version>> {
    Ok(maven(agent, url)?
        .into_iter()
        .filter_map(|version| neoforge_minecraft(&version))
        .map(|version| Version {
            stable: release_version(&version),
            version,
            recommended: false,
        })
        .collect())
}

fn mohist_games(agent: &ureq::Agent, url: &str, kind: InstallKind) -> Result<Vec<Version>> {
    #[derive(Deserialize)]
    struct Project {
        project: String,
        #[serde(default)]
        versions: Vec<String>,
    }
    let projects: Vec<Project> = install::metadata(agent, url)?;
    let name = if kind == InstallKind::Mohist {
        "mohist"
    } else {
        "banner"
    };
    let versions = projects
        .into_iter()
        .find(|project| project.project == name)
        .map(|project| project.versions)
        .unwrap_or_default();
    Ok(versions
        .into_iter()
        .map(|version| Version {
            stable: release_version(&version),
            version,
            recommended: false,
        })
        .collect())
}

#[derive(Deserialize)]
struct GithubRelease {
    #[serde(default)]
    target_commitish: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
}

/// Arclight/CatServer publish releases only; collect Minecraft versions from assets.
fn github_games(
    agent: &ureq::Agent,
    base: &str,
    kind: InstallKind,
    token: Option<&str>,
) -> Result<Vec<Version>> {
    let mut seen: HashMap<String, bool> = HashMap::new();
    for page in 1..=100 {
        let url = format!("{base}?per_page=100&page={page}");
        let releases: Vec<GithubRelease> = install::hybrid::github_json(agent, &url, token)?;
        let last_page = releases.len() < 100;
        for release in releases.into_iter().filter(|release| !release.draft) {
            let stable = !release.prerelease;
            let mut candidates = Vec::new();
            // Arclight's target_commitish is a branch name; CatServer records Minecraft there.
            if kind == InstallKind::CatServer && release_version(&release.target_commitish) {
                candidates.push(release.target_commitish);
            }
            for asset in release.assets {
                if let Some(minecraft) = github_asset_minecraft(kind, &asset.name) {
                    candidates.push(minecraft);
                }
            }
            for minecraft in candidates {
                *seen.entry(minecraft).or_insert(false) |= stable;
            }
        }
        if last_page {
            break;
        }
    }
    Ok(seen
        .into_iter()
        .map(|(version, stable)| Version {
            version,
            stable,
            recommended: false,
        })
        .collect())
}

fn github_asset_minecraft(kind: InstallKind, name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    let rest = if kind == InstallKind::Arclight {
        ["arclight-forge-", "arclight-neoforge-", "arclight-fabric-"]
            .iter()
            .find_map(|prefix| name.strip_prefix(prefix))?
    } else {
        name.strip_prefix("catserver-")?
    };
    let minecraft = rest.split('-').next()?;
    release_version(minecraft).then(|| minecraft.to_owned())
}

struct Sources<'a> {
    minecraft: &'a str,
    fabric: &'a str,
    forge: &'a str,
    promotions: &'a str,
    neoforge: &'a str,
    paper: &'a str,
    folia: &'a str,
    purpur: &'a str,
    leaves: &'a str,
    arclight: &'a str,
    catserver: &'a str,
    mohist: &'a str,
    mohist_projects: &'a str,
}

impl Default for Sources<'_> {
    fn default() -> Self {
        Self {
            minecraft: MINECRAFT,
            fabric: FABRIC,
            forge: FORGE,
            promotions: PROMOTIONS,
            neoforge: NEOFORGE,
            paper: install::fill::PAPER,
            folia: install::fill::FOLIA,
            purpur: install::purpur::API,
            leaves: install::leaves::API,
            arclight: install::hybrid::ARCLIGHT,
            catserver: install::hybrid::CATSERVER,
            mohist: install::hybrid::MOHIST,
            mohist_projects: MOHIST_PROJECTS,
        }
    }
}

fn query_with(
    request: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<Version>> {
    let all = request.include_unstable;
    match (&request.minecraft, request.kind) {
        (None, None) => minecraft(agent, sources.minecraft, all),
        (Some(mc), Some(kind)) => {
            install::validate_version(mc)?;
            match kind {
                InstallKind::Arclight
                | InstallKind::CatServer
                | InstallKind::Mohist
                | InstallKind::Banner => {
                    let base = match kind {
                        InstallKind::Arclight => sources.arclight,
                        InstallKind::CatServer => sources.catserver,
                        _ => sources.mohist,
                    };
                    Ok(install::hybrid::catalog(
                        agent,
                        base,
                        kind,
                        mc,
                        request.github_token.as_deref(),
                        false,
                    )?
                    .into_iter()
                    .filter(|build| all || build.stable)
                    .map(|build| Version {
                        version: build.version,
                        stable: build.stable,
                        recommended: build.recommended,
                    })
                    .collect())
                }
                InstallKind::Folia => fill(agent, sources.folia, mc, all),
                InstallKind::Leaves => leaves(agent, sources.leaves, mc, all),
                InstallKind::Purpur => purpur(agent, sources.purpur, mc),
                InstallKind::Paper => fill(agent, sources.paper, mc, all),
                InstallKind::Fabric => fabric(agent, sources.fabric, mc, all),
                InstallKind::Forge => forge(agent, sources.forge, sources.promotions, mc, all),
                InstallKind::NeoForge => neoforge(agent, sources.neoforge, mc, all),
            }
        }
        _ => Err(Error::invalid(
            "version query",
            "provide both Minecraft version and loader kind, or neither",
        )),
    }
}

fn leaves(agent: &ureq::Agent, base: &str, mc: &str, all: bool) -> Result<Vec<Version>> {
    let mut versions: Vec<_> = install::leaves::builds(agent, base, mc)?
        .into_iter()
        .map(|build| Version {
            version: build.build.to_string(),
            stable: build.stable(),
            recommended: false,
        })
        .collect();
    finish(&mut versions, all);
    recommend_latest(&mut versions);
    Ok(versions)
}

fn purpur(agent: &ureq::Agent, base: &str, mc: &str) -> Result<Vec<Version>> {
    let builds = install::purpur::builds(agent, base, mc)?;
    let mut versions: Vec<_> = builds
        .all
        .into_iter()
        .map(|version| Version {
            recommended: version == builds.latest,
            version,
            // Purpur publishes a build catalog without stable/prerelease channels.
            stable: true,
        })
        .collect();
    finish(&mut versions, true);
    Ok(versions)
}

fn fill(agent: &ureq::Agent, base: &str, mc: &str, all: bool) -> Result<Vec<Version>> {
    let mut versions: Vec<_> = install::fill::builds(agent, base, mc)?
        .into_iter()
        .map(|build| Version {
            version: build.id.to_string(),
            stable: build.stable(),
            recommended: false,
        })
        .collect();
    finish(&mut versions, all);
    recommend_latest(&mut versions);
    Ok(versions)
}

#[derive(Deserialize)]
struct GameManifest {
    latest: GameLatest,
    versions: Vec<GameVersion>,
}
#[derive(Deserialize)]
struct GameLatest {
    release: String,
}
#[derive(Deserialize)]
struct GameVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

fn minecraft(agent: &ureq::Agent, url: &str, all: bool) -> Result<Vec<Version>> {
    let manifest: GameManifest = install::metadata(agent, url)?;
    // Mojang's order is chronological, including snapshots with non-numeric names.
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| all || v.kind == "release")
        .map(|v| Version {
            stable: v.kind == "release",
            recommended: v.id == manifest.latest.release,
            version: v.id,
        })
        .collect())
}

#[derive(Deserialize)]
struct FabricRow {
    loader: FabricVersion,
}
#[derive(Deserialize)]
struct FabricVersion {
    version: String,
    #[serde(default)]
    stable: bool,
}

/// Fabric marks only its newest loader as `stable`, so derive release vs prerelease
/// from the version string; `+build.N` suffixes are release metadata, not prereleases.
fn fabric_release(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    !lower.contains("-beta")
        && !lower.contains("-pre")
        && !lower.contains("-snapshot")
        && !lower.contains("-alpha")
        && !lower.contains("-rc")
}

pub(crate) fn fabric(agent: &ureq::Agent, base: &str, mc: &str, all: bool) -> Result<Vec<Version>> {
    let rows: Vec<FabricRow> = install::metadata(agent, &format!("{base}/loader/{mc}"))?;
    let mut versions: Vec<_> = rows
        .into_iter()
        .map(|row| Version {
            stable: row.loader.stable || fabric_release(&row.loader.version),
            version: row.loader.version,
            recommended: false,
        })
        .collect();
    finish(&mut versions, all);
    recommend_latest(&mut versions);
    Ok(versions)
}

#[derive(Deserialize)]
struct MavenMetadata {
    versioning: MavenVersioning,
}
#[derive(Deserialize)]
struct MavenVersioning {
    versions: MavenVersions,
}
#[derive(Deserialize)]
struct MavenVersions {
    #[serde(rename = "version", default)]
    values: Vec<String>,
}

fn maven(agent: &ureq::Agent, url: &str) -> Result<Vec<String>> {
    let bytes = install::response_bytes(agent, url)?;
    let metadata: MavenMetadata =
        quick_xml::de::from_reader(bytes.as_slice()).map_err(|error| Error::VersionMetadata {
            url: url.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(metadata.versioning.versions.values)
}

#[derive(Deserialize)]
struct Promotions {
    promos: HashMap<String, String>,
}

fn forge(
    agent: &ureq::Agent,
    url: &str,
    promotions: &str,
    mc: &str,
    all: bool,
) -> Result<Vec<Version>> {
    let prefix = format!("{mc}-");
    let mut versions: Vec<_> = maven(agent, url)?
        .into_iter()
        .filter_map(|v| v.strip_prefix(&prefix).map(str::to_owned))
        .map(build)
        .collect();
    finish(&mut versions, all);
    if !versions.is_empty() {
        // The full list remains usable if the supplementary promotion service is unavailable.
        if let Ok(promotions) = install::metadata::<Promotions>(agent, promotions)
            && let Some(recommended) = promotions.promos.get(&format!("{mc}-recommended"))
        {
            for version in &mut versions {
                version.recommended = &version.version == recommended;
            }
        }
    }
    Ok(versions)
}

pub(crate) fn neoforge_prefix(mc: &str) -> String {
    if let Some(legacy) = mc.strip_prefix("1.") {
        if legacy.contains('.') {
            format!("{legacy}.")
        } else {
            format!("{legacy}.0.")
        }
    } else if mc.matches('.').count() == 1 {
        format!("{mc}.0.")
    } else {
        format!("{mc}.")
    }
}

pub(crate) fn neoforge(
    agent: &ureq::Agent,
    url: &str,
    mc: &str,
    all: bool,
) -> Result<Vec<Version>> {
    let prefix = neoforge_prefix(mc);
    let mut versions: Vec<_> = maven(agent, url)?
        .into_iter()
        .filter(|v| v.starts_with(&prefix))
        .map(build)
        .collect();
    finish(&mut versions, all);
    recommend_latest(&mut versions);
    Ok(versions)
}

fn build(version: String) -> Version {
    let stable = version
        .bytes()
        .all(|c| c.is_ascii_digit() || b".-".contains(&c));
    Version {
        version,
        stable,
        recommended: false,
    }
}

fn finish(versions: &mut Vec<Version>, all: bool) {
    versions.retain(|v| all || v.stable);
    versions.sort_by(|a, b| {
        version_compare::compare(&b.version, &a.version)
            .ok()
            .and_then(|v| v.ord())
            .unwrap_or_else(|| b.version.cmp(&a.version))
            .then_with(|| b.version.cmp(&a.version))
    });
    versions.dedup_by(|a, b| a.version == b.version);
}

fn recommend_latest(versions: &mut [Version]) {
    if let Some(version) = versions.iter_mut().find(|v| v.stable) {
        version.recommended = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::test_support::{serve, serve_responses};
    use std::time::Duration;

    fn sources(url: &str) -> Sources<'_> {
        Sources {
            minecraft: url,
            fabric: url,
            forge: url,
            promotions: url,
            neoforge: url,
            paper: url,
            folia: url,
            purpur: url,
            leaves: url,
            arclight: url,
            catserver: url,
            mohist: url,
            mohist_projects: url,
        }
    }

    fn test_agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into()
    }

    fn get(request: VersionQuery, bodies: Vec<Vec<u8>>) -> Result<Vec<Version>> {
        let (url, worker) = serve(bodies);
        let result = query_with(&request, &test_agent(), &sources(&url));
        worker.join().unwrap();
        result
    }

    fn supported(kind: InstallKind, all: bool, bodies: Vec<Vec<u8>>) -> Result<Vec<Version>> {
        let (url, worker) = serve(bodies);
        let result = supported_with(kind, all, &test_agent(), &sources(&url), None);
        worker.join().unwrap();
        result
    }

    fn names(versions: &[Version]) -> Vec<&str> {
        versions.iter().map(|v| v.version.as_str()).collect()
    }

    #[test]
    fn kinds_for_skips_cores_without_builds_and_keeps_kind_order() {
        let (url, worker) = serve_responses(vec![(404, Vec::new()); InstallKind::ALL.len()]);
        let supported = kinds_for_with("26.3", &test_agent(), &sources(&url), None).unwrap();
        assert!(supported.is_empty());
        worker.join().unwrap();
    }

    #[test]
    fn supported_minecraft_versions_come_from_each_catalog() {
        let fabric = br#"[{"version":"26.3","stable":true},{"version":"26.2","stable":true},{"version":"26.4-snapshot-1","stable":false}]"#.to_vec();
        let versions = supported(InstallKind::Fabric, false, vec![fabric]).unwrap();
        assert_eq!(names(&versions), ["26.3", "26.2"]);
        assert!(versions[0].recommended);

        let project =
            br#"{"versions":{"26.3":["26.3","26.3-rc-3"],"1.21.11":["1.21.11","1.21.11-pre5"]}}"#
                .to_vec();
        let versions = supported(InstallKind::Paper, false, vec![project.clone()]).unwrap();
        assert_eq!(names(&versions), ["26.3", "1.21.11"]);
        assert_eq!(
            supported(InstallKind::Folia, true, vec![project])
                .unwrap()
                .len(),
            4
        );

        let catalog = br#"{"versions":["26.3","26.1.2"]}"#.to_vec();
        assert_eq!(
            names(&supported(InstallKind::Purpur, false, vec![catalog.clone()]).unwrap()),
            ["26.3", "26.1.2"]
        );
        assert_eq!(
            names(&supported(InstallKind::Leaves, false, vec![catalog]).unwrap()),
            ["26.3", "26.1.2"]
        );

        let forge = br#"<metadata><versioning><versions><version>1.20.1-47.3.0</version><version>1.20.1-47.2.0</version><version>26.3-66.1.0</version><version>1.2.5-3.4.9.171</version></versions></versioning></metadata>"#.to_vec();
        assert_eq!(
            names(&supported(InstallKind::Forge, false, vec![forge]).unwrap()),
            ["26.3", "1.20.1", "1.2.5"]
        );

        let neoforge = br#"<metadata><versioning><versions><version>20.4.237</version><version>21.1.219</version><version>26.3.0.100</version><version>26.1.2.125</version><version>26.4.0.1-beta</version></versions></versioning></metadata>"#.to_vec();
        assert_eq!(
            names(&supported(InstallKind::NeoForge, false, vec![neoforge]).unwrap()),
            ["26.3", "26.1.2", "1.21.1", "1.20.4"]
        );

        let projects = br#"[{"project":"mohist","versions":["1.20.1","1.21.4"]},{"project":"banner","versions":["1.21.4-hack","1.21.4"]}]"#.to_vec();
        assert_eq!(
            names(&supported(InstallKind::Mohist, false, vec![projects.clone()]).unwrap()),
            ["1.21.4", "1.20.1"]
        );
        assert_eq!(
            names(&supported(InstallKind::Banner, false, vec![projects]).unwrap()),
            ["1.21.4"]
        );

        let releases = br#"[
            {"target_commitish":"FeudalKings","draft":false,"prerelease":false,"assets":[{"name":"arclight-forge-1.21.1-1.0.1.jar"},{"name":"arclight-fabric-1.21.1-1.0.1.jar"}]},
            {"target_commitish":"GroupOfSoldiers","draft":false,"prerelease":true,"assets":[{"name":"arclight-forge-1.21.2-1.0.0-SNAPSHOT.jar"}]},
            {"target_commitish":"1.16.5","draft":false,"prerelease":false,"assets":[{"name":"CatServer-1.16.5-abc-server.jar"}]}
        ]"#.to_vec();
        assert_eq!(
            names(&supported(InstallKind::Arclight, false, vec![releases.clone()]).unwrap()),
            ["1.21.1"]
        );
        assert_eq!(
            names(&supported(InstallKind::Arclight, true, vec![releases.clone()]).unwrap()),
            ["1.21.2", "1.21.1"]
        );
        assert_eq!(
            names(&supported(InstallKind::CatServer, false, vec![releases]).unwrap()),
            ["1.16.5"]
        );
    }

    #[test]
    fn hybrid_queries_route_all_four_cores_and_filter_github_prereleases() {
        for kind in [InstallKind::Mohist, InstallKind::Banner] {
            let rows = get(
                VersionQuery::loader("1.20.1", kind),
                vec![br#"[{"id":9,"file_sha256":"abc"},{"id":10,"file_sha256":"abc"}]"#.to_vec()],
            )
            .unwrap();
            assert_eq!(rows[0].version, "10");
            assert!(rows[0].recommended);
        }
        for (kind, mc, name) in [
            (
                InstallKind::Arclight,
                "1.21.1",
                "arclight-forge-1.21.1-1.0.jar",
            ),
            (
                InstallKind::CatServer,
                "1.12.2",
                "CatServer-4168d848-universal.jar",
            ),
        ] {
            let body=serde_json::to_vec(&serde_json::json!([{"tag_name":"tag","target_commitish":mc,"draft":false,"prerelease":true,"assets":[{"name":name,"size":1,"browser_download_url":"https://example.com/official.jar","digest":null}]}])).unwrap();
            let mut query = VersionQuery::loader(mc, kind);
            assert!(get(query.clone(), vec![body.clone()]).unwrap().is_empty());
            query.include_unstable = true;
            let rows = get(query, vec![body]).unwrap();
            assert_eq!(rows[0].version, name);
            assert!(!rows[0].stable && !rows[0].recommended);
        }
    }

    #[test]
    fn github_token_is_forwarded_to_hybrid_queries() {
        let rows = serde_json::json!([{
            "tag_name": "tag",
            "target_commitish": "1.12.2",
            "prerelease": false,
            "draft": false,
            "assets": [{
                "name": "CatServer-abcdef12-universal.jar",
                "size": 1,
                "browser_download_url": "none",
                "digest": null
            }]
        }]);
        let (url, worker) = serve(vec![serde_json::to_vec(&rows).unwrap()]);
        let sources = Sources {
            minecraft: &url,
            fabric: &url,
            forge: &url,
            promotions: &url,
            neoforge: &url,
            paper: &url,
            folia: &url,
            purpur: &url,
            leaves: &url,
            arclight: &url,
            catserver: &url,
            mohist: &url,
            mohist_projects: &url,
        };
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let query = VersionQuery {
            minecraft: Some("1.12.2".into()),
            kind: Some(InstallKind::CatServer),
            github_token: Some("secret".into()),
            ..Default::default()
        };
        let versions = query_with(&query, &agent, &sources).unwrap();
        assert_eq!(versions.len(), 1);
        let requests = worker.join().unwrap();
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer secret")
        );
    }

    #[test]
    fn leaves_filters_experimental_builds_sorts_and_ignores_missing_artifacts() {
        let body = br#"{"code":200,"project_id":"leaves","version":"1.21.8","builds":[{"build":9,"channel":"default","downloads":{"application":{"name":"leaves.jar","sha256":"abc"}}},{"build":11,"channel":"experimental","downloads":{"application":{"name":"leaves.jar","sha256":"abc"}}},{"build":10,"channel":"default","downloads":{"application":{"name":"leaves.jar","sha256":"abc"}}},{"build":12,"channel":"default","downloads":{}}]}"#.to_vec();
        let mut request = VersionQuery::loader("1.21.8", InstallKind::Leaves);
        let rows = get(request.clone(), vec![body.clone()]).unwrap();
        assert_eq!(
            rows.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            ["10", "9"]
        );
        assert!(rows[0].recommended && !rows[1].recommended);
        request.include_unstable = true;
        let rows = get(request, vec![body]).unwrap();
        assert_eq!(rows[0].version, "11");
        assert!(!rows[0].stable && !rows[0].recommended);
        assert!(rows[1].recommended);
    }

    #[test]
    fn purpur_catalog_is_numeric_deduplicated_and_marks_the_official_latest() {
        let body = br#"{"project":"purpur","version":"26.3","builds":{"latest":"9","all":["9","10","9"]}}"#.to_vec();
        for all in [false, true] {
            let mut request = VersionQuery::loader("26.3", InstallKind::Purpur);
            request.include_unstable = all;
            let rows = get(request, vec![body.clone()]).unwrap();
            assert_eq!(
                rows.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
                ["10", "9"]
            );
            assert!(rows.iter().all(|v| v.stable));
            assert!(!rows[0].recommended && rows[1].recommended);
        }
        let request = VersionQuery::loader("26.3", InstallKind::Purpur);
        for body in [
            br#"{"project":"paper","version":"26.3","builds":{"latest":"9","all":["9"]}}"#.to_vec(),
            br#"{"project":"purpur","version":"26.3","builds":{"latest":"../bad","all":[]}}"#
                .to_vec(),
        ] {
            assert!(matches!(
                get(request.clone(), vec![body]),
                Err(Error::VersionMetadata { .. })
            ));
        }
    }

    #[test]
    fn fill_filters_channels_sorts_numeric_builds_and_ignores_missing_artifacts() {
        let body = br#"[{"id":9,"channel":"STABLE","downloads":{"server:default":{"url":"https://example.com/9.jar","size":1,"checksums":{"sha256":"abc"}}}},{"id":11,"channel":"BETA","downloads":{"server:default":{"url":"https://example.com/11.jar","size":1,"checksums":{"sha256":"abc"}}}},{"id":10,"channel":"STABLE","downloads":{"server:default":{"url":"https://example.com/10.jar","size":1,"checksums":{"sha256":"abc"}}}},{"id":12,"channel":"STABLE","downloads":{}}]"#.to_vec();
        for kind in [InstallKind::Paper, InstallKind::Folia] {
            let mut request = VersionQuery::loader("26.3", kind);
            let rows = get(request.clone(), vec![body.clone()]).unwrap();
            assert_eq!(
                rows.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
                ["10", "9"]
            );
            assert!(rows[0].recommended && !rows[1].recommended);
            request.include_unstable = true;
            let rows = get(request, vec![body.clone()]).unwrap();
            assert_eq!(rows[0].version, "11");
            assert!(!rows[0].stable && !rows[0].recommended);
            assert!(rows[1].recommended);
        }
    }

    #[test]
    fn minecraft_filters_releases_and_preserves_official_chronology() {
        let body = br#"{"latest":{"release":"26.3"},"versions":[{"id":"26.4-snapshot-6","type":"snapshot"},{"id":"26.3","type":"release"},{"id":"26.2","type":"release"},{"id":"c0.0.11a","type":"old_alpha"}]}"#.to_vec();
        let versions = get(VersionQuery::default(), vec![body.clone()]).unwrap();
        assert_eq!(
            versions
                .iter()
                .map(|v| v.version.as_str())
                .collect::<Vec<_>>(),
            ["26.3", "26.2"]
        );
        assert!(versions[0].recommended && versions.iter().all(|v| v.stable));
        let versions = get(
            VersionQuery {
                include_unstable: true,
                ..Default::default()
            },
            vec![body],
        )
        .unwrap();
        assert_eq!(versions.len(), 4);
        assert_eq!(versions[0].version, "26.4-snapshot-6");
        assert!(!versions[0].stable);
    }

    #[test]
    fn fabric_uses_compatible_loaders_and_numeric_order_without_duplicates() {
        // Fabric marks only the newest loader stable; older releases must still be listed.
        let body = br#"[{"loader":{"version":"0.9.0","stable":false}},{"loader":{"version":"0.10.0","stable":true}},{"loader":{"version":"0.10.6+build.214","stable":false}},{"loader":{"version":"0.11.0-beta.1","stable":false}},{"loader":{"version":"0.10.0","stable":false}}]"#.to_vec();
        let mut query = VersionQuery::loader("26.3", InstallKind::Fabric);
        let stable = get(query.clone(), vec![body.clone()]).unwrap();
        assert_eq!(
            stable
                .iter()
                .map(|v| v.version.as_str())
                .collect::<Vec<_>>(),
            ["0.10.6+build.214", "0.10.0", "0.9.0"]
        );
        assert!(stable.iter().all(|v| v.stable));
        assert!(stable[0].recommended);
        query.include_unstable = true;
        let all = get(query, vec![body]).unwrap();
        assert_eq!(all.len(), 4);
        assert!(
            all.iter()
                .any(|v| v.version == "0.11.0-beta.1" && !v.stable)
        );
        assert_eq!(all.iter().filter(|v| v.recommended).count(), 1);
    }

    #[test]
    fn forge_returns_all_matching_builds_and_marks_the_official_recommendation() {
        let xml = br#"<metadata><versioning><versions><version>26.3-66.1.9</version><version>26.3-66.1.10</version><version>26.3-66.2.0-beta</version><version>26.2-65.1.5</version></versions></versioning></metadata>"#.to_vec();
        let promos = br#"{"promos":{"26.3-recommended":"66.1.9"}}"#.to_vec();
        let query = VersionQuery::loader("26.3", InstallKind::Forge);
        let versions = get(query.clone(), vec![xml.clone(), promos.clone()]).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, "66.1.10");
        assert!(!versions[0].recommended && versions[1].recommended);
        let versions = get(
            VersionQuery {
                include_unstable: true,
                ..query
            },
            vec![xml, promos],
        )
        .unwrap();
        assert_eq!(versions.len(), 3);
        assert!(versions.iter().any(|v| !v.stable));
    }

    #[test]
    fn neoforge_matches_legacy_and_calendar_versions_and_filters_prereleases() {
        let xml = br#"<metadata><versioning><versions><version>21.4.157</version><version>21.4.158-beta</version><version>26.1.0.10</version><version>26.1.2.125</version><version>26.3.0.9</version><version>26.3.0.100</version></versions></versioning></metadata>"#.to_vec();
        for (mc, expected) in [
            ("1.21.4", "21.4.157"),
            ("26.1", "26.1.0.10"),
            ("26.1.2", "26.1.2.125"),
            ("26.3", "26.3.0.100"),
        ] {
            let result = get(
                VersionQuery::loader(mc, InstallKind::NeoForge),
                vec![xml.clone()],
            )
            .unwrap();
            assert_eq!(result[0].version, expected);
            assert!(result[0].recommended);
        }
        let versions = get(
            VersionQuery {
                include_unstable: true,
                ..VersionQuery::loader("1.21.4", InstallKind::NeoForge)
            },
            vec![xml.clone()],
        )
        .unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|v| !v.stable && !v.recommended));
        assert!(
            get(
                VersionQuery::loader("99.9", InstallKind::NeoForge),
                vec![xml]
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn malformed_metadata_is_an_error_and_invalid_queries_fail_without_network() {
        let query = VersionQuery::loader("26.3", InstallKind::NeoForge);
        assert!(matches!(
            get(query, vec![b"not XML".to_vec()]),
            Err(Error::VersionMetadata { .. })
        ));
        assert!(matches!(
            get(VersionQuery::default(), vec![b"not JSON".to_vec()]),
            Err(Error::InvalidMetadata { .. })
        ));
        for query in [
            VersionQuery {
                minecraft: Some("26.3".into()),
                ..Default::default()
            },
            VersionQuery {
                kind: Some(InstallKind::Fabric),
                ..Default::default()
            },
            VersionQuery::loader("../bad", InstallKind::Forge),
        ] {
            assert!(matches!(
                query_with(&query, &install::agent(), &Sources::default()),
                Err(Error::InvalidInput { .. })
            ));
        }
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Version>();
        send_sync::<VersionQuery>();
    }
}
