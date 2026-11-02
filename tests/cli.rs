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
    let text = success(cli(temp.path()).args(["list", "--table"]).output().unwrap());
    assert!(
        String::from_utf8(text.stdout)
            .unwrap()
            .contains("生存 世界")
    );
}

#[test]
fn list_defaults_to_json_and_keeps_the_table_behind_an_explicit_flag() {
    let temp = TempDir::new().unwrap();
    jar(&temp.path().join("Servers/demo/server.jar"), "unknown.Main");
    let default = success(cli(temp.path()).arg("list").output().unwrap());
    let rows: Value = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(rows[0]["name"], "demo");
    assert_eq!(rows[0]["eula"], "not-accepted");
    let explicit = success(cli(temp.path()).args(["list", "--json"]).output().unwrap());
    assert_eq!(explicit.stdout, default.stdout);
    let table = success(cli(temp.path()).args(["list", "--table"]).output().unwrap());
    let text = String::from_utf8(table.stdout).unwrap();
    assert!(text.contains("SERVER") && text.contains("EULA") && text.contains("demo"));
    assert_eq!(
        cli(temp.path())
            .args(["list", "--json", "--table"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn list_and_run_share_the_legacy_inventory_selection() {
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("Servers/demo");
    jar(&directory.join("a.jar"), "a.Main");
    jar(&directory.join("b.jar"), "net.minecraft.bundler.Main");
    let list = || success(cli(temp.path()).arg("list").output().unwrap());
    let rows: Value = serde_json::from_slice(&list().stdout).unwrap();
    assert_eq!(rows[0]["server_type"], "ambiguous");
    assert!(rows[0].get("selected_jar").is_none());
    fs::write(
        temp.path().join("list.json"),
        r#"[{"server_name":"demo","jar_name":"b.jar","jar_path":"/obsolete/b.jar"}]"#,
    )
    .unwrap();
    let rows: Value = serde_json::from_slice(&list().stdout).unwrap();
    assert_eq!(rows[0]["server_type"], "vanilla");
    assert_eq!(rows[0]["selected_jar"], "b.jar");
    let plan: Value = serde_json::from_slice(
        &success(
            cli(temp.path())
                .args(["run", "demo", "--dry-run"])
                .output()
                .unwrap(),
        )
        .stdout,
    )
    .unwrap();
    assert!(plan["args"][1].as_str().unwrap().ends_with("b.jar"));
    fs::write(temp.path().join("list.json"), "not json").unwrap();
    let rows: Value = serde_json::from_slice(&list().stdout).unwrap();
    assert_eq!(rows[0]["server_type"], "ambiguous");
    assert!(rows[0].get("selected_jar").is_none());
}

#[test]
fn eula_state_is_reported_per_server_and_proxies_are_exempt() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("Servers");
    jar(&root.join("accepted/server.jar"), "unknown.Main");
    jar(&root.join("pending/server.jar"), "unknown.Main");
    jar(&root.join("missing/server.jar"), "unknown.Main");
    fs::write(
        root.join("accepted/eula.txt"),
        "# keep\neula=false\neula=true\n",
    )
    .unwrap();
    fs::write(root.join("pending/eula.txt"), "eula=false\n").unwrap();
    jar(
        &root.join("proxy/server.jar"),
        "com.velocitypowered.proxy.Velocity",
    );
    let output = success(cli(temp.path()).arg("list").output().unwrap());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    let state = |name: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap()["eula"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(state("accepted"), "accepted");
    assert_eq!(state("pending"), "not-accepted");
    assert_eq!(state("missing"), "not-accepted");
    assert_eq!(state("proxy"), "not-required");
    let table = success(cli(temp.path()).args(["list", "--table"]).output().unwrap());
    let text = String::from_utf8(table.stdout).unwrap();
    assert!(text.contains("accepted") && text.contains("n/a"));
}

#[cfg(unix)]
#[test]
fn unreadable_server_directories_are_skipped_and_reported() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("Servers");
    jar(&root.join("good/server.jar"), "unknown.Main");
    let locked = root.join("locked");
    jar(&locked.join("server.jar"), "unknown.Main");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        // Running with privileges that bypass the mode bits; nothing to assert.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let output = cli(temp.path()).arg("list").output().unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(output.status.code(), Some(0));
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["name"], "good");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("skipped"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
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
            .args(["install", "26.3", "fabric"])
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
fn version_queries_have_discoverable_help_and_explicit_proxy_overrides_config() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("config.json"), "broken config").unwrap();
    let help = success(
        cli(temp.path())
            .args(["versions", "--help"])
            .output()
            .unwrap(),
    );
    let text = String::from_utf8(help.stdout).unwrap();
    for token in [
        "--all",
        "--limit",
        "--table",
        "--json",
        "MINECRAFT|KIND",
        "fabric",
        "forge",
        "neoforge",
        "purpur",
    ] {
        assert!(text.contains(token));
    }
    for args in [
        vec!["versions", "nope", "--no-proxy"],
        vec!["versions", "26.3", "nope", "--no-proxy"],
    ] {
        assert_eq!(
            cli(temp.path()).args(&args).output().unwrap().status.code(),
            Some(2),
            "{args:?}"
        );
    }
    assert_eq!(
        cli(temp.path())
            .args(["versions", "26.3", "fabric", "--json", "--table"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        cli(temp.path())
            .args(["versions", "26.3", "fabric", "--limit", "many"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    let help = success(
        cli(temp.path())
            .args(["install", "--help"])
            .output()
            .unwrap(),
    );
    assert!(String::from_utf8_lossy(&help.stdout).contains("jartender versions"));
    // `versions 26.3` now lists supporting cores, which needs the network; cover the
    // offline validation path instead.
    let output = cli(temp.path())
        .args(["versions", "bad/path", "fabric", "--no-proxy"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid version"));
    assert!(!temp.path().join("Servers").exists());
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
                .args(["install", "1.21.4", "fabric", "latest", name])
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

#[test]
fn cli_discovers_argument_files_and_exposes_target_and_eula_policy() {
    let temp = TempDir::new().unwrap();
    let base = Path::new("libraries/net/neoforged/neoforge");
    let directory = temp.path().join("Servers/NeoForge 世界");
    for version in ["one", "two"] {
        let parent = directory.join(base).join(version);
        fs::create_dir_all(&parent).unwrap();
        for file in ["unix_args.txt", "win_args.txt"] {
            fs::write(parent.join(file), "example.Server\n").unwrap();
        }
    }
    let output = success(cli(temp.path()).args(["list", "--json"]).output().unwrap());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["server_type"], "ambiguous");
    assert_eq!(rows[0]["argument_files"].as_array().unwrap().len(), 2);
    let output = cli(temp.path())
        .args(["run", "NeoForge 世界", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--args-file"));
    let selected = base.join("two").join(if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    });
    let output = success(
        cli(temp.path())
            .args(["run", "NeoForge 世界", "--dry-run", "--args-file"])
            .arg(&selected)
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["server_type"], "neoforge");
    assert_eq!(plan["requires_eula"], true);
    assert!(plan["args"][0].as_str().unwrap().starts_with('@'));
    assert!(!directory.join("eula.txt").exists());
    let output = cli(temp.path())
        .args(["run", "NeoForge 世界", "--dry-run", "--args-file"])
        .arg(selected)
        .args(["--jar", "server.jar"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn cli_runs_both_proxy_types_without_eula_files_or_nogui() {
    let temp = TempDir::new().unwrap();
    let java = temp.path().join("fake-java");
    fake_java(&java, "printf '%s\\n' \"$@\"\nexit 37");
    for (name, main) in [
        ("one", "com.velocitypowered.proxy.Velocity"),
        ("two", "net.md_5.bungee.Bootstrap"),
    ] {
        let directory = temp.path().join("Servers").join(name);
        jar(&directory.join("server.jar"), main);
        for accept in [false, true] {
            let mut cmd = cli(temp.path());
            cmd.args(["run", name, "--java"]).arg(&java);
            if accept {
                cmd.arg("--accept-eula");
            }
            let output = cmd.output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(37),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!String::from_utf8_lossy(&output.stdout).contains("nogui"));
            assert!(!directory.join("eula.txt").exists());
        }
    }
}

#[test]
fn install_takes_version_kind_kind_version_name_and_validates_locally() {
    let temp = TempDir::new().unwrap();
    for args in [
        vec!["install", "bad/../x", "forge", "latest", "ok"],
        vec!["install", "26.3", "forge", "latest", "../x"],
        vec!["install", "26.3", "forge", "bad/x", "ok"],
        vec!["install", "26.3", "neoforge", "latest", "a/b"],
        vec!["install", "26.3", "neoforge", "bad/x", "ok"],
        vec!["install", "26.3", "fabric", "bad/x", "ok"],
        vec!["install", "26.3", "paper", "bad", "ok"],
        vec!["install", "26.3", "purpur", "bad", "ok"],
        vec!["install", "1.21.8", "leaves", "bad", "ok"],
        vec!["install", "1.21.11", "folia", "bad", "ok"],
        vec!["install", "1.20.1", "mohist", "bad", "ok"],
        vec!["install", "1.20.1", "banner", "bad", "ok"],
        vec!["install", "1.21.1", "arclight", "latest", "../bad"],
        vec!["install", "1.16.5", "catserver", "latest", "../bad"],
        vec!["install", "26.3", "purpur", "latest", "../x"],
    ] {
        assert_eq!(
            cli(temp.path()).args(&args).output().unwrap().status.code(),
            Some(1),
            "{args:?}"
        );
    }
    // Kind-specific flags are rejected when they do not apply.
    for args in [
        vec![
            "install", "1.21.1", "arclight", "latest", "ok", "--java", "java",
        ],
        vec![
            "install",
            "1.20.1",
            "mohist",
            "latest",
            "ok",
            "--installer",
            "1",
        ],
        vec![
            "install", "1.20.1", "banner", "latest", "ok", "--java", "java",
        ],
        vec![
            "install",
            "1.16.5",
            "catserver",
            "latest",
            "ok",
            "--installer",
            "1",
        ],
        vec![
            "install", "1.21.11", "folia", "latest", "ok", "--java", "java",
        ],
        vec![
            "install",
            "1.21.11",
            "folia",
            "latest",
            "ok",
            "--installer",
            "1.0",
        ],
        vec![
            "install", "1.21.8", "leaves", "latest", "ok", "--java", "java",
        ],
        vec![
            "install",
            "1.21.8",
            "leaves",
            "latest",
            "ok",
            "--installer",
            "1.0",
        ],
        vec![
            "install", "26.3", "purpur", "latest", "ok", "--java", "java",
        ],
        vec![
            "install",
            "26.3",
            "purpur",
            "latest",
            "ok",
            "--installer",
            "1.0",
        ],
        vec!["install", "26.3", "paper", "latest", "ok", "--java", "java"],
        vec![
            "install",
            "26.3",
            "paper",
            "latest",
            "ok",
            "--installer",
            "1.0",
        ],
        vec![
            "install", "26.3", "fabric", "latest", "ok", "--java", "java",
        ],
        vec![
            "install",
            "26.3",
            "forge",
            "latest",
            "ok",
            "--installer",
            "1.0.3",
        ],
    ] {
        assert_eq!(
            cli(temp.path()).args(&args).output().unwrap().status.code(),
            Some(1),
            "{args:?}"
        );
    }
    let help = success(
        cli(temp.path())
            .args(["install", "--help"])
            .output()
            .unwrap(),
    );
    let text = String::from_utf8(help.stdout).unwrap();
    for token in [
        "MINECRAFT",
        "KIND",
        "VERSION",
        "NAME",
        "fabric",
        "paper",
        "purpur",
        "leaves",
        "folia",
        "arclight",
        "mohist",
        "banner",
        "catserver",
        "forge",
        "neoforge",
    ] {
        assert!(text.contains(token), "{token} missing from {text}");
    }
    assert!(!temp.path().join("Servers").exists());
    // Usage errors (missing positionals) are still distinct from operational ones.
    assert_eq!(
        cli(temp.path())
            .args(["install", "26.3"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
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
fn java_install_is_discoverable_and_validates_without_reading_server_config() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("config.json"), "broken").unwrap();
    let help = success(
        cli(root.path())
            .args(["java", "install", "--help"])
            .output()
            .unwrap(),
    );
    let text = String::from_utf8(help.stdout).unwrap();
    for value in ["oracle", "openjdk", "--dry-run"] {
        assert!(text.contains(value));
    }
    let result = cli(root.path())
        .args(["java", "install", "../bad", "oracle", "--no-proxy"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("Java version"));
    assert!(!root.path().join("java").exists());
    assert!(!root.path().join("Servers").exists());
    let result = cli(root.path())
        .args(["java", "install", "25", "unknown"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
}

#[test]
fn java_types_uses_registry_and_ignores_server_config() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("config.json"), "broken").unwrap();
    let output = success(cli(root.path()).args(["java", "types"]).output().unwrap());
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let providers = jartender::java::install::providers();
    assert_eq!(rows.as_array().unwrap().len(), providers.len());
    for (row, provider) in rows.as_array().unwrap().iter().zip(providers) {
        assert_eq!(row["distribution"], provider.info().distribution.name());
    }
    let table = success(
        cli(root.path())
            .args(["java", "types", "--table"])
            .output()
            .unwrap(),
    );
    assert!(String::from_utf8_lossy(&table.stdout).contains("Oracle JDK"));
    let help = success(
        cli(root.path())
            .args(["java", "version", "--help"])
            .output()
            .unwrap(),
    );
    assert!(String::from_utf8_lossy(&help.stdout).contains("21 openjdk"));
    for args in [
        vec!["java", "version", "21", "unknown"],
        vec!["java", "version", "zulu", "8"],
    ] {
        assert_eq!(
            cli(root.path()).args(args).output().unwrap().status.code(),
            Some(2)
        );
    }
    let help = success(
        cli(root.path())
            .args(["java", "versions", "--help"])
            .output()
            .unwrap(),
    );
    let help = String::from_utf8_lossy(&help.stdout);
    for provider in providers {
        assert!(help.contains(provider.info().distribution.name()));
    }
    for args in [
        vec!["install", "java", "25", "oracle"],
        vec!["java", "versions", "unknown"],
        vec!["java", "versions", "oracle", "invalid"],
        vec!["java", "--json", "types"],
    ] {
        assert_eq!(
            cli(root.path()).args(args).output().unwrap().status.code(),
            Some(2)
        );
    }
    let zero = cli(root.path())
        .args(["java", "versions", "oracle", "0", "--no-proxy"])
        .output()
        .unwrap();
    assert_eq!(zero.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&zero.stderr).contains("Java major"));
    let zero = cli(root.path())
        .args(["java", "version", "0", "--no-proxy"])
        .output()
        .unwrap();
    assert_eq!(zero.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&zero.stderr).contains("Java major"));
    let help = success(
        cli(root.path())
            .args(["java", "version", "--help"])
            .output()
            .unwrap(),
    );
    assert!(String::from_utf8_lossy(&help.stdout).contains("<MAJOR>"));
    assert!(!root.path().join("java").exists());
}

#[cfg(unix)]
#[test]
fn java_scan_is_independent_of_config_and_preserves_failures_in_json() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("config.json"), "broken").unwrap();
    let directory = root.path().join("jdks");
    fs::create_dir_all(&directory).unwrap();
    fake_java(
        &directory.join("java"),
        "[ -z \"$JAVA_TOOL_OPTIONS$JDK_JAVA_OPTIONS$_JAVA_OPTIONS$CLASSPATH\" ] || exit 9\nprintf 'java.version = 21.0.9\\njava.vendor = Example\\nos.arch = aarch64\\n' >&2",
    );
    let output = success(
        cli(root.path())
            .env("JAVA_TOOL_OPTIONS", "must-be-removed")
            .env("CLASSPATH", "must-be-removed")
            .args(["java", "--no-system", "--path", "jdks"])
            .output()
            .unwrap(),
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let explicit = success(
        cli(root.path())
            .args(["java", "list", "--no-system", "--path", "jdks"])
            .output()
            .unwrap(),
    );
    let explicit: Value = serde_json::from_slice(&explicit.stdout).unwrap();
    assert_eq!(data, explicit);
    for command in [vec!["java"], vec!["java", "list"]] {
        let json = success(
            cli(root.path())
                .args(&command)
                .args(["--no-system", "--path", "jdks", "--json"])
                .output()
                .unwrap(),
        );
        assert_eq!(data, serde_json::from_slice::<Value>(&json.stdout).unwrap());
        let table = success(
            cli(root.path())
                .args(&command)
                .args(["--no-system", "--path", "jdks", "--table"])
                .output()
                .unwrap(),
        );
        let text = String::from_utf8_lossy(&table.stdout);
        assert!(text.contains("VERSION") && text.contains("21.0.9"));
        assert!(serde_json::from_slice::<Value>(&table.stdout).is_err());
        let conflict = cli(root.path())
            .args(&command)
            .args(["--json", "--table"])
            .output()
            .unwrap();
        assert_eq!(conflict.status.code(), Some(2));
    }

    assert_eq!(data["runtimes"][0]["major"], 21);
    assert_eq!(data["runtimes"][0]["vendor"], "Example");
    assert_eq!(data["skipped"].as_array().unwrap().len(), 0);
    assert!(!root.path().join("java_list.json").exists());
    assert!(!root.path().join("Servers").exists());
}

#[cfg(unix)]
#[test]
fn saved_launch_defaults_and_cli_overrides_preserve_argument_boundaries_without_running_java() {
    let root = TempDir::new().unwrap();
    let directory = root.path().join("Servers/demo");
    jar(&directory.join("server.jar"), "net.minecraft.bundler.Main");
    fs::create_dir(directory.join("runtime")).unwrap();
    fake_java(
        &directory.join("runtime/java"),
        "printf 'should not execute' > ran; exit 9",
    );
    let saved = r#"{"java":"./runtime/java","jvm_args":["-Xmx2G","-Dmessage=with spaces"],"server_args":["--port","25566"],"jar":"server.jar","gui":true}"#;
    fs::write(directory.join("jartender.json"), saved).unwrap();
    let output = success(
        cli(root.path())
            .env("JAVA_HOME", "/missing/default-java")
            .args(["run", "demo", "--dry-run"])
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        Path::new(plan["java"].as_str().unwrap()),
        directory.join("runtime/java").canonicalize().unwrap()
    );
    let args = plan["args"].as_array().unwrap();
    assert_eq!(args[0], "-Xmx2G");
    assert_eq!(args[1], "-Dmessage=with spaces");
    assert!(args.contains(&Value::from("25566")));
    assert!(!args.contains(&Value::from("nogui")));
    let output = success(
        cli(root.path())
            .args([
                "run",
                "demo",
                "--java",
                "/usr/bin/true",
                "--jvm-arg=-Xmx4G",
                "--gui=false",
                "--dry-run",
                "--",
                "--port",
                "25567",
            ])
            .output()
            .unwrap(),
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    let args = plan["args"].as_array().unwrap();
    assert_eq!(args[0], "-Xmx4G");
    assert!(!args.contains(&Value::from("-Xmx2G")));
    assert!(args.contains(&Value::from("25567")));
    assert!(!args.contains(&Value::from("25566")));
    assert!(args.contains(&Value::from("nogui")));
    assert!(!directory.join("ran").exists());
    assert!(!directory.join("eula.txt").exists());
    assert_eq!(
        fs::read_to_string(directory.join("jartender.json")).unwrap(),
        saved
    );
    // Listing is independent of launch configuration errors; running reports them.
    fs::write(directory.join("jartender.json"), "{broken").unwrap();
    success(cli(root.path()).arg("list").output().unwrap());
    assert_eq!(
        cli(root.path())
            .args(["run", "demo", "--dry-run"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
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
