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
    metadata_jar(path, "net.minecraft.bundler.Main", None);
}

fn metadata_jar(path: &Path, main: &str, versions: Option<&str>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    zip.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
        .unwrap();
    writeln!(zip, "Main-Class: {main}").unwrap();
    if let Some(index) = versions {
        zip.start_file("META-INF/versions.list", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(index.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

fn platform_args() -> &'static str {
    if cfg!(windows) {
        "win_args.txt"
    } else {
        "unix_args.txt"
    }
}

fn argument_layout(directory: &Path, base: &str, version: &str) -> std::path::PathBuf {
    let parent = directory.join(base).join(version);
    fs::create_dir_all(&parent).unwrap();
    for filename in ["unix_args.txt", "win_args.txt"] {
        fs::write(parent.join(filename), "-Dmarker=fixture\nexample.Server\n").unwrap();
    }
    Path::new(base).join(version).join(platform_args())
}

fn jar_entries(path: &Path, entries: &[(&str, &str)]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, contents) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn versions_come_from_payload_metadata_and_proxy_artifacts_not_directory_names() {
    let temp = TempDir::new().unwrap();
    for (name, main, key, value, expected) in [
        (
            "vanilla-999",
            "net.minecraft.bundler.Main",
            "version.json",
            r#"{"id":"1.21.4"}"#,
            "1.21.4",
        ),
        (
            "renamed-fabric",
            "net.fabricmc.installer.ServerLauncher",
            "install.properties",
            "game-version=26.3\nfabric-loader-version=0.20.1\n",
            "26.3",
        ),
        (
            "renamed-proxy",
            "net.md_5.bungee.Bootstrap",
            "META-INF/maven/net.md-5/bungeecord-bootstrap/pom.properties",
            "version=26.1-R0.1-SNAPSHOT\n",
            "26.1-R0.1-SNAPSHOT",
        ),
    ] {
        let manifest = format!("Main-Class: {main}\n");
        jar_entries(
            &temp.path().join(name).join("server.jar"),
            &[("META-INF/MANIFEST.MF", &manifest), (key, value)],
        );
        let server = discover(temp.path())
            .unwrap()
            .into_iter()
            .find(|s| s.name == name)
            .unwrap();
        assert_eq!(server.version.as_deref(), Some(expected));
        assert_eq!(
            server.map_size_bytes,
            if name == "renamed-proxy" {
                None
            } else {
                Some(0)
            }
        );
    }
    let relative = argument_layout(
        &temp.path().join("neo"),
        "libraries/net/neoforged/neoforge",
        "not-the-mc-version",
    );
    fs::write(
        temp.path().join("neo").join(relative),
        "example.Server\n--fml.mcVersion 26.2\n",
    )
    .unwrap();
    assert_eq!(
        discover(temp.path())
            .unwrap()
            .into_iter()
            .find(|s| s.name == "neo")
            .unwrap()
            .version
            .as_deref(),
        Some("26.2")
    );
}

#[test]
fn map_size_counts_configured_world_and_split_dimensions_but_not_server_files() {
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("demo");
    jar(&directory.join("server.jar"));
    let config =
        "# Unicode and escaped spaces are valid Java properties\nlevel-name=地图\\ world\n";
    fs::write(directory.join("server.properties"), config).unwrap();
    for (name, size) in [
        ("地图 world/region/r.0.0.mca", 1024),
        ("地图 world/DIM-1/region/r.0.0.mca", 2048),
        ("地图 world_nether/DIM-1/region/r.0.0.mca", 256),
        ("地图 world_the_end/DIM1/region/r.0.0.mca", 128),
        ("logs/large.log", 10000),
        ("world/unselected.mca", 10000),
    ] {
        let path = directory.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap().set_len(size).unwrap();
    }
    let server = discover(temp.path()).unwrap().remove(0);
    assert_eq!(server.map_size_bytes, Some(3456));
    assert_eq!(
        fs::read_to_string(directory.join("server.properties")).unwrap(),
        config
    );
    fs::write(
        directory.join("server.properties"),
        "level-name=../outside\n",
    )
    .unwrap();
    assert_eq!(discover(temp.path()).unwrap()[0].map_size_bytes, None);
}

#[cfg(unix)]
#[test]
fn world_size_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("demo");
    jar(&directory.join("server.jar"));
    fs::create_dir(directory.join("world")).unwrap();
    fs::write(directory.join("world/level.dat"), b"12345").unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("region.mca"), b"not counted").unwrap();
    symlink(outside.path(), directory.join("world/linked-world")).unwrap();
    assert_eq!(discover(temp.path()).unwrap()[0].map_size_bytes, Some(5));
}

#[test]
fn per_server_config_is_explicit_strict_and_round_trips_without_environment_reads() {
    use jartender::config::ServerConfig;
    let root = TempDir::new().unwrap();
    let directory = root.path().join("demo");
    jar(&directory.join("server.jar"));
    assert!(ServerConfig::load(root.path(), "demo").unwrap().is_none());
    let config = ServerConfig {
        java: Some("./jdk/bin/java".into()),
        jvm_args: vec!["-Xmx2G".into(), "-Dmessage=with spaces".into()],
        server_args: vec!["--port".into(), "25566".into()],
        jar: Some("server.jar".into()),
        ..Default::default()
    };
    config.save(root.path(), "demo").unwrap();
    let config = ServerConfig::load(root.path(), "demo").unwrap().unwrap();
    let mut options = LaunchOptions::new("demo", "host-java");
    config.apply(&directory, &mut options).unwrap();
    assert_eq!(options.java, directory.join("./jdk/bin/java"));
    assert_eq!(options.jvm_args[1], "-Dmessage=with spaces");
    assert_eq!(options.server_args[1], "25566");
    assert!(!directory.join("eula.txt").exists());
    assert!(ServerConfig::load(root.path(), "../outside").is_err());
    for value in [
        r#"{"jvm_arg":["typo"]}"#,
        r#"{"java":""}"#,
        r#"{"jar":"a.jar","args_file":"b.txt"}"#,
    ] {
        fs::write(directory.join("jartender.json"), value).unwrap();
        assert!(ServerConfig::load(root.path(), "demo").is_err());
    }
    fs::write(directory.join("jartender.json"), " ".repeat(65537)).unwrap();
    assert!(ServerConfig::load(root.path(), "demo").is_err());
}

#[test]
fn hybrid_detection_and_minecraft_versions_come_from_jar_metadata() {
    let root = TempDir::new().unwrap();
    let cases: [(&str, &str, &str, &str, &str); 5] = [
        (
            "forge-looking",
            "arclight",
            "1.21.1",
            "Main-Class: io.izzel.arclight.server.Launcher\n\n",
            "META-INF/installer.json",
        ),
        (
            "fabric-looking",
            "banner",
            "1.20.1",
            "Main-Class: com.mohistmc.banner.BannerLauncher\nFabric-Minecraft-Version: 1.20.1\n\n",
            "fabric.mod.json",
        ),
        (
            "paper-looking",
            "mohist",
            "1.20.1",
            "Main-Class: com.mohistmc.MohistMCStart\nGit-Branch: 1.20.1\n\n",
            "unused",
        ),
        (
            "vanilla-looking",
            "catserver",
            "1.16.5",
            "Main-Class: catserver.server.CatServerLaunch\nGit-Branch: 1.16.5\n\n",
            "unused",
        ),
        (
            "generic-bootstrap",
            "catserver",
            "1.16.5",
            "Main-Class: cpw.mods.bootstraplauncher.BootstrapLauncher\nGit-Branch: 1.16.5\n\n",
            "catserver/server/CatServerLaunch.class",
        ),
    ];
    for (name, _, _, manifest, metadata) in cases {
        let contents = match metadata {
            "META-INF/installer.json" => r#"{"installer":{"minecraft":"1.21.1","forge":"52.1.1"}}"#,
            "fabric.mod.json" => r#"{"id":"banner-launcher","depends":{"minecraft":"1.20.1"}}"#,
            _ => "",
        };
        jar_entries(
            &root.path().join(name).join("renamed.jar"),
            &[("META-INF/MANIFEST.MF", manifest), (metadata, contents)],
        );
    }
    let servers = discover(root.path()).unwrap();
    for (name, kind, mc, _, _) in cases {
        let server = servers.iter().find(|s| s.name == name).unwrap();
        assert_eq!(server.server_type, kind);
        assert_eq!(server.version.as_deref(), Some(mc));
        let plan = prepare_launch(root.path(), &LaunchOptions::new(name, "java")).unwrap();
        assert_eq!(plan.server_type, kind);
        assert!(plan.requires_eula);
        assert!(!plan.directory.join("eula.txt").exists());
    }
    jar_entries(
        &root.path().join("range-only").join("banner-1.20.1.jar"),
        &[
            (
                "META-INF/MANIFEST.MF",
                "Main-Class: net.fabricmc.loader.impl.launch.server.FabricServerLauncher\nGit-Branch: main\n\nName: bundled/library\nMinecraft-Version: 1.19.4\n",
            ),
            (
                "fabric.mod.json",
                r#"{"id":"banner","depends":{"minecraft":">=1.20"}}"#,
            ),
        ],
    );
    let servers = discover(root.path()).unwrap();
    let server = servers.iter().find(|s| s.name == "range-only").unwrap();
    assert_eq!(server.server_type, "banner");
    assert_eq!(server.version, None);
}

#[test]
fn metadata_distinguishes_paper_forks_and_proxies_independently_of_directory_names() {
    let temp = TempDir::new().unwrap();
    let cases = [
        (
            "six",
            "org.leavesmc.leavesclip.Main",
            Some("hash\t1.21.8\t1.21.8/leaves-1.21.8.jar\n"),
            "leaves",
        ),
        (
            "seven",
            "io.papermc.paperclip.Main",
            Some("hash\t1.21.8\t1.21.8/leaves-1.21.8.jar\n"),
            "leaves",
        ),
        (
            "one",
            "io.papermc.paperclip.Main",
            Some("hash\t26.3\t26.3/purpur-26.3.jar\n"),
            "purpur",
        ),
        (
            "two",
            "io.papermc.paperclip.Main",
            Some("hash\t26.3\t26.3/folia-26.3.jar\n"),
            "folia",
        ),
        (
            "three",
            "io.papermc.paperclip.Main",
            Some("hash\t26.3\t26.3/paper-26.3.jar\n"),
            "paper",
        ),
        (
            "four",
            "com.velocitypowered.proxy.Velocity",
            None,
            "velocity",
        ),
        ("five", "net.md_5.bungee.Bootstrap", None, "bungeecord"),
        (
            "velocity-named-but-not-a-proxy",
            "unknown.Main",
            None,
            "unknown",
        ),
    ];
    for (name, main, index, expected) in cases {
        metadata_jar(&temp.path().join(name).join("renamed.jar"), main, index);
        let plan = prepare_launch(temp.path(), &LaunchOptions::new(name, "java")).unwrap();
        assert_eq!(plan.server_type, expected);
        assert_eq!(
            plan.requires_eula,
            !matches!(expected, "velocity" | "bungeecord")
        );
        assert_eq!(
            plan.args.iter().any(|arg| arg == "nogui"),
            plan.requires_eula
        );
    }
    let servers = discover(temp.path()).unwrap();
    assert_eq!(servers.len(), cases.len());
    for (name, _, _, expected) in cases {
        assert_eq!(
            servers.iter().find(|s| s.name == name).unwrap().server_type,
            expected
        );
    }
}

#[test]
fn platform_argument_layouts_are_discovered_and_preserve_jvm_and_program_argument_order() {
    let temp = TempDir::new().unwrap();
    for (name, base, expected) in [
        ("one", "libraries/net/neoforged/neoforge", "neoforge"),
        ("two", "libraries/net/minecraftforge/forge", "forge"),
    ] {
        let directory = temp.path().join(name);
        let relative = argument_layout(&directory, base, "test-version");
        fs::write(directory.join("user_jvm_args.txt"), "-Xmx512M\n").unwrap();
        let mut options = LaunchOptions::new(name, "java");
        options.jvm_args.push("-Xmx1G".into());
        options.server_args = vec!["--port".into(), "25566".into()];
        let plan = prepare_launch(temp.path(), &options).unwrap();
        assert_eq!(plan.server_type, expected);
        assert!(plan.requires_eula);
        assert_eq!(plan.args[0], "@user_jvm_args.txt");
        assert_eq!(plan.args[1], "-Xmx1G");
        assert_eq!(
            plan.args[2],
            format!(
                "@{}",
                directory.canonicalize().unwrap().join(&relative).display()
            )
            .as_str()
        );
        assert_eq!(&plan.args[3..], ["nogui", "--port", "25566"]);
        assert!(!plan.args.iter().any(|arg| arg == "-jar"));
        assert!(!directory.join("eula.txt").exists());
        assert!(matches!(
            plan.check_eula(),
            Err(Error::EulaNotAccepted { .. })
        ));
    }
    for server in discover(temp.path()).unwrap() {
        assert!(server.jars.is_empty());
        assert_eq!(server.argument_files.len(), 1);
        assert_eq!(
            server.argument_files[0].file_name().unwrap(),
            platform_args()
        );
    }
}

#[test]
fn multiple_argument_versions_require_an_explicit_choice_and_scripts_are_never_executed() {
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("demo");
    let base = "libraries/net/neoforged/neoforge";
    let first = argument_layout(&directory, base, "one");
    argument_layout(&directory, base, "two");
    let mut options = LaunchOptions::new("demo", "java");
    assert!(
        matches!(prepare_launch(temp.path(), &options), Err(Error::AmbiguousArgumentFiles { candidates, .. }) if candidates.len() == 2)
    );
    assert_eq!(discover(temp.path()).unwrap()[0].server_type, "ambiguous");
    options.args_file = Some(first);
    assert_eq!(
        prepare_launch(temp.path(), &options).unwrap().server_type,
        "neoforge"
    );
    options.jar = Some("server.jar".into());
    assert!(matches!(
        prepare_launch(temp.path(), &options),
        Err(Error::InvalidInput { .. })
    ));
    options.jar = None;
    for path in ["../outside.txt", "/outside.txt", "missing.txt"] {
        options.args_file = Some(path.into());
        assert!(prepare_launch(temp.path(), &options).is_err());
    }
    let other = TempDir::new().unwrap();
    fs::create_dir(other.path().join("scripts-only")).unwrap();
    fs::write(other.path().join("scripts-only/run.sh"), "exit 99\n").unwrap();
    assert!(discover(other.path()).unwrap().is_empty());
    assert!(matches!(
        prepare_launch(other.path(), &LaunchOptions::new("scripts-only", "java")),
        Err(Error::NoJar { .. })
    ));
}

#[test]
fn selected_proxy_needs_no_eula_and_never_creates_or_rewrites_it() {
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("mixed");
    metadata_jar(
        &directory.join("proxy.jar"),
        "com.velocitypowered.proxy.Velocity",
        None,
    );
    jar(&directory.join("minecraft.jar"));
    let mut options = LaunchOptions::new("mixed", "java");
    options.jar = Some("proxy.jar".into());
    let proxy = prepare_launch(temp.path(), &options).unwrap();
    proxy.check_eula().unwrap();
    proxy.accept_eula().unwrap();
    assert!(!directory.join("eula.txt").exists());
    fs::write(directory.join("eula.txt"), "# unchanged\neula=false\n").unwrap();
    proxy.check_eula().unwrap();
    proxy.accept_eula().unwrap();
    assert_eq!(
        fs::read_to_string(directory.join("eula.txt")).unwrap(),
        "# unchanged\neula=false\n"
    );
    options.jar = Some("minecraft.jar".into());
    let minecraft = prepare_launch(temp.path(), &options).unwrap();
    assert!(matches!(
        minecraft.check_eula(),
        Err(Error::EulaNotAccepted { .. })
    ));
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
fn scan_reports_skipped_directories_legacy_selection_and_eula_state() {
    use jartender::{EulaState, scan};

    let temp = TempDir::new().unwrap();
    let root = temp.path().join("servers");
    metadata_jar(&root.join("demo/a.jar"), "a.Main", None);
    metadata_jar(&root.join("demo/b.jar"), "net.minecraft.bundler.Main", None);
    fs::write(root.join("demo/eula.txt"), "eula=false\neula=true\n").unwrap();
    metadata_jar(
        &root.join("proxy/server.jar"),
        "com.velocitypowered.proxy.Velocity",
        None,
    );
    let inventory = temp.path().join("list.json");
    fs::write(&inventory, r#"[{"server_name":"demo","jar_name":"b.jar"}]"#).unwrap();

    let plain = scan(&root, None).unwrap();
    assert!(plain.skipped.is_empty());
    let demo = plain.servers.iter().find(|s| s.name == "demo").unwrap();
    assert_eq!(demo.server_type, "ambiguous");
    assert_eq!(demo.selected_jar, None);
    assert_eq!(demo.eula, EulaState::Accepted);
    let proxy = plain.servers.iter().find(|s| s.name == "proxy").unwrap();
    assert_eq!(proxy.eula, EulaState::NotRequired);

    let picked = scan(&root, Some(&inventory)).unwrap();
    let demo = picked.servers.iter().find(|s| s.name == "demo").unwrap();
    assert_eq!(demo.server_type, "vanilla");
    assert_eq!(demo.selected_jar.as_deref(), Some(Path::new("b.jar")));

    // A relative inventory would silently depend on the host's working directory.
    assert!(matches!(
        scan(&root, Some(Path::new("list.json"))),
        Err(Error::InvalidInput {
            field: "legacy inventory",
            ..
        })
    ));
}

#[cfg(unix)]
#[test]
fn scan_reports_unreadable_directories_instead_of_failing() {
    use jartender::scan;
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let root = temp.path().join("servers");
    metadata_jar(&root.join("good/server.jar"), "unknown.Main", None);
    let locked = root.join("locked");
    metadata_jar(&locked.join("server.jar"), "unknown.Main", None);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let result = scan(&root, None);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let result = result.unwrap();
    assert_eq!(result.servers.len(), 1);
    assert_eq!(result.servers[0].name, "good");
    assert_eq!(result.skipped.len(), 1);
    assert!(result.skipped[0].path.ends_with("locked"));
}

#[test]
fn library_results_can_cross_worker_thread_boundaries() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Error>();
    send_sync::<LaunchOptions>();
    send_sync::<LaunchPlan>();
    send_sync::<InstallEvent>();
    send_sync::<InstallOutcome>();
    send_sync::<jartender::Discovery>();
    send_sync::<jartender::EulaState>();
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
