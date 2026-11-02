//! Shared verified download and publication for standalone server JARs.
use super::{
    InstallEvent, InstallOutcome, InstallRequest, download, notify, publish, staging, write_eula,
};
use crate::{Error, Result, server};
use md5::Md5;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, ops::ControlFlow, path::Path};

const LIMIT: u64 = 256 * 1024 * 1024;

pub(super) enum Checksum<'a> {
    /// Some older official GitHub releases publish no checksum.
    Unpublished,
    Sha256(&'a str),
    Md5(&'a str),
}

pub(super) struct Artifact<'a> {
    pub version: &'a str,
    pub url: &'a str,
    pub size: Option<u64>,
    pub checksum: Checksum<'a>,
}

pub(super) fn install(
    root: &Path,
    target: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    artifact: &Artifact<'_>,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let expected = match artifact.checksum {
        Checksum::Unpublished => None,
        Checksum::Sha256(value) => Some((value, 64, "SHA-256")),
        Checksum::Md5(value) => Some((value, 32, "MD5")),
    };
    if let Some((expected, length, algorithm)) = expected
        && (expected.len() != length || !expected.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(Error::VersionMetadata {
            url: artifact.url.to_owned(),
            reason: format!("missing or invalid {algorithm} checksum"),
        });
    }
    if artifact.size.is_some_and(|size| size > LIMIT) {
        return Err(Error::DownloadTooLarge { limit: LIMIT });
    }
    if expected.is_none() {
        notify(
            progress,
            InstallEvent::ChecksumUnavailable {
                component: request.kind.name().into(),
                version: artifact.version.into(),
            },
        )?;
    }
    notify(
        progress,
        InstallEvent::DownloadingServer {
            component: request.kind.name().into(),
            version: artifact.version.into(),
        },
    )?;
    let staging = staging(root)?;
    let work = staging.path().join("server");
    fs::create_dir(&work).map_err(|e| Error::io("create staging server", &work, e))?;
    let jar = work.join("server.jar");
    download(agent, artifact.url, &jar, LIMIT, progress)?;
    if let Some(expected_size) = artifact.size {
        let size = fs::metadata(&jar)
            .map_err(|e| Error::io("read download size", &jar, e))?
            .len();
        if size != expected_size {
            return Err(Error::InvalidJar {
                path: jar,
                reason: format!("expected {expected_size} bytes, got {size}"),
            });
        }
    }
    let actual = match artifact.checksum {
        Checksum::Unpublished => None,
        Checksum::Sha256(_) => Some(hash::<Sha256>(&jar)?),
        Checksum::Md5(_) => Some(hash::<Md5>(&jar)?),
    };
    if let (Some(actual), Some((expected, _, _))) = (actual, expected)
        && !actual.eq_ignore_ascii_case(expected)
    {
        return Err(Error::ChecksumMismatch {
            url: artifact.url.into(),
            expected: expected.into(),
            actual,
        });
    }
    if !server::manifest(&jar)?
        .lines()
        .any(|line| line.starts_with("Main-Class:"))
    {
        return Err(Error::InvalidJar {
            path: jar,
            reason: "missing Main-Class".into(),
        });
    }
    if matches!(
        request.kind,
        super::InstallKind::Arclight
            | super::InstallKind::Mohist
            | super::InstallKind::Banner
            | super::InstallKind::CatServer
    ) {
        let detected = server::identify(&jar);
        if detected != request.kind.name().to_ascii_lowercase() {
            return Err(Error::InvalidJar {
                path: jar,
                reason: format!("expected {}, detected {detected}", request.kind.name()),
            });
        }
        if let Some(version) =
            crate::details::version(&work, Some(Path::new("server.jar")), &[], detected)
            && version != request.minecraft
        {
            return Err(Error::InvalidJar {
                path: jar,
                reason: format!(
                    "expected Minecraft {}, detected {version}",
                    request.minecraft
                ),
            });
        }
    }
    write_eula(&work)?;
    publish(&work, target)?;
    Ok(InstallOutcome {
        kind: request.kind,
        directory: target.to_owned(),
        jar: Some(target.join("server.jar")),
        args_file: None,
        minecraft: request.minecraft.clone(),
        loader: Some(artifact.version.into()),
        installer: None,
    })
}

fn hash<D: Digest>(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| Error::io("read download", path, e))?;
    let mut hash = D::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|e| Error::io("read download", path, e))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
