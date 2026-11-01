//! These tests link to the public library just like a separate launcher crate.
use jartender::install::{FabricRequest, InstallEvent, InstallOutcome, install_fabric};
use jartender::{
    Error, LaunchOptions, LaunchPlan, accept_eula, check_eula, discover, prepare_launch,
};
use std::fs::{self, File};
use std::io::Write;
use std::ops::ControlFlow;
use std::path::Path;
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn jar(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"Main-Class: net.minecraft.bundler.Main\n")
        .unwrap();
    zip.finish().unwrap();
}

#[test]
fn public_api_discovers_and_plans_without_config_or_eula_writes() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("servers");
    jar(&root.join("生存 世界/server core.jar"));
    fs::write(temp.path().join("config.json"), "invalid ambient config").unwrap();
    fs::write(temp.path().join("list.json"), "invalid ambient inventory").unwrap();
    let servers = discover(&root).unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].name, "生存 世界");
    let mut options = LaunchOptions::new("生存 世界", "host-chosen-java");
    options.jvm_args.push("-Xmx2G".into());
    options.server_args.push("argument with spaces".into());
    let plan = prepare_launch(&root, &options).unwrap();
    assert_eq!(plan.java, Path::new("host-chosen-java"));
    assert_eq!(
        plan.directory,
        root.join("生存 世界").canonicalize().unwrap()
    );
    let command = plan.to_command();
    assert_eq!(command.get_program(), "host-chosen-java");
    assert_eq!(command.get_current_dir(), Some(plan.directory.as_path()));
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args[0], "-Xmx2G");
    assert_eq!(args[1], "-jar");
    assert_eq!(args[3], "nogui");
    assert_eq!(args[4], "argument with spaces");
    assert!(!plan.directory.join("eula.txt").exists());
    assert!(matches!(
        check_eula(&plan.directory),
        Err(Error::EulaNotAccepted { .. })
    ));
    accept_eula(&plan.directory).unwrap();
    check_eula(&plan.directory).unwrap();
}

#[test]
fn ambiguous_jars_are_structured_and_inventory_is_opt_in() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("servers");
    jar(&root.join("demo/a.jar"));
    jar(&root.join("demo/b.jar"));
    let inventory = temp.path().join("list.json");
    fs::write(&inventory, r#"[{"server_name":"demo","jar_name":"b.jar"}]"#).unwrap();
    let mut options = LaunchOptions::new("demo", "java");
    match prepare_launch(&root, &options).unwrap_err() {
        Error::AmbiguousJars {
            directory,
            candidates,
        } => {
            assert_eq!(directory, root.join("demo").canonicalize().unwrap());
            assert_eq!(candidates, [Path::new("a.jar"), Path::new("b.jar")]);
        }
        error => panic!("unexpected error: {error}"),
    }
    options.legacy_list = Some(inventory);
    let plan = prepare_launch(&root, &options).unwrap();
    assert!(Path::new(&plan.args[1]).ends_with("b.jar"));
}

#[test]
fn relative_library_paths_are_rejected_instead_of_reading_host_working_directory() {
    assert!(matches!(
        discover(Path::new("Servers")),
        Err(Error::InvalidInput {
            field: "server root",
            ..
        })
    ));
    let temp = TempDir::new().unwrap();
    jar(&temp.path().join("demo/server.jar"));
    let options = LaunchOptions::new("demo", "./jdk/bin/java");
    assert!(matches!(
        prepare_launch(temp.path(), &options),
        Err(Error::InvalidInput {
            field: "Java executable",
            ..
        })
    ));
}

#[test]
fn host_can_cancel_before_network_or_filesystem_changes() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("not-created");
    let mut events = Vec::new();
    let result = install_fabric(&root, &FabricRequest::new("demo", "1.21.4"), |event| {
        events.push(event);
        ControlFlow::Break(())
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(
        matches!(events.as_slice(), [InstallEvent::Resolving { minecraft }] if minecraft == "1.21.4")
    );
    assert!(!root.exists());
}

#[test]
fn library_results_can_cross_worker_thread_boundaries() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Error>();
    send_sync::<LaunchOptions>();
    send_sync::<LaunchPlan>();
    send_sync::<InstallEvent>();
    send_sync::<InstallOutcome>();
}

#[cfg(unix)]
#[test]
fn host_owns_child_stdio_and_survives_its_exit() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("servers");
    jar(&root.join("demo/server.jar"));
    let java = temp.path().join("fake java");
    fs::write(&java, "#!/bin/sh\nprintf '%s\\n' \"$$\" \"$PWD\"\nIFS= read -r line\nprintf '%s\\n' \"$line\"\nexit 37\n").unwrap();
    fs::set_permissions(&java, fs::Permissions::from_mode(0o755)).unwrap();
    let host_pid = std::process::id();
    let plan = prepare_launch(&root, &LaunchOptions::new("demo", java)).unwrap();
    accept_eula(&plan.directory).unwrap();
    let mut child = plan
        .to_command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let child_pid = child.id();
    assert_ne!(child_pid, host_pid);
    child.stdin.take().unwrap().write_all(b"stop\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(std::process::id(), host_pid);
    assert_eq!(output.status.code(), Some(37));
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines[0], child_pid.to_string());
    assert_eq!(Path::new(lines[1]), plan.directory);
    assert_eq!(lines[2], "stop");
}
