//! Synchronous Fabric installation with host-owned progress reporting.
use crate::{
    Error, Result,
    config::{require_absolute, validate_name},
    server,
};
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

const META: &str = "https://meta.fabricmc.net/v2/versions";

#[derive(Debug, Clone)]
pub struct FabricRequest {
    pub minecraft: String,
    pub name: String,
    pub loader: Option<String>,
    pub installer: Option<String>,
}

impl FabricRequest {
    pub fn new(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            minecraft: minecraft.into(),
            loader: None,
            installer: None,
        }
    }
}

/// Owned events can be forwarded to a host UI or worker channel.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallEvent {
    Resolving { minecraft: String },
    Downloading { loader: String, installer: String },
    Progress { downloaded: u64, total: Option<u64> },
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub directory: PathBuf,
    pub jar: PathBuf,
    pub minecraft: String,
    pub loader: String,
    pub installer: String,
}

#[derive(Deserialize)]
struct Version {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[derive(Deserialize)]
struct Loader {
    loader: Version,
}

fn version(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(&b))
    {
        return Err(Error::invalid(
            "version",
            format!("unsupported characters in {value:?}"),
        ));
    }
    Ok(())
}

fn choose(
    versions: &[Version],
    requested: Option<&str>,
    component: &'static str,
) -> Result<String> {
    let chosen = match requested {
        Some(value) => versions.iter().find(|v| v.version == value),
        None => versions.iter().find(|v| v.stable),
    };
    chosen
        .map(|v| v.version.clone())
        .ok_or_else(|| Error::NoCompatibleVersion {
            component,
            requested: requested.map(str::to_owned),
        })
}

fn notify(
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
    event: InstallEvent,
) -> Result<()> {
    match progress(event) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(()) => Err(Error::Cancelled),
    }
}

fn metadata<T: serde::de::DeserializeOwned>(agent: &ureq::Agent, url: String) -> Result<T> {
    let mut response = agent.get(&url).call().map_err(|source| Error::Download {
        url: url.clone(),
        source,
    })?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|source| Error::Download {
            url: url.clone(),
            source,
        })?;
    serde_json::from_slice(&bytes).map_err(|source| Error::InvalidMetadata { url, source })
}

/// Download a launcher into a new server directory. Never runs Java or accepts EULA.
///
/// `root` must be absolute. The callback runs on the calling thread. Return
/// `ControlFlow::Break(())` to cancel before publication and remove staging files.
/// Cancellation is checked between events/chunks; blocking HTTP operations have a
/// 180-second timeout. GUI callers should run installation on their own worker thread.
pub fn install_fabric(
    root: &Path,
    request: &FabricRequest,
    mut progress: impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .into();
    install_from(root, request, &agent, META, &mut progress)
}

fn install_from(
    root: &Path,
    request: &FabricRequest,
    agent: &ureq::Agent,
    meta: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    require_absolute(root, "server root")?;
    validate_name(&request.name)?;
    version(&request.minecraft)?;
    for value in [&request.loader, &request.installer].into_iter().flatten() {
        version(value)?;
    }
    let target = root.join(&request.name);
    if target
        .try_exists()
        .map_err(|e| Error::io("check target", &target, e))?
    {
        return Err(Error::io(
            "create server",
            &target,
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "choose another server name",
            ),
        ));
    }
    notify(
        progress,
        InstallEvent::Resolving {
            minecraft: request.minecraft.clone(),
        },
    )?;
    let loaders: Vec<Loader> = metadata(agent, format!("{meta}/loader/{}", request.minecraft))?;
    let loaders: Vec<_> = loaders.into_iter().map(|l| l.loader).collect();
    let loader = choose(&loaders, request.loader.as_deref(), "Fabric Loader")?;
    let installers: Vec<Version> = metadata(agent, format!("{meta}/installer"))?;
    let installer = choose(
        &installers,
        request.installer.as_deref(),
        "Fabric installer",
    )?;
    version(&loader)?;
    version(&installer)?;
    notify(
        progress,
        InstallEvent::Downloading {
            loader: loader.clone(),
            installer: installer.clone(),
        },
    )?;
    fs::create_dir_all(root).map_err(|e| Error::io("create server root", root, e))?;
    let staging = tempfile::Builder::new()
        .prefix(".jartender-install-")
        .tempdir_in(root)
        .map_err(|e| Error::io("create staging directory", root, e))?;
    let url = format!(
        "{meta}/loader/{}/{loader}/{installer}/server/jar",
        request.minecraft
    );
    let jar = staging.path().join("server.jar");
    let mut response = agent.get(&url).call().map_err(|source| Error::Download {
        url: url.clone(),
        source,
    })?;
    let total = response.body().content_length();
    let mut file = File::create(&jar).map_err(|e| Error::io("create launcher", &jar, e))?;
    const LIMIT: u64 = 256 * 1024 * 1024;
    let mut downloaded = 0;
    let mut buffer = [0; 64 * 1024];
    let mut reader = response.body_mut().as_reader();
    notify(progress, InstallEvent::Progress { downloaded, total })?;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| Error::io("download launcher", &jar, e))?;
        if count == 0 {
            break;
        }
        downloaded += count as u64;
        if downloaded > LIMIT {
            return Err(Error::DownloadTooLarge { limit: LIMIT });
        }
        file.write_all(&buffer[..count])
            .map_err(|e| Error::io("write launcher", &jar, e))?;
        notify(progress, InstallEvent::Progress { downloaded, total })?;
    }
    file.sync_all()
        .map_err(|e| Error::io("sync launcher", &jar, e))?;
    drop(file);
    let manifest = server::manifest(&jar)?;
    if !manifest.lines().any(|line| line.starts_with("Main-Class:")) {
        return Err(Error::InvalidJar {
            path: jar,
            reason: "missing Main-Class".into(),
        });
    }
    let eula = staging.path().join("eula.txt");
    fs::write(&eula, "eula=false\n").map_err(|e| Error::io("write EULA", &eula, e))?;
    // Reserve the final name exclusively. A server becomes discoverable after both files are ready.
    fs::create_dir(&target).map_err(|e| Error::io("create server", &target, e))?;
    let publish = (|| -> std::io::Result<()> {
        fs::rename(&eula, target.join("eula.txt"))?;
        fs::rename(&jar, target.join("server.jar"))?;
        Ok(())
    })();
    if let Err(e) = publish {
        let _ = fs::remove_dir_all(&target);
        return Err(Error::io("finish installation", &target, e));
    }
    Ok(InstallOutcome {
        jar: target.join("server.jar"),
        directory: target,
        minecraft: request.minecraft.clone(),
        loader,
        installer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Cursor, Write};
    use std::net::TcpListener;
    use std::thread;
    use tempfile::TempDir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn launcher() -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"Main-Class: net.fabricmc.loader.launch.server.FabricServerLauncher\n")
            .unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn mock(payload: Vec<u8>) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let bodies = [
                br#"[{"loader":{"version":"0.17.0-beta","stable":false}},{"loader":{"version":"0.16.10","stable":true}}]"#.to_vec(),
                br#"[{"version":"1.0.3","stable":true}]"#.to_vec(),
                payload,
            ];
            let mut requests = Vec::new();
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                requests.push(first);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                }
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            requests
        });
        (url, worker)
    }

    fn options() -> FabricRequest {
        FabricRequest {
            minecraft: "1.21.4".into(),
            name: "demo".into(),
            loader: None,
            installer: None,
        }
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into()
    }

    #[test]
    fn install_downloads_a_launcher_without_running_java_or_accepting_eula() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let jar = launcher();
        let (url, worker) = mock(jar.clone());
        let mut events = Vec::new();
        let outcome = install_from(&root, &options(), &agent(), &url, &mut |event| {
            events.push(event);
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(outcome.directory, root.join("demo"));
        assert_eq!(outcome.jar, root.join("demo/server.jar"));
        assert_eq!(outcome.loader, "0.16.10");
        assert_eq!(outcome.installer, "1.0.3");
        assert!(
            matches!(events.last(), Some(InstallEvent::Progress { downloaded, total: Some(total) }) if *downloaded == jar.len() as u64 && downloaded == total)
        );
        let requests = worker.join().unwrap();
        assert!(requests[0].starts_with("GET /loader/1.21.4 "));
        assert!(requests[2].starts_with("GET /loader/1.21.4/0.16.10/1.0.3/server/jar "));
        assert_eq!(fs::read(root.join("demo/server.jar")).unwrap(), jar);
        assert_eq!(
            fs::read_to_string(root.join("demo/eula.txt")).unwrap(),
            "eula=false\n"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(fs::read_dir(root.join("demo")).unwrap().count(), 2);
    }

    #[test]
    fn cancellation_during_download_removes_partial_files_and_preserves_target_name() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let (url, worker) = mock(launcher());
        let result = install_from(&root, &options(), &agent(), &url, &mut |event| {
            if matches!(event, InstallEvent::Progress { downloaded, .. } if downloaded > 0) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        worker.join().unwrap();
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        assert!(!root.join("demo").exists());
    }

    #[test]
    fn invalid_download_is_cleaned_up_and_existing_servers_are_never_overwritten() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let (url, worker) = mock(b"not a JAR".to_vec());
        assert!(
            install_from(&root, &options(), &agent(), &url, &mut |_| {
                ControlFlow::Continue(())
            })
            .is_err()
        );
        worker.join().unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::create_dir(root.join("demo")).unwrap();
        fs::write(root.join("demo/keep"), "original").unwrap();
        assert!(
            install_from(
                &root,
                &options(),
                &agent(),
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Continue(())
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(root.join("demo/keep")).unwrap(),
            "original"
        );
    }
}
