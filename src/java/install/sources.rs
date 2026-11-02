use super::{
    Arch, AvailableVersion, Distribution, Os, Plan, PlanFile, Provider, Request, VersionQuery,
};
use crate::{Error, Result};
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::HashMap;

pub(super) struct Sources<'a> {
    pub oracle_page: &'a str,
    pub oracle_archive: &'a str,
    pub openjdk_page: &'a str,
    pub oracle_prefix: &'a str,
    pub openjdk_prefix: &'a str,
    pub adoptium: &'a str,
    pub zulu: &'a str,
    pub semeru: &'a str,
    pub mojang: &'a str,
    pub graalvm: &'a str,
}
impl Default for Sources<'_> {
    fn default() -> Self {
        Self {
            oracle_page: "https://www.oracle.com/java/technologies/downloads/",
            oracle_archive: "https://www.oracle.com/java/technologies/javase/jdk{major}-archive-downloads.html",
            openjdk_page: "https://jdk.java.net/archive/",
            oracle_prefix: "https://download.oracle.com/java/",
            openjdk_prefix: "https://download.java.net/java/GA/",
            adoptium: "https://api.adoptium.net/v3",
            zulu: "https://api.azul.com/metadata/v1",
            semeru: "https://api.github.com/repos/ibmruntimes",
            mojang: "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
            graalvm: "https://api.github.com/repos/graalvm/graalvm-ce-builds",
        }
    }
}

fn links(agent: &ureq::Agent, page: &str) -> Result<Vec<String>> {
    let bytes = crate::install::response_bytes(agent, page)?;
    let html = Html::parse_document(&String::from_utf8_lossy(&bytes));
    let selector = Selector::parse("a[href]").expect("static selector");
    Ok(html
        .select(&selector)
        .filter_map(|a| a.value().attr("href"))
        .map(str::to_owned)
        .collect())
}

fn packages(
    links: Vec<String>,
    distribution: Distribution,
    os: Os,
    arch: Arch,
    prefix: &str,
) -> Vec<(String, String)> {
    let suffix = format!(
        "_{}-{}_bin.{}",
        os.name(),
        arch.name(),
        if os == Os::Windows { "zip" } else { "tar.gz" }
    );
    let old_mac_suffix = suffix.replace("_macos-", "_osx-");
    let stem = match distribution {
        Distribution::Oracle => "jdk-",
        Distribution::OpenJdk => "openjdk-",
        _ => unreachable!("HTML packages are only used by Oracle and jdk.java.net"),
    };
    let mut result: Vec<_> = links
        .into_iter()
        .filter(|url| url.starts_with(prefix))
        .filter_map(|url| {
            let filename = url.rsplit('/').next()?;
            let version = filename
                .strip_prefix(stem)?
                .strip_suffix(&suffix)
                .or_else(|| filename.strip_prefix(stem)?.strip_suffix(&old_mac_suffix))?;
            if version
                .split('.')
                .any(|v| v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()))
            {
                return None;
            }
            Some((version.to_owned(), url))
        })
        .collect();
    result.sort_by(|a, b| {
        version_compare::compare(&b.0, &a.0)
            .ok()
            .and_then(|v| v.ord())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result.dedup();
    result
}

fn candidates(links: Vec<String>, request: &Request, prefix: &str) -> Vec<(String, String)> {
    packages(
        links,
        request.distribution,
        request.os,
        request.arch,
        prefix,
    )
    .into_iter()
    .filter(|(version, _)| {
        if request.version.contains('.') {
            version == &request.version
        } else {
            super::super::major_version(version) == super::super::major_version(&request.version)
        }
    })
    .collect()
}

pub(super) fn versions(
    query: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<AvailableVersion>> {
    match query.distribution {
        Distribution::Mojang => return mojang_versions(query, agent, sources),
        Distribution::Adoptium => return adoptium_versions(query, agent, sources),
        Distribution::Zulu => return zulu_versions(query, agent, sources),
        Distribution::Semeru => return semeru_versions(query, agent, sources),
        Distribution::Graalvm => return graalvm_versions(query, agent, sources),
        Distribution::Oracle | Distribution::OpenJdk => {}
    }
    let (page, prefix) = match query.distribution {
        Distribution::Oracle => (
            match query.major {
                Some(major) => sources
                    .oracle_archive
                    .replace("{major}", &major.to_string()),
                None => sources.oracle_page.to_owned(),
            },
            sources.oracle_prefix,
        ),
        Distribution::OpenJdk => (sources.openjdk_page.to_owned(), sources.openjdk_prefix),
        _ => unreachable!("only HTML providers reach this branch"),
    };
    let page_links = match links(agent, &page) {
        // Oracle has no archive page for unsupported majors. Other failures remain errors.
        Err(Error::Download {
            source: ureq::Error::StatusCode(404),
            ..
        }) if query.distribution == Distribution::Oracle && query.major.is_some() => {
            return Ok(Vec::new());
        }
        result => result?,
    };
    Ok(
        packages(page_links, query.distribution, query.os, query.arch, prefix)
            .into_iter()
            .filter(|(version, _)| {
                query
                    .major
                    .is_none_or(|major| super::super::major_version(version) == Some(major))
            })
            .map(|(version, url)| AvailableVersion {
                distribution: query.distribution,
                version,
                os: query.os,
                arch: query.arch,
                latest: url.contains("/latest/"),
                url,
            })
            .collect(),
    )
}

pub(super) fn resolve(
    request: &Request,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Plan> {
    match request.distribution {
        Distribution::Mojang => return mojang_resolve(request, agent, sources),
        Distribution::Adoptium => return adoptium_resolve(request, agent, sources),
        Distribution::Zulu => return zulu_resolve(request, agent, sources),
        Distribution::Semeru => return semeru_resolve(request, agent, sources),
        Distribution::Graalvm => return graalvm_resolve(request, agent, sources),
        Distribution::Oracle | Distribution::OpenJdk => {}
    }
    let candidates = match request.distribution {
        Distribution::OpenJdk => candidates(
            links(agent, sources.openjdk_page)?,
            request,
            sources.openjdk_prefix,
        ),
        Distribution::Oracle => {
            let current = if request.version.contains('.') {
                Vec::new()
            } else {
                candidates(
                    links(agent, sources.oracle_page)?,
                    request,
                    sources.oracle_prefix,
                )
            };
            // Prefer the vendor's live latest URL when it is publicly linked for this platform.
            if current.iter().any(|(_, url)| url.contains("/latest/")) {
                current
                    .into_iter()
                    .filter(|(_, url)| url.contains("/latest/"))
                    .collect()
            } else {
                let page = sources.oracle_archive.replace(
                    "{major}",
                    &super::super::major_version(&request.version)
                        .unwrap_or(0)
                        .to_string(),
                );
                candidates(links(agent, &page)?, request, sources.oracle_prefix)
            }
        }
        _ => unreachable!("only HTML providers reach this branch"),
    };
    let (version, url) =
        candidates
            .into_iter()
            .next()
            .ok_or_else(|| Error::NoCompatibleVersion {
                component: request.distribution.info().name,
                requested: Some(format!(
                    "{} for {} {}",
                    request.version,
                    request.os.name(),
                    request.arch.name()
                )),
            })?;
    let checksum_url = format!("{url}.sha256");
    let bytes = crate::install::response_bytes(agent, &checksum_url)?;
    let sha256 = String::from_utf8_lossy(&bytes)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::VersionMetadata {
            url: checksum_url,
            reason: "missing or invalid SHA-256".into(),
        });
    }
    Ok(Plan {
        distribution: request.distribution,
        version,
        os: request.os,
        arch: request.arch,
        url,
        sha256,
        files: Vec::new(),
    })
}

fn newest(rows: &mut Vec<AvailableVersion>) {
    rows.sort_by(|a, b| {
        version_compare::compare(&b.version, &a.version)
            .ok()
            .and_then(|v| v.ord())
            .unwrap_or_else(|| b.version.cmp(&a.version))
            .then_with(|| b.version.cmp(&a.version))
    });
    rows.dedup_by(|a, b| a.version == b.version);
    if let Some(first) = rows.first_mut() {
        first.latest = true;
    }
}

fn no_version(request: &Request) -> Error {
    Error::NoCompatibleVersion {
        component: request.distribution.info().name,
        requested: Some(format!(
            "{} for {} {}",
            request.version,
            request.os.name(),
            request.arch.name()
        )),
    }
}

fn missing(error: &Error) -> bool {
    matches!(
        error,
        Error::Download {
            source: ureq::Error::StatusCode(400 | 404),
            ..
        }
    )
}

fn invalid_checksum(url: &str, reason: &str) -> Error {
    Error::VersionMetadata {
        url: url.to_owned(),
        reason: reason.to_owned(),
    }
}

// ---- Eclipse Temurin (Adoptium) ----

#[derive(Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
    version: AdoptiumVersion,
}
#[derive(Deserialize)]
struct AdoptiumRelease {
    binaries: Vec<AdoptiumBinary>,
    #[serde(rename = "version_data")]
    version: AdoptiumVersion,
}
#[derive(Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}
#[derive(Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
}
#[derive(Deserialize)]
struct AdoptiumVersion {
    semver: String,
}

fn adoptium_os(os: Os) -> &'static str {
    match os {
        Os::Macos => "mac",
        Os::Linux => "linux",
        Os::Windows => "windows",
    }
}
fn adoptium_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "x64",
        Arch::Aarch64 => "aarch64",
    }
}
fn adoptium_version(semver: &str) -> String {
    semver.split('+').next().unwrap_or(semver).to_owned()
}
fn adoptium_url(base: &str, path: &str, os: Os, arch: Arch) -> String {
    format!(
        "{base}/assets/{path}?os={}&architecture={}&image_type=jdk&vendor=eclipse",
        adoptium_os(os),
        adoptium_arch(arch)
    )
}

fn adoptium_versions(
    query: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<AvailableVersion>> {
    let mut rows = Vec::new();
    if let Some(major) = query.major {
        let url = format!(
            "{}&page=0&page_size=100",
            adoptium_url(
                sources.adoptium,
                &format!("feature_releases/{major}/ga"),
                query.os,
                query.arch
            )
        );
        let releases: Vec<AdoptiumRelease> = match crate::install::metadata(agent, &url) {
            Ok(releases) => releases,
            // Temurin does not publish every major for every platform (8 on macOS arm64).
            Err(error) if missing(&error) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        for release in releases {
            if let Some(binary) = release.binaries.into_iter().next() {
                rows.push(AvailableVersion {
                    distribution: query.distribution,
                    version: adoptium_version(&release.version.semver),
                    os: query.os,
                    arch: query.arch,
                    latest: false,
                    url: binary.package.link,
                });
            }
        }
    } else {
        #[derive(Deserialize)]
        struct Releases {
            available_releases: Vec<u32>,
        }
        let info: Releases = crate::install::metadata(
            agent,
            &format!("{}/info/available_releases", sources.adoptium),
        )?;
        for major in info.available_releases {
            let url = adoptium_url(
                sources.adoptium,
                &format!("latest/{major}/hotspot"),
                query.os,
                query.arch,
            );
            match crate::install::metadata::<Vec<AdoptiumAsset>>(agent, &url) {
                Ok(assets) => {
                    if let Some(asset) = assets.into_iter().next() {
                        rows.push(AvailableVersion {
                            distribution: query.distribution,
                            version: adoptium_version(&asset.version.semver),
                            os: query.os,
                            arch: query.arch,
                            latest: false,
                            url: asset.binary.package.link,
                        });
                    }
                }
                Err(Error::Download {
                    source: ureq::Error::StatusCode(400 | 404),
                    ..
                }) => continue,
                Err(error) => return Err(error),
            }
        }
    }
    newest(&mut rows);
    Ok(rows)
}

fn adoptium_resolve(request: &Request, agent: &ureq::Agent, sources: &Sources<'_>) -> Result<Plan> {
    let (version, package) = if request.version.contains('.') {
        let major = super::super::major_version(&request.version).unwrap_or(0);
        let url = format!(
            "{}&page=0&page_size=100",
            adoptium_url(
                sources.adoptium,
                &format!("feature_releases/{major}/ga"),
                request.os,
                request.arch
            )
        );
        let releases: Vec<AdoptiumRelease> = match crate::install::metadata(agent, &url) {
            Ok(releases) => releases,
            Err(error) if missing(&error) => return Err(no_version(request)),
            Err(error) => return Err(error),
        };
        releases
            .into_iter()
            .find(|release| adoptium_version(&release.version.semver) == request.version)
            .and_then(|release| {
                release
                    .binaries
                    .into_iter()
                    .next()
                    .map(|binary| (request.version.clone(), binary.package))
            })
            .ok_or_else(|| no_version(request))?
    } else {
        let url = adoptium_url(
            sources.adoptium,
            &format!("latest/{}/hotspot", request.version),
            request.os,
            request.arch,
        );
        let assets: Vec<AdoptiumAsset> = match crate::install::metadata(agent, &url) {
            Ok(assets) => assets,
            Err(error) if missing(&error) => return Err(no_version(request)),
            Err(error) => return Err(error),
        };
        let asset = assets
            .into_iter()
            .next()
            .ok_or_else(|| no_version(request))?;
        (
            adoptium_version(&asset.version.semver),
            asset.binary.package,
        )
    };
    if package.checksum.len() != 64 || !package.checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid_checksum(
            &package.link,
            "missing or invalid SHA-256",
        ));
    }
    Ok(Plan {
        distribution: request.distribution,
        version,
        os: request.os,
        arch: request.arch,
        url: package.link,
        sha256: package.checksum,
        files: Vec::new(),
    })
}

// ---- Azul Zulu ----

#[derive(Deserialize)]
struct ZuluPackage {
    package_uuid: String,
    name: String,
    java_version: Vec<u32>,
    distro_version: Vec<u32>,
    download_url: String,
}
#[derive(Deserialize)]
struct ZuluDetail {
    sha256_hash: String,
    download_url: String,
}

fn zulu_os(os: Os) -> &'static str {
    match os {
        Os::Macos => "macos",
        Os::Linux => "linux",
        Os::Windows => "windows",
    }
}
fn zulu_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "x64",
        Arch::Aarch64 => "aarch64",
    }
}
fn zulu_archive(os: Os) -> &'static str {
    if os == Os::Windows { "zip" } else { "tar.gz" }
}
fn zulu_version(package: &ZuluPackage) -> String {
    package
        .java_version
        .iter()
        .take(3)
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".")
}
fn zulu_url(base: &str, query: &VersionQuery, version: Option<&str>, latest: bool) -> String {
    let mut url = format!(
        "{base}/zulu/packages/?os={}&arch={}&archive_type={}&java_package_type=jdk&release_status=ga&page_size=100",
        zulu_os(query.os),
        zulu_arch(query.arch),
        zulu_archive(query.os)
    );
    if let Some(version) = version {
        url.push_str("&java_version=");
        url.push_str(version);
    }
    if latest {
        url.push_str("&latest=true");
    }
    url
}

fn zulu_versions(
    query: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<AvailableVersion>> {
    let major = query.major.map(|major| major.to_string());
    // Without a major the API returns the latest package for every major.
    let url = zulu_url(sources.zulu, query, major.as_deref(), query.major.is_none());
    let mut packages: Vec<ZuluPackage> = match crate::install::metadata(agent, &url) {
        Ok(packages) => packages,
        Err(error) if missing(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    // The API also returns CRaC and JavaFX variants; keep the standard JDK.
    packages.retain(|package| !package.name.contains("-crac-") && !package.name.contains("-fx-"));
    packages.sort_by(|a, b| b.distro_version.cmp(&a.distro_version));
    let mut rows: Vec<AvailableVersion> = packages
        .into_iter()
        .map(|package| AvailableVersion {
            distribution: query.distribution,
            version: zulu_version(&package),
            os: query.os,
            arch: query.arch,
            latest: false,
            url: package.download_url,
        })
        .collect();
    newest(&mut rows);
    Ok(rows)
}

fn zulu_resolve(request: &Request, agent: &ureq::Agent, sources: &Sources<'_>) -> Result<Plan> {
    let url = zulu_url(
        sources.zulu,
        &VersionQuery {
            distribution: request.distribution,
            major: super::super::major_version(&request.version),
            os: request.os,
            arch: request.arch,
            proxy: request.proxy.clone(),
        },
        Some(&request.version),
        true,
    );
    let mut packages: Vec<ZuluPackage> = match crate::install::metadata(agent, &url) {
        Ok(packages) => packages,
        Err(error) if missing(&error) => return Err(no_version(request)),
        Err(error) => return Err(error),
    };
    packages.sort_by(|a, b| b.distro_version.cmp(&a.distro_version));
    let package = packages
        .into_iter()
        .next()
        .ok_or_else(|| no_version(request))?;
    let detail_url = format!("{}/zulu/packages/{}", sources.zulu, package.package_uuid);
    let detail: ZuluDetail = match crate::install::metadata(agent, &detail_url) {
        Ok(detail) => detail,
        Err(error) if missing(&error) => return Err(no_version(request)),
        Err(error) => return Err(error),
    };
    if detail.sha256_hash.len() != 64 || !detail.sha256_hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid_checksum(&detail_url, "missing or invalid SHA-256"));
    }
    Ok(Plan {
        distribution: request.distribution,
        version: zulu_version(&package),
        os: request.os,
        arch: request.arch,
        url: detail.download_url,
        sha256: detail.sha256_hash,
        files: Vec::new(),
    })
}

// ---- IBM Semeru Open ----

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
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
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn semeru_os(os: Os) -> &'static str {
    match os {
        Os::Macos => "mac",
        Os::Linux => "linux",
        Os::Windows => "windows",
    }
}
fn semeru_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "x64",
        Arch::Aarch64 => "aarch64",
    }
}
fn semeru_version(release: &GithubRelease) -> Option<&str> {
    release.tag_name.strip_prefix("jdk-")
}
fn semeru_asset(release: &GithubRelease, os: Os, arch: Arch) -> Option<&GithubAsset> {
    let version = semeru_version(release)?;
    let extension = if os == Os::Windows { "zip" } else { "tar.gz" };
    let name = format!(
        "ibm-semeru-open-jdk_{}_{}_{}.{}",
        semeru_arch(arch),
        semeru_os(os),
        version,
        extension
    );
    release.assets.iter().find(|asset| asset.name == name)
}
fn semeru_releases_url(base: &str, major: u32) -> String {
    format!("{base}/semeru{major}-binaries/releases?per_page=100&page=1")
}
fn semeru_skip(release: &GithubRelease) -> bool {
    release.draft || release.prerelease || release.tag_name.contains("-m")
}
fn semeru_digest(asset: &GithubAsset) -> Option<String> {
    asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_owned)
}

fn semeru_versions(
    query: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<AvailableVersion>> {
    let majors: Vec<u32> = match query.major {
        Some(major) => vec![major],
        // Published Semeru lines. Missing repositories are treated as no builds.
        None => vec![8, 11, 17, 21, 25],
    };
    let mut rows = Vec::new();
    for major in majors {
        let url = semeru_releases_url(sources.semeru, major);
        let releases: Vec<GithubRelease> = match crate::install::metadata(agent, &url) {
            Ok(releases) => releases,
            Err(error) if missing(&error) => continue,
            Err(error) => return Err(error),
        };
        for release in releases.into_iter().filter(|release| !semeru_skip(release)) {
            let Some(version) = semeru_version(&release) else {
                continue;
            };
            let Some(asset) = semeru_asset(&release, query.os, query.arch) else {
                continue;
            };
            rows.push(AvailableVersion {
                distribution: query.distribution,
                version: version.to_owned(),
                os: query.os,
                arch: query.arch,
                latest: false,
                url: asset.browser_download_url.clone(),
            });
        }
    }
    newest(&mut rows);
    Ok(rows)
}

fn semeru_resolve(request: &Request, agent: &ureq::Agent, sources: &Sources<'_>) -> Result<Plan> {
    let major = super::super::major_version(&request.version).unwrap_or(0);
    let url = semeru_releases_url(sources.semeru, major);
    let releases: Vec<GithubRelease> = match crate::install::metadata(agent, &url) {
        Ok(releases) => releases,
        Err(error) if missing(&error) => return Err(no_version(request)),
        Err(error) => return Err(error),
    };
    let release = releases
        .into_iter()
        .filter(|release| !semeru_skip(release))
        .find(|release| {
            if request.version.contains('.') {
                semeru_version(release) == Some(request.version.as_str())
            } else {
                semeru_asset(release, request.os, request.arch).is_some()
            }
        })
        .ok_or_else(|| no_version(request))?;
    let asset =
        semeru_asset(&release, request.os, request.arch).ok_or_else(|| no_version(request))?;
    let Some(version) = semeru_version(&release) else {
        return Err(no_version(request));
    };
    let Some(sha256) = semeru_digest(asset) else {
        return Err(invalid_checksum(
            &asset.browser_download_url,
            "missing SHA-256 digest",
        ));
    };
    Ok(Plan {
        distribution: request.distribution,
        version: version.to_owned(),
        os: request.os,
        arch: request.arch,
        url: asset.browser_download_url.clone(),
        sha256,
        files: Vec::new(),
    })
}

// ---- GraalVM Community ----

fn graalvm_platform(os: Os, arch: Arch) -> Option<(&'static str, &'static str)> {
    match (os, arch) {
        (Os::Macos, Arch::X64) => Some(("macos-x64", "tar.gz")),
        (Os::Macos, Arch::Aarch64) => Some(("macos-aarch64", "tar.gz")),
        (Os::Linux, Arch::X64) => Some(("linux-x64", "tar.gz")),
        (Os::Linux, Arch::Aarch64) => Some(("linux-aarch64", "tar.gz")),
        (Os::Windows, Arch::X64) => Some(("windows-x64", "zip")),
        (Os::Windows, Arch::Aarch64) => None,
    }
}

fn graalvm_asset(release: &GithubRelease, os: Os, arch: Arch) -> Option<(&GithubAsset, String)> {
    let (platform, extension) = graalvm_platform(os, arch)?;
    let suffix = format!("_{platform}_bin.{extension}");
    for asset in &release.assets {
        let Some(rest) = asset.name.strip_prefix("graalvm-community-jdk-") else {
            continue;
        };
        let Some(stem) = rest.strip_suffix(&suffix) else {
            continue;
        };
        // Newer assets add a release segment: 25i4-25.0.4.1.1.
        let version = stem.rsplit('-').next()?.to_owned();
        if !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return Some((asset, version));
        }
    }
    None
}

fn graalvm_releases(agent: &ureq::Agent, sources: &Sources<'_>) -> Result<Vec<GithubRelease>> {
    let mut releases = Vec::new();
    for page in 1..=3 {
        let url = format!("{}/releases?per_page=100&page={page}", sources.graalvm);
        let batch: Vec<GithubRelease> = match crate::install::metadata(agent, &url) {
            Ok(batch) => batch,
            Err(error) if missing(&error) && page == 1 => return Ok(Vec::new()),
            Err(error) if missing(&error) => break,
            Err(error) => return Err(error),
        };
        let short = batch.len() < 100;
        releases.extend(batch);
        if short {
            break;
        }
    }
    Ok(releases)
}

fn graalvm_versions(
    query: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<AvailableVersion>> {
    let releases = graalvm_releases(agent, sources)?;
    let mut rows = Vec::new();
    for release in releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
    {
        let Some((asset, version)) = graalvm_asset(&release, query.os, query.arch) else {
            continue;
        };
        if !query
            .major
            .is_none_or(|major| super::super::major_version(&version) == Some(major))
        {
            continue;
        }
        rows.push(AvailableVersion {
            distribution: query.distribution,
            version,
            os: query.os,
            arch: query.arch,
            latest: false,
            url: asset.browser_download_url.clone(),
        });
    }
    newest(&mut rows);
    Ok(rows)
}

fn graalvm_resolve(request: &Request, agent: &ureq::Agent, sources: &Sources<'_>) -> Result<Plan> {
    let releases = graalvm_releases(agent, sources)?;
    let best = releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| graalvm_asset(release, request.os, request.arch))
        .filter(|(_, version)| {
            if request.version.contains('.') {
                version == &request.version
            } else {
                super::super::major_version(version)
                    == super::super::major_version(&request.version)
            }
        })
        .max_by(|a, b| {
            version_compare::compare(&a.1, &b.1)
                .ok()
                .and_then(|v| v.ord())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| no_version(request))?;
    let (asset, version) = best;
    let Some(sha256) = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .map(str::to_owned)
    else {
        return Err(invalid_checksum(
            &asset.browser_download_url,
            "missing SHA-256 digest",
        ));
    };
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid_checksum(
            &asset.browser_download_url,
            "invalid SHA-256 digest",
        ));
    }
    Ok(Plan {
        distribution: request.distribution,
        version,
        os: request.os,
        arch: request.arch,
        url: asset.browser_download_url.clone(),
        sha256,
        files: Vec::new(),
    })
}

// ---- Mojang Java runtime ----

#[derive(Deserialize)]
struct MojangRuntime {
    manifest: MojangManifest,
    version: MojangVersion,
}
#[derive(Deserialize)]
struct MojangManifest {
    url: String,
    sha1: String,
}
#[derive(Deserialize)]
struct MojangVersion {
    name: String,
}
#[derive(Deserialize)]
struct MojangFileSet {
    files: HashMap<String, MojangEntry>,
}
#[derive(Deserialize)]
struct MojangEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    executable: bool,
    #[serde(default)]
    downloads: Option<MojangDownloads>,
}
#[derive(Deserialize)]
struct MojangDownloads {
    raw: MojangDownload,
}
#[derive(Deserialize)]
struct MojangDownload {
    url: String,
    sha1: String,
    size: u64,
}

fn mojang_platform(os: Os, arch: Arch) -> Option<&'static str> {
    match (os, arch) {
        (Os::Macos, Arch::X64) => Some("mac-os"),
        (Os::Macos, Arch::Aarch64) => Some("mac-os-arm64"),
        (Os::Linux, Arch::X64) => Some("linux"),
        (Os::Windows, Arch::X64) => Some("windows-x64"),
        (Os::Windows, Arch::Aarch64) => Some("windows-arm64"),
        (Os::Linux, Arch::Aarch64) => None,
    }
}

fn mojang_entries(
    all: &HashMap<String, HashMap<String, Vec<MojangRuntime>>>,
    os: Os,
    arch: Arch,
) -> Option<Vec<(&str, &MojangRuntime)>> {
    let components = all.get(mojang_platform(os, arch)?)?;
    let mut entries = Vec::new();
    for (component, runtimes) in components {
        for runtime in runtimes {
            entries.push((component.as_str(), runtime));
        }
    }
    Some(entries)
}

fn mojang_numeric(version: &str) -> bool {
    !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

fn mojang_versions(
    query: &VersionQuery,
    agent: &ureq::Agent,
    sources: &Sources<'_>,
) -> Result<Vec<AvailableVersion>> {
    let all: HashMap<String, HashMap<String, Vec<MojangRuntime>>> =
        crate::install::metadata(agent, sources.mojang)?;
    let Some(entries) = mojang_entries(&all, query.os, query.arch) else {
        return Ok(Vec::new());
    };
    let mut rows: Vec<AvailableVersion> = entries
        .into_iter()
        .filter(|(_, runtime)| mojang_numeric(&runtime.version.name))
        .filter(|(_, runtime)| {
            query.major.is_none_or(|major| {
                super::super::major_version(&runtime.version.name) == Some(major)
            })
        })
        .map(|(_, runtime)| AvailableVersion {
            distribution: query.distribution,
            version: runtime.version.name.clone(),
            os: query.os,
            arch: query.arch,
            latest: false,
            url: runtime.manifest.url.clone(),
        })
        .collect();
    newest(&mut rows);
    Ok(rows)
}

fn verify_sha1(bytes: &[u8], expected: &str) -> bool {
    use sha1::{Digest, Sha1};
    let mut hash = Sha1::new();
    hash.update(bytes);
    format!("{:x}", hash.finalize()).eq_ignore_ascii_case(expected)
}

fn mojang_resolve(request: &Request, agent: &ureq::Agent, sources: &Sources<'_>) -> Result<Plan> {
    let all: HashMap<String, HashMap<String, Vec<MojangRuntime>>> =
        crate::install::metadata(agent, sources.mojang)?;
    let entries =
        mojang_entries(&all, request.os, request.arch).ok_or_else(|| no_version(request))?;
    let runtime = entries
        .into_iter()
        .filter(|(_, runtime)| mojang_numeric(&runtime.version.name))
        .filter(|(_, runtime)| {
            if request.version.contains('.') {
                runtime.version.name == request.version
            } else {
                super::super::major_version(&runtime.version.name)
                    == super::super::major_version(&request.version)
            }
        })
        .map(|(_, runtime)| runtime)
        .max_by(|a, b| {
            version_compare::compare(&a.version.name, &b.version.name)
                .ok()
                .and_then(|v| v.ord())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| no_version(request))?;
    let bytes = crate::install::response_bytes(agent, &runtime.manifest.url)?;
    if !verify_sha1(&bytes, &runtime.manifest.sha1) {
        return Err(Error::ChecksumMismatch {
            url: runtime.manifest.url.clone(),
            expected: runtime.manifest.sha1.clone(),
            actual: "manifest SHA-1 mismatch".into(),
        });
    }
    let manifest: MojangFileSet =
        serde_json::from_slice(&bytes).map_err(|error| Error::VersionMetadata {
            url: runtime.manifest.url.clone(),
            reason: error.to_string(),
        })?;
    let mut files: Vec<PlanFile> = manifest
        .files
        .into_iter()
        .filter(|(_, entry)| entry.kind == "file")
        .filter_map(|(path, entry)| {
            let download = entry.downloads?.raw;
            Some(PlanFile {
                path,
                url: download.url,
                sha1: download.sha1,
                size: download.size,
                executable: entry.executable,
            })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if files.is_empty() {
        return Err(invalid_checksum(
            &runtime.manifest.url,
            "manifest lists no files",
        ));
    }
    Ok(Plan {
        distribution: request.distribution,
        version: runtime.version.name.clone(),
        os: request.os,
        arch: request.arch,
        url: runtime.manifest.url.clone(),
        sha256: runtime.manifest.sha1.clone(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::java::install::Arch;
    fn test_agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .into()
    }

    #[test]
    fn missing_platform_packages_are_empty_not_errors() {
        use crate::install::test_support::serve_responses;
        let (page, worker) = serve_responses(vec![(404, Vec::new())]);
        let sources = Sources {
            adoptium: &page,
            ..Sources::default()
        };
        let query = VersionQuery {
            distribution: Distribution::Adoptium,
            major: Some(8),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        assert!(
            versions(&query, &test_agent(), &sources)
                .unwrap()
                .is_empty()
        );
        worker.join().unwrap();

        let (page, worker) = serve_responses(vec![(404, Vec::new())]);
        let sources = Sources {
            adoptium: &page,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Adoptium,
            version: "8".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        assert!(matches!(
            resolve(&request, &test_agent(), &sources),
            Err(Error::NoCompatibleVersion { .. })
        ));
        worker.join().unwrap();
    }

    #[test]
    fn graalvm_selects_community_assets_and_digests() {
        use crate::install::test_support::serve;
        let checksum = "d".repeat(64);
        let releases = format!(
            r#"[{{"tag_name":"graal-25.4.4.1.1","draft":false,"prerelease":false,"assets":[{{"name":"graalvm-community-jdk-25i4-25.0.4.1.1_macos-aarch64_bin.tar.gz","browser_download_url":"https://github.example/g25.tar.gz","digest":"sha256:{checksum}"}},{{"name":"graalvm-community-jdk-25i4-25.0.4.1.1_linux-x64_bin.tar.gz","browser_download_url":"https://github.example/g25linux.tar.gz","digest":"sha256:{checksum}"}}]}},{{"tag_name":"jdk-21.0.2","draft":false,"prerelease":false,"assets":[{{"name":"graalvm-community-jdk-21.0.2_macos-aarch64_bin.tar.gz","browser_download_url":"https://github.example/g21.tar.gz","digest":"sha256:{checksum}"}}]}}]"#
        );
        let (page, worker) = serve(vec![releases.clone().into_bytes()]);
        let sources = Sources {
            graalvm: &page,
            ..Sources::default()
        };
        let query = VersionQuery {
            distribution: Distribution::Graalvm,
            major: Some(25),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let rows = versions(&query, &test_agent(), &sources).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.version.as_str())
                .collect::<Vec<_>>(),
            ["25.0.4.1.1"]
        );
        worker.join().unwrap();

        let (page, worker) = serve(vec![releases.into_bytes()]);
        let sources = Sources {
            graalvm: &page,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Graalvm,
            version: "21.0.2".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let plan = resolve(&request, &test_agent(), &sources).unwrap();
        assert_eq!(plan.version, "21.0.2");
        assert_eq!(plan.url, "https://github.example/g21.tar.gz");
        assert_eq!(plan.sha256, checksum);
        worker.join().unwrap();
    }

    #[test]
    fn adoptium_lists_and_resolves_checksummed_tarballs() {
        use crate::install::test_support::serve;
        let checksum = "a".repeat(64);
        let list = format!(
            r#"[{{"binaries":[{{"package":{{"link":"https://cdn.example/adoptium.tar.gz","checksum":"{checksum}"}}}}],"version_data":{{"semver":"21.0.13+9.0.LTS"}}}}]"#
        );
        let (page, worker) = serve(vec![list.into_bytes()]);
        let sources = Sources {
            adoptium: &page,
            ..Sources::default()
        };
        let query = VersionQuery {
            distribution: Distribution::Adoptium,
            major: Some(21),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let rows = versions(&query, &test_agent(), &sources).unwrap();
        assert_eq!(rows[0].version, "21.0.13");
        assert!(rows[0].latest);
        assert_eq!(rows[0].url, "https://cdn.example/adoptium.tar.gz");
        worker.join().unwrap();

        let latest = format!(
            r#"[{{"binary":{{"package":{{"link":"https://cdn.example/adoptium.tar.gz","checksum":"{checksum}"}}}},"version":{{"semver":"21.0.13+9.0.LTS"}}}}]"#
        );
        let (page, worker) = serve(vec![latest.into_bytes()]);
        let sources = Sources {
            adoptium: &page,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Adoptium,
            version: "21".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let plan = resolve(&request, &test_agent(), &sources).unwrap();
        assert_eq!(plan.version, "21.0.13");
        assert_eq!(plan.sha256, checksum);
        assert!(plan.files.is_empty());
        worker.join().unwrap();
    }

    #[test]
    fn zulu_lists_patches_and_fetches_the_package_checksum() {
        use crate::install::test_support::serve;
        let checksum = "b".repeat(64);
        let list = br#"[{"package_uuid":"pkg","name":"zulu21.54.109-ca-jdk21.0.13.0.3-macosx_aarch64.tar.gz","java_version":[21,0,13,0,3],"distro_version":[21,54,109,0],"download_url":"https://cdn.example/zulu.tar.gz"},{"package_uuid":"crac","name":"zulu21.52.203-ca-crac-jdk21.0.12.1-macosx_aarch64.tar.gz","java_version":[21,0,12,1,3],"distro_version":[21,52,203,0],"download_url":"https://cdn.example/crac.tar.gz"}]"#.to_vec();
        let (page, worker) = serve(vec![list]);
        let sources = Sources {
            zulu: &page,
            ..Sources::default()
        };
        let query = VersionQuery {
            distribution: Distribution::Zulu,
            major: Some(21),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let rows = versions(&query, &test_agent(), &sources).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.version.as_str())
                .collect::<Vec<_>>(),
            ["21.0.13"]
        );
        worker.join().unwrap();

        let detail = format!(
            r#"{{"sha256_hash":"{checksum}","download_url":"https://cdn.example/zulu.tar.gz"}}"#
        );
        let (page, worker) = serve(vec![br#"[{"package_uuid":"pkg","name":"zulu21.54.109-ca-jdk21.0.13.0.3-macosx_aarch64.tar.gz","java_version":[21,0,13,0,3],"distro_version":[21,54,109,0],"download_url":"https://cdn.example/zulu.tar.gz"}]"#.to_vec(), detail.into_bytes()]);
        let sources = Sources {
            zulu: &page,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Zulu,
            version: "21".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let plan = resolve(&request, &test_agent(), &sources).unwrap();
        assert_eq!(plan.version, "21.0.13");
        assert_eq!(plan.sha256, checksum);
        assert!(worker.join().unwrap()[1].starts_with("GET /zulu/packages/pkg "));
    }

    #[test]
    fn semeru_reads_platform_assets_and_github_digests() {
        use crate::install::test_support::serve;
        let checksum = "c".repeat(64);
        let releases = format!(
            r#"[{{"tag_name":"jdk-21.0.13.0","draft":false,"prerelease":false,"assets":[{{"name":"ibm-semeru-open-jdk_aarch64_mac_21.0.13.0.tar.gz","browser_download_url":"https://github.example/semeru.tar.gz","digest":"sha256:{checksum}"}},{{"name":"ibm-semeru-open-jdk_x64_mac_21.0.13.0.tar.gz","browser_download_url":"https://github.example/x64.tar.gz","digest":"sha256:{checksum}"}}]}},{{"tag_name":"jdk-21.0.13.0-m2","draft":false,"prerelease":false,"assets":[]}}]"#
        );
        let (page, worker) = serve(vec![releases.into_bytes()]);
        let sources = Sources {
            semeru: &page,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Semeru,
            version: "21".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let plan = resolve(&request, &test_agent(), &sources).unwrap();
        assert_eq!(plan.version, "21.0.13.0");
        assert_eq!(plan.sha256, checksum);
        assert_eq!(plan.url, "https://github.example/semeru.tar.gz");
        assert!(worker.join().unwrap()[0].contains("semeru21-binaries"));
    }

    #[test]
    fn mojang_lists_runtimes_and_expands_the_file_manifest() {
        use crate::install::test_support::serve_responses_on;
        use sha1::{Digest, Sha1};
        use std::net::TcpListener;
        let manifest = br#"{"files":{"release":{"type":"file","downloads":{"raw":{"url":"https://piston.example/release","sha1":"aaaa","size":1}}},"bin/java":{"type":"file","executable":true,"downloads":{"raw":{"url":"https://piston.example/java","sha1":"bbbb","size":2}}}}}"#;
        let mut hash = Sha1::new();
        hash.update(manifest);
        let manifest_sha1 = format!("{:x}", hash.finalize());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let page = format!("http://{}", listener.local_addr().unwrap());
        let all = format!(
            r#"{{"linux":{{"java-runtime-epsilon":[{{"manifest":{{"url":"{page}/manifest.json","sha1":"{manifest_sha1}"}},"version":{{"name":"25.0.1"}}}}]}}}}"#
        );
        let (_, worker) = serve_responses_on(
            listener,
            vec![(200, all.into_bytes()), (200, manifest.to_vec())],
        );
        let mojang = format!("{page}/all.json");
        let sources = Sources {
            mojang: &mojang,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Mojang,
            version: "25".into(),
            name: "test".into(),
            os: Os::Linux,
            arch: Arch::X64,
            proxy: Default::default(),
        };
        let plan = resolve(&request, &test_agent(), &sources).unwrap();
        assert_eq!(plan.version, "25.0.1");
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.files[0].path, "bin/java");
        assert!(plan.files[0].executable);
        assert_eq!(plan.sha256, manifest_sha1);
        worker.join().unwrap();
    }

    #[test]
    fn version_queries_share_package_filtering_and_sort_newest_first() {
        use crate::install::test_support::serve;
        let root = "https://download.java.net/java/GA/";
        let names = [
            "openjdk-21.0.1_macos-aarch64_bin.tar.gz",
            "openjdk-21.0.2_macos-aarch64_bin.tar.gz",
            "openjdk-25_macos-aarch64_bin.tar.gz",
            "openjdk-25_linux-aarch64_bin.tar.gz",
            "openjdk-21.0.2_macos-aarch64_bin.tar.gz",
        ];
        let html = names
            .iter()
            .map(|name| format!("<a href='{root}{name}'>jdk</a>"))
            .collect::<String>();
        let (page, worker) = serve(vec![html.as_bytes().to_vec(), html.into_bytes()]);
        let sources = Sources {
            openjdk_page: &page,
            ..Sources::default()
        };
        let agent = ureq::Agent::config_builder().proxy(None).build().into();
        let mut query = VersionQuery {
            distribution: Distribution::OpenJdk,
            major: None,
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let all = versions(&query, &agent, &sources).unwrap();
        assert_eq!(
            all.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            ["25", "21.0.2", "21.0.1"]
        );
        assert!(all.iter().all(|v| !v.latest && v.os == Os::Macos));
        query.major = Some(21);
        let filtered = versions(&query, &agent, &sources).unwrap();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].version, "21.0.2");
        worker.join().unwrap();
    }

    #[test]
    fn oracle_queries_current_or_major_archive_without_checksum_requests() {
        use crate::install::test_support::serve;
        let root = "https://download.oracle.com/java/";
        let (page, worker) = serve(vec![
            format!("<a href='{root}25/latest/jdk-25_macos-aarch64_bin.tar.gz'>latest</a>")
                .into_bytes(),
            format!("<a href='{root}21/archive/jdk-21.0.2_macos-aarch64_bin.tar.gz'>archive</a>")
                .into_bytes(),
        ]);
        let archive = format!("{page}/jdk{{major}}");
        let sources = Sources {
            oracle_page: &page,
            oracle_archive: &archive,
            ..Sources::default()
        };
        let agent = ureq::Agent::config_builder().proxy(None).build().into();
        let mut query = VersionQuery {
            distribution: Distribution::Oracle,
            major: None,
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let current = versions(&query, &agent, &sources).unwrap();
        assert_eq!(current[0].version, "25");
        assert!(current[0].latest);
        query.major = Some(21);
        let archive = versions(&query, &agent, &sources).unwrap();
        assert_eq!(archive[0].version, "21.0.2");
        assert!(!archive[0].latest);
        let paths = worker.join().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[1].starts_with("GET /jdk21 "));
    }

    #[test]
    fn oracle_uses_public_archive_if_latest_requires_login() {
        use crate::install::test_support::serve;
        use std::time::Duration;
        let (download, download_worker) = serve(vec![b"a".repeat(64)]);
        let url = format!("{download}/jdk-21.0.12.1_macos-x64_bin.tar.gz");
        let (page, page_worker) = serve(vec![
            b"<a href='https://login.example/jdk-21_macos-x64_bin.tar.gz'>login</a>".to_vec(),
            format!("<a href='{url}'>public archive</a>").into_bytes(),
        ]);
        let sources = Sources {
            oracle_page: &page,
            oracle_archive: &format!("{page}/jdk{{major}}"),
            oracle_prefix: &download,
            ..Sources::default()
        };
        let request = Request {
            distribution: Distribution::Oracle,
            version: "21".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::X64,
            proxy: Default::default(),
        };
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let plan = resolve(&request, &agent, &sources).unwrap();
        assert_eq!(plan.version, "21.0.12.1");
        assert_eq!(plan.url, url);
        let paths = page_worker.join().unwrap();
        assert!(paths[1].starts_with("GET /jdk21 "));
        download_worker.join().unwrap();
    }
    #[test]
    fn public_links_are_filtered_by_distribution_version_os_and_arch() {
        let request = Request {
            distribution: Distribution::OpenJdk,
            version: "25".into(),
            name: "test".into(),
            os: Os::Macos,
            arch: Arch::Aarch64,
            proxy: Default::default(),
        };
        let root = "https://download.java.net/java/GA/";
        let links = [
            "jdk25/openjdk-25_macos-aarch64_bin.tar.gz",
            "jdk25/openjdk-25.0.2_macos-aarch64_bin.tar.gz",
            "jdk25/openjdk-25.0.2_macos-x64_bin.tar.gz",
            "jdk25/openjdk-25.0.2_linux-aarch64_bin.tar.gz",
            "jdk26/openjdk-26_macos-aarch64_bin.tar.gz",
            "jdk25/openjdk-25_macos-aarch64_bin.tar.gz.sha256",
        ]
        .map(|name| format!("{root}{name}"));
        let mut links = links.to_vec();
        links.push("https://unofficial.example/openjdk-25.0.9_macos-aarch64_bin.tar.gz".into());
        let rows = candidates(links, &request, root);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "25.0.2");
        let request = Request {
            version: "11.0.2".into(),
            arch: Arch::X64,
            ..request
        };
        assert_eq!(
            candidates(
                vec![format!("{root}jdk11/openjdk-11.0.2_osx-x64_bin.tar.gz")],
                &request,
                root
            )
            .len(),
            1
        );
    }
}
