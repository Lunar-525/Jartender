//! Download verified JVM distributions (Oracle, jdk.java.net, Mojang, Temurin, Zulu, Semeru,
//! GraalVM Community) without executing them.
//!
//! ```no_run
//! use jartender::java::install::{self, Distribution, Request};
//! use std::ops::ControlFlow;
//! let request = Request::new(Distribution::OpenJdk, "21")?;
//! let root = std::env::current_dir()?.join("java");
//! let installed = install::install(&root, &request, |_| ControlFlow::Continue(()))?;
//! // The host can save installed.java in a ServerConfig when desired.
//! # let _ = installed;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod archive;
mod sources;

use crate::{
    Error, Result,
    config::{require_absolute, validate_name},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Distribution {
    Oracle,
    OpenJdk,
    Mojang,
    Adoptium,
    Zulu,
    Semeru,
    Graalvm,
}
impl Distribution {
    pub fn name(self) -> &'static str {
        match self {
            Self::Oracle => "oracle",
            Self::OpenJdk => "openjdk",
            Self::Mojang => "mojang",
            Self::Adoptium => "adoptium",
            Self::Zulu => "zulu",
            Self::Semeru => "semeru",
            Self::Graalvm => "graalvm",
        }
    }
}

/// Metadata for a registered JVM distribution. Shared by CLI help and type queries.
#[derive(Debug, Clone, Serialize)]
pub struct TypeInfo {
    pub distribution: Distribution,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub source: &'static str,
}

/// A distribution supplies metadata; the installer owns download, verification and extraction.
/// Add a distribution implementation and register it in `providers()` to extend built-in support.
pub trait Provider: Send + Sync {
    fn info(&self) -> TypeInfo;
    fn versions(&self, query: &VersionQuery) -> Result<Vec<AvailableVersion>>;
    fn resolve(&self, request: &Request) -> Result<Plan>;
}

/// Built-in providers, also used to generate the CLI's accepted type values.
pub fn providers() -> &'static [&'static dyn Provider] {
    &[
        &Distribution::Oracle,
        &Distribution::OpenJdk,
        &Distribution::Mojang,
        &Distribution::Adoptium,
        &Distribution::Zulu,
        &Distribution::Semeru,
        &Distribution::Graalvm,
    ]
}

fn provider(distribution: Distribution) -> &'static dyn Provider {
    *providers()
        .iter()
        .find(|p| p.info().distribution == distribution)
        .expect("built-in distribution is registered")
}

impl Provider for Distribution {
    fn info(&self) -> TypeInfo {
        match self {
            Self::Oracle => TypeInfo {
                distribution: *self,
                name: "Oracle JDK",
                aliases: &["official"],
                source: "https://www.oracle.com/java/technologies/downloads/",
            },
            Self::OpenJdk => TypeInfo {
                distribution: *self,
                name: "OpenJDK (jdk.java.net)",
                aliases: &[],
                source: "https://jdk.java.net/archive/",
            },
            Self::Mojang => TypeInfo {
                distribution: *self,
                name: "Mojang Java Runtime",
                aliases: &["minecraft"],
                source: "https://launchermeta.mojang.com/v1/products/java-runtime/",
            },
            Self::Adoptium => TypeInfo {
                distribution: *self,
                name: "Eclipse Temurin (Adoptium)",
                aliases: &["temurin"],
                source: "https://adoptium.net/temurin/releases/",
            },
            Self::Zulu => TypeInfo {
                distribution: *self,
                name: "Azul Zulu",
                aliases: &["azul"],
                source: "https://www.azul.com/downloads/",
            },
            Self::Semeru => TypeInfo {
                distribution: *self,
                name: "IBM Semeru Open",
                aliases: &["ibm", "openj9"],
                source: "https://developer.ibm.com/languages/java/semeru-runtimes/downloads/",
            },
            Self::Graalvm => TypeInfo {
                distribution: *self,
                name: "GraalVM Community",
                aliases: &["graal", "graalvm-community"],
                source: "https://www.graalvm.org/downloads/",
            },
        }
    }
    fn versions(&self, query: &VersionQuery) -> Result<Vec<AvailableVersion>> {
        if *self != query.distribution {
            return Err(Error::invalid(
                "Java provider",
                "distribution does not match query",
            ));
        }
        query.validate()?;
        sources::versions(
            query,
            &query.proxy.agent(Duration::from_secs(600)),
            &sources::Sources::default(),
        )
    }
    fn resolve(&self, request: &Request) -> Result<Plan> {
        if *self != request.distribution {
            return Err(Error::invalid(
                "Java provider",
                "distribution does not match request",
            ));
        }
        request.validate()?;
        sources::resolve(
            request,
            &request.proxy.agent(Duration::from_secs(600)),
            &sources::Sources::default(),
        )
    }
}

/// Query packages for one platform. No JDK is downloaded or executed.
#[derive(Debug, Clone)]
pub struct VersionQuery {
    pub distribution: Distribution,
    pub major: Option<u32>,
    pub os: Os,
    pub arch: Arch,
    pub proxy: crate::network::ProxyConfig,
}
impl VersionQuery {
    pub fn new(distribution: Distribution) -> Result<Self> {
        Ok(Self {
            distribution,
            major: None,
            os: Os::current()?,
            arch: Arch::current()?,
            proxy: Default::default(),
        })
    }
    fn validate(&self) -> Result<()> {
        if self.major == Some(0) {
            return Err(Error::invalid("Java major", "must be greater than zero"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AvailableVersion {
    pub distribution: Distribution,
    pub version: String,
    pub os: Os,
    pub arch: Arch,
    /// Newest entry from this source; Oracle's mutable latest URL advertises a major instead of an
    /// exact version.
    pub latest: bool,
    pub url: String,
}

/// Newest first, limited to public packages for the requested platform.
/// Oracle without a major lists current downloads; with a major, its public archive.
pub fn versions(query: &VersionQuery) -> Result<Vec<AvailableVersion>> {
    provider(query.distribution).versions(query)
}

/// Registered JVM types with publicly downloadable packages for this major and platform.
/// Network/metadata errors are returned rather than silently reporting an incomplete list.
pub fn types_for_major(major: u32, os: Os, arch: Arch) -> Result<Vec<TypeInfo>> {
    types_for_major_with_proxy(major, os, arch, &Default::default())
}

/// Query installable JVM types with an explicit proxy policy.
pub fn types_for_major_with_proxy(
    major: u32,
    os: Os,
    arch: Arch,
    proxy: &crate::network::ProxyConfig,
) -> Result<Vec<TypeInfo>> {
    types_for_major_with(major, os, arch, providers(), proxy)
}

fn types_for_major_with(
    major: u32,
    os: Os,
    arch: Arch,
    providers: &[&dyn Provider],
    proxy: &crate::network::ProxyConfig,
) -> Result<Vec<TypeInfo>> {
    if major == 0 {
        return Err(Error::invalid("Java major", "must be greater than zero"));
    }
    let mut rows = Vec::new();
    for provider in providers {
        let info = provider.info();
        let query = VersionQuery {
            distribution: info.distribution,
            major: Some(major),
            os,
            arch,
            proxy: proxy.clone(),
        };
        if !provider.versions(&query)?.is_empty() {
            rows.push(info);
        }
    }
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Linux,
    Macos,
    Windows,
}
impl Os {
    pub fn current() -> Result<Self> {
        match std::env::consts::OS {
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::Macos),
            "windows" => Ok(Self::Windows),
            _ => Err(Error::invalid(
                "Java platform",
                "supported systems: Linux, macOS, Windows",
            )),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    X64,
    Aarch64,
}
impl Arch {
    pub fn current() -> Result<Self> {
        match std::env::consts::ARCH {
            "x86_64" => Ok(Self::X64),
            "aarch64" => Ok(Self::Aarch64),
            _ => Err(Error::invalid(
                "Java architecture",
                "supported architectures: x86_64, aarch64",
            )),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Aarch64 => "aarch64",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Request {
    pub distribution: Distribution,
    pub version: String,
    pub name: String,
    pub os: Os,
    pub arch: Arch,
    pub proxy: crate::network::ProxyConfig,
}
impl Request {
    pub fn new(distribution: Distribution, version: impl Into<String>) -> Result<Self> {
        let os = Os::current()?;
        let arch = Arch::current()?;
        let version = version.into();
        let request = Self {
            distribution,
            name: format!(
                "{}-{}-{}-{}",
                distribution.name(),
                version,
                os.name(),
                arch.name()
            ),
            version,
            os,
            arch,
            proxy: Default::default(),
        };
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<()> {
        if self
            .version
            .split('.')
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            || super::major_version(&self.version).is_none()
        {
            return Err(Error::invalid(
                "Java version",
                "use a numeric major or exact version, e.g. 25 or 21.0.2",
            ));
        }
        validate_name(&self.name)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub distribution: Distribution,
    /// Version advertised by the URL; Oracle latest URLs advertise only the major.
    pub version: String,
    pub os: Os,
    pub arch: Arch,
    pub url: String,
    /// Archive SHA-256, or the manifest SHA-1 when `files` is non-empty (Mojang).
    pub sha256: String,
    /// Non-empty for file-based runtimes; empty for single archives.
    pub files: Vec<PlanFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanFile {
    pub path: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
    pub executable: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub distribution: Distribution,
    pub version: String,
    pub os: Os,
    pub arch: Arch,
    pub directory: PathBuf,
    pub home: PathBuf,
    pub java: PathBuf,
}
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Event {
    Resolving,
    Downloading { url: String },
    Progress { downloaded: u64, total: Option<u64> },
    Extracting,
    Unpacking { entries: usize },
}

#[cfg(test)]
fn agent() -> ureq::Agent {
    crate::network::ProxyConfig::Direct.agent(Duration::from_secs(600))
}
fn notify(progress: &mut impl FnMut(Event) -> ControlFlow<()>, event: Event) -> Result<()> {
    match progress(event) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(()) => Err(Error::Cancelled),
    }
}

/// Resolve only official download links and fetch their mandatory SHA-256 sidecars.
pub fn resolve(request: &Request) -> Result<Plan> {
    provider(request.distribution).resolve(request)
}

/// Install below an absolute root, into a new directory. Never executes the JDK,
/// changes JAVA_HOME/PATH, or writes a server's launch configuration.
pub fn install(
    root: &Path,
    request: &Request,
    mut progress: impl FnMut(Event) -> ControlFlow<()>,
) -> Result<Outcome> {
    let agent = request.proxy.agent(Duration::from_secs(600));
    if request.distribution == Distribution::Mojang {
        return install_mojang_using(
            root,
            request,
            &agent,
            &sources::Sources::default(),
            &mut progress,
        );
    }
    install_using(root, request, &agent, &mut progress, || resolve(request))
}

/// Mojang ships a file manifest instead of a single archive; download every file.
fn install_mojang_using(
    root: &Path,
    request: &Request,
    agent: &ureq::Agent,
    sources: &sources::Sources<'_>,
    progress: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<Outcome> {
    require_absolute(root, "Java installation root")?;
    request.validate()?;
    let target = root.join(&request.name);
    if target
        .try_exists()
        .map_err(|e| Error::io("check Java target", &target, e))?
    {
        return Err(Error::io(
            "create Java installation",
            &target,
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "choose another installation name",
            ),
        ));
    }
    notify(progress, Event::Resolving)?;
    let plan = sources::resolve(request, agent, sources)?;
    if plan.files.is_empty() {
        return Err(Error::invalid("Mojang runtime", "manifest lists no files"));
    }
    notify(
        progress,
        Event::Downloading {
            url: plan.url.clone(),
        },
    )?;
    fs::create_dir_all(root).map_err(|e| Error::io("create Java root", root, e))?;
    let staging = tempfile::Builder::new()
        .prefix(".jartender-java-")
        .tempdir_in(root)
        .map_err(|e| Error::io("stage Java", root, e))?;
    let unpacked = staging.path().join("unpacked");
    fs::create_dir(&unpacked).map_err(|e| Error::io("create JDK staging", &unpacked, e))?;
    let total: u64 = plan.files.iter().map(|file| file.size).sum();
    let next = AtomicUsize::new(0);
    let finished = AtomicU64::new(0);
    let failure = Mutex::new(None::<Error>);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= plan.files.len() || failure.lock().unwrap().is_some() {
                        break;
                    }
                    match download_mojang_file(agent, &unpacked, &plan.files[index]) {
                        Ok(()) => {
                            finished.fetch_add(plan.files[index].size, Ordering::Relaxed);
                        }
                        Err(error) => {
                            *failure.lock().unwrap() = Some(error);
                            break;
                        }
                    }
                }
            });
        }
    });
    if let Some(error) = failure.into_inner().unwrap() {
        return Err(error);
    }
    if progress(Event::Progress {
        downloaded: finished.load(Ordering::Relaxed),
        total: Some(total),
    })
    .is_break()
    {
        return Err(Error::Cancelled);
    }
    notify(
        progress,
        Event::Unpacking {
            entries: plan.files.len(),
        },
    )?;
    notify(progress, Event::Extracting)?;
    let home = if request.os == Os::Macos {
        unpacked.join("jre.bundle/Contents/Home")
    } else {
        unpacked.clone()
    };
    let release = read_release(&home)?;
    let version = release
        .get("JAVA_VERSION")
        .ok_or_else(|| Error::invalid("JDK release", "missing JAVA_VERSION"))?
        .clone();
    let compatible = if request.version.contains('.') {
        version == request.version
    } else {
        super::major_version(&version) == super::major_version(&request.version)
    };
    // Mojang selects the platform in its runtime manifest; its JRE release files
    // do not consistently carry OS_NAME/OS_ARCH.
    if !compatible {
        return Err(Error::invalid(
            "JDK release",
            format!("version mismatch: Java {version}, want {}", request.version),
        ));
    }
    let executable = if request.os == Os::Windows {
        "java.exe"
    } else {
        "java"
    };
    if !home.join("bin").join(executable).is_file() {
        return Err(Error::invalid(
            "Mojang runtime",
            format!("missing bin/{executable}"),
        ));
    }
    let relative_home = home
        .strip_prefix(&unpacked)
        .map_err(|e| Error::invalid("JDK home", e.to_string()))?
        .to_owned();
    crate::install::publish(&unpacked, &target)?;
    let home = target.join(relative_home);
    Ok(Outcome {
        distribution: request.distribution,
        version,
        os: request.os,
        arch: request.arch,
        java: home.join("bin").join(executable),
        home,
        directory: target,
    })
}

fn download_mojang_file(agent: &ureq::Agent, unpacked: &Path, file: &PlanFile) -> Result<()> {
    let relative = Path::new(&file.path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(Error::invalid(
            "Mojang runtime",
            format!("unsafe file path {}", relative.display()),
        ));
    }
    let path = unpacked.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io("create JDK directory", parent, e))?;
    }
    crate::install::download(agent, &file.url, &path, 512 * 1024 * 1024, &mut |_| {
        ControlFlow::Continue(())
    })?;
    let actual = sha1_file(&path)?;
    if !actual.eq_ignore_ascii_case(&file.sha1) {
        return Err(Error::ChecksumMismatch {
            url: file.url.clone(),
            expected: file.sha1.clone(),
            actual,
        });
    }
    #[cfg(unix)]
    if file.executable {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::io("set JDK executable", &path, e))?;
    }
    Ok(())
}

fn sha1_file(path: &Path) -> Result<String> {
    use sha1::{Digest, Sha1};
    let mut file = fs::File::open(path).map_err(|e| Error::io("read JDK file", path, e))?;
    let mut hash = Sha1::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| Error::io("read JDK file", path, e))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
fn install_with(
    root: &Path,
    request: &Request,
    agent: &ureq::Agent,
    sources: &sources::Sources<'_>,
    progress: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<Outcome> {
    install_using(root, request, agent, progress, || {
        sources::resolve(request, agent, sources)
    })
}

fn install_using(
    root: &Path,
    request: &Request,
    agent: &ureq::Agent,
    progress: &mut impl FnMut(Event) -> ControlFlow<()>,
    resolve_plan: impl FnOnce() -> Result<Plan>,
) -> Result<Outcome> {
    require_absolute(root, "Java installation root")?;
    request.validate()?;
    let target = root.join(&request.name);
    if target
        .try_exists()
        .map_err(|e| Error::io("check Java target", &target, e))?
    {
        return Err(Error::io(
            "create Java installation",
            &target,
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "choose another installation name",
            ),
        ));
    }
    notify(progress, Event::Resolving)?;
    let plan = resolve_plan()?;
    notify(
        progress,
        Event::Downloading {
            url: plan.url.clone(),
        },
    )?;
    fs::create_dir_all(root).map_err(|e| Error::io("create Java root", root, e))?;
    let staging = tempfile::Builder::new()
        .prefix(".jartender-java-")
        .tempdir_in(root)
        .map_err(|e| Error::io("stage Java", root, e))?;
    let file = staging.path().join("jdk.archive");
    crate::install::download(agent, &plan.url, &file, 1024 * 1024 * 1024, &mut |event| {
        if let crate::install::InstallEvent::Progress { downloaded, total } = event {
            progress(Event::Progress { downloaded, total })
        } else {
            ControlFlow::Continue(())
        }
    })?;
    let mut input = fs::File::open(&file).map_err(|e| Error::io("read JDK archive", &file, e))?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|e| Error::io("read JDK archive", &file, e))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let actual = format!("{:x}", hash.finalize());
    if !actual.eq_ignore_ascii_case(&plan.sha256) {
        return Err(Error::ChecksumMismatch {
            url: plan.url,
            expected: plan.sha256,
            actual,
        });
    }
    notify(progress, Event::Extracting)?;
    let extracted = staging.path().join("unpacked");
    fs::create_dir(&extracted).map_err(|e| Error::io("create JDK staging", &extracted, e))?;
    archive::extract(&file, &extracted, plan.url.ends_with(".zip"), progress)?;
    let entries = fs::read_dir(&extracted)
        .map_err(|e| Error::io("inspect JDK", &extracted, e))?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| Error::io("inspect JDK", &extracted, e))?;
    if entries.len() != 1
        || !entries[0]
            .file_type()
            .map_err(|e| Error::io("inspect JDK package", &extracted, e))?
            .is_dir()
    {
        return Err(Error::InvalidArchive {
            path: file,
            reason: "expected exactly one JDK directory".into(),
        });
    }
    let package = entries[0].path();
    archive::validate_links(&package)?;
    let home = if request.os == Os::Macos {
        package.join("Contents/Home")
    } else {
        package.clone()
    };
    let release = read_release(&home)?;
    let version = release
        .get("JAVA_VERSION")
        .ok_or_else(|| Error::invalid("JDK release", "missing JAVA_VERSION"))?
        .clone();
    let compatible = if request.version.contains('.') {
        version == request.version
    } else {
        super::major_version(&version) == super::major_version(&request.version)
    };
    let os = release.get("OS_NAME").map(String::as_str);
    let arch = release.get("OS_ARCH").map(String::as_str);
    if !compatible
        || !matches!(
            (request.os, os),
            (Os::Macos, Some("Darwin" | "macOS"))
                | (Os::Linux, Some("Linux"))
                | (Os::Windows, Some("Windows"))
        )
        || !matches!(
            (request.arch, arch),
            (Arch::X64, Some("x86_64" | "amd64" | "x64"))
                | (Arch::Aarch64, Some("aarch64" | "arm64"))
        )
    {
        return Err(Error::invalid(
            "JDK release",
            format!("version/platform mismatch: Java {version}, {os:?}, {arch:?}"),
        ));
    }
    let executable = if request.os == Os::Windows {
        "java.exe"
    } else {
        "java"
    };
    for name in [
        executable,
        if request.os == Os::Windows {
            "javac.exe"
        } else {
            "javac"
        },
    ] {
        if !home.join("bin").join(name).is_file() {
            return Err(Error::invalid("JDK archive", format!("missing bin/{name}")));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = home.join("bin").join(name);
            if request.os != Os::Windows
                && fs::metadata(&path)
                    .map_err(|e| Error::io("inspect JDK executable", &path, e))?
                    .permissions()
                    .mode()
                    & 0o111
                    == 0
            {
                return Err(Error::invalid(
                    "JDK archive",
                    format!("bin/{name} is not executable"),
                ));
            }
        }
    }
    let relative_home = home
        .strip_prefix(&package)
        .map_err(|e| Error::invalid("JDK home", e.to_string()))?
        .to_owned();
    crate::install::publish(&package, &target)?;
    let home = target.join(relative_home);
    Ok(Outcome {
        distribution: request.distribution,
        version,
        os: request.os,
        arch: request.arch,
        java: home.join("bin").join(executable),
        home,
        directory: target,
    })
}

fn read_release(home: &Path) -> Result<HashMap<String, String>> {
    let path = home.join("release");
    let file = fs::File::open(&path).map_err(|e| Error::io("read JDK release", &path, e))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io("read JDK release", &path, e))?;
    if bytes.len() > 64 * 1024 {
        return Err(Error::invalid("JDK release", "exceeds 64 KiB"));
    }
    let mut result = HashMap::new();
    java_properties::PropertiesIter::new_with_encoding(bytes.as_slice(), encoding_rs::UTF_8)
        .read_into(|key, value| {
            result.insert(key, value.trim_matches('"').to_owned());
        })
        .map_err(|e| Error::invalid("JDK release", e.to_string()))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    fn major_type_query_filters_registry_and_propagates_provider_errors() {
        struct Fake {
            distribution: Distribution,
            available: bool,
            fails: bool,
        }
        impl Provider for Fake {
            fn info(&self) -> TypeInfo {
                self.distribution.info()
            }
            fn resolve(&self, _: &Request) -> Result<Plan> {
                unreachable!()
            }
            fn versions(&self, query: &VersionQuery) -> Result<Vec<AvailableVersion>> {
                assert_eq!(query.major, Some(21));
                assert_eq!(query.os, Os::Linux);
                assert_eq!(query.arch, Arch::X64);
                assert_eq!(query.distribution, self.distribution);
                if self.fails {
                    return Err(Error::invalid("fixture", "metadata unavailable"));
                }
                Ok(if self.available {
                    vec![AvailableVersion {
                        distribution: self.distribution,
                        version: "21.0.2".into(),
                        os: query.os,
                        arch: query.arch,
                        latest: false,
                        url: "fixture".into(),
                    }]
                } else {
                    vec![]
                })
            }
        }
        let oracle = Fake {
            distribution: Distribution::Oracle,
            available: false,
            fails: false,
        };
        let openjdk = Fake {
            distribution: Distribution::OpenJdk,
            available: true,
            fails: false,
        };
        let rows = types_for_major_with(
            21,
            Os::Linux,
            Arch::X64,
            &[&oracle, &openjdk],
            &Default::default(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].distribution, Distribution::OpenJdk);
        assert!(
            types_for_major_with(0, Os::Linux, Arch::X64, &[&openjdk], &Default::default())
                .is_err()
        );
        let failing = Fake {
            fails: true,
            ..oracle
        };
        assert!(
            types_for_major_with(
                21,
                Os::Linux,
                Arch::X64,
                &[&openjdk, &failing],
                &Default::default()
            )
            .is_err()
        );
        assert!(
            types_for_major_with(21, Os::Linux, Arch::X64, &[], &Default::default())
                .unwrap()
                .is_empty()
        );
    }

    use super::*;
    use crate::install::test_support::serve;
    use std::io::{Cursor, Write};
    use tempfile::TempDir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn request(distribution: Distribution, version: &str, os: Os) -> Request {
        Request {
            distribution,
            version: version.into(),
            name: "test-jdk".into(),
            os,
            arch: Arch::X64,
            proxy: Default::default(),
        }
    }
    fn package(os: Os, arch: Arch, version: &str) -> Vec<u8> {
        let home = if os == Os::Macos {
            "jdk-test/Contents/Home"
        } else {
            "jdk-test"
        };
        let release = format!(
            "JAVA_VERSION=\"{version}\"\nOS_NAME=\"{}\"\nOS_ARCH=\"{}\"\n",
            match os {
                Os::Linux => "Linux",
                Os::Macos => "Darwin",
                Os::Windows => "Windows",
            },
            match arch {
                Arch::X64 => "x86_64",
                Arch::Aarch64 => "aarch64",
            }
        );
        let suffix = if os == Os::Windows { ".exe" } else { "" };
        let files = [
            (format!("{home}/release"), release.as_bytes()),
            (
                format!("{home}/bin/java{suffix}"),
                b"This test JDK must never execute".as_slice(),
            ),
            (
                format!("{home}/bin/javac{suffix}"),
                b"Not executed".as_slice(),
            ),
        ];
        if os == Os::Windows {
            let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
            for (name, data) in files {
                zip.start_file(name, SimpleFileOptions::default().unix_permissions(0o755))
                    .unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap().into_inner()
        } else {
            let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            let mut tar = tar::Builder::new(encoder);
            for (name, data) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append_data(&mut header, name, data).unwrap();
            }
            tar.into_inner().unwrap().finish().unwrap()
        }
    }
    #[test]
    fn jdk_metadata_checksum_and_archive_share_the_configured_proxy() {
        let request = request(Distribution::OpenJdk, "21", Os::Linux);
        let archive = package(Os::Linux, Arch::X64, "21.0.2");
        let digest = format!("{:x}", Sha256::digest(&archive));
        let url = "http://jdk.invalid/openjdk-21.0.2_linux-x64_bin.tar.gz";
        let (proxy, worker) = crate::network::tests::serve_proxy(vec![
            format!("<a href='{url}'>JDK</a>").into_bytes(),
            digest.into_bytes(),
            archive,
        ]);
        let sources = sources::Sources {
            openjdk_page: "http://jdk.invalid/archive/",
            openjdk_prefix: "http://jdk.invalid/",
            ..sources::Sources::default()
        };
        let temp = TempDir::new().unwrap();
        let installed = install_with(
            temp.path(),
            &request,
            &proxy.agent(Duration::from_secs(5)),
            &sources,
            &mut |_| ControlFlow::Continue(()),
        )
        .unwrap();
        assert_eq!(installed.version, "21.0.2");
        assert!(installed.java.is_file());
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(
            requests
                .iter()
                .all(|r| r.starts_with("CONNECT jdk.invalid:80 "))
        );
        assert!(requests[1].contains(".tar.gz.sha256 "));
        assert!(requests[2].contains(".tar.gz "));
    }

    fn fixture(
        root: &Path,
        request: &Request,
        data: Vec<u8>,
        checksum: Option<&str>,
        progress: &mut impl FnMut(Event) -> ControlFlow<()>,
    ) -> Result<Outcome> {
        let digest = checksum
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{:x}", Sha256::digest(&data)));
        let mut bodies = vec![digest.as_bytes().to_vec()];
        if digest.len() == 64 {
            bodies.push(data);
        }
        let (download, download_worker) = serve(bodies);
        let filename = format!(
            "{}-{}_{}-{}_bin.{}",
            if request.distribution == Distribution::Oracle {
                "jdk"
            } else {
                "openjdk"
            },
            if request.distribution == Distribution::Oracle {
                request.version.as_str()
            } else {
                "25.0.1"
            },
            request.os.name(),
            request.arch.name(),
            if request.os == Os::Windows {
                "zip"
            } else {
                "tar.gz"
            }
        );
        let url = format!("{download}/latest/{filename}");
        let html = format!("<a href=\"{url}\">official</a><a href=\"{url}.sha256\">sha256</a>");
        let (page, page_worker) = serve(vec![html.into_bytes()]);
        let sources = sources::Sources {
            oracle_page: &page,
            oracle_archive: &page,
            openjdk_page: &page,
            oracle_prefix: &download,
            openjdk_prefix: &download,
            ..sources::Sources::default()
        };
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .timeout_global(Some(Duration::from_secs(5)))
            .build()
            .into();
        let result = install_with(root, request, &agent, &sources, progress);
        page_worker.join().unwrap();
        download_worker.join().unwrap();
        result
    }

    #[test]
    fn both_distributions_and_all_archive_layouts_install_without_executing_java() {
        for distribution in [Distribution::Oracle, Distribution::OpenJdk] {
            for os in [Os::Linux, Os::Macos, Os::Windows] {
                let root = TempDir::new().unwrap();
                let request = request(distribution, "25", os);
                let outcome = fixture(
                    root.path(),
                    &request,
                    package(os, Arch::X64, "25.0.1"),
                    None,
                    &mut |_| ControlFlow::Continue(()),
                )
                .unwrap();
                assert_eq!(outcome.version, "25.0.1");
                assert!(outcome.java.is_file());
                assert!(outcome.home.join("release").is_file());
                assert_eq!(outcome.directory, root.path().join("test-jdk"));
                assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
                assert!(!outcome.home.join("eula.txt").exists());
                assert!(!outcome.home.join("jartender.json").exists());
            }
        }
    }

    #[test]
    fn oracle_exact_version_and_mandatory_checksums_are_verified() {
        let root = TempDir::new().unwrap();
        let request = request(Distribution::Oracle, "25.0.1", Os::Linux);
        assert_eq!(
            fixture(
                root.path(),
                &request,
                package(Os::Linux, Arch::X64, "25.0.1"),
                None,
                &mut |_| ControlFlow::Continue(())
            )
            .unwrap()
            .version,
            "25.0.1"
        );
        for checksum in ["invalid".to_owned(), "0".repeat(64)] {
            let root = TempDir::new().unwrap();
            let error = fixture(
                root.path(),
                &request,
                package(Os::Linux, Arch::X64, "25.0.1"),
                Some(&checksum),
                &mut |_| ControlFlow::Continue(()),
            )
            .unwrap_err();
            assert!(matches!(
                error,
                Error::VersionMetadata { .. } | Error::ChecksumMismatch { .. }
            ));
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn mismatching_release_metadata_and_cancellation_clean_staging() {
        for scenario in [
            "version",
            "architecture",
            "download-cancel",
            "unpack-cancel",
        ] {
            let root = TempDir::new().unwrap();
            let request = request(Distribution::OpenJdk, "25", Os::Linux);
            let data = package(
                Os::Linux,
                if scenario == "architecture" {
                    Arch::Aarch64
                } else {
                    Arch::X64
                },
                if scenario == "version" {
                    "26.0.1"
                } else {
                    "25.0.1"
                },
            );
            let error = fixture(root.path(), &request, data, None, &mut |event| {
                if (scenario == "download-cancel" && matches!(event, Event::Progress { .. }))
                    || (scenario == "unpack-cancel" && matches!(event, Event::Unpacking { .. }))
                {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap_err();
            if scenario.ends_with("cancel") {
                assert!(matches!(error, Error::Cancelled));
            } else {
                assert!(matches!(error, Error::InvalidInput { .. }));
            }
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn malformed_traversing_oversized_and_escaping_archives_are_rejected() {
        for scenario in [
            "bad-gzip",
            "traversal",
            "symlink",
            "expanded-size",
            "zip-traversal",
        ] {
            let os = if scenario == "zip-traversal" {
                Os::Windows
            } else {
                Os::Linux
            };
            let data = if scenario == "bad-gzip" {
                b"not gzip".to_vec()
            } else if scenario == "zip-traversal" {
                let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
                zip.start_file("../escaped", SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"bad").unwrap();
                zip.finish().unwrap().into_inner()
            } else {
                let mut header = tar::Header::new_gnu();
                header.set_path("jdk-test/file").unwrap();
                header.set_size(0);
                header.set_mode(0o644);
                if scenario == "traversal" {
                    let name = b"../escaped";
                    header.as_mut_bytes()[..name.len()].copy_from_slice(name);
                    header.as_mut_bytes()[name.len()] = 0;
                }
                if scenario == "symlink" {
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_link_name("../../outside").unwrap();
                }
                if scenario == "expanded-size" {
                    header.set_size(2 * 1024 * 1024 * 1024 + 1);
                }
                header.set_cksum();
                let mut gzip =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                gzip.write_all(header.as_bytes()).unwrap();
                gzip.write_all(&[0; 1024]).unwrap();
                gzip.finish().unwrap()
            };
            let root = TempDir::new().unwrap();
            let request = request(Distribution::OpenJdk, "25", os);
            let error = fixture(root.path(), &request, data, None, &mut |_| {
                ControlFlow::Continue(())
            })
            .unwrap_err();
            assert!(
                matches!(error, Error::InvalidArchive { .. }),
                "{scenario}: {error}"
            );
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn invalid_requests_existing_targets_and_initial_cancellation_do_not_use_network() {
        let root = TempDir::new().unwrap();
        let mut request = request(Distribution::OpenJdk, "25", Os::Linux);
        let agent = agent();
        let sources = sources::Sources {
            oracle_page: "http://127.0.0.1:1",
            ..sources::Sources::default()
        };
        request.version = "../bad".into();
        assert!(
            install_with(root.path(), &request, &agent, &sources, &mut |_| {
                ControlFlow::Continue(())
            })
            .is_err()
        );
        request.version = "25".into();
        assert!(matches!(
            install_with(root.path(), &request, &agent, &sources, &mut |_| {
                ControlFlow::Break(())
            }),
            Err(Error::Cancelled)
        ));
        fs::create_dir(root.path().join(&request.name)).unwrap();
        fs::write(root.path().join(&request.name).join("keep"), "untouched").unwrap();
        assert!(
            install_with(root.path(), &request, &agent, &sources, &mut |_| {
                ControlFlow::Continue(())
            })
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(root.path().join(&request.name).join("keep")).unwrap(),
            "untouched"
        );
    }
}
