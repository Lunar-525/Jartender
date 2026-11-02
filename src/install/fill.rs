//! PaperMC Fill v3 build metadata and verified Paper/Folia downloads.
use super::{InstallEvent, InstallOutcome, InstallRequest, metadata, notify, prepare};
use crate::{Error, Result};
use serde::Deserialize;
use std::{collections::HashMap, ops::ControlFlow, path::Path};

pub(crate) const PAPER: &str = "https://fill.papermc.io/v3/projects/paper";
pub(crate) const FOLIA: &str = "https://fill.papermc.io/v3/projects/folia";

#[derive(Deserialize)]
pub(crate) struct Build {
    pub id: u64,
    channel: String,
    downloads: HashMap<String, Download>,
}

impl Build {
    pub fn stable(&self) -> bool {
        matches!(self.channel.as_str(), "STABLE" | "RECOMMENDED")
    }
}

#[derive(Deserialize)]
struct Download {
    url: String,
    size: u64,
    checksums: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256: String,
}

pub(crate) fn builds(agent: &ureq::Agent, base: &str, mc: &str) -> Result<Vec<Build>> {
    let mut builds: Vec<Build> = metadata(agent, &format!("{base}/versions/{mc}/builds"))?;
    builds.retain(|build| build.downloads.contains_key("server:default"));
    builds.sort_by_key(|build| std::cmp::Reverse(build.id));
    builds.dedup_by_key(|build| build.id);
    Ok(builds)
}

pub(super) fn install(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    base: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    let component = match request.kind {
        super::InstallKind::Folia => "Folia build",
        _ => "Paper build",
    };
    let requested = request
        .version
        .as_deref()
        .map(|value| {
            value.parse::<u64>().map_err(|_| {
                Error::invalid(component, "expected a non-negative integer build number")
            })
        })
        .transpose()?;
    notify(
        progress,
        InstallEvent::Resolving {
            minecraft: request.minecraft.clone(),
        },
    )?;
    let build = builds(agent, base, &request.minecraft)?
        .into_iter()
        .find(|build| requested.map_or_else(|| build.stable(), |id| build.id == id))
        .ok_or_else(|| Error::NoCompatibleVersion {
            component,
            requested: request.version.clone(),
        })?;
    let artifact = &build.downloads["server:default"];
    let version = build.id.to_string();
    super::jar::install(
        root,
        &target,
        request,
        agent,
        &super::jar::Artifact {
            version: &version,
            url: &artifact.url,
            size: Some(artifact.size),
            checksum: super::jar::Checksum::Sha256(&artifact.checksums.sha256),
        },
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::InstallKind;
    use crate::install::test_support::serve;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::{
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

    fn jar() -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"Main-Class: io.papermc.paperclip.Main\n")
            .unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn row(id: u64, channel: &str, url: &str, bytes: &[u8]) -> serde_json::Value {
        json!({"id":id, "channel":channel, "downloads":{"server:default":{
            "url":url, "size":bytes.len(), "checksums":{"sha256":format!("{:x}", Sha256::digest(bytes))}
        }}})
    }

    #[test]
    fn selects_latest_stable_or_explicit_build_and_publishes_without_java() {
        for kind in [InstallKind::Paper, InstallKind::Folia] {
            for requested in [None, Some("11".to_owned())] {
                let root = TempDir::new().unwrap();
                let bytes = jar();
                let (url, download_worker) = serve(vec![bytes.clone()]);
                let rows = vec![
                    row(2, "STABLE", &url, &bytes),
                    row(11, "BETA", &url, &bytes),
                    row(10, "STABLE", &url, &bytes),
                ];
                let (base, worker) = serve(vec![serde_json::to_vec(&rows).unwrap()]);
                let mut request = InstallRequest::new(kind, "demo", "26.3");
                request.version = requested.clone();
                let mut events = Vec::new();
                let result = install(root.path(), &request, &agent(), &base, &mut |event| {
                    events.push(event);
                    ControlFlow::Continue(())
                })
                .unwrap();
                assert_eq!(result.kind, kind);
                assert_eq!(
                    result.loader.as_deref(),
                    Some(requested.as_deref().unwrap_or("10"))
                );
                assert_eq!(fs::read(result.jar.unwrap()).unwrap(), bytes);
                assert_eq!(
                    fs::read_to_string(result.directory.join("eula.txt")).unwrap(),
                    "eula=false\n"
                );
                assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
                assert!(events.iter().any(|e| matches!(e, InstallEvent::Progress { downloaded, .. } if *downloaded == bytes.len() as u64)));
                assert!(worker.join().unwrap()[0].starts_with("GET /versions/26.3/builds "));
                download_worker.join().unwrap();
            }
        }
    }

    #[test]
    fn folia_without_stable_builds_does_not_download_or_fall_back() {
        let root = TempDir::new().unwrap();
        let rows = vec![row(7, "BETA", "http://127.0.0.1:1/not-downloaded", &jar())];
        let (base, worker) = serve(vec![serde_json::to_vec(&rows).unwrap()]);
        let error = install(
            root.path(),
            &InstallRequest::folia("demo", "26.2"),
            &agent(),
            &base,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            Error::NoCompatibleVersion {
                component: "Folia build",
                requested: None
            }
        ));
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        assert!(worker.join().unwrap()[0].starts_with("GET /versions/26.2/builds "));
    }

    #[test]
    fn rejects_corrupt_downloads_and_cleans_staging_on_failure_or_cancellation() {
        for scenario in ["checksum", "size", "jar", "cancel"] {
            let root = TempDir::new().unwrap();
            let bytes = if scenario == "jar" {
                b"not a jar".to_vec()
            } else {
                jar()
            };
            let (url, download_worker) = serve(vec![bytes.clone()]);
            let mut build = row(10, "STABLE", &url, &bytes);
            match scenario {
                "checksum" => {
                    build["downloads"]["server:default"]["checksums"]["sha256"] =
                        json!("0".repeat(64))
                }
                "size" => build["downloads"]["server:default"]["size"] = json!(bytes.len() + 1),
                _ => (),
            }
            let (base, worker) = serve(vec![serde_json::to_vec(&vec![build]).unwrap()]);
            let error = install(
                root.path(),
                &InstallRequest::paper("demo", "26.3"),
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
            download_worker.join().unwrap();
        }
    }

    #[test]
    fn missing_stable_or_requested_build_and_invalid_checksum_never_download() {
        for scenario in ["beta", "missing", "checksum"] {
            let root = TempDir::new().unwrap();
            let mut build = row(
                10,
                if scenario == "beta" { "BETA" } else { "STABLE" },
                "http://127.0.0.1:1",
                &jar(),
            );
            if scenario == "checksum" {
                build["downloads"]["server:default"]["checksums"]["sha256"] = json!("invalid");
            }
            let (base, worker) = serve(vec![serde_json::to_vec(&vec![build]).unwrap()]);
            let mut request = InstallRequest::paper("demo", "26.3");
            if scenario == "missing" {
                request.version = Some("99".into());
            }
            let error = install(root.path(), &request, &agent(), &base, &mut |_| {
                ControlFlow::Continue(())
            })
            .unwrap_err();
            if scenario == "checksum" {
                assert!(matches!(error, Error::VersionMetadata { .. }));
            } else {
                assert!(matches!(error, Error::NoCompatibleVersion { .. }));
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
            worker.join().unwrap();
        }
    }

    #[test]
    fn invalid_build_existing_target_and_initial_cancellation_do_not_contact_network() {
        let root = TempDir::new().unwrap();
        let mut request = InstallRequest::paper("demo", "26.3");
        request.version = Some("abc".into());
        assert!(matches!(
            install(
                root.path(),
                &request,
                &agent(),
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Continue(())
            ),
            Err(Error::InvalidInput { .. })
        ));
        request.version = None;
        assert!(matches!(
            install(
                root.path(),
                &request,
                &agent(),
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Break(())
            ),
            Err(Error::Cancelled)
        ));
        fs::create_dir(root.path().join("demo")).unwrap();
        fs::write(root.path().join("demo/keep"), "untouched").unwrap();
        assert!(matches!(
            install(
                root.path(),
                &request,
                &agent(),
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Continue(())
            ),
            Err(Error::Io { .. })
        ));
        assert_eq!(
            fs::read_to_string(root.path().join("demo/keep")).unwrap(),
            "untouched"
        );
    }
}
