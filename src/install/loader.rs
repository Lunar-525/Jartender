//! Forge and NeoForge installs drive the official installer in Java.
//!
//! The installers unpack a Minecraft payload and run Java "processors" (binary
//! patchers), so reimplementing them in Rust would be fragile. Installing therefore
//! executes downloaded third-party code, unlike the Fabric path.
use super::{
    INSTALL_TIMEOUT, InstallEvent, InstallOutcome, InstallRequest, download, notify, prepare,
    publish, response_bytes, staging, validate_version, verify_sha1, write_eula,
};
use crate::{Error, Result, server};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(super) const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
pub(super) const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
pub(super) const NEOFORGE_METADATA: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
pub(super) const NEOFORGE_MAVEN: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge";

const MOJANG: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const INSTALLER_LIMIT: u64 = 64 * 1024 * 1024;
const TAIL: usize = 40;

/// The resolved installer to download and execute.
struct Installer<'a> {
    component: &'a str,
    version: &'a str,
    /// Legacy Maven coordinates add the Minecraft version again, so try both.
    urls: Vec<String>,
}

/// Forge used `{mc}-{build}` until 1.7, then `{mc}-{build}-{mc}` for 1.7.10-1.9.4.
fn forge_installer_urls(maven: &str, mc: &str, version: &str) -> Vec<String> {
    vec![
        format!("{maven}/{mc}-{version}/forge-{mc}-{version}-installer.jar"),
        format!("{maven}/{mc}-{version}-{mc}/forge-{mc}-{version}-{mc}-installer.jar"),
    ]
}

pub(super) fn install_forge(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    promotions: &str,
    maven: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    let java = java_executable(request)?;
    let version = match &request.version {
        Some(version) => version.clone(),
        None => forge_version(agent, promotions, &request.minecraft)?,
    };
    validate_version(&version)?;
    install_with_installer(
        root,
        &target,
        request,
        &java,
        agent,
        &Installer {
            component: "Forge",
            version: &version,
            urls: forge_installer_urls(maven, &request.minecraft, &version),
        },
        progress,
    )
}

pub(super) fn install_neoforge(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    metadata: &str,
    maven: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    let java = java_executable(request)?;
    let version = match &request.version {
        Some(version) => version.clone(),
        None => neoforge_version(agent, metadata, &request.minecraft)?,
    };
    validate_version(&version)?;
    install_with_installer(
        root,
        &target,
        request,
        &java,
        agent,
        &Installer {
            component: "NeoForge",
            version: &version,
            urls: vec![format!(
                "{maven}/{version}/neoforge-{version}-installer.jar"
            )],
        },
        progress,
    )
}

fn install_with_installer(
    root: &Path,
    target: &Path,
    request: &InstallRequest,
    java: &Path,
    agent: &ureq::Agent,
    installer: &Installer<'_>,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    // Validate Java support before fetching artifacts or starting the installer.
    let proxy_args = crate::network::installer_proxy_args(agent)?;
    notify(
        progress,
        InstallEvent::Resolving {
            minecraft: request.minecraft.clone(),
        },
    )?;
    // Best effort: a fake or unusual Java must not block the installer's own check.
    if let Some(found) = java_major(java)
        && let Some(required) = required_java(agent, &request.minecraft)
        && found < required
    {
        return Err(Error::JavaTooOld { required, found });
    }
    notify(
        progress,
        InstallEvent::Installing {
            component: installer.component.to_owned(),
            version: installer.version.to_owned(),
        },
    )?;

    let staging = staging(root)?;
    let path = staging.path().join("installer.jar");
    let url = download_first(agent, &installer.urls, &path, progress)?;
    verify_sha1(agent, &url, &path)?;

    let work = staging.path().join("server");
    fs::create_dir(&work).map_err(|e| Error::io("create staging server", &work, e))?;
    run_installer_with_timeout(java, &path, &work, &proxy_args, progress, INSTALL_TIMEOUT)?;
    let layout = inspect_layout(&work)?;
    remove_installer_logs(&work);
    write_eula(&work)?;
    publish(&work, target)?;

    let (jar, args_file) = match layout {
        Layout::Arguments(file) => (None, Some(file)),
        Layout::LegacyJar(file) => (Some(target.join(file)), None),
    };
    Ok(InstallOutcome {
        kind: request.kind,
        directory: target.to_owned(),
        jar,
        args_file,
        minecraft: request.minecraft.clone(),
        loader: Some(installer.version.to_owned()),
        installer: None,
    })
}

/// Forge promotes one build per Minecraft version; prefer `recommended` over `latest`.
pub(super) fn forge_version(
    agent: &ureq::Agent,
    promotions: &str,
    minecraft: &str,
) -> Result<String> {
    let bytes = response_bytes(agent, promotions)?;
    let metadata: Value =
        serde_json::from_slice(&bytes).map_err(|source| Error::InvalidMetadata {
            url: promotions.to_owned(),
            source,
        })?;
    let promos = metadata
        .get("promos")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::NoCompatibleVersion {
            component: "Forge",
            requested: Some(minecraft.to_owned()),
        })?;
    for suffix in ["recommended", "latest"] {
        if let Some(version) = promos
            .get(&format!("{minecraft}-{suffix}"))
            .and_then(Value::as_str)
        {
            validate_version(version)?;
            return Ok(version.to_owned());
        }
    }
    Err(Error::NoCompatibleVersion {
        component: "Forge",
        requested: Some(minecraft.to_owned()),
    })
}

/// Map both legacy Minecraft 1.x and calendar-version releases to NeoForge builds.
pub(super) fn neoforge_version(
    agent: &ureq::Agent,
    metadata: &str,
    minecraft: &str,
) -> Result<String> {
    crate::versions::neoforge(agent, metadata, minecraft, false)?
        .into_iter()
        .next()
        .map(|v| v.version)
        .ok_or_else(|| Error::NoCompatibleVersion {
            component: "NeoForge",
            requested: Some(minecraft.to_owned()),
        })
}

fn java_executable(request: &InstallRequest) -> Result<PathBuf> {
    let java = request.java.clone().ok_or_else(|| {
        Error::invalid(
            "Java executable",
            "required to run the Forge/NeoForge installer; pass --java PATH",
        )
    })?;
    if java.as_os_str().is_empty() {
        return Err(Error::invalid("Java executable", "cannot be empty"));
    }
    if !java.is_absolute()
        && (java.components().count() != 1 || java.to_string_lossy().contains(['/', '\\', ':']))
    {
        return Err(Error::invalid(
            "Java executable",
            "use an absolute path or a bare executable name",
        ));
    }
    Ok(java)
}

fn java_major(java: &Path) -> Option<u32> {
    let output = Command::new(java)
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "java.specification.version")
            .then(|| parse_major(value.trim()))
            .flatten()
    })
}

fn parse_major(value: &str) -> Option<u32> {
    let mut parts = value.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn required_java(agent: &ureq::Agent, minecraft: &str) -> Option<u32> {
    let manifest: Value = serde_json::from_slice(&response_bytes(agent, MOJANG).ok()?).ok()?;
    let url = manifest
        .get("versions")?
        .as_array()?
        .iter()
        .find(|version| version.get("id").and_then(Value::as_str) == Some(minecraft))?
        .get("url")?
        .as_str()?;
    let detail: Value = serde_json::from_slice(&response_bytes(agent, url).ok()?).ok()?;
    detail
        .get("javaVersion")?
        .get("majorVersion")?
        .as_u64()
        .map(|major| major as u32)
}

/// What the installer produced: a modern argument file or a legacy launcher JAR.
enum Layout {
    Arguments(PathBuf),
    LegacyJar(PathBuf),
}

fn inspect_layout(work: &Path) -> Result<Layout> {
    if !work.join("libraries").is_dir() {
        return Err(Error::InstallIncomplete {
            directory: work.to_owned(),
            missing: "libraries/",
        });
    }
    if let Some(arguments) = server::argument_files(work)?.into_iter().next() {
        return Ok(Layout::Arguments(arguments));
    }
    match legacy_launcher(work) {
        Some(jar) => Ok(Layout::LegacyJar(jar)),
        None => Err(Error::InstallIncomplete {
            directory: work.to_owned(),
            missing: "launcher JAR or argument file",
        }),
    }
}

/// Pre-1.13 installs keep the launcher at the top level next to the Minecraft server JAR.
fn legacy_launcher(work: &Path) -> Option<PathBuf> {
    let mut found = None;
    for entry in fs::read_dir(work).ok()? {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let text = name.to_string_lossy().to_ascii_lowercase();
        if !text.ends_with(".jar")
            || text.contains("installer")
            || text.contains("sources")
            || text.contains("javadoc")
        {
            continue;
        }
        if !is_legacy_forge_launcher(&entry.path()) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(PathBuf::from(name));
    }
    found
}

fn is_legacy_forge_launcher(path: &Path) -> bool {
    server::manifest(path)
        .map(|manifest| {
            let text = manifest.to_ascii_lowercase();
            text.contains("serverlaunchwrapper")
                || text.contains("serverlaunchargs:")
                || text.contains("net.minecraftforge.server.servermain")
        })
        .unwrap_or(false)
}

fn download_first(
    agent: &ureq::Agent,
    urls: &[String],
    path: &Path,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<String> {
    let mut missing = None;
    for url in urls {
        match download(agent, url, path, INSTALLER_LIMIT, progress) {
            Ok(()) => return Ok(url.clone()),
            Err(
                error @ Error::Download {
                    source: ureq::Error::StatusCode(404),
                    ..
                },
            ) => missing = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(missing.unwrap_or_else(|| Error::invalid("installer", "no download URL")))
}

fn remove_installer_logs(work: &Path) {
    let Ok(entries) = fs::read_dir(work) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".jar.log")
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn run_installer_with_timeout(
    java: &Path,
    installer: &Path,
    work: &Path,
    proxy_args: &[String],
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
    timeout: Duration,
) -> Result<()> {
    let mut child = Command::new(java)
        .args(proxy_args)
        .arg("-jar")
        .arg(installer)
        .arg("--installServer")
        .current_dir(work)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::io("start installer", java, e))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::invalid("installer", "stdout was not captured"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::invalid("installer", "stderr was not captured"))?;
    let (sender, receiver) = mpsc::channel();
    let readers = [
        spawn_reader(stdout, sender.clone()),
        spawn_reader(stderr, sender.clone()),
    ];
    drop(sender);

    let started = Instant::now();
    let mut tail = VecDeque::with_capacity(TAIL);
    let result = (|| -> Result<std::process::ExitStatus> {
        let mut status = None;
        loop {
            // Check on every iteration, even while logs arrive continuously.
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Err(Error::InstallerTimeout {
                    seconds: timeout.as_secs(),
                });
            }
            if status.is_none() {
                status = child
                    .try_wait()
                    .map_err(|e| Error::io("wait for installer", java, e))?;
            }
            match receiver.recv_timeout(remaining.min(Duration::from_millis(100))) {
                Ok(line) => {
                    if tail.len() == TAIL {
                        tail.pop_front();
                    }
                    tail.push_back(line.clone());
                    notify(progress, InstallEvent::Output { line })?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(status) = status {
                        return Ok(status);
                    }
                    // Closed pipes do not imply process exit. Poll under the same deadline.
                    thread::sleep(remaining.min(Duration::from_millis(10)));
                }
            }
        }
    })();
    let status = match result {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    // Disconnection means both readers have finished and every queued line was handled.
    for reader in readers {
        let _ = reader.join();
    }
    if !status.success() {
        return Err(Error::InstallerFailed {
            code: status.code(),
            tail: tail.into_iter().collect(),
        });
    }
    Ok(())
}

/// Installer output carries ANSI colors and carriage returns; keep it readable.
fn clean_line(line: &str) -> String {
    let mut cleaned = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for code in chars.by_ref() {
                    if code.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' {
            cleaned.push(c);
        }
    }
    cleaned
}

fn spawn_reader<R: Read + Send + 'static>(
    stream: R,
    sender: mpsc::Sender<String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buffer);
                    if sender.send(clean_line(text.trim_end())).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::install::test_support::serve_responses;
    use sha1::{Digest, Sha1};
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn executable(directory: &Path, script: &str) -> PathBuf {
        let path = directory.join("fake-java");
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn jar_with_manifest(path: &Path, lines: &[&str]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut zip = ZipWriter::new(File::create(path).unwrap());
        zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .unwrap();
        for line in lines {
            writeln!(zip, "{line}").unwrap();
        }
        zip.finish().unwrap();
    }

    fn jar_with_main(path: &Path, main: &str) {
        jar_with_manifest(path, &[&format!("Main-Class: {main}")]);
    }

    #[test]
    fn proxy_properties_reach_the_installer_before_jar_arguments() {
        let temp = TempDir::new().unwrap();
        let java = executable(temp.path(), "printf '%s\\n' \"$@\" > arguments.txt");
        let agent = crate::network::ProxyConfig::new("http://127.0.0.1:7890")
            .unwrap()
            .agent(Duration::from_secs(5));
        let proxy_args = crate::network::installer_proxy_args(&agent).unwrap();
        run_installer_with_timeout(
            &java,
            Path::new("installer.jar"),
            temp.path(),
            &proxy_args,
            &mut |_| ControlFlow::Continue(()),
            Duration::from_secs(5),
        )
        .unwrap();
        let args = fs::read_to_string(temp.path().join("arguments.txt")).unwrap();
        let args: Vec<_> = args.lines().collect();
        assert_eq!(
            &args[proxy_args.len()..],
            &["-jar", "installer.jar", "--installServer"]
        );
        assert!(args[..proxy_args.len()].contains(&"-Dhttps.proxyHost=127.0.0.1"));
    }

    #[test]
    fn legacy_forge_server_main_launchers_are_supported() {
        let installer = b"forge 1.13 installer".to_vec();
        let checksum = format!("{:x}", Sha1::digest(&installer));
        let (base, worker) = serve_responses(vec![
            (200, br#"{"promos":{"1.13.2-latest":"25.0.223"}}"#.to_vec()),
            (200, installer.clone()),
            (200, checksum.into_bytes()),
        ]);
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("layout");
        jar_with_main(
            &source.join("minecraft_server.1.13.2.jar"),
            "net.minecraft.server.MinecraftServer",
        );
        jar_with_manifest(
            &source.join("forge-1.13.2-25.0.223.jar"),
            &[
                "Main-Class: net.minecraftforge.server.ServerMain",
                "ServerLaunchArgs: --gameDir . --launchTarget fmlserver",
            ],
        );
        let java = executable(
            temp.path(),
            &format!(
                "case \"$*\" in *--installServer*) cp '{}'/*.jar .; mkdir -p libraries; exit 0;; esac; exit 0",
                source.display()
            ),
        );
        let root = temp.path().join("servers");
        let request = InstallRequest::forge("demo", "1.13.2").with_java(&java);
        let outcome = install_forge(
            &root,
            &request,
            &crate::install::agent(),
            &format!("{base}/promotions"),
            &base,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        worker.join().unwrap();
        assert_eq!(outcome.args_file, None);
        assert!(
            outcome
                .jar
                .as_deref()
                .unwrap()
                .ends_with("forge-1.13.2-25.0.223.jar")
        );
        let servers = crate::discover(&root).unwrap();
        assert_eq!(servers[0].server_type, "forge");
        assert_eq!(servers[0].version.as_deref(), Some("1.13.2"));
        assert_eq!(
            servers[0].selected_jar.as_deref(),
            Some(Path::new("forge-1.13.2-25.0.223.jar"))
        );
    }

    #[test]
    fn legacy_forge_coordinates_and_top_level_launchers_are_supported() {
        let installer = b"forge installer bytes".to_vec();
        let checksum = format!("{:x}", Sha1::digest(&installer));
        let (base, worker) = serve_responses(vec![
            (
                200,
                br#"{"promos":{"1.7.10-recommended":"10.13.4.1614"}}"#.to_vec(),
            ),
            (404, Vec::new()),
            (200, installer.clone()),
            (200, checksum.into_bytes()),
        ]);
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("layout");
        jar_with_main(
            &source.join("minecraft_server.1.7.10.jar"),
            "net.minecraft.server.MinecraftServer",
        );
        jar_with_main(
            &source.join("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"),
            "net.minecraftforge.fml.relauncher.ServerLaunchWrapper",
        );
        let java = executable(
            temp.path(),
            &format!(
                "case \"$*\" in *--installServer*) cp '{}'/*.jar .; mkdir -p libraries; exit 0;; esac; exit 0",
                source.display()
            ),
        );
        let root = temp.path().join("servers");
        let request = InstallRequest::forge("demo", "1.7.10").with_java(&java);
        let outcome = install_forge(
            &root,
            &request,
            &crate::install::agent(),
            &format!("{base}/promotions"),
            &base,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        let requests = worker.join().unwrap();
        assert!(requests[1].contains("/1.7.10-10.13.4.1614/"));
        assert!(requests[2].contains("/1.7.10-10.13.4.1614-1.7.10/"));
        assert_eq!(outcome.args_file, None);
        assert!(
            outcome
                .jar
                .as_deref()
                .unwrap()
                .ends_with("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar")
        );
        let servers = crate::discover(&root).unwrap();
        assert_eq!(servers[0].server_type, "forge");
        assert_eq!(servers[0].version.as_deref(), Some("1.7.10"));
        assert_eq!(
            servers[0].selected_jar.as_deref(),
            Some(Path::new("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"))
        );
        let plan =
            crate::prepare_launch(&root, &crate::LaunchOptions::new("demo", "java")).unwrap();
        assert!(
            Path::new(&plan.args[1])
                .to_string_lossy()
                .ends_with("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar")
        );
    }

    #[test]
    fn deadline_applies_to_continuous_output_and_closed_pipes() {
        for script in [
            "while :; do printf 'still running\\n'; /bin/sleep 0.01; done",
            "exec 1>&- 2>&-; exec /bin/sleep 10",
        ] {
            let temp = TempDir::new().unwrap();
            let java = executable(temp.path(), script);
            let started = Instant::now();
            let mut lines = 0;
            let result = run_installer_with_timeout(
                &java,
                Path::new("unused.jar"),
                temp.path(),
                &[],
                &mut |_| {
                    lines += 1;
                    ControlFlow::Continue(())
                },
                Duration::from_millis(250),
            );
            assert!(
                matches!(result, Err(Error::InstallerTimeout { .. })),
                "{result:?}"
            );
            assert!(started.elapsed() < Duration::from_secs(5));
            if script.starts_with("while") {
                assert!(lines > 0);
            }
        }
    }

    #[test]
    fn completed_output_is_delivered_and_cancellation_stops_the_child() {
        let temp = TempDir::new().unwrap();
        let java = executable(temp.path(), "printf 'first\\nlast\\n'");
        let mut lines = Vec::new();
        run_installer_with_timeout(
            &java,
            Path::new("unused.jar"),
            temp.path(),
            &[],
            &mut |event| {
                if let InstallEvent::Output { line } = event {
                    lines.push(line);
                }
                ControlFlow::Continue(())
            },
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(lines, ["first", "last"]);
        let java = executable(temp.path(), "printf 'ready\\n'; exec /bin/sleep 10");
        let started = Instant::now();
        let result = run_installer_with_timeout(
            &java,
            Path::new("unused.jar"),
            temp.path(),
            &[],
            &mut |_| ControlFlow::Break(()),
            Duration::from_secs(5),
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
