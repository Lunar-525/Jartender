//! Official hybrid-server catalogs. Downloads prepare launchers; first run installs libraries.
use super::{
    InstallEvent, InstallKind, InstallOutcome, InstallRequest, jar, metadata, notify, prepare,
    validate_version,
};
use crate::{Error, Result};
use serde::Deserialize;
use std::{collections::HashSet, ops::ControlFlow, path::Path};

pub(crate) const ARCLIGHT: &str = "https://api.github.com/repos/IzzelAliz/Arclight/releases";
pub(crate) const CATSERVER: &str = "https://api.github.com/repos/Luohuayu/CatServer/releases";
pub(crate) const MOHIST: &str = "https://api.mohistmc.com/project";

pub(crate) fn source(kind: InstallKind) -> &'static str {
    match kind {
        InstallKind::Arclight => ARCLIGHT,
        InstallKind::CatServer => CATSERVER,
        _ => MOHIST,
    }
}

pub(crate) struct Build {
    pub version: String,
    pub stable: bool,
    pub recommended: bool,
    group: String,
    url: String,
    size: Option<u64>,
    checksum: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    target_commitish: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
    digest: Option<String>,
}
#[derive(Deserialize)]
struct MohistBuild {
    id: u64,
    file_sha256: String,
}

pub(crate) fn catalog(
    agent: &ureq::Agent,
    base: &str,
    kind: InstallKind,
    mc: &str,
    token: Option<&str>,
    latest_only: bool,
) -> Result<Vec<Build>> {
    validate_version(mc)?;
    let mut builds = match kind {
        InstallKind::Mohist | InstallKind::Banner => {
            let project = if kind == InstallKind::Mohist {
                "mohist"
            } else {
                "banner"
            };
            let url = format!("{base}/{project}/{mc}/builds");
            let rows: Vec<MohistBuild> = metadata(agent, &url)?;
            let mut builds: Vec<_> = rows
                .into_iter()
                .map(|row| Build {
                    version: row.id.to_string(),
                    stable: true,
                    recommended: false,
                    group: row.id.to_string(),
                    url: format!("{url}/{}/download", row.id),
                    size: None,
                    checksum: Some(row.file_sha256),
                })
                .collect();
            builds
                .sort_by_key(|build| std::cmp::Reverse(build.version.parse::<u64>().unwrap_or(0)));
            builds
        }
        InstallKind::Arclight | InstallKind::CatServer => {
            github(agent, base, kind, mc, token, latest_only)?
        }
        _ => return Err(Error::invalid("hybrid core", "unsupported core")),
    };
    let mut seen = HashSet::new();
    builds.retain(|build| seen.insert(build.version.clone()));
    let latest = builds.iter().find(|b| b.stable).map(|b| b.group.clone());
    for build in &mut builds {
        build.recommended = build.stable && Some(&build.group) == latest.as_ref();
    }
    Ok(builds)
}

fn github(
    agent: &ureq::Agent,
    base: &str,
    kind: InstallKind,
    mc: &str,
    token: Option<&str>,
    latest_only: bool,
) -> Result<Vec<Build>> {
    let mut builds = Vec::new();
    // GitHub returns releases newest first; follow every page, including older MC versions.
    for page in 1..=100 {
        let url = format!("{base}?per_page=100&page={page}");
        let releases: Vec<Release> = github_json(agent, &url, token)?;
        let last_page = releases.len() < 100;
        for release in releases.into_iter().filter(|release| !release.draft) {
            for asset in release.assets {
                let name = asset.name.to_ascii_lowercase();
                let compatible = match kind {
                    InstallKind::Arclight => ["forge", "neoforge", "fabric"]
                        .iter()
                        .any(|platform| name.starts_with(&format!("arclight-{platform}-{mc}-"))),
                    _ => {
                        name.starts_with(&format!("catserver-{mc}-"))
                            || (mc == release.target_commitish
                                && name
                                    .strip_prefix("catserver-")
                                    .and_then(|name| name.strip_suffix("-universal.jar"))
                                    .is_some_and(|hash| {
                                        hash.len() >= 7
                                            && hash.bytes().all(|b| b.is_ascii_hexdigit())
                                    }))
                    }
                };
                if !compatible
                    || !name.ends_with(".jar")
                    || name.ends_with("-sources.jar")
                    || name.ends_with("-javadoc.jar")
                {
                    continue;
                }
                validate_version(&asset.name).map_err(|e| Error::VersionMetadata {
                    url: url.clone(),
                    reason: e.to_string(),
                })?;
                let checksum = asset
                    .digest
                    .map(|digest| {
                        digest
                            .strip_prefix("sha256:")
                            .map(str::to_owned)
                            .ok_or_else(|| Error::VersionMetadata {
                                url: url.clone(),
                                reason: "unsupported GitHub asset digest".into(),
                            })
                    })
                    .transpose()?;
                builds.push(Build {
                    version: asset.name,
                    stable: !release.prerelease,
                    recommended: false,
                    group: release.tag_name.clone(),
                    url: asset.browser_download_url,
                    size: Some(asset.size),
                    checksum,
                });
            }
        }
        // Installs only need the newest matching release; queries keep walking for older builds.
        if last_page || (latest_only && !builds.is_empty()) {
            return Ok(builds);
        }
    }
    Err(Error::VersionMetadata {
        url: base.into(),
        reason: "GitHub release catalog exceeds 100 pages".into(),
    })
}

/// GitHub metadata with an optional token; anonymous requests are limited to 60 per hour.
pub(crate) fn github_json<T: serde::de::DeserializeOwned>(
    agent: &ureq::Agent,
    url: &str,
    token: Option<&str>,
) -> Result<T> {
    let mut request = agent.get(url);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let mut response = request.call().map_err(|source| Error::Download {
        url: url.to_owned(),
        source,
    })?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|source| Error::Download {
            url: url.to_owned(),
            source,
        })?;
    serde_json::from_slice(&bytes).map_err(|source| Error::InvalidMetadata {
        url: url.to_owned(),
        source,
    })
}

pub(super) fn install(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    base: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    if matches!(request.kind, InstallKind::Mohist | InstallKind::Banner)
        && let Some(id) = &request.version
        && (id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(Error::invalid("build", "expected an integer build number"));
    }
    notify(
        progress,
        InstallEvent::Resolving {
            minecraft: request.minecraft.clone(),
        },
    )?;
    // Only `latest` may stop early; an explicit filename can live in an older release.
    let builds = catalog(
        agent,
        base,
        request.kind,
        &request.minecraft,
        request.github_token.as_deref(),
        request.version.is_none(),
    )?;
    let candidates: Vec<_> = builds
        .iter()
        .filter(|build| match &request.version {
            Some(version) => &build.version == version,
            None => build.recommended,
        })
        .collect();
    if candidates.len() > 1 {
        return Err(Error::invalid(
            "Arclight platform",
            format!(
                "multiple platform JARs available; choose a filename from `versions {} arclight` and pass it as VERSION",
                request.minecraft
            ),
        ));
    }
    // Arclight/CatServer name assets; Mohist/Banner use build numbers.
    let requested = match (&request.version, request.kind) {
        (Some(version), InstallKind::Arclight | InstallKind::CatServer) => {
            Some(format!("asset {version}"))
        }
        (Some(version), _) => Some(version.clone()),
        (None, _) => Some(format!("for Minecraft {}", request.minecraft)),
    };
    let build = candidates
        .first()
        .ok_or_else(|| Error::NoCompatibleVersion {
            component: request.kind.name(),
            requested,
        })?;
    jar::install(
        root,
        &target,
        request,
        agent,
        &jar::Artifact {
            version: &build.version,
            url: &build.url,
            size: build.size,
            checksum: match &build.checksum {
                Some(value) => jar::Checksum::Sha256(value),
                None => jar::Checksum::Unpublished,
            },
        },
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::test_support::serve;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Cursor, Write},
        time::Duration,
    };
    use tempfile::TempDir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into()
    }
    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }
    fn jar(kind: InstallKind, mc: &str) -> Vec<u8> {
        let main = match kind {
            InstallKind::Arclight => "io.izzel.arclight.server.Launcher",
            InstallKind::Mohist => "com.mohistmc.MohistMCStart",
            InstallKind::Banner => "com.mohistmc.banner.BannerLauncher",
            _ => "catserver.server.CatServerLaunch",
        };
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        write!(zip, "Main-Class: {main}\nGit-Branch: {mc}\n\n").unwrap();
        zip.finish().unwrap().into_inner()
    }
    fn asset(name: &str, url: &str, jar: &[u8], checksum: bool) -> Value {
        json!({"name":name,"size":jar.len(),"browser_download_url":url,
            "digest":if checksum { Some(format!("sha256:{:x}",Sha256::digest(jar))) } else { None }})
    }
    fn release(tag: &str, mc: &str, pre: bool, assets: Vec<Value>) -> Value {
        json!({"tag_name":tag,"target_commitish":mc,"prerelease":pre,"draft":false,"assets":assets})
    }

    #[test]
    fn mohist_and_banner_select_numeric_builds_verify_and_publish_launchers() {
        for kind in [InstallKind::Mohist, InstallKind::Banner] {
            let root = TempDir::new().unwrap();
            let data = jar(kind, "1.20.1");
            let rows = json!([{"id":9,"file_sha256":format!("{:x}",Sha256::digest(&data))},{"id":10,"file_sha256":format!("{:x}",Sha256::digest(&data))}]);
            let (base, worker) = serve(vec![bytes(&rows), data.clone()]);
            let result = install(
                root.path(),
                &InstallRequest::new(kind, "demo", "1.20.1"),
                &agent(),
                &base,
                &mut |_| ControlFlow::Continue(()),
            )
            .unwrap();
            assert_eq!(result.kind, kind);
            assert_eq!(result.loader.as_deref(), Some("10"));
            assert_eq!(fs::read(result.jar.unwrap()).unwrap(), data);
            assert_eq!(
                fs::read_to_string(result.directory.join("eula.txt")).unwrap(),
                "eula=false\n"
            );
            let servers = crate::discover(root.path()).unwrap();
            assert_eq!(servers[0].server_type, kind.name().to_ascii_lowercase());
            assert_eq!(servers[0].version.as_deref(), Some("1.20.1"));
            let requests = worker.join().unwrap();
            assert!(requests[1].starts_with(&format!(
                "GET /{}/1.20.1/builds/10/download ",
                kind.name().to_ascii_lowercase()
            )));
        }
    }

    #[test]
    fn github_filters_mc_drafts_and_prereleases_without_choosing_a_platform() {
        let data = jar(InstallKind::Arclight, "1.21.1");
        let rows = json!([
            release("beta","1.21.1",true,vec![asset("arclight-forge-1.21.1-beta.jar","none",&data,true)]),
            release("1.0.1","1.21.1",false,vec![asset("arclight-fabric-1.21.1-1.0.1.jar","none",&data,true),asset("arclight-forge-1.21.1-1.0.1.jar","none",&data,true),asset("arclight-forge-1.21.10-1.0.1.jar","none",&data,true)]),
            {"tag_name":"draft","target_commitish":"1.21.1","prerelease":false,"draft":true,"assets":[asset("arclight-forge-1.21.1-draft.jar","none",&data,true)]}
        ]);
        let (base, worker) = serve(vec![bytes(&rows)]);
        let builds = catalog(
            &agent(),
            &base,
            InstallKind::Arclight,
            "1.21.1",
            None,
            false,
        )
        .unwrap();
        assert_eq!(builds.len(), 3);
        assert!(!builds[0].stable);
        assert_eq!(builds.iter().filter(|b| b.recommended).count(), 2);
        worker.join().unwrap();
        let root = TempDir::new().unwrap();
        let (base, worker) = serve(vec![bytes(&rows)]);
        let error = install(
            root.path(),
            &InstallRequest::arclight("demo", "1.21.1"),
            &agent(),
            &base,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            Error::InvalidInput {
                field: "Arclight platform",
                ..
            }
        ));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        worker.join().unwrap();
    }

    #[test]
    fn github_explicit_arclight_and_legacy_catserver_install_with_honest_checksum_events() {
        for (kind, mc, name, checksum) in [
            (
                InstallKind::Arclight,
                "1.21.1",
                "arclight-forge-1.21.1-1.0.1.jar",
                true,
            ),
            (
                InstallKind::CatServer,
                "1.12.2",
                "CatServer-4168d848-universal.jar",
                false,
            ),
        ] {
            let root = TempDir::new().unwrap();
            let data = jar(kind, mc);
            let (download_url, download_worker) = serve(vec![data]);
            let rows = json!([release(
                "tag",
                mc,
                false,
                vec![asset(name, &download_url, &jar(kind, mc), checksum)]
            )]);
            let (base, worker) = serve(vec![bytes(&rows)]);
            let mut request = InstallRequest::new(kind, "demo", mc);
            if kind == InstallKind::Arclight {
                request.version = Some(name.into());
            }
            let mut events = Vec::new();
            let outcome = install(root.path(), &request, &agent(), &base, &mut |event| {
                events.push(event);
                ControlFlow::Continue(())
            })
            .unwrap();
            assert_eq!(outcome.loader.as_deref(), Some(name));
            assert_eq!(
                events
                    .iter()
                    .any(|event| matches!(event, InstallEvent::ChecksumUnavailable { .. })),
                !checksum
            );
            assert_eq!(
                crate::discover(root.path()).unwrap()[0].server_type,
                kind.name().to_ascii_lowercase()
            );
            worker.join().unwrap();
            download_worker.join().unwrap();
        }
    }

    #[test]
    fn github_pagination_and_catserver_branch_matching_cover_older_releases() {
        let data = jar(InstallKind::CatServer, "1.12.2");
        let rows = json!([release(
            "old",
            "1.12.2",
            false,
            vec![
                asset("CatServer-abcdef12-universal.jar", "none", &data, false),
                asset(
                    "CatServer-1.16.5-abcdef12-universal.jar",
                    "none",
                    &data,
                    false
                )
            ]
        )]);
        let empty = release("other", "master", false, vec![]);
        let (base, worker) = serve(vec![bytes(&json!(vec![empty; 100])), bytes(&rows)]);
        let builds = catalog(
            &agent(),
            &base,
            InstallKind::CatServer,
            "1.12.2",
            None,
            false,
        )
        .unwrap();
        assert_eq!(builds.len(), 1);
        let requests = worker.join().unwrap();
        assert!(requests[1].contains("page=2"));
    }

    #[test]
    fn github_token_is_sent_and_install_stops_at_the_first_matching_page() {
        let data = jar(InstallKind::CatServer, "1.12.2");
        let (download_url, download_worker) = serve(vec![data.clone()]);
        let empty = release("other", "1.16.5", false, vec![]);
        let page1 = json!(vec![empty.clone(); 100]);
        let mut page2 = vec![release(
            "target",
            "1.12.2",
            false,
            vec![asset(
                "CatServer-abcdef34-universal.jar",
                &download_url,
                &data,
                false,
            )],
        )];
        page2.extend(std::iter::repeat_n(empty, 99));
        // Two full pages are served; a third request would fail against the mock.
        let (base, worker) = serve(vec![bytes(&page1), bytes(&json!(page2))]);
        let root = TempDir::new().unwrap();
        let request = InstallRequest::new(InstallKind::CatServer, "demo", "1.12.2")
            .with_github_token("secret");
        let outcome = install(root.path(), &request, &agent(), &base, &mut |_| {
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(
            outcome.loader.as_deref(),
            Some("CatServer-abcdef34-universal.jar")
        );
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer secret")
        );
        assert!(requests[1].contains("page=2"));
        download_worker.join().unwrap();

        let (download_url, download_worker) = serve(vec![data.clone()]);
        let page = json!([release(
            "target",
            "1.12.2",
            false,
            vec![asset(
                "CatServer-abcdef34-universal.jar",
                &download_url,
                &data,
                false
            )]
        )]);
        let (base, worker) = serve(vec![bytes(&page)]);
        let root = TempDir::new().unwrap();
        install(
            root.path(),
            &InstallRequest::new(InstallKind::CatServer, "demo", "1.12.2"),
            &agent(),
            &base,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
        download_worker.join().unwrap();
    }

    #[test]
    fn explicit_github_filenames_keep_paging_past_newer_releases() {
        let data = jar(InstallKind::CatServer, "1.12.2");
        let (download_url, download_worker) = serve(vec![data.clone()]);
        let empty = release("other", "1.16.5", false, vec![]);
        let mut page1 = vec![release(
            "newer",
            "1.12.2",
            false,
            vec![asset(
                "CatServer-99999999-universal.jar",
                "none",
                &data,
                false,
            )],
        )];
        page1.extend(std::iter::repeat_n(empty, 99));
        let page2 = json!([release(
            "older",
            "1.12.2",
            false,
            vec![asset(
                "CatServer-abcdef34-universal.jar",
                &download_url,
                &data,
                false
            )]
        )]);
        let (base, worker) = serve(vec![bytes(&json!(page1)), bytes(&page2)]);
        let root = TempDir::new().unwrap();
        let mut request = InstallRequest::new(InstallKind::CatServer, "demo", "1.12.2");
        request.version = Some("CatServer-abcdef34-universal.jar".into());
        let outcome = install(root.path(), &request, &agent(), &base, &mut |_| {
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(
            outcome.loader.as_deref(),
            Some("CatServer-abcdef34-universal.jar")
        );
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contains("page=2"));
        download_worker.join().unwrap();
    }

    #[test]
    fn wrong_core_wrong_mc_checksum_mismatch_and_cancellation_clean_up() {
        for scenario in ["core", "mc", "checksum", "cancel"] {
            let root = TempDir::new().unwrap();
            let data = jar(
                if scenario == "core" {
                    InstallKind::Banner
                } else {
                    InstallKind::Mohist
                },
                if scenario == "mc" { "1.20.2" } else { "1.20.1" },
            );
            let checksum = if scenario == "checksum" {
                "0".repeat(64)
            } else {
                format!("{:x}", Sha256::digest(&data))
            };
            let (base, worker) = serve(vec![
                bytes(&json!([{"id":10,"file_sha256":checksum}])),
                data,
            ]);
            let error = install(
                root.path(),
                &InstallRequest::mohist("demo", "1.20.1"),
                &agent(),
                &base,
                &mut |event| {
                    if scenario == "cancel" && matches!(event, InstallEvent::Progress { .. }) {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                },
            )
            .unwrap_err();
            match scenario {
                "checksum" => assert!(matches!(error, Error::ChecksumMismatch { .. })),
                "cancel" => assert!(matches!(error, Error::Cancelled)),
                _ => assert!(matches!(error, Error::InvalidJar { .. })),
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
            worker.join().unwrap();
        }
    }

    #[test]
    fn invalid_metadata_missing_builds_and_initial_cancellation_never_download() {
        for scenario in ["missing", "digest", "metadata"] {
            let root = TempDir::new().unwrap();
            let mut row = asset(
                "arclight-forge-1.21.1-1.0.jar",
                "none",
                &jar(InstallKind::Arclight, "1.21.1"),
                true,
            );
            if scenario == "digest" {
                row["digest"] = json!("sha1:abcd");
            }
            let body = if scenario == "metadata" {
                b"not json".to_vec()
            } else if scenario == "missing" {
                bytes(&json!([]))
            } else {
                bytes(&json!([release("tag", "1.21.1", false, vec![row])]))
            };
            let (base, worker) = serve(vec![body]);
            let error = install(
                root.path(),
                &InstallRequest::arclight("demo", "1.21.1"),
                &agent(),
                &base,
                &mut |_| ControlFlow::Continue(()),
            )
            .unwrap_err();
            match scenario {
                "missing" => assert!(matches!(error, Error::NoCompatibleVersion { .. })),
                "digest" => assert!(matches!(error, Error::VersionMetadata { .. })),
                _ => assert!(matches!(error, Error::InvalidMetadata { .. })),
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
            worker.join().unwrap();
        }
        let root = TempDir::new().unwrap();
        assert!(matches!(
            install(
                root.path(),
                &InstallRequest::catserver("demo", "1.16.5"),
                &agent(),
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Break(())
            ),
            Err(Error::Cancelled)
        ));
    }
}
