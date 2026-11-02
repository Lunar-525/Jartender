//! Synchronous Minecraft server installation with host-owned progress reporting.
//!
//! Fabric and server launchers are plain downloads; hybrid cores prepare libraries on first run.
//! Forge and NeoForge run their official installers in
//! Java, so those installs execute downloaded third-party code; hosts that cannot
//! accept that should keep using `list`/`run` on pre-installed servers.
mod fabric;
pub(crate) mod fill;
pub(crate) mod hybrid;
mod jar;
pub(crate) mod leaves;
mod loader;
pub(crate) mod purpur;

use crate::{
    Error, Result,
    config::{require_absolute, validate_name},
};
use sha1::{Digest, Sha1};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use fabric::{FabricRequest, install_fabric};

/// Installation timeout for the Forge/NeoForge installer process.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Which server core or loader to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    Arclight,
    Mohist,
    Banner,
    CatServer,
    Folia,
    Leaves,
    Purpur,
    Paper,
    Fabric,
    Forge,
    NeoForge,
}

impl InstallKind {
    /// Every core or loader, in a stable order for listings.
    pub const ALL: &'static [InstallKind] = &[
        Self::Arclight,
        Self::Mohist,
        Self::Banner,
        Self::CatServer,
        Self::Folia,
        Self::Leaves,
        Self::Purpur,
        Self::Paper,
        Self::Fabric,
        Self::Forge,
        Self::NeoForge,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Arclight => "Arclight",
            Self::Mohist => "Mohist",
            Self::Banner => "Banner",
            Self::CatServer => "CatServer",
            Self::Folia => "Folia",
            Self::Leaves => "Leaves",
            Self::Purpur => "Purpur",
            Self::Paper => "Paper",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

/// Host-supplied installation settings. Forge and NeoForge require `java`.
#[derive(Debug, Clone)]
pub struct InstallRequest {
    pub kind: InstallKind,
    pub name: String,
    pub minecraft: String,
    /// Build number, loader version, or Arclight/CatServer asset filename.
    /// `None` selects the default; Arclight requires a filename if platforms are ambiguous.
    pub version: Option<String>,
    /// Fabric launcher installer version; ignored by other loaders.
    pub installer: Option<String>,
    /// Java executable used to run the official installer.
    pub java: Option<PathBuf>,
    /// GitHub token for Arclight/CatServer metadata; anonymous requests are rate-limited.
    pub github_token: Option<String>,
    /// Shared policy for all requests and the Forge/NeoForge installer process.
    pub proxy: crate::network::ProxyConfig,
}

impl InstallRequest {
    pub fn arclight(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Arclight, name, minecraft)
    }
    pub fn mohist(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Mohist, name, minecraft)
    }
    pub fn banner(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Banner, name, minecraft)
    }
    pub fn catserver(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::CatServer, name, minecraft)
    }
    pub fn folia(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Folia, name, minecraft)
    }

    pub fn leaves(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Leaves, name, minecraft)
    }

    pub fn purpur(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Purpur, name, minecraft)
    }

    pub fn paper(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Paper, name, minecraft)
    }

    pub fn new(kind: InstallKind, name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            minecraft: minecraft.into(),
            version: None,
            installer: None,
            java: None,
            github_token: None,
            proxy: Default::default(),
        }
    }

    pub fn fabric(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Fabric, name, minecraft)
    }

    pub fn forge(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::Forge, name, minecraft)
    }

    pub fn neoforge(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self::new(InstallKind::NeoForge, name, minecraft)
    }

    pub fn with_java(mut self, java: impl Into<PathBuf>) -> Self {
        self.java = Some(java.into());
        self
    }

    pub fn with_github_token(mut self, token: impl Into<String>) -> Self {
        self.github_token = Some(token.into());
        self
    }
}

/// Owned events can be forwarded to a host UI or worker channel.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallEvent {
    /// An official release did not publish a checksum; size and JAR checks still apply.
    ChecksumUnavailable {
        component: String,
        version: String,
    },
    /// A server JAR is being downloaded without running an installer.
    DownloadingServer {
        component: String,
        version: String,
    },
    Resolving {
        minecraft: String,
    },
    /// Fabric downloads two resolved artifacts at once.
    Downloading {
        loader: String,
        installer: String,
    },
    /// A Forge/NeoForge installer is being fetched and executed.
    Installing {
        component: String,
        version: String,
    },
    /// One line of installer output. Hosts decide whether to display it.
    Output {
        line: String,
    },
    Progress {
        downloaded: u64,
        total: Option<u64>,
    },
}

#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub kind: InstallKind,
    pub directory: PathBuf,
    pub jar: Option<PathBuf>,
    /// Relative platform argument file for Forge/NeoForge installs.
    pub args_file: Option<PathBuf>,
    pub minecraft: String,
    /// Resolved server build number or loader version.
    pub loader: Option<String>,
    pub installer: Option<String>,
}

/// Install a server into a new directory. Never starts the server or accepts the EULA.
///
/// `root` must be absolute. The callback runs on the calling thread; returning
/// `ControlFlow::Break(())` cancels the install and removes staging files. Forge and
/// NeoForge additionally spawn the official installer with `request.java`.
pub fn install(
    root: &Path,
    request: &InstallRequest,
    progress: impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    install_with(
        root,
        request,
        &request.proxy.agent(Duration::from_secs(180)),
        progress,
    )
}

fn install_with(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    mut progress: impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    match request.kind {
        InstallKind::Arclight
        | InstallKind::Mohist
        | InstallKind::Banner
        | InstallKind::CatServer => hybrid::install(
            root,
            request,
            agent,
            hybrid::source(request.kind),
            &mut progress,
        ),
        InstallKind::Folia => fill::install(root, request, agent, fill::FOLIA, &mut progress),
        InstallKind::Leaves => leaves::install(root, request, agent, leaves::API, &mut progress),
        InstallKind::Purpur => purpur::install(root, request, agent, purpur::API, &mut progress),
        InstallKind::Paper => fill::install(root, request, agent, fill::PAPER, &mut progress),
        InstallKind::Fabric => fabric::install(root, request, agent, fabric::META, &mut progress),
        InstallKind::Forge => loader::install_forge(
            root,
            request,
            agent,
            loader::FORGE_PROMOTIONS,
            loader::FORGE_MAVEN,
            &mut progress,
        ),
        InstallKind::NeoForge => loader::install_neoforge(
            root,
            request,
            agent,
            loader::NEOFORGE_METADATA,
            loader::NEOFORGE_MAVEN,
            &mut progress,
        ),
    }
}

#[cfg(test)]
pub(crate) fn agent() -> ureq::Agent {
    crate::network::ProxyConfig::Direct.agent(Duration::from_secs(180))
}

pub(super) fn notify(
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
    event: InstallEvent,
) -> Result<()> {
    match progress(event) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(()) => Err(Error::Cancelled),
    }
}

pub(super) fn metadata<T: serde::de::DeserializeOwned>(
    agent: &ureq::Agent,
    url: &str,
) -> Result<T> {
    let bytes = response_bytes(agent, url)?;
    serde_json::from_slice(&bytes).map_err(|source| Error::InvalidMetadata {
        url: url.to_owned(),
        source,
    })
}

pub(super) fn response_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let mut response = agent.get(url).call().map_err(|source| Error::Download {
        url: url.to_owned(),
        source,
    })?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|source| Error::Download {
            url: url.to_owned(),
            source,
        })
}

/// Reject versions that could break out of a metadata URL.
pub(super) fn validate_version(value: &str) -> Result<()> {
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

/// Validate request fields and return the not-yet-created target directory.
pub(super) fn prepare(root: &Path, request: &InstallRequest) -> Result<PathBuf> {
    require_absolute(root, "server root")?;
    validate_name(&request.name)?;
    validate_version(&request.minecraft)?;
    for value in [&request.version, &request.installer].into_iter().flatten() {
        validate_version(value)?;
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
    Ok(target)
}

pub(super) fn staging(root: &Path) -> Result<tempfile::TempDir> {
    fs::create_dir_all(root).map_err(|e| Error::io("create server root", root, e))?;
    tempfile::Builder::new()
        .prefix(".jartender-install-")
        .tempdir_in(root)
        .map_err(|e| Error::io("create staging directory", root, e))
}

/// Move a fully prepared staging directory into the reserved target name.
pub(super) fn publish(work: &Path, target: &Path) -> Result<()> {
    // Reserve the final name exclusively, then move each prepared entry.
    fs::create_dir(target).map_err(|e| Error::io("create server", target, e))?;
    let moved = (|| -> std::io::Result<()> {
        for entry in fs::read_dir(work)? {
            let entry = entry?;
            fs::rename(entry.path(), target.join(entry.file_name()))?;
        }
        Ok(())
    })();
    if let Err(e) = moved {
        let _ = fs::remove_dir_all(target);
        return Err(Error::io("finish installation", target, e));
    }
    Ok(())
}

pub(super) fn write_eula(directory: &Path) -> Result<()> {
    let path = directory.join("eula.txt");
    fs::write(&path, "eula=false\n").map_err(|e| Error::io("write EULA", &path, e))
}

pub(super) fn download(
    agent: &ureq::Agent,
    url: &str,
    destination: &Path,
    limit: u64,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<()> {
    let mut response = agent.get(url).call().map_err(|source| Error::Download {
        url: url.to_owned(),
        source,
    })?;
    let total = response.body().content_length();
    let mut file =
        File::create(destination).map_err(|e| Error::io("create download", destination, e))?;
    let mut reader = response.body_mut().as_reader();
    let mut downloaded = 0u64;
    let mut buffer = [0; 64 * 1024];
    notify(progress, InstallEvent::Progress { downloaded, total })?;
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|e| Error::io("download artifact", destination, e))?;
        if count == 0 {
            break;
        }
        downloaded += count as u64;
        if downloaded > limit {
            return Err(Error::DownloadTooLarge { limit });
        }
        file.write_all(&buffer[..count])
            .map_err(|e| Error::io("write download", destination, e))?;
        notify(progress, InstallEvent::Progress { downloaded, total })?;
    }
    file.sync_all()
        .map_err(|e| Error::io("sync download", destination, e))?;
    Ok(())
}

/// Verify a Maven-style `.sha1` sidecar when the repository publishes one.
pub(super) fn verify_sha1(agent: &ureq::Agent, url: &str, path: &Path) -> Result<()> {
    let Ok(bytes) = response_bytes(agent, &format!("{url}.sha1")) else {
        return Ok(());
    };
    let expected = String::from_utf8_lossy(&bytes)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if expected.len() != 40 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(());
    }
    let actual = format!(
        "{:x}",
        Sha1::digest(fs::read(path).map_err(|e| Error::io("read download", path, e))?)
    );
    if actual != expected {
        return Err(Error::ChecksumMismatch {
            url: url.to_owned(),
            expected,
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    #[cfg(unix)]
    use std::path::Path;
    use std::thread;

    /// Serve a fixed list of `(status, body)` responses and return the request texts.
    pub fn serve_responses(
        responses: Vec<(u16, Vec<u8>)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        serve_responses_on(listener, responses)
    }

    /// Serve on an already-bound listener, so tests can embed its URL in response bodies.
    pub fn serve_responses_on(
        listener: TcpListener,
        responses: Vec<(u16, Vec<u8>)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut request = first.clone();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                requests.push(request);
                let reason = if status == 200 { "OK" } else { "Not Found" };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            requests
        });
        (url, worker)
    }

    /// Serve successful responses in order.
    pub fn serve(bodies: Vec<Vec<u8>>) -> (String, thread::JoinHandle<Vec<String>>) {
        serve_responses(bodies.into_iter().map(|body| (200, body)).collect())
    }

    /// A fake `java` that only produces the expected layout for `--installServer`.
    #[cfg(unix)]
    pub fn fake_installer_java(path: &Path, exit: i32) {
        use std::os::unix::fs::PermissionsExt;
        let script = format!(
            r#"#!/bin/sh
case "$*" in
  *--installServer*)
    mkdir -p libraries/net/neoforged/neoforge/1.0
    printf 'libraries/example.jar\n' > libraries/net/neoforged/neoforge/1.0/unix_args.txt
    printf 'libraries/example.jar\n' > libraries/net/neoforged/neoforge/1.0/win_args.txt
    printf '# jvm args\n' > user_jvm_args.txt
    printf 'installer output line\n'
    exit {exit}
    ;;
esac
exit 0
"#
        );
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::serve;
    use super::*;
    use sha1::{Digest, Sha1};
    use std::io::Cursor;
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

    fn sha1(bytes: &[u8]) -> String {
        format!("{:x}", Sha1::digest(bytes))
    }

    #[test]
    fn fabric_metadata_and_launcher_share_the_configured_proxy() {
        let temp = TempDir::new().unwrap();
        let jar = launcher();
        let (proxy, worker) = crate::network::tests::serve_proxy(vec![
            br#"[{"loader":{"version":"0.16.10","stable":true}}]"#.to_vec(),
            br#"[{"version":"1.0.3","stable":true}]"#.to_vec(),
            jar.clone(),
        ]);
        let mut request = InstallRequest::fabric("demo", "1.21.4");
        request.proxy = proxy;
        let outcome = fabric::install(
            temp.path(),
            &request,
            &request.proxy.agent(Duration::from_secs(5)),
            "http://fabric.invalid",
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert_eq!(fs::read(outcome.jar.unwrap()).unwrap(), jar);
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(
            requests
                .iter()
                .all(|r| r.starts_with("CONNECT fabric.invalid:80 "))
        );
        assert!(requests[2].contains("GET /loader/1.21.4/0.16.10/1.0.3/server/jar "));
    }

    #[test]
    fn fabric_install_publishes_the_launcher_without_java() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let jar = launcher();
        let (url, worker) = serve(vec![
            br#"[{"loader":{"version":"0.17.0-beta","stable":false}},{"loader":{"version":"0.16.10","stable":true}}]"#.to_vec(),
            br#"[{"version":"1.0.3","stable":true}]"#.to_vec(),
            jar.clone(),
        ]);
        let mut events = Vec::new();
        let outcome = fabric::install(
            &root,
            &InstallRequest::fabric("demo", "1.21.4"),
            &agent(),
            &url,
            &mut |event| {
                events.push(event);
                ControlFlow::Continue(())
            },
        )
        .unwrap();
        assert_eq!(outcome.kind, InstallKind::Fabric);
        assert_eq!(outcome.jar, Some(root.join("demo/server.jar")));
        assert_eq!(outcome.args_file, None);
        assert_eq!(outcome.loader.as_deref(), Some("0.16.10"));
        assert_eq!(outcome.installer.as_deref(), Some("1.0.3"));
        assert!(
            matches!(events.last(), Some(InstallEvent::Progress { downloaded, total: Some(total) }) if *downloaded == jar.len() as u64 && downloaded == total)
        );
        let requests = worker.join().unwrap();
        assert!(requests[0].starts_with("GET /loader/1.21.4 "));
        assert!(requests[2].starts_with("GET /loader/1.21.4/0.16.10/1.0.3/server/jar "));
        assert_eq!(std::fs::read(root.join("demo/server.jar")).unwrap(), jar);
        assert_eq!(
            std::fs::read_to_string(root.join("demo/eula.txt")).unwrap(),
            "eula=false\n"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn cancellation_removes_partial_files_and_preserves_the_target_name() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let (url, worker) = serve(vec![
            br#"[{"loader":{"version":"0.16.10","stable":true}}]"#.to_vec(),
            br#"[{"version":"1.0.3","stable":true}]"#.to_vec(),
            launcher(),
        ]);
        let result = fabric::install(
            &root,
            &InstallRequest::fabric("demo", "1.21.4"),
            &agent(),
            &url,
            &mut |event| {
                if matches!(event, InstallEvent::Progress { downloaded, .. } if downloaded > 0) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        );
        worker.join().unwrap();
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        assert!(!root.join("demo").exists());
    }

    #[test]
    fn invalid_downloads_are_cleaned_up_and_existing_servers_are_never_overwritten() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let (url, worker) = serve(vec![
            br#"[{"loader":{"version":"0.16.10","stable":true}}]"#.to_vec(),
            br#"[{"version":"1.0.3","stable":true}]"#.to_vec(),
            b"not a JAR".to_vec(),
        ]);
        assert!(
            fabric::install(
                &root,
                &InstallRequest::fabric("demo", "1.21.4"),
                &agent(),
                &url,
                &mut |_| ControlFlow::Continue(())
            )
            .is_err()
        );
        worker.join().unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::create_dir(root.join("demo")).unwrap();
        std::fs::write(root.join("demo/keep"), "original").unwrap();
        assert!(
            install(
                &root,
                &InstallRequest::fabric("demo", "1.21.4"),
                &mut |_| ControlFlow::Continue(())
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(root.join("demo/keep")).unwrap(),
            "original"
        );
    }

    #[test]
    fn forge_version_prefers_recommended_then_latest() {
        let (url, worker) = serve(vec![
            br#"{"promos":{"26.3-recommended":"66.1.0","26.3-latest":"66.1.2","26.4-latest":"67.0.0"}}"#.to_vec(),
        ]);
        assert_eq!(
            loader::forge_version(&agent(), &url, "26.3").unwrap(),
            "66.1.0"
        );
        worker.join().unwrap();
        assert!(matches!(
            loader::forge_version(&agent(), "http://127.0.0.1:1", "26.3"),
            Err(Error::Download { .. })
        ));
    }

    #[test]
    fn neoforge_version_uses_the_highest_stable_build_for_the_minecraft_version() {
        let xml = br#"<metadata><versioning><versions>
            <version>20.2.93</version>
            <version>21.0.167</version>
            <version>21.1.9</version>
            <version>21.1.100</version>
            <version>21.4.157</version>
            <version>21.4.158-beta</version>
            <version>26.1.2.123</version>
            <version>26.1.2.125</version>
            <version>26.3.0.9</version>
            <version>26.3.0.100</version>
            <version>26.3.0.101-beta</version>
            <version>26.2.0.93</version>
        </versions></versioning></metadata>"#;
        for (minecraft, expected) in [
            ("1.20.2", "20.2.93"),
            ("1.21", "21.0.167"),
            ("1.21.1", "21.1.100"),
            ("1.21.4", "21.4.157"),
            ("26.3", "26.3.0.100"),
            ("26.1.2", "26.1.2.125"),
        ] {
            let (url, worker) = serve(vec![xml.to_vec()]);
            assert_eq!(
                loader::neoforge_version(&agent(), &url, minecraft).unwrap(),
                expected
            );
            worker.join().unwrap();
        }
    }

    #[cfg(unix)]
    fn forge_fixture(
        exit: i32,
        installer: &[u8],
        checksum: Option<String>,
    ) -> (
        TempDir,
        std::path::PathBuf,
        std::path::PathBuf,
        String,
        std::thread::JoinHandle<Vec<String>>,
    ) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let java = temp.path().join("fake-java");
        test_support::fake_installer_java(&java, exit);
        let mut bodies = vec![
            br#"{"promos":{"26.3-recommended":"66.1.0"}}"#.to_vec(),
            installer.to_vec(),
        ];
        if let Some(checksum) = checksum {
            bodies.push(checksum.into_bytes());
        }
        let (url, worker) = serve(bodies);
        (temp, root, java, url, worker)
    }

    #[cfg(unix)]
    #[test]
    fn forge_install_runs_the_official_installer_and_publishes_the_layout() {
        let installer = b"forge installer bytes".to_vec();
        let (temp, root, java, url, worker) = forge_fixture(0, &installer, Some(sha1(&installer)));
        let mut events = Vec::new();
        let request = InstallRequest::forge("demo", "26.3").with_java(&java);
        let outcome = loader::install_forge(
            &root,
            &request,
            &agent(),
            &format!("{url}/promotions"),
            &url,
            &mut |event| {
                events.push(event);
                ControlFlow::Continue(())
            },
        )
        .unwrap();
        let requests = worker.join().unwrap();
        assert_eq!(outcome.kind, InstallKind::Forge);
        assert_eq!(outcome.jar, None);
        assert_eq!(outcome.loader.as_deref(), Some("66.1.0"));
        assert_eq!(
            outcome.args_file.as_deref(),
            Some(Path::new(
                "libraries/net/neoforged/neoforge/1.0/unix_args.txt"
            ))
        );
        assert!(requests[1].starts_with("GET /26.3-66.1.0/forge-26.3-66.1.0-installer.jar "));
        assert!(requests[2].starts_with("GET /26.3-66.1.0/forge-26.3-66.1.0-installer.jar.sha1 "));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, InstallEvent::Installing { component, .. } if component == "Forge"))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, InstallEvent::Output { line } if line == "installer output line"))
        );
        let demo = root.join("demo");
        assert!(demo.join("libraries").is_dir());
        assert!(demo.join("user_jvm_args.txt").is_file());
        assert_eq!(
            std::fs::read_to_string(demo.join("eula.txt")).unwrap(),
            "eula=false\n"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(temp);
    }

    #[cfg(unix)]
    #[test]
    fn failing_or_tampered_installers_leave_no_server_behind() {
        let installer = b"forge installer bytes".to_vec();
        let (temp, root, java, url, worker) = forge_fixture(1, &installer, Some(sha1(&installer)));
        let request = InstallRequest::forge("demo", "26.3").with_java(&java);
        let result = loader::install_forge(
            &root,
            &request,
            &agent(),
            &format!("{url}/promotions"),
            &url,
            &mut |_| ControlFlow::Continue(()),
        );
        worker.join().unwrap();
        assert!(matches!(
            result,
            Err(Error::InstallerFailed { code: Some(1), .. })
        ));
        assert!(!root.join("demo").exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        drop(temp);

        let (temp, root, java, url, worker) =
            forge_fixture(0, &installer, Some(sha1(b"other bytes")));
        let request = InstallRequest::forge("demo", "26.3").with_java(&java);
        let result = loader::install_forge(
            &root,
            &request,
            &agent(),
            &format!("{url}/promotions"),
            &url,
            &mut |_| ControlFlow::Continue(()),
        );
        worker.join().unwrap();
        assert!(matches!(result, Err(Error::ChecksumMismatch { .. })));
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        drop(temp);
    }

    #[test]
    fn java_selection_rejects_relative_paths_and_requires_a_value() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("servers");
        let request = InstallRequest::forge("demo", "26.3");
        assert!(matches!(
            loader::install_forge(
                &root,
                &request,
                &agent(),
                "http://127.0.0.1:1",
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Continue(())
            ),
            Err(Error::InvalidInput {
                field: "Java executable",
                ..
            })
        ));
        let request = InstallRequest::forge("demo", "26.3").with_java("./jdk/bin/java");
        assert!(matches!(
            loader::install_forge(
                &root,
                &request,
                &agent(),
                "http://127.0.0.1:1",
                "http://127.0.0.1:1",
                &mut |_| ControlFlow::Continue(())
            ),
            Err(Error::InvalidInput {
                field: "Java executable",
                ..
            })
        ));
    }
}
