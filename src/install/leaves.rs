//! Official Leaves v2 build catalog and SHA-256 verified downloads.
use super::{
    InstallEvent, InstallOutcome, InstallRequest, jar, metadata, notify, prepare, validate_version,
};
use crate::{Error, Result};
use serde::Deserialize;
use std::{collections::HashMap, ops::ControlFlow, path::Path};

pub(crate) const API: &str = "https://api.leavesmc.org/v2/projects/leaves";

#[derive(Deserialize)]
struct Catalog {
    code: u16,
    project_id: String,
    version: String,
    builds: Vec<Build>,
}

#[derive(Deserialize)]
pub(crate) struct Build {
    pub build: u64,
    channel: String,
    downloads: HashMap<String, Download>,
}

impl Build {
    pub fn stable(&self) -> bool {
        self.channel == "default"
    }
}

#[derive(Deserialize)]
struct Download {
    name: String,
    sha256: String,
}

pub(crate) fn builds(agent: &ureq::Agent, base: &str, mc: &str) -> Result<Vec<Build>> {
    let url = format!("{base}/versions/{mc}/builds");
    let mut catalog: Catalog = metadata(agent, &url)?;
    if catalog.code != 200 || catalog.project_id != "leaves" || catalog.version != mc {
        return Err(Error::VersionMetadata {
            url,
            reason:
                "Leaves project, Minecraft version or response status does not match the request"
                    .into(),
        });
    }
    catalog
        .builds
        .retain(|build| build.downloads.contains_key("application"));
    catalog
        .builds
        .sort_by_key(|build| std::cmp::Reverse(build.build));
    catalog.builds.dedup_by_key(|build| build.build);
    Ok(catalog.builds)
}

pub(super) fn install(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    base: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    let requested = request
        .version
        .as_deref()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| Error::invalid("Leaves build", "expected an integer build number"))
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
        .find(|build| requested.map_or_else(|| build.stable(), |id| id == build.build))
        .ok_or_else(|| Error::NoCompatibleVersion {
            component: "Leaves build",
            requested: request.version.clone(),
        })?;
    let artifact = &build.downloads["application"];
    if validate_version(&artifact.name).is_err() || !artifact.name.ends_with(".jar") {
        return Err(Error::VersionMetadata {
            url: base.into(),
            reason: "invalid Leaves download filename".into(),
        });
    }
    let version = build.build.to_string();
    let url = format!(
        "{base}/versions/{}/builds/{version}/downloads/{}",
        request.minecraft, artifact.name
    );
    jar::install(
        root,
        &target,
        request,
        agent,
        &jar::Artifact {
            version: &version,
            url: &url,
            size: None,
            checksum: jar::Checksum::Sha256(&artifact.sha256),
        },
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::{InstallKind, test_support::serve};
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
    fn jar() -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"Main-Class: org.leavesmc.leavesclip.Main\n")
            .unwrap();
        zip.start_file("META-INF/versions.list", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"hash\t1.21.8\t1.21.8/leaves-1.21.8.jar\n")
            .unwrap();
        zip.finish().unwrap().into_inner()
    }
    fn catalog(bytes: &[u8]) -> Value {
        let row = |id, channel| json!({"build":id,"channel":channel,"downloads":{"application":{"name":"leaves-1.21.8.jar","sha256":format!("{:x}",Sha256::digest(bytes))}}});
        json!({"code":200,"project_id":"leaves","version":"1.21.8","builds":[row(9,"default"),row(11,"experimental"),row(10,"default")]})
    }

    #[test]
    fn installs_latest_default_or_explicit_experimental_build_without_java() {
        for requested in [None, Some("11".to_owned())] {
            let root = TempDir::new().unwrap();
            let bytes = jar();
            let (base, worker) = serve(vec![
                serde_json::to_vec(&catalog(&bytes)).unwrap(),
                bytes.clone(),
            ]);
            let mut request = InstallRequest::leaves("demo", "1.21.8");
            request.version = requested.clone();
            let id = requested.as_deref().unwrap_or("10");
            let result = install(root.path(), &request, &agent(), &base, &mut |_| {
                ControlFlow::Continue(())
            })
            .unwrap();
            assert_eq!(result.kind, InstallKind::Leaves);
            assert_eq!(result.loader.as_deref(), Some(id));
            assert_eq!(fs::read(result.jar.unwrap()).unwrap(), bytes);
            assert_eq!(
                fs::read_to_string(result.directory.join("eula.txt")).unwrap(),
                "eula=false\n"
            );
            let servers = crate::discover(root.path()).unwrap();
            assert_eq!(servers[0].server_type, "leaves");
            assert_eq!(servers[0].version.as_deref(), Some("1.21.8"));
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
            let requests = worker.join().unwrap();
            assert!(requests[1].starts_with(&format!(
                "GET /versions/1.21.8/builds/{id}/downloads/leaves-1.21.8.jar "
            )));
        }
    }

    #[test]
    fn unavailable_builds_and_bad_metadata_never_download() {
        for scenario in [
            "project",
            "version",
            "status",
            "filename",
            "checksum",
            "missing",
            "experimental-only",
        ] {
            let root = TempDir::new().unwrap();
            let mut body = catalog(&jar());
            let mut request = InstallRequest::leaves("demo", "1.21.8");
            match scenario {
                "project" => body["project_id"] = json!("paper"),
                "version" => body["version"] = json!("1.21.4"),
                "status" => body["code"] = json!(500),
                "filename" => {
                    body["builds"][2]["downloads"]["application"]["name"] = json!("../escape.jar")
                }
                "checksum" => {
                    body["builds"][2]["downloads"]["application"]["sha256"] = json!("invalid")
                }
                "missing" => request.version = Some("99".into()),
                "experimental-only" => body["builds"] = json!([body["builds"][1].clone()]),
                _ => unreachable!(),
            }
            let (base, worker) = serve(vec![serde_json::to_vec(&body).unwrap()]);
            let error = install(root.path(), &request, &agent(), &base, &mut |_| {
                ControlFlow::Continue(())
            })
            .unwrap_err();
            if matches!(scenario, "missing" | "experimental-only") {
                assert!(matches!(error, Error::NoCompatibleVersion { .. }));
            } else {
                assert!(matches!(error, Error::VersionMetadata { .. }));
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
            worker.join().unwrap();
        }
        let root = TempDir::new().unwrap();
        let mut request = InstallRequest::leaves("demo", "1.21.8");
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

    #[test]
    fn tampered_download_and_cancelled_transfer_clean_staging() {
        for cancel in [false, true] {
            let root = TempDir::new().unwrap();
            let bytes = jar();
            let mut body = catalog(&bytes);
            if !cancel {
                body["builds"][2]["downloads"]["application"]["sha256"] = json!("0".repeat(64));
            }
            let (base, worker) = serve(vec![serde_json::to_vec(&body).unwrap(), bytes]);
            let error = install(
                root.path(),
                &InstallRequest::leaves("demo", "1.21.8"),
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
}
