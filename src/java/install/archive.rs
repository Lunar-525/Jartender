use super::{Event, notify};
use crate::{Error, Result};
use std::{
    fs,
    io::{Read, Write},
    ops::ControlFlow,
    path::{Component, Path},
};

const EXPANDED_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const ENTRY_LIMIT: usize = 100_000;

fn error(path: &Path, reason: impl ToString) -> Error {
    Error::InvalidArchive {
        path: path.into(),
        reason: reason.to_string(),
    }
}
fn safe(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.to_string_lossy().contains(['\\', ':'])
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

pub(super) fn extract(
    file: &Path,
    directory: &Path,
    zip: bool,
    progress: &mut impl FnMut(Event) -> ControlFlow<()>,
) -> Result<()> {
    let input = fs::File::open(file).map_err(|e| Error::io("open JDK archive", file, e))?;
    let mut total = 0u64;
    if zip {
        let mut archive = zip::ZipArchive::new(input).map_err(|e| error(file, e))?;
        if archive.len() > ENTRY_LIMIT {
            return Err(error(file, "too many entries"));
        }
        for i in 0..archive.len() {
            notify(progress, Event::Unpacking { entries: i + 1 })?;
            let mut entry = archive.by_index(i).map_err(|e| error(file, e))?;
            let path = entry
                .enclosed_name()
                .ok_or_else(|| error(file, "unsafe ZIP path"))?;
            if !safe(&path) || entry.is_symlink() {
                return Err(error(file, "unsafe ZIP entry"));
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| error(file, "expanded size overflow"))?;
            if total > EXPANDED_LIMIT {
                return Err(error(file, "expanded archive exceeds 2 GiB"));
            }
            let target = directory.join(path);
            if entry.is_dir() {
                fs::create_dir_all(&target).map_err(|e| Error::io("unpack JDK", &target, e))?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| Error::io("unpack JDK", parent, e))?;
            }
            let mut output =
                fs::File::create(&target).map_err(|e| Error::io("unpack JDK", &target, e))?;
            let expected = entry.size();
            let written = std::io::copy(
                &mut (&mut entry).take(expected.saturating_add(1)),
                &mut output,
            )
            .map_err(|e| error(file, e))?;
            if written != expected {
                return Err(error(file, "ZIP entry size mismatch"));
            }
            output
                .flush()
                .map_err(|e| Error::io("unpack JDK", &target, e))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = entry.unix_mode() {
                    fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))
                        .map_err(|e| Error::io("set JDK permissions", &target, e))?;
                }
            }
        }
    } else {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(input));
        for (i, entry) in archive.entries().map_err(|e| error(file, e))?.enumerate() {
            notify(progress, Event::Unpacking { entries: i + 1 })?;
            if i >= ENTRY_LIMIT {
                return Err(error(file, "too many entries"));
            }
            let mut entry = entry.map_err(|e| error(file, e))?;
            let path = entry.path().map_err(|e| error(file, e))?.into_owned();
            if !safe(&path) {
                return Err(error(file, "unsafe tar path"));
            }
            let kind = entry.header().entry_type();
            if !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link()) {
                return Err(error(file, "unsupported tar entry type"));
            }
            if kind.is_symlink() || kind.is_hard_link() {
                let link = entry
                    .link_name()
                    .map_err(|e| error(file, e))?
                    .ok_or_else(|| error(file, "missing link target"))?;
                if link.is_absolute() || link.to_string_lossy().contains(['\\', ':']) {
                    return Err(error(file, "unsafe archive link"));
                }
                let mut depth = if kind.is_symlink() {
                    path.parent().map(|p| p.components().count()).unwrap_or(0)
                } else {
                    0
                };
                for part in link.components() {
                    match part {
                        Component::Normal(_) => depth += 1,
                        Component::CurDir => (),
                        Component::ParentDir if depth > 0 => depth -= 1,
                        _ => return Err(error(file, "escaping archive link")),
                    }
                }
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| error(file, "expanded size overflow"))?;
            if total > EXPANDED_LIMIT {
                return Err(error(file, "expanded archive exceeds 2 GiB"));
            }
            if !entry.unpack_in(directory).map_err(|e| error(file, e))? {
                return Err(error(file, "entry escapes archive directory"));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_links(package: &Path) -> Result<()> {
    let root = package
        .canonicalize()
        .map_err(|e| Error::io("inspect JDK links", package, e))?;
    for entry in walkdir::WalkDir::new(package).follow_links(false) {
        let entry = entry.map_err(|e| error(package, e))?;
        if entry.file_type().is_symlink() {
            let resolved = entry.path().canonicalize().map_err(|e| error(package, e))?;
            if !resolved.starts_with(&root) {
                return Err(error(package, "link escapes JDK package"));
            }
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn relative_links_inside_a_jdk_are_preserved() {
        let root = tempfile::TempDir::new().unwrap();
        let file = root.path().join("jdk.tar.gz");
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(7);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "jdk/legal/base/LICENSE", b"license".as_slice())
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("../base/LICENSE").unwrap();
        header.set_cksum();
        builder
            .append_data(&mut header, "jdk/legal/other/LICENSE", &[][..])
            .unwrap();
        fs::write(&file, builder.into_inner().unwrap().finish().unwrap()).unwrap();
        let target = root.path().join("unpacked");
        fs::create_dir(&target).unwrap();
        extract(&file, &target, false, &mut |_| ControlFlow::Continue(())).unwrap();
        validate_links(&target.join("jdk")).unwrap();
        assert_eq!(
            fs::read_to_string(target.join("jdk/legal/other/LICENSE")).unwrap(),
            "license"
        );
    }
}
