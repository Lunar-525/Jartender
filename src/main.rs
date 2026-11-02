mod list;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum, ValueHint};
use jartender::{
    LaunchOptions,
    install::{InstallEvent, InstallKind, InstallRequest},
    network::ProxyConfig,
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
    /// Print Forge/NeoForge installer output when installing
    #[arg(long, global = true)]
    verbose: bool,
    /// Root directory for servers (config.json's serverpath or ./Servers) and java install (./java)
    #[arg(long, global = true, value_name = "DIR", value_hint = ValueHint::DirPath)]
    root: Option<PathBuf>,
    /// Proxy URL for every download (overrides config.json and environment)
    #[arg(long, global = true, value_name = "URL", conflicts_with = "no_proxy")]
    proxy: Option<String>,
    /// Connect directly, ignoring saved and environment proxy settings
    #[arg(long, global = true)]
    no_proxy: bool,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// List servers without starting Java or changing files
    List {
        /// Print the aligned table instead of the default JSON
        #[arg(long, conflicts_with = "json")]
        table: bool,
        /// Print JSON (the default; accepted for compatibility)
        #[arg(long)]
        json: bool,
    },
    /// Run a server in its directory with an attached console
    #[command(
        after_help = "Defaults come from <SERVER>/jartender.json (java, jvm_args, server_args, jar, args_file, gui);\ncommand-line options override them.\n\nExamples:\n  jartender run survival\n  jartender run survival --gui\n  jartender run survival --dry-run"
    )]
    Run(RunArgs),
    /// Install a server into a new directory
    #[command(
        after_help = "Examples:\n  jartender install 26.3 fabric latest survival\n  jartender install 26.3 forge 66.1.0 survival --java /path/to/jdk/bin/java\n\nVERSION is a build or loader version. Arclight and CatServer also accept an\nasset filename from `jartender versions`. `latest` (or `-`) picks the default;\nArclight asks for a filename when several platforms exist.\n\nDiscover versions with:\n  jartender versions\n  jartender versions <MINECRAFT> <KIND>"
    )]
    Install(InstallArgs),
    /// Query Minecraft releases, supported cores, or loader builds
    #[command(
        after_help = "KIND values: arclight, mohist, banner, catserver, folia, leaves, purpur, paper, fabric, forge, neoforge

Examples:
  jartender versions                      # Minecraft releases
  jartender versions paper                # Minecraft versions Paper supports
  jartender versions 1.21.4               # cores that can install 1.21.4
  jartender versions 26.3 paper           # Paper builds for 26.3
  jartender versions 26.3 paper --table"
    )]
    Versions(VersionsArgs),
    /// Discover, query and install Java runtimes
    Java(JavaArgs),
    /// Generate a shell completion script on stdout
    Completions {
        /// Shell to generate the script for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Args)]
struct VersionsArgs {
    /// Minecraft version (lists cores that support it), or a KIND (lists its versions)
    #[arg(value_name = "MINECRAFT|KIND")]
    minecraft: Option<String>,
    /// Core or loader (required with MINECRAFT)
    #[arg(value_name = "KIND")]
    kind: Option<String>,
    /// Include snapshots/prereleases and list every result (no default limit)
    #[arg(long)]
    all: bool,
    /// Maximum entries to print; 0 lists everything (default: 10)
    #[arg(long, value_name = "N")]
    limit: Option<usize>,
    /// Print the aligned table instead of the default JSON
    #[arg(long, conflicts_with = "json")]
    table: bool,
    /// Print JSON (the default; accepted for compatibility)
    #[arg(long)]
    json: bool,
}

fn github_token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
}

fn parse_kind(value: &str) -> Option<Kind> {
    Kind::from_str(value, false).ok()
}

fn usage_error(message: impl Into<String>) -> ! {
    let mut command = Cli::command();
    command
        .error(clap::error::ErrorKind::InvalidValue, message.into())
        .exit()
}

/// Paper/Folia/Leaves answer 404 for Minecraft versions they never published.
fn missing_version(error: &jartender::Error) -> bool {
    matches!(
        error,
        jartender::Error::Download {
            // Fabric answers 400 for unknown game versions; Fill/Purpur/Leaves use 404.
            source: ureq::Error::StatusCode(400 | 404),
            ..
        }
    )
}

/// Explain an empty loader query with the Minecraft versions the core supports.
fn hint_supported(
    kind: InstallKind,
    minecraft: &str,
    all: bool,
    token: Option<&str>,
    proxy: &ProxyConfig,
) {
    let Ok(supported) =
        jartender::versions::supported_minecraft_with_proxy(kind, false, token, proxy)
    else {
        return;
    };
    if supported.is_empty() {
        return;
    }
    let versions: Vec<&str> = supported.iter().map(|v| v.version.as_str()).collect();
    let argument = kind.name().to_ascii_lowercase();
    // A snapshot/prerelease Minecraft version is hidden from the stable-only list above.
    let unstable = !all
        && !versions.contains(&minecraft)
        && jartender::versions::supported_minecraft_with_proxy(kind, true, token, proxy)
            .map(|any| any.iter().any(|v| v.version == minecraft))
            .unwrap_or(false);
    if versions.contains(&minecraft) || unstable {
        eprintln!(
            "jartender: {} has no {}builds for Minecraft {minecraft}",
            kind.name(),
            if all { "" } else { "stable " }
        );
        if !all {
            eprintln!("jartender: try --all to include prerelease/experimental builds");
        }
    } else {
        let shown = versions
            .iter()
            .take(10)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if versions.len() > 10 { " …" } else { "" };
        eprintln!(
            "jartender: {} does not support Minecraft {minecraft}",
            kind.name()
        );
        eprintln!(
            "jartender: supported versions: {shown}{more} ({} total)",
            versions.len()
        );
        eprintln!("jartender: run `jartender versions {argument}` for the full list");
    }
}

/// Calendar releases, legacy `1.x` releases, and snapshots/prereleases (`24w14a`, `1.21.4-pre1`).
fn looks_like_version(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with(|c: char| c.is_ascii_digit())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(&b))
}

/// `versions <MINECRAFT>` lists the cores that can install that version.
fn show_kinds(minecraft: &str, options: &VersionsArgs, proxy: &ProxyConfig) -> Result<i32> {
    let token = github_token();
    let supported = jartender::versions::kinds_for_with_proxy(minecraft, token.as_deref(), proxy)?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    if options.table {
        if supported.is_empty() {
            writeln!(out, "No core supports Minecraft {minecraft}.")?;
        } else {
            use comfy_table::{ContentArrangement, Table, presets::ASCII_FULL_CONDENSED};
            let mut table = Table::new();
            table
                .load_style(ASCII_FULL_CONDENSED)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(["KIND", "STABLE", "LATEST"]);
            for support in &supported {
                table.add_row([
                    support.kind.name().to_ascii_lowercase(),
                    if support.stable { "yes" } else { "no" }.to_owned(),
                    support.latest.clone().unwrap_or_else(|| "-".to_owned()),
                ]);
            }
            writeln!(out, "Cores for Minecraft {minecraft}\n{table}")?;
            writeln!(out, "{} cores", supported.len())?;
        }
    } else {
        let rows: Vec<_> = supported
            .iter()
            .map(|support| {
                serde_json::json!({
                    "kind": support.kind.name().to_ascii_lowercase(),
                    "stable": support.stable,
                    "latest": support.latest,
                })
            })
            .collect();
        serde_json::to_writer_pretty(&mut out, &rows)?;
        writeln!(out)?;
        if supported.is_empty() {
            eprintln!(
                "jartender: no core supports Minecraft {minecraft}; `jartender versions` lists Minecraft releases"
            );
        }
    }
    out.flush()?;
    Ok(0)
}

fn show_versions(options: VersionsArgs, proxy: &ProxyConfig) -> Result<i32> {
    let all = options.all;
    let token = github_token();
    let (versions, context) = match (options.minecraft.as_deref(), options.kind.as_deref()) {
        (None, None) => (
            jartender::versions::query(&jartender::versions::VersionQuery {
                include_unstable: all,
                github_token: token.clone(),
                proxy: proxy.clone(),
                ..Default::default()
            })?,
            if all {
                "Minecraft versions"
            } else {
                "Minecraft releases"
            }
            .to_owned(),
        ),
        (Some(value), None) => match parse_kind(value) {
            Some(kind) => {
                let kind = InstallKind::from(kind);
                (
                    jartender::versions::supported_minecraft_with_proxy(
                        kind,
                        all,
                        token.as_deref(),
                        proxy,
                    )?,
                    format!("{} supported Minecraft versions", kind.name()),
                )
            }
            None if looks_like_version(value) => return show_kinds(value, &options, proxy),
            None => usage_error(format!(
                "unknown KIND {value:?}; use `versions <MINECRAFT> <KIND>` or a KIND such as `versions paper`"
            )),
        },
        (None, Some(_)) => usage_error("provide a MINECRAFT version together with KIND"),
        (Some(minecraft), Some(value)) => match parse_kind(value) {
            Some(kind) => {
                let kind = InstallKind::from(kind);
                let versions =
                    match jartender::versions::query(&jartender::versions::VersionQuery {
                        minecraft: Some(minecraft.to_owned()),
                        kind: Some(kind),
                        include_unstable: all,
                        github_token: token.clone(),
                        proxy: proxy.clone(),
                    }) {
                        Ok(versions) => versions,
                        // Unsupported Minecraft versions surface as 404 from Fill/Purpur/Leaves.
                        Err(error) if missing_version(&error) => Vec::new(),
                        Err(error) => return Err(error.into()),
                    };
                if versions.is_empty() {
                    hint_supported(kind, minecraft, all, token.as_deref(), proxy);
                }
                (
                    versions,
                    format!("{} for Minecraft {minecraft}", kind.name()),
                )
            }
            None => usage_error(format!(
                "unknown KIND {value:?}; use `jartender versions <MINECRAFT> <KIND>`"
            )),
        },
    };
    // Keep the default output short; `--all` (or `--limit 0`) prints everything.
    let limit = options.limit.unwrap_or(if all { 0 } else { 10 });
    let total = versions.len();
    let shown = if limit == 0 { total } else { total.min(limit) };
    let visible = &versions[..shown];
    let mut out = io::BufWriter::new(io::stdout().lock());
    if options.table {
        if versions.is_empty() {
            writeln!(out, "No matching versions found.")?;
        } else {
            use comfy_table::{ContentArrangement, Table, presets::ASCII_FULL_CONDENSED};
            let mut table = Table::new();
            table
                .load_style(ASCII_FULL_CONDENSED)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(["VERSION", "STABLE", "RECOMMENDED"]);
            for version in visible {
                table.add_row([
                    version.version.as_str(),
                    if version.stable { "yes" } else { "no" },
                    if version.recommended { "yes" } else { "-" },
                ]);
            }
            writeln!(out, "{context}\n{table}")?;
            if shown < total {
                writeln!(
                    out,
                    "Showing {shown} of {total} versions; use --all or --limit 0 to list everything."
                )?;
            } else {
                writeln!(out, "{total} versions")?;
            }
        }
    } else {
        serde_json::to_writer_pretty(&mut out, visible)?;
        writeln!(out)?;
        if shown < total {
            eprintln!(
                "jartender: showing {shown} of {total} versions; use --all or --limit 0 to list everything"
            );
        }
    }
    out.flush()?;
    Ok(0)
}

#[derive(Args)]
struct RunArgs {
    /// Server directory name under the server root
    server: String,
    /// Select a JAR when the directory contains several
    #[arg(long, value_name = "FILE")]
    jar: Option<PathBuf>,
    /// Select a Forge/NeoForge argument file relative to the server directory
    #[arg(long, value_name = "FILE", conflicts_with = "jar")]
    args_file: Option<PathBuf>,
    /// Java executable (default: jartender.json's java, then JAVA_HOME/bin/java, then PATH)
    #[arg(long, value_name = "PATH", value_hint = ValueHint::ExecutablePath)]
    java: Option<PathBuf>,
    /// JVM argument, repeatable (example: --jvm-arg=-Xmx2G)
    #[arg(long, value_name = "ARG", allow_hyphen_values = true)]
    jvm_arg: Vec<OsString>,
    /// Enable the Minecraft server's graphical console (--gui=false overrides a saved gui)
    #[arg(long,num_args=0..=1,default_missing_value="true",require_equals=true)]
    gui: Option<bool>,
    /// Accept https://aka.ms/MinecraftEULA for this server (not needed for Velocity/BungeeCord)
    #[arg(long)]
    accept_eula: bool,
    /// Print the launch plan as JSON without changing files or starting Java
    #[arg(long)]
    dry_run: bool,
    /// Arguments passed to the server after --
    #[arg(last = true)]
    server_args: Vec<OsString>,
}

#[derive(Args)]
struct InstallArgs {
    /// Minecraft version
    #[arg(value_name = "MINECRAFT")]
    minecraft: String,
    /// Server core or loader
    #[arg(value_enum, value_name = "KIND")]
    kind: Kind,
    /// Build number, loader version, or Arclight/CatServer asset filename; `latest` or `-` picks the default
    #[arg(value_name = "VERSION")]
    version: String,
    /// New server directory name
    #[arg(value_name = "NAME")]
    name: String,
    /// Fabric launcher installer version (default: latest stable)
    #[arg(long, value_name = "VERSION")]
    installer: Option<String>,
    /// Java executable for the Forge/NeoForge installer (default: JAVA_HOME/bin/java, then PATH)
    #[arg(long, value_name = "PATH", value_hint = ValueHint::ExecutablePath)]
    java: Option<PathBuf>,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct JavaArgs {
    #[command(subcommand)]
    command: Option<JavaCommand>,
    #[command(flatten)]
    scan: JavaScanArgs,
}

#[derive(Args)]
struct JavaScanArgs {
    /// Additional executable, JDK home, or installation directory; repeatable
    #[arg(long, value_name = "PATH")]
    path: Vec<PathBuf>,
    /// Inspect only --path entries
    #[arg(long, requires = "path")]
    no_system: bool,
    /// Print an aligned table instead of the default JSON
    #[arg(long, conflicts_with = "json")]
    table: bool,
    /// Print runtimes and skipped probes as JSON (the default)
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct JavaOutputArgs {
    /// Print an aligned table instead of the default JSON
    #[arg(long, conflicts_with = "json")]
    table: bool,
    /// Print JSON (the default)
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum JavaCommand {
    /// Discover installed local runtimes (also the default for `java`)
    List(JavaScanArgs),
    /// List supported JVM distributions without accessing the network
    Types(JavaOutputArgs),
    /// List JVM types for a Java major, or one type's downloadable versions
    #[command(
        after_help = "Examples:\n  jartender java version 21                 # types with Java 21 builds\n  jartender java version 21 openjdk         # up to 10 OpenJDK 21 builds\n  jartender java versions openjdk 21 --all  # every OpenJDK 21 build"
    )]
    Version {
        /// Java major, for example 21
        major: u32,
        /// Optional TYPE; lists that distribution's versions instead of all types
        #[arg(value_name = "TYPE", value_parser = java_distribution_parser())]
        distribution: Option<String>,
        #[command(flatten)]
        output: JavaOutputArgs,
    },
    /// Query downloadable versions for this OS and architecture
    Versions {
        /// JVM distribution; see `jartender java types`
        #[arg(value_name = "TYPE", value_parser = java_distribution_parser())]
        distribution: String,
        /// Optional Java major; limits results (Oracle queries that major's public archive)
        major: Option<u32>,
        /// Maximum results; 0 lists everything (ignored with --all)
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// List every result
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        output: JavaOutputArgs,
    },
    /// Download and verify a JDK or JRE without executing it
    #[command(
        after_help = "Examples:\n  jartender java install 25 oracle\n  jartender java install 21.0.2 openjdk java21\n  jartender java install 25 oracle --dry-run"
    )]
    Install(JavaInstallArgs),
}

#[derive(Args)]
struct JavaInstallArgs {
    /// Java major or exact version, e.g. 25 or 21.0.2
    version: String,
    /// JVM distribution; see `jartender java types`
    #[arg(value_name = "TYPE", value_parser = java_distribution_parser())]
    distribution: String,
    /// Directory name (default: distribution-version-os-arch)
    name: Option<String>,
    /// Resolve the download without installing
    #[arg(long)]
    dry_run: bool,
}

fn java_distribution_parser() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(jartender::java::install::providers().iter().map(
        |provider| {
            let info = provider.info();
            let value = clap::builder::PossibleValue::new(info.distribution.name());
            info.aliases
                .iter()
                .fold(value, |value, alias| value.alias(*alias))
        },
    ))
}

fn java_distribution(value: &str) -> jartender::java::install::Distribution {
    jartender::java::install::providers()
        .iter()
        .find_map(|provider| {
            let info = provider.info();
            (info.distribution.name() == value || info.aliases.contains(&value))
                .then_some(info.distribution)
        })
        .expect("clap validated the registered distribution")
}

impl From<Kind> for InstallKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Arclight => Self::Arclight,
            Kind::Mohist => Self::Mohist,
            Kind::Banner => Self::Banner,
            Kind::Catserver => Self::CatServer,
            Kind::Folia => Self::Folia,
            Kind::Leaves => Self::Leaves,
            Kind::Purpur => Self::Purpur,
            Kind::Paper => Self::Paper,
            Kind::Fabric => Self::Fabric,
            Kind::Forge => Self::Forge,
            Kind::Neoforge => Self::NeoForge,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Arclight,
    Mohist,
    Banner,
    Catserver,
    Folia,
    Leaves,
    Purpur,
    Paper,
    Fabric,
    Forge,
    #[value(name = "neoforge")]
    Neoforge,
}

#[derive(Deserialize)]
struct LegacyConfig {
    serverpath: Option<PathBuf>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct DownloadConfig {
    proxy: Option<String>,
    no_proxy: bool,
}

fn download_proxy(explicit: Option<&str>, direct: bool) -> Result<ProxyConfig> {
    if direct {
        return Ok(ProxyConfig::Direct);
    }
    if let Some(url) = explicit {
        return Ok(ProxyConfig::new(url)?);
    }
    let path = std::env::current_dir()?.join("config.json");
    let saved: DownloadConfig = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "cannot read {}: {e}; fix it or use --proxy URL / --no-proxy",
                path.display()
            )
        })?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => DownloadConfig::default(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display()).into()),
    };
    if saved.no_proxy && saved.proxy.is_some() {
        return Err("config.json: proxy and no_proxy=true are mutually exclusive".into());
    }
    match (saved.no_proxy, saved.proxy.as_deref()) {
        (true, _) => Ok(ProxyConfig::Direct),
        (_, Some(url)) => Ok(ProxyConfig::new(url)?),
        _ => Ok(ProxyConfig::Environment),
    }
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
            Ok(bytes) => serde_json::from_slice::<LegacyConfig>(&bytes)
                .map_err(|e| {
                    format!(
                        "cannot read {}: {e}; fix it or use --root DIR",
                        path.display()
                    )
                })?
                .serverpath
                .unwrap_or_else(|| PathBuf::from("Servers")),
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
    let Cli {
        verbose,
        root: explicit_root,
        proxy,
        no_proxy,
        command,
    } = cli;
    // Local discovery, launch and completions do not depend on download settings.
    let network = match &command {
        Action::Install(_) | Action::Versions(_) => download_proxy(proxy.as_deref(), no_proxy)?,
        Action::Java(JavaArgs {
            command:
                Some(
                    JavaCommand::Install(_)
                    | JavaCommand::Versions { .. }
                    | JavaCommand::Version { .. },
                ),
            ..
        }) => download_proxy(proxy.as_deref(), no_proxy)?,
        _ => ProxyConfig::Environment,
    };
    match command {
        Action::Java(options) => show_java(options, explicit_root, &network),
        Action::Completions { shell } => {
            let mut script = Vec::new();
            clap_complete::generate(shell, &mut Cli::command(), "jartender", &mut script);
            io::stdout().lock().write_all(&script)?;
            Ok(0)
        }
        Action::Versions(options) => show_versions(options, &network),
        Action::List { table, json: _ } => {
            let (root, legacy_list) = root(explicit_root)?;
            let discovery = jartender::scan(&root, Some(&legacy_list))?;
            for skipped in &discovery.skipped {
                eprintln!(
                    "jartender: skipped {}: {}",
                    skipped.path.display(),
                    skipped.reason
                );
            }
            let mut out = io::BufWriter::new(io::stdout().lock());
            if table {
                list::write(&mut out, &discovery.servers)?;
            } else {
                serde_json::to_writer_pretty(&mut out, &discovery.servers)?;
                writeln!(out)?;
            }
            out.flush()?;
            Ok(0)
        }
        Action::Run(options) => {
            let (root, legacy_list) = root(explicit_root)?;
            let saved =
                jartender::config::ServerConfig::load(&root, &options.server)?.unwrap_or_default();
            let mut request = LaunchOptions::new(&options.server, "java");
            saved.apply(&root.join(&options.server), &mut request)?;
            request.legacy_list = Some(legacy_list);
            if options.java.is_some() || saved.java.is_none() {
                request.java = java(options.java)?;
            }
            if options.jar.is_some() {
                request.jar = options.jar;
                request.args_file = None;
            }
            if options.args_file.is_some() {
                request.args_file = options.args_file;
                request.jar = None;
            }
            if !options.jvm_arg.is_empty() {
                request.jvm_args = options.jvm_arg;
            }
            if !options.server_args.is_empty() {
                request.server_args = options.server_args;
            }
            if let Some(gui) = options.gui {
                request.gui = gui;
            }
            let plan = jartender::prepare_launch(&root, &request)?;
            if options.dry_run {
                let display = serde_json::json!({
                    "directory": plan.directory, "java": plan.java,
                    "args": plan.args.iter().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
                    "accept_eula": options.accept_eula,
                    "server_type": plan.server_type, "requires_eula": plan.requires_eula,
                });
                writeln!(
                    io::stdout().lock(),
                    "{}",
                    serde_json::to_string_pretty(&display)?
                )?;
                return Ok(0);
            }
            if options.accept_eula {
                plan.accept_eula()?;
            }
            plan.check_eula()?;
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
            let InstallArgs {
                minecraft,
                kind,
                version,
                name,
                installer,
                java: java_path,
            } = options;
            let (root, _) = root(explicit_root)?;
            let version = match version.as_str() {
                "latest" | "-" => None,
                value => Some(value.to_owned()),
            };
            let mut request = InstallRequest::new(kind.into(), name, minecraft);
            request.version = version;
            request.github_token = github_token();
            request.proxy = network;
            match kind {
                Kind::Paper
                | Kind::Purpur
                | Kind::Leaves
                | Kind::Folia
                | Kind::Arclight
                | Kind::Mohist
                | Kind::Banner
                | Kind::Catserver => {
                    if java_path.is_some() || installer.is_some() {
                        return Err(format!(
                            "{} installs download a server JAR and do not use --java or --installer",
                            InstallKind::from(kind).name()
                        )
                        .into());
                    }
                }
                Kind::Fabric => {
                    if java_path.is_some() {
                        return Err(
                            "--java is only used for Forge and NeoForge installs; use --installer for Fabric"
                                .into(),
                        );
                    }
                    request.installer = installer;
                }
                Kind::Forge | Kind::Neoforge => {
                    if installer.is_some() {
                        return Err("--installer is only used for Fabric installs".into());
                    }
                    request.java = Some(java(java_path)?);
                }
            }
            let kind = request.kind;
            let outcome = jartender::install::install(&root, &request, |event| {
                match event {
                    InstallEvent::ChecksumUnavailable { component, version } => eprintln!(
                        "jartender: {component} {version}: upstream publishes no checksum; checking size and JAR"
                    ),
                    InstallEvent::Resolving { minecraft } => eprintln!(
                        "jartender: resolving {} for Minecraft {minecraft}",
                        kind.name()
                    ),
                    InstallEvent::Downloading { loader, installer } => eprintln!(
                        "jartender: downloading Fabric Loader {loader}, launcher {installer}"
                    ),
                    InstallEvent::DownloadingServer { component, version } => {
                        // Build numbers read better with "build"; Arclight/CatServer name assets.
                        let label = if version.bytes().all(|b| b.is_ascii_digit()) {
                            "build "
                        } else {
                            ""
                        };
                        eprintln!("jartender: downloading {component} {label}{version}")
                    }
                    InstallEvent::Installing { component, version } => eprintln!(
                        "jartender: running the {component} {version} installer; it downloads libraries and may take minutes"
                    ),
                    InstallEvent::Output { line } if verbose => eprintln!("{line}"),
                    _ => {}
                }
                ControlFlow::Continue(())
            })?;
            eprintln!(
                "jartender: installed {} in {}; review the EULA, then run this server",
                kind.name(),
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
                    Some(jartender::Error::AmbiguousArgumentFiles { .. }) => {
                        eprintln!("jartender: choose one with --args-file FILE")
                    }
                    Some(jartender::Error::EulaNotAccepted { .. }) => eprintln!(
                        "jartender: set eula=true after reading the EULA, or run with --accept-eula"
                    ),
                    Some(jartender::Error::JavaTooOld { required, .. }) => eprintln!(
                        "jartender: use --java PATH with a newer Java, or run `jartender java version {required}` to find installable runtimes"
                    ),
                    _ => {}
                }
                1
            }
        }
    };
    std::process::exit(code);
}

fn scan_java(
    JavaScanArgs {
        path,
        no_system,
        table,
        json: _,
    }: JavaScanArgs,
) -> Result<i32> {
    let mut options = if no_system {
        jartender::java::ScanOptions::default()
    } else {
        jartender::java::ScanOptions::system()
    };
    let cwd = std::env::current_dir()?;
    if !no_system {
        options.paths.push(cwd.join("java"));
    }
    for path in path {
        let path = expand_home(path)?;
        let path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        if !path.exists() {
            return Err(format!("Java search path does not exist: {}", path.display()).into());
        }
        options.paths.push(path);
    }
    let found = jartender::java::scan(&options)?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    if !table {
        serde_json::to_writer_pretty(&mut out, &found)?;
        writeln!(out)?;
    } else {
        use comfy_table::{Table, presets::ASCII_FULL_CONDENSED};
        let mut table = Table::new();
        table
            .load_style(ASCII_FULL_CONDENSED)
            .set_header(["VERSION", "MAJOR", "VENDOR", "ARCH", "PATH"]);
        for runtime in &found.runtimes {
            table.add_row([
                runtime.version.clone(),
                runtime.major.to_string(),
                runtime.vendor.clone().unwrap_or_else(|| "-".into()),
                runtime.arch.clone().unwrap_or_else(|| "-".into()),
                runtime.path.display().to_string(),
            ]);
        }
        writeln!(out, "{table}\n{} Java runtimes", found.runtimes.len())?;
        for skipped in found.skipped {
            eprintln!(
                "jartender: skipped Java {}: {}",
                skipped.path.display(),
                skipped.reason
            );
        }
    }
    out.flush()?;
    Ok(0)
}

fn install_java(
    options: JavaInstallArgs,
    explicit_root: Option<PathBuf>,
    proxy: &ProxyConfig,
) -> Result<i32> {
    let JavaInstallArgs {
        version,
        distribution,
        name,
        dry_run,
    } = options;
    use jartender::java::install::Request;
    let distribution = java_distribution(&distribution);
    let mut request = Request::new(distribution, version)?;
    request.proxy = proxy.clone();
    if let Some(name) = name {
        request.name = name;
    }
    let cwd = std::env::current_dir()?;
    let path = expand_home(explicit_root.unwrap_or_else(|| cwd.join("java")))?;
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    if dry_run {
        let plan = jartender::java::install::resolve(&request)?;
        let mut display = serde_json::to_value(&plan)?;
        display["directory"] = serde_json::to_value(path.join(&request.name))?;
        writeln!(
            io::stdout().lock(),
            "{}",
            serde_json::to_string_pretty(&display)?
        )?;
    } else {
        use jartender::java::install::Event;
        let display = jartender::java::install::providers()
            .iter()
            .find(|provider| provider.info().distribution == distribution)
            .map(|provider| provider.info().name)
            .unwrap_or_else(|| distribution.name());
        let outcome = jartender::java::install::install(&path, &request, |event| {
            match event {
                Event::Resolving => eprintln!("jartender: resolving {display} {}", request.version),
                Event::Downloading { .. } => {
                    eprintln!(
                        "jartender: downloading {}",
                        if distribution == jartender::java::install::Distribution::Mojang {
                            "Mojang runtime files"
                        } else {
                            "JDK archive"
                        }
                    )
                }
                Event::Extracting => {
                    eprintln!(
                        "jartender: {}",
                        if distribution == jartender::java::install::Distribution::Mojang {
                            "verifying Mojang runtime files"
                        } else {
                            "verifying and extracting JDK"
                        }
                    )
                }
                _ => (),
            }
            ControlFlow::Continue(())
        })?;
        writeln!(
            io::stdout().lock(),
            "{}",
            serde_json::to_string_pretty(&outcome)?
        )?;
    }
    Ok(0)
}

fn show_java(options: JavaArgs, root: Option<PathBuf>, proxy: &ProxyConfig) -> Result<i32> {
    use jartender::java::install;
    match options.command {
        None => scan_java(options.scan),
        Some(JavaCommand::List(scan)) => scan_java(scan),
        Some(JavaCommand::Install(options)) => install_java(options, root, proxy),
        Some(JavaCommand::Types(output)) => show_java_types(
            install::providers().iter().map(|p| p.info()).collect(),
            output,
        ),
        Some(JavaCommand::Version {
            major,
            distribution,
            output,
        }) => match distribution {
            Some(distribution) => show_java_versions(
                java_distribution(&distribution),
                Some(major),
                10,
                false,
                output,
                proxy,
            ),
            None => show_java_types(
                install::types_for_major_with_proxy(
                    major,
                    install::Os::current()?,
                    install::Arch::current()?,
                    proxy,
                )?,
                output,
            ),
        },
        Some(JavaCommand::Versions {
            distribution,
            major,
            limit,
            all,
            output,
        }) => show_java_versions(
            java_distribution(&distribution),
            major,
            limit,
            all,
            output,
            proxy,
        ),
    }
}

fn show_java_versions(
    distribution: jartender::java::install::Distribution,
    major: Option<u32>,
    limit: usize,
    all: bool,
    output: JavaOutputArgs,
    proxy: &ProxyConfig,
) -> Result<i32> {
    use jartender::java::install;
    let mut query = install::VersionQuery::new(distribution)?;
    query.major = major;
    query.proxy = proxy.clone();
    let mut rows = install::versions(&query)?;
    let total = rows.len();
    if !all && limit != 0 {
        rows.truncate(limit);
    }
    if output.table {
        let mut table = comfy_table::Table::new();
        table
            .load_style(comfy_table::presets::ASCII_FULL_CONDENSED)
            .set_header(["TYPE", "VERSION", "OS", "ARCH", "LATEST"]);
        for row in &rows {
            table.add_row([
                row.distribution.name(),
                &row.version,
                row.os.name(),
                row.arch.name(),
                if row.latest { "yes" } else { "-" },
            ]);
        }
        writeln!(io::stdout().lock(), "{table}")?;
    } else {
        writeln!(
            io::stdout().lock(),
            "{}",
            serde_json::to_string_pretty(&rows)?
        )?;
    }
    if rows.len() < total {
        let major = major.map(|major| format!(" {major}")).unwrap_or_default();
        eprintln!(
            "jartender: showing {} of {total} versions; use `jartender java versions {}{major} --all` to list everything",
            rows.len(),
            distribution.name()
        );
    }
    Ok(0)
}

fn show_java_types(
    rows: Vec<jartender::java::install::TypeInfo>,
    output: JavaOutputArgs,
) -> Result<i32> {
    if output.table {
        let mut table = comfy_table::Table::new();
        table
            .load_style(comfy_table::presets::ASCII_FULL_CONDENSED)
            .set_header(["TYPE", "NAME", "SOURCE"]);
        for row in &rows {
            table.add_row([row.distribution.name(), row.name, row.source]);
        }
        writeln!(io::stdout().lock(), "{table}")?;
    } else {
        writeln!(
            io::stdout().lock(),
            "{}",
            serde_json::to_string_pretty(&rows)?
        )?;
    }
    Ok(0)
}
