use clap::{Args, Parser, Subcommand, ValueEnum};
use jartender::{
    LaunchOptions,
    install::{FabricRequest, InstallEvent},
};
use serde::Deserialize;
use std::ffi::OsString;
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::path::{Component, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Parser)]
#[command(version, about, arg_required_else_help = true)]
struct Cli {
    /// Server root (default: config.json's serverpath, or ./Servers)
    #[arg(long, global = true, value_name = "DIR")]
    root: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// List servers without starting Java or changing files
    List {
        /// Print machine-readable JSON
        #[arg(long)]
        json: bool,
    },
    /// Run a server in its directory with an attached console
    Run(RunArgs),
    /// Download a Fabric server into a new directory
    Install(InstallArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Server directory name under the server root
    server: String,
    /// Select a JAR when the directory contains several
    #[arg(long, value_name = "FILE")]
    jar: Option<PathBuf>,
    /// Java executable (default: JAVA_HOME/bin/java, then PATH)
    #[arg(long, value_name = "PATH")]
    java: Option<PathBuf>,
    /// JVM argument, repeatable (example: --jvm-arg=-Xmx2G)
    #[arg(long, value_name = "ARG", allow_hyphen_values = true)]
    jvm_arg: Vec<OsString>,
    /// Enable the Minecraft server's graphical console
    #[arg(long)]
    gui: bool,
    /// Accept https://aka.ms/MinecraftEULA for this server
    #[arg(long)]
    accept_eula: bool,
    /// Print the launch plan as JSON without changing files or starting Java
    #[arg(long)]
    dry_run: bool,
    /// Arguments passed to the server after --
    #[arg(last = true)]
    server_args: Vec<OsString>,
}

#[derive(Clone, ValueEnum)]
enum Kind {
    Fabric,
}

#[derive(Args)]
struct InstallArgs {
    #[arg(value_enum)]
    kind: Kind,
    /// Minecraft version
    #[arg(long, value_name = "VERSION")]
    minecraft: String,
    /// New server directory name
    #[arg(long)]
    name: String,
    /// Fabric Loader version (default: latest stable compatible version)
    #[arg(long, value_name = "VERSION")]
    loader: Option<String>,
    /// Fabric launcher installer version (default: latest stable)
    #[arg(long, value_name = "VERSION")]
    installer: Option<String>,
}

#[derive(Deserialize)]
struct LegacyConfig {
    serverpath: PathBuf,
}

fn expand_home(path: PathBuf) -> Result<PathBuf> {
    if path.components().next() != Some(Component::Normal("~".as_ref())) {
        return Ok(path);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("cannot expand ~: HOME/USERPROFILE is unset")?;
    Ok(PathBuf::from(home).join(path.strip_prefix("~")?))
}

fn root(explicit: Option<PathBuf>) -> Result<(PathBuf, PathBuf)> {
    let cwd = std::env::current_dir()?;
    let path = cwd.join("config.json");
    let root = match explicit {
        Some(path) => path,
        None => match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice::<LegacyConfig>(&bytes)
                    .map_err(|e| {
                        format!(
                            "cannot read {}: {e}; fix it or use --root DIR",
                            path.display()
                        )
                    })?
                    .serverpath
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => PathBuf::from("Servers"),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display()).into()),
        },
    };
    let root = expand_home(root)?;
    Ok((
        if root.is_absolute() {
            root
        } else {
            cwd.join(root)
        },
        cwd.join("list.json"),
    ))
}

fn java(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = match explicit {
        Some(path) => expand_home(path)?,
        None => match std::env::var_os("JAVA_HOME").filter(|s| !s.is_empty()) {
            Some(home) => PathBuf::from(home).join("bin").join(if cfg!(windows) {
                "java.exe"
            } else {
                "java"
            }),
            None => return Ok(PathBuf::from("java")),
        },
    };
    if path.components().count() == 1 && !path.is_absolute() {
        return Ok(path);
    }
    path.canonicalize()
        .map_err(|e| format!("cannot use Java {}: {e}; use --java PATH", path.display()).into())
}

fn execute(cli: Cli) -> Result<i32> {
    let (root, legacy_list) = root(cli.root)?;
    match cli.command {
        Action::List { json } => {
            let servers = jartender::discover(&root)?;
            let mut out = io::BufWriter::new(io::stdout().lock());
            if json {
                serde_json::to_writer_pretty(&mut out, &servers)?;
                writeln!(out)?;
            } else {
                writeln!(out, "NAME\tTYPE\tJAR")?;
                for server in servers {
                    let jars = server
                        .jars
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(
                        out,
                        "{}\t{}\t{}",
                        server.name.escape_debug(),
                        server.server_type,
                        jars.escape_debug()
                    )?;
                }
            }
            out.flush()?;
            Ok(0)
        }
        Action::Run(options) => {
            let request = LaunchOptions {
                server: options.server.clone(),
                java: java(options.java)?,
                jar: options.jar,
                legacy_list: Some(legacy_list),
                jvm_args: options.jvm_arg,
                gui: options.gui,
                server_args: options.server_args,
            };
            let plan = jartender::prepare_launch(&root, &request)?;
            if options.dry_run {
                let display = serde_json::json!({
                    "directory": plan.directory, "java": plan.java,
                    "args": plan.args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
                    "accept_eula": options.accept_eula,
                });
                writeln!(
                    io::stdout().lock(),
                    "{}",
                    serde_json::to_string_pretty(&display)?
                )?;
                return Ok(0);
            }
            if options.accept_eula {
                jartender::accept_eula(&plan.directory)?;
            }
            jartender::check_eula(&plan.directory)?;
            eprintln!(
                "jartender: running {:?} in {} using {}",
                options.server,
                plan.directory.display(),
                plan.java.display()
            );
            let mut command = plan.to_command();
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                Err(format!(
                    "cannot start Java {}: {}; use --java PATH",
                    plan.java.display(),
                    command.exec()
                )
                .into())
            }
            #[cfg(not(unix))]
            {
                let status = command.status().map_err(|e| {
                    format!(
                        "cannot start Java {}: {e}; use --java PATH",
                        plan.java.display()
                    )
                })?;
                Ok(status.code().unwrap_or(1))
            }
        }
        Action::Install(options) => {
            let request = FabricRequest {
                minecraft: options.minecraft,
                name: options.name,
                loader: options.loader,
                installer: options.installer,
            };
            let outcome = jartender::install::install_fabric(&root, &request, |event| {
                match event {
                    InstallEvent::Resolving { minecraft } => {
                        eprintln!("jartender: resolving Fabric for Minecraft {minecraft}")
                    }
                    InstallEvent::Downloading { loader, installer } => eprintln!(
                        "jartender: downloading Fabric Loader {loader}, launcher {installer}"
                    ),
                    _ => {}
                }
                ControlFlow::Continue(())
            })?;
            eprintln!(
                "jartender: installed Fabric in {}; review the EULA, then run this server",
                outcome.directory.display()
            );
            Ok(0)
        }
    }
}

fn main() {
    let code = match execute(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            let io_kind = error
                .downcast_ref::<io::Error>()
                .map(io::Error::kind)
                .or_else(|| {
                    error
                        .downcast_ref::<serde_json::Error>()
                        .and_then(|e| e.io_error_kind())
                });
            if io_kind == Some(io::ErrorKind::BrokenPipe) {
                0
            } else {
                eprintln!("jartender: {error}");
                match error.downcast_ref::<jartender::Error>() {
                    Some(jartender::Error::AmbiguousJars { .. }) => {
                        eprintln!("jartender: choose one with --jar FILE")
                    }
                    Some(jartender::Error::EulaNotAccepted { .. }) => eprintln!(
                        "jartender: set eula=true after reading the EULA, or run with --accept-eula"
                    ),
                    _ => {}
                }
                1
            }
        }
    };
    std::process::exit(code);
}
