#![cfg(feature = "cli")]

use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
#[cfg(unix)]
use std::process::Stdio;
use std::process::{Command, Output};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn cli(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jartender"));
    command.current_dir(dir).env_remove("JAVA_HOME");
    command
}

fn jar(path: &Path, main_class: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
        .unwrap();
    write!(zip, "Manifest-Version: 1.0\r\nMain-Class: {main_class}\r\n").unwrap();
    zip.finish().unwrap();
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn discovery_is_read_only_sorted_and_handles_unicode_and_incomplete_directories() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("Servers");
    jar(
        &root.join("生存 世界/server.jar"),
        "net.fabricmc.loader.impl.launch.server.FabricServerLauncher",
    );
    jar(&root.join("alpha/server.jar"), "net.minecraft.bundler.Main");
    jar(&root.join(".unfinished/server.jar"), "something");
    jar(
        &root.join("installer-only/forge-installer.jar"),
        "something",
    );
    fs::write(root.join("alpha/readme.txt"), "untouched").unwrap();
    let output = success(cli(temp.path()).args(["list", "--json"]).output().unwrap());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["name"], "alpha");
    assert_eq!(rows[0]["server_type"], "vanilla");
    assert_eq!(rows[1]["name"], "生存 世界");
    assert_eq!(rows[1]["server_type"], "fabric");
    assert!(!root.join("alpha/eula.txt").exists());
    assert!(!temp.path().join("list.json").exists());
    assert!(!temp.path().join("java_list.json").exists());
    let text = success(cli(temp.path()).arg("list").output().unwrap());
    assert!(
        String::from_utf8(text.stdout)
            .unwrap()
            .contains("生存 世界")
    );
}

#[test]
fn root_resolution_preserves_legacy_config_and_explicit_override() {
    let temp = TempDir::new().unwrap();
    jar(&temp.path().join("custom/one/server.jar"), "unknown.Main");
    fs::write(
        temp.path().join("config.json"),
        r#"{"serverpath":"custom"}"#,
    )
    .unwrap();
    let output = success(cli(temp.path()).args(["list", "--json"]).output().unwrap());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["name"], "one");
    assert_eq!(rows[0]["server_type"], "unknown");
    fs::write(temp.path().join("config.json"), "broken").unwrap();
    assert_eq!(
        cli(temp.path()).arg("list").output().unwrap().status.code(),
        Some(1)
    );
    success(
        cli(temp.path())
            .args(["list", "--root", "custom"])
            .output()
            .unwrap(),
    );
}

#[test]
fn missing_root_and_usage_errors_have_distinct_exit_codes() {
    let temp = TempDir::new().unwrap();
    assert_eq!(
        cli(temp.path()).arg("list").output().unwrap().status.code(),
        Some(1)
    );
    assert!(!temp.path().join("Servers").exists());
    assert_eq!(
        cli(temp.path())
            .args(["install", "fabric", "--name", "x"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        cli(temp.path())
            .arg("--help")
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );
}

#[test]
fn multiple_jars_need_a_choice_and_legacy_selection_must_still_exist() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("Servers/test");
    jar(&dir.join("a.jar"), "a.Main");
    jar(&dir.join("b.jar"), "b.Main");
    let output = cli(temp.path())
        .args(["run", "test", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--jar"));
    fs::write(
        temp.path().join("list.json"),
        r#"[{"server_name":"test","jar_name":"b.jar","jar_path":"/obsolete/b.jar"}]"#,
    )
    .unwrap();
    let output = success(
        cli(temp.path())
            .args(["run", "test", "--dry-run"])
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(plan["args"][1].as_str().unwrap().ends_with("b.jar"));
    let output = success(
        cli(temp.path())
            .args(["run", "test", "--jar", "a.jar", "--dry-run"])
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(plan["args"][1].as_str().unwrap().ends_with("a.jar"));
    fs::remove_file(dir.join("b.jar")).unwrap();
    success(
        cli(temp.path())
            .args(["run", "test", "--dry-run"])
            .output()
            .unwrap(),
    );
}

#[test]
fn dry_run_is_json_and_does_not_accept_the_eula_or_start_java() {
    let temp = TempDir::new().unwrap();
    jar(&temp.path().join("Servers/demo/server.jar"), "unknown.Main");
    let output = success(
        cli(temp.path())
            .args([
                "run",
                "demo",
                "--dry-run",
                "--accept-eula",
                "--jvm-arg=-Xmx2G",
                "--",
                "--port",
                "25566",
            ])
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["java"], "java");
    assert_eq!(plan["args"][0], "-Xmx2G");
    assert_eq!(plan["args"][1], "-jar");
    assert_eq!(plan["args"][3], "nogui");
    assert_eq!(plan["args"][4], "--port");
    assert_eq!(plan["args"][5], "25566");
    assert!(!temp.path().join("Servers/demo/eula.txt").exists());
    let output = cli(temp.path()).args(["run", "demo"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("EULA not accepted"));
}

#[test]
fn directory_traversal_and_missing_jars_are_rejected() {
    let temp = TempDir::new().unwrap();
    jar(&temp.path().join("Servers/demo/server.jar"), "unknown.Main");
    for name in ["..", "../demo", ".hidden", "a/b", "a\\b", "C:demo"] {
        assert_eq!(
            cli(temp.path())
                .args(["run", name, "--dry-run"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(1)
        );
        assert_eq!(
            cli(temp.path())
                .args(["install", "fabric", "--minecraft", "1.21.4", "--name", name])
                .output()
                .unwrap()
                .status
                .code(),
            Some(1)
        );
    }
    for jar in ["../server.jar", "..", "missing.jar"] {
        assert_eq!(
            cli(temp.path())
                .args(["run", "demo", "--jar", jar, "--dry-run"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(1)
        );
    }
}

#[cfg(unix)]
fn fake_java(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn launch_preserves_cwd_argument_boundaries_console_input_pid_and_exit_code() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("Servers/生存 世界");
    jar(&dir.join("server core.jar"), "unknown.Main");
    fs::write(
        dir.join("eula.txt"),
        "# keep this\neula=false\nother=value\n",
    )
    .unwrap();
    let java = temp.path().join("fake java");
    fake_java(
        &java,
        "printf '%s\\n' \"$$\" \"$PWD\" \"$@\"\nIFS= read -r line\nprintf 'input=%s\\n' \"$line\"\nexit 37",
    );
    let mut command = cli(temp.path());
    command
        .args([
            "run",
            "生存 世界",
            "--java",
            "./fake java",
            "--accept-eula",
            "--jvm-arg=-Xmx2G",
            "--",
            "value with spaces",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let pid = child.id();
    child.stdin.take().unwrap().write_all(b"stop\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(37));
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines[0], pid.to_string());
    assert_eq!(Path::new(lines[1]), dir.canonicalize().unwrap());
    assert_eq!(
        &lines[2..],
        [
            "-Xmx2G",
            "-jar",
            dir.canonicalize()
                .unwrap()
                .join("server core.jar")
                .to_str()
                .unwrap(),
            "nogui",
            "value with spaces",
            "input=stop"
        ]
    );
    assert_eq!(
        fs::read_to_string(dir.join("eula.txt")).unwrap(),
        "# keep this\nother=value\neula=true\n"
    );
}

#[cfg(unix)]
#[test]
fn java_home_is_used_and_gui_omits_nogui() {
    let temp = TempDir::new().unwrap();
    jar(&temp.path().join("Servers/demo/server.jar"), "unknown.Main");
    let home = temp.path().join("jdk");
    fs::create_dir_all(home.join("bin")).unwrap();
    fake_java(&home.join("bin/java"), "exit 0");
    let output = success(
        cli(temp.path())
            .env("JAVA_HOME", &home)
            .args(["run", "demo", "--gui", "--dry-run"])
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        plan["java"].as_str().unwrap(),
        home.join("bin/java")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!(plan["args"].as_array().unwrap().len(), 2);
}
