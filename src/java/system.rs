//! Search locations follow the existing Python javafinder (Prism Launcher/MultiMC credits there).
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn variable(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn named_children(root: &Path, result: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if ["java", "jdk", "openjdk", "openj9", "semeru"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                result.push(entry.path());
            }
        }
    }
}

pub(super) fn paths() -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Some(home) = variable("JAVA_HOME") {
        result.push(home);
    }
    if let Some(path) = env::var_os("PATH") {
        result.extend(
            env::split_paths(&path)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(if cfg!(windows) { "java.exe" } else { "java" })),
        );
    }
    let home = variable("HOME").or_else(|| variable("USERPROFILE"));
    if let Some(home) = home {
        result.extend([
            variable("SDKMAN_DIR")
                .unwrap_or_else(|| home.join(".sdkman"))
                .join("candidates/java"),
            variable("ASDF_DATA_DIR")
                .unwrap_or_else(|| home.join(".asdf"))
                .join("installs/java"),
            home.join(".jdks"),
            home.join(".jabba/jdk"),
            home.join(".local/share/mise/installs/java"),
        ]);
        #[cfg(target_os = "macos")]
        result.extend([
            home.join("Library/Java/JavaVirtualMachines"),
            home.join("Library/Application Support/PrismLauncher/java"),
        ]);
        #[cfg(not(windows))]
        result.push(
            variable("XDG_DATA_HOME")
                .unwrap_or_else(|| home.join(".local/share"))
                .join("PrismLauncher/java"),
        );
    }
    #[cfg(target_os = "macos")]
    {
        result.extend(
            [
                "/Library/Java/JavaVirtualMachines",
                "/System/Library/Java/JavaVirtualMachines",
                "/usr/bin/java",
                "/Library/Internet Plug-Ins/JavaAppletPlugin.plugin/Contents/Home/bin/java",
            ]
            .map(PathBuf::from),
        );
        for root in ["/opt/homebrew/opt", "/usr/local/opt"] {
            named_children(Path::new(root), &mut result);
        }
    }
    #[cfg(target_os = "linux")]
    {
        for root in [
            "/usr/java",
            "/usr/lib/jvm",
            "/usr/lib64/jvm",
            "/usr/lib32/jvm",
            "/opt/jdk",
            "/opt/jdks",
            "/opt/ibm",
            "/app/jdk",
        ] {
            result.push(PathBuf::from(root));
            if let Some(snap) = variable("SNAP") {
                result.push(snap.join(root.trim_start_matches('/')));
            }
        }
        for root in ["/usr/lib", "/usr/lib64", "/opt"] {
            named_children(Path::new(root), &mut result);
        }
    }
    #[cfg(windows)]
    {
        for variable_name in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(root) = variable(variable_name) {
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "Eclipse Foundation",
                    "AdoptOpenJDK",
                    "Amazon Corretto",
                    "Zulu",
                    "BellSoft",
                    "Semeru",
                    "SapMachine",
                ] {
                    result.push(root.join(vendor));
                }
                named_children(&root.join("Microsoft"), &mut result);
            }
        }
        if let Some(appdata) = variable("APPDATA") {
            result.push(appdata.join("PrismLauncher/java"));
        }
        if let Some(local) = variable("LOCALAPPDATA") {
            result.push(local.join("Programs/Eclipse Adoptium"));
            named_children(&local.join("Programs/Microsoft"), &mut result);
        }
        registry(&mut result);
    }
    result.sort();
    result.dedup();
    result
}

#[cfg(windows)]
fn registry(result: &mut Vec<PathBuf>) {
    use winreg::{RegKey, enums::*};
    fn visit(key: &RegKey, depth: usize, view: u32, result: &mut Vec<PathBuf>) {
        for name in ["JavaHome", "Path", "InstallationPath"] {
            if let Ok(value) = key.get_value::<String, _>(name) {
                let path = PathBuf::from(value);
                if path.is_absolute() {
                    result.push(path);
                }
            }
        }
        if depth == 0 {
            return;
        }
        for name in key.enum_keys().flatten() {
            if let Ok(child) = key.open_subkey_with_flags(name, KEY_READ | view) {
                visit(&child, depth - 1, view, result);
            }
        }
    }
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            for vendor in [
                "JavaSoft",
                "AdoptOpenJDK",
                "Eclipse Foundation",
                "Eclipse Adoptium",
                "Semeru",
                "Microsoft\\JDK",
                "Azul Systems\\Zulu",
                "BellSoft",
                "SapMachine",
            ] {
                if let Ok(key) = RegKey::predef(hive)
                    .open_subkey_with_flags(format!("SOFTWARE\\{vendor}"), KEY_READ | view)
                {
                    visit(&key, 4, view, result);
                }
            }
        }
    }
}
