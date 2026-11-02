//! Fabric server launcher install: a plain download, no Java required.
use super::{
    InstallEvent, InstallKind, InstallOutcome, InstallRequest, download, metadata, notify, prepare,
    publish, staging, validate_version, write_eula,
};
use crate::{Error, Result, server};
use serde::Deserialize;
use std::ops::ControlFlow;
use std::path::Path;

pub(super) const META: &str = "https://meta.fabricmc.net/v2/versions";
const LIMIT: u64 = 256 * 1024 * 1024;

/// Fabric-only request kept for hosts that already use [`install_fabric`].
#[derive(Debug, Clone)]
pub struct FabricRequest {
    pub minecraft: String,
    pub name: String,
    pub loader: Option<String>,
    pub installer: Option<String>,
    pub proxy: crate::network::ProxyConfig,
}

impl FabricRequest {
    pub fn new(name: impl Into<String>, minecraft: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            minecraft: minecraft.into(),
            loader: None,
            installer: None,
            proxy: Default::default(),
        }
    }
}

impl From<&FabricRequest> for InstallRequest {
    fn from(value: &FabricRequest) -> Self {
        Self {
            kind: InstallKind::Fabric,
            name: value.name.clone(),
            minecraft: value.minecraft.clone(),
            version: value.loader.clone(),
            installer: value.installer.clone(),
            java: None,
            github_token: None,
            proxy: value.proxy.clone(),
        }
    }
}

/// Download the Fabric launcher into a new server directory. Never runs Java or accepts the EULA.
pub fn install_fabric(
    root: &Path,
    request: &FabricRequest,
    progress: impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    super::install(root, &request.into(), progress)
}

#[derive(Deserialize)]
struct Version {
    version: String,
    #[serde(default)]
    stable: bool,
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

pub(super) fn install(
    root: &Path,
    request: &InstallRequest,
    agent: &ureq::Agent,
    meta: &str,
    progress: &mut impl FnMut(InstallEvent) -> ControlFlow<()>,
) -> Result<InstallOutcome> {
    let target = prepare(root, request)?;
    notify(
        progress,
        InstallEvent::Resolving {
            minecraft: request.minecraft.clone(),
        },
    )?;
    let loaders: Vec<_> = crate::versions::fabric(agent, meta, &request.minecraft, true)?
        .into_iter()
        .map(|v| Version {
            version: v.version,
            stable: v.stable,
        })
        .collect();
    let loader = choose(&loaders, request.version.as_deref(), "Fabric Loader")?;
    let installers: Vec<Version> = metadata(agent, &format!("{meta}/installer"))?;
    let installer = choose(
        &installers,
        request.installer.as_deref(),
        "Fabric installer",
    )?;
    validate_version(&loader)?;
    validate_version(&installer)?;
    notify(
        progress,
        InstallEvent::Downloading {
            loader: loader.clone(),
            installer: installer.clone(),
        },
    )?;

    let staging = staging(root)?;
    let work = staging.path().join("server");
    std::fs::create_dir(&work).map_err(|e| Error::io("create staging server", &work, e))?;
    let jar = work.join("server.jar");
    let url = format!(
        "{meta}/loader/{}/{loader}/{installer}/server/jar",
        request.minecraft
    );
    download(agent, &url, &jar, LIMIT, progress)?;
    let manifest = server::manifest(&jar)?;
    if !manifest.lines().any(|line| line.starts_with("Main-Class:")) {
        return Err(Error::InvalidJar {
            path: jar,
            reason: "missing Main-Class".into(),
        });
    }
    write_eula(&work)?;
    publish(&work, &target)?;
    let jar = target.join("server.jar");
    Ok(InstallOutcome {
        kind: InstallKind::Fabric,
        directory: target,
        jar: Some(jar),
        args_file: None,
        minecraft: request.minecraft.clone(),
        loader: Some(loader),
        installer: Some(installer),
    })
}
