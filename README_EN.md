# Jartender

Install and run Minecraft servers from the terminal.

[中文](README.md)

The Rust CLI has three commands: `list`, `run`, and `install`. Server directories are the source of truth; there is no inventory to initialize or synchronize. The original Python version remains available through `python3 jartender.py`.

## Build

Install [stable Rust](https://rustup.rs/), then:

```sh
cargo build --release --locked
./target/release/jartender --help
```

Or install with `cargo install --path . --locked`. Windows produces `target/release/jartender.exe`.

Running a server requires a compatible Java version. Jartender itself, `list`, and `install` require neither Python nor Java.

## Use

```sh
jartender list
jartender run survival
jartender install fabric --minecraft 1.21.4 --name survival
```

Before the first run, read the [Minecraft EULA](https://aka.ms/MinecraftEULA) and set `eula=true` in the server's `eula.txt`. If you agree, you can instead explicitly run:

```sh
jartender run survival --accept-eula
```

Type server commands such as `stop` directly into the attached console. The Minecraft graphical console is disabled by default; use `--gui` to enable it.

### Directories and legacy data

All commands accept `--root DIR`, before or after the subcommand:

```sh
jartender --root /srv/minecraft list
jartender run survival --root /srv/minecraft
```

The root is chosen from `--root`, then `serverpath` in the calling directory's legacy `config.json`, then `./Servers`. Relative paths are resolved against the calling directory; `~` is supported. No config is required and legacy files are never rewritten. The executable's location does not affect resolution.

`list` reads top-level JARs in each immediate server directory, excluding filenames containing `installer` and skipping hidden directories. It does not require `eula.txt`, `libraries`, or `server.properties`. Types come from JAR manifests; unrecognized types stay `unknown`. Listing never starts Java, accesses the network, or writes files. Forge layouts using argument files without a top-level launcher JAR are not supported yet.

### Running

```sh
jartender run survival --java "/path/to/jdk/bin/java"
jartender run survival --jar fabric-server-launch.jar
jartender run survival --jvm-arg=-Xms1G --jvm-arg=-Xmx2G -- --port 25566
jartender run survival --dry-run
```

Java selection: `--java`, then `JAVA_HOME/bin/java`, then `java` on `PATH`. Jartender does not choose among installed JDKs or maintain `java_list.json`; use `--java` for another version. Explicit Java paths are resolved before changing directories.

A single candidate JAR is selected automatically. With multiple candidates, an existing selection in the calling directory's legacy `list.json` can disambiguate; stale absolute `jar_path` values are ignored. Otherwise `--jar FILE` is required. An explicit JAR always wins and must be a filename inside the server directory. No inventory maintenance is required.

Servers run in their own directory with inherited stdin, stdout, and stderr. On Unix, Java replaces Jartender, preserving its PID and native signal/exit behavior. On Windows, Jartender waits for Java and returns its exit code.

### Fabric installation

Installation uses the [official Fabric Meta API](https://github.com/FabricMC/fabric-meta). Defaults select the latest stable compatible Loader and latest stable installer. Pin all versions for a specific result:

```sh
jartender install fabric --minecraft 1.21.4 --loader 0.16.10 --installer 1.0.3 --name survival
```

Installation prepares `server.jar` and `eula.txt` with `eula=false`. It does not run Java or generate a world. On first launch, the Fabric launcher downloads the remaining server files, so network access is required.

Existing directories are never overwritten. Downloads have time and size limits. Failed downloads and invalid JARs are cleaned up. Forced termination can leave a hidden `.jartender-install-*` directory; remove it after confirming no install is active. Failed installs are not retried automatically.

## Scripting

`list --json` returns a name-sorted array with `name`, `server_type`, and `jars` (candidate filenames relative to the server directory). Multiple candidates are labeled `ambiguous`. An empty root returns `[]`; a missing or unreadable root is an error.

`run --dry-run` prints JSON containing `directory`, `java`, `args`, and `accept_eula`. It never starts Java or writes files, even with `--accept-eula`.

Exit codes: `0` for success, `1` for operational errors, `2` for usage errors. After launch, Java's exit code is preserved; Unix also preserves signal termination. Jartender diagnostics go to stderr; list and dry-run results go to stdout. The running server owns its console streams.

## Use as a Rust library

The same crate provides a library and CLI. Disable the CLI dependency in a host launcher:

```toml
[dependencies]
jartender = { path = "../Jartender", default-features = false }
```

```rust,no_run
use jartender::{LaunchOptions, check_eula, discover, prepare_launch};
use std::path::Path;

fn main() -> jartender::Result<()> {
    let root = Path::new("/srv/minecraft");
    let servers = discover(root)?;
    let options = LaunchOptions::new("survival", "/path/to/jdk/bin/java");
    let plan = prepare_launch(root, &options)?;
    check_eula(&plan.directory)?;
    let mut child = plan.to_command().spawn()?;
    let status = child.wait()?;
    Ok(())
}
```

- `discover` and `prepare_launch` only read files and return server records or a `LaunchPlan`.
- `LaunchPlan::to_command()` creates an unstarted `std::process::Command`. The host owns stdio, spawning, stopping, and process lifetime. Library calls never replace or exit the host.
- `check_eula` is read-only. After obtaining the user's agreement, the host can explicitly call `accept_eula`.
- `install::install_fabric` takes a `FabricRequest` and progress callback and returns an `InstallOutcome`. Callbacks receive `InstallEvent` values. Return `ControlFlow::Break(())` to request cancellation and clean up staging files. Cancellation is checked between events/download chunks; blocking HTTP operations are timeout-bound.
- The public `Error` enum distinguishes ambiguous JARs, missing EULA consent, download failures, cancellation, and other errors. Results and errors can cross worker-thread boundaries.

The library requires absolute root and legacy inventory paths. Java must be absolute or a bare executable name. It does not read current-directory config, `JAVA_HOME`, or home-directory settings, and never prints to stdout/stderr. A bare name such as `java` is searched on PATH when the host starts the command. Legacy inventories are used only when explicitly supplied through `LaunchOptions::legacy_list`.

All APIs are synchronous; GUI hosts should download on their own worker threads. This is a Rust library, with no C ABI or other language bindings yet; other languages can use the CLI. See [examples/launcher.rs](examples/launcher.rs) for a complete host example.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --no-default-features
cargo doc --locked --no-default-features --no-deps
```

One crate: `lib.rs` exposes core APIs; `main.rs` owns argument parsing, default configuration, terminal output, and process replacement. Tests use temporary directories, mock Java, and local HTTP responses, and exercise the public API as an external crate. CI is configured for Windows, macOS, and Linux and checks both full CLI and library-only builds.

This version supports JAR server discovery, execution, and Fabric launcher installation. Forge installation, Java downloads, plugin/world management, and a TUI are not implemented.

## License

[GNU GPL v3](LICENSE). Parts of the original Python implementation referenced [Prism Launcher](https://github.com/PrismLauncher/PrismLauncher) and [MultiMC](https://github.com/MultiMC/MultiMC5).
