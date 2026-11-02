//! Purpur's official v2 build catalog and server download metadata.
use super::{InstallEvent, InstallOutcome, InstallRequest, jar, metadata, notify, prepare};
use crate::{Error, Result};
use serde::Deserialize;
use std::{ops::ControlFlow, path::Path};

pub(crate) const API: &str = "https://api.purpurmc.org/v2/purpur";

#[derive(Deserialize)]
struct Version {
    project: String,
    version: String,
    builds: Builds,
}

#[derive(Deserialize)]
pub(crate) struct Builds {
    pub latest: String,
    pub all: Vec<String>,
}

#[derive(Deserialize)]
struct Build {
    project: String,
    version: String,
    build: String,
    result: String,
    md5: String,
}

fn validate_build(value: &str) -> Result<()> {
    if value.is_empty()
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.parse::<u64>().is_err()
    {
        return Err(Error::invalid(
            "Purpur build",
            "expected an integer build number",
        ));
    }
    Ok(())
}

pub(crate) fn builds(agent: &ureq::Agent, base: &str, mc: &str) -> Result<Builds> {
    let url = format!("{base}/{mc}");
    let version: Version = metadata(agent, &url)?;
    if version.project != "purpur" || version.version != mc {
        return Err(Error::VersionMetadata {
            url,
            reason: "Purpur project or Minecraft version does not match the request".into(),
        });
    }
    for id in version
        .builds
        .all
        .iter()
        .chain(std::iter::once(&version.builds.latest))
    {
        validate_build(id).map_err(|e| Error::VersionMetadata {
            url: url.clone(),
            reason: e.to_string(),
        })?;
    }
    Ok(version.builds)
}

pub(super) fn install(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    base: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    if let Some(value) = &request.version {
        validate_build(value)?;
    }
    notify(
        progress,
        InstallEvent::Resolving {
            minecraft: request.minecraft.clone(),
        },
    )?;
    let catalog = builds(agent, base, &request.minecraft)?;
    let version = request.version.as_deref().unwrap_or(&catalog.latest);
    let unavailable = || Error::NoCompatibleVersion {
        component: "Purpur build",
        requested: Some(version.to_owned()),
    };
    if !catalog.all.iter().any(|id| id == version) {
        return Err(unavailable());
    }
    let url = format!("{base}/{}/{version}", request.minecraft);
    let build: Build = metadata(agent, &url)?;
    if build.project != "purpur" || build.version != request.minecraft || build.build != version {
        return Err(Error::VersionMetadata {
            url,
            reason: "Purpur build metadata does not match the request".into(),
        });
    }
    if build.result != "SUCCESS" {
        return Err(unavailable());
    }
    jar::install(
        root,
        &target,
        request,
        agent,
        &jar::Artifact {
            version,
            url: &format!("{url}/download"),
            size: None,
            checksum: jar::Checksum::Md5(&build.md5),
        },
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::{InstallKind, test_support::serve};
    use md5::{Digest, Md5};
    use serde_json::json;
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

    fn jar() -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"Main-Class: io.papermc.paperclip.Main\n")
            .unwrap();
        zip.start_file("META-INF/versions.list", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hash\t26.3\t26.3/purpur-26.3.jar\n")
            .unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn catalog() -> Vec<u8> {
        serde_json::to_vec(
            &json!({"project":"purpur","version":"26.3","builds":{"latest":"10","all":["9","10"]}}),
        )
        .unwrap()
    }

    fn build(id: &str, bytes: &[u8]) -> serde_json::Value {
        json!({"project":"purpur","version":"26.3","build":id,"result":"SUCCESS","md5":format!("{:x}",Md5::digest(bytes))})
    }

    #[test]
    fn latest_and_explicit_builds_download_pinned_artifacts_and_are_discoverable() {
        for requested in [None, Some("9".to_owned())] {
            let root = TempDir::new().unwrap();
            let bytes = jar();
            let id = requested.as_deref().unwrap_or("10");
            let (base, worker) = serve(vec![
                catalog(),
                serde_json::to_vec(&build(id, &bytes)).unwrap(),
                bytes.clone(),
            ]);
            let mut request = InstallRequest::purpur("demo", "26.3");
            request.version = requested.clone();
            let mut events = Vec::new();
            let outcome = install(root.path(), &request, &agent(), &base, &mut |event| {
                events.push(event);
                ControlFlow::Continue(())
            })
            .unwrap();
            assert_eq!(outcome.kind, InstallKind::Purpur);
            assert_eq!(outcome.loader.as_deref(), Some(id));
            assert_eq!(fs::read(outcome.jar.unwrap()).unwrap(), bytes);
            assert_eq!(
                fs::read_to_string(outcome.directory.join("eula.txt")).unwrap(),
                "eula=false\n"
            );
            assert_eq!(
                crate::discover(root.path()).unwrap()[0].server_type,
                "purpur"
            );
            assert!(events.iter().any(|e| matches!(e, InstallEvent::DownloadingServer { component, version } if component == "Purpur" && version == id)));
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
            let paths = worker.join().unwrap();
            assert!(paths[0].starts_with("GET /26.3 "));
            assert!(paths[1].starts_with(&format!("GET /26.3/{id} ")));
            assert!(paths[2].starts_with(&format!("GET /26.3/{id}/download ")));
        }
    }

    #[test]
    fn tampered_download_and_cancellation_leave_no_server_or_staging() {
        for cancel in [false, true] {
            let root = TempDir::new().unwrap();
            let bytes = jar();
            let mut detail = build("10", &bytes);
            if !cancel {
                detail["md5"] = json!("0".repeat(32));
            }
            let (base, worker) =
                serve(vec![catalog(), serde_json::to_vec(&detail).unwrap(), bytes]);
            let error = install(
                root.path(),
                &InstallRequest::purpur("demo", "26.3"),
                &agent(),
                &base,
                &mut |event| {
                    if cancel && matches!(event, InstallEvent::Progress { .. }) {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                },
            )
            .unwrap_err();
            if cancel {
                assert!(matches!(error, Error::Cancelled));
            } else {
                assert!(matches!(error, Error::ChecksumMismatch { .. }));
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
            worker.join().unwrap();
        }
    }

    #[test]
    fn failed_mismatched_missing_or_invalid_builds_never_download() {
        for scenario in ["failed", "identity", "checksum", "missing"] {
            let root = TempDir::new().unwrap();
            let mut detail = build("10", &jar());
            match scenario {
                "failed" => detail["result"] = json!("FAILURE"),
                "identity" => detail["version"] = json!("1.21.4"),
                "checksum" => detail["md5"] = json!("invalid"),
                _ => (),
            }
            let mut bodies = vec![catalog()];
            if scenario != "missing" {
                bodies.push(serde_json::to_vec(&detail).unwrap());
            }
            let (base, worker) = serve(bodies);
            let mut request = InstallRequest::purpur("demo", "26.3");
            if scenario == "missing" {
                request.version = Some("99".into());
            }
            let error = install(root.path(), &request, &agent(), &base, &mut |_| {
                ControlFlow::Continue(())
            })
            .unwrap_err();
            if scenario == "failed" || scenario == "missing" {
                assert!(matches!(error, Error::NoCompatibleVersion { .. }));
            } else {
                assert!(matches!(error, Error::VersionMetadata { .. }));
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
            worker.join().unwrap();
        }
        let root = TempDir::new().unwrap();
        let mut request = InstallRequest::purpur("demo", "26.3");
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
    }
}
