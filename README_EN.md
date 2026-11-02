# Jartender

Install and run Minecraft servers from the terminal.

The Tauri 2 desktop app reuses the same Rust core. See [JarTender Desktop](desktop/README.md) for setup, usage, and packaging.

[中文](README.md)

The Rust CLI provides `list`, `run`, `install`, `versions`, `java`, and `completions`. Server directories are the source of truth; there is no inventory to initialize or synchronize. The original Python version remains available through `python3 jartender.py`.

## Build

Install [stable Rust](https://rustup.rs/), then:

```sh
cargo build --release --locked
./target/release/jartender --help
```

Or install with `cargo install --path . --locked`. Windows produces `target/release/jartender.exe`.

Running a server requires a compatible Java version. Jartender itself and `list` need no Java; Fabric/Paper/Folia/Purpur/Leaves and hybrid launcher downloads do not either, while Forge/NeoForge installation runs the official installer and therefore requires Java.

## Use

```sh
jartender list
jartender run survival
jartender install 26.3 fabric latest survival
```

Before the first run, read the [Minecraft EULA](https://aka.ms/MinecraftEULA) and set `eula=true` in the server's `eula.txt`. If you agree, you can instead explicitly run:

```sh
jartender run survival --accept-eula
```

Type server commands such as `stop` directly into the attached console. The Minecraft graphical console is disabled by default; use `--gui` to enable it.

### Download proxy

Set a proxy with a global option, which also works after the subcommand:

```sh
jartender --proxy http://127.0.0.1:7890 install 26.3 fabric latest survival
jartender --proxy socks5h://127.0.0.1:1080 java install 21 openjdk
jartender --proxy http://127.0.0.1:7890 versions paper
jartender --no-proxy java versions oracle
```

To save a default, add `proxy` to `config.json` in the directory where you invoke the command. It can coexist with `serverpath`; a proxy-only file keeps the default `./Servers` server root:

```json
{
  "proxy": "http://127.0.0.1:7890"
}
```

Precedence: `--proxy` / `--no-proxy`, then `config.json`, then environment variables. `--no-proxy` forces direct connections; save `{"no_proxy": true}` for a persistent direct default. These two settings are mutually exclusive. Without an explicit setting, `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` (including lowercase names) and `NO_PROXY` retain their existing behavior. Explicit proxies route every target through the proxy, ignoring environment bypass lists. `--root` does not change the configuration file location.

The policy covers Minecraft/core/Java version queries, metadata, checksums, server JARs, installers, JDK archives, and dependencies fetched by Forge/NeoForge installers. Supported schemes are `http://`, `https://`, `socks5://` and `socks5h://`. URLs may include credentials; percent-encode special characters. Forge/NeoForge's Java installers require an unauthenticated HTTP or SOCKS5 proxy; unsupported combinations fail with an error. Proxy failures never cause a direct retry. This setting applies to queries and installation in the Rust tool; it does not change networking inside running Minecraft servers, plugins, or the legacy Python tool.

Library hosts set the `proxy` field on install requests and version queries with `network::ProxyConfig::{new, Direct, Environment}`. Supported Minecraft and Java type queries provide `versions::supported_minecraft_with_proxy` and `java::install::types_for_major_with_proxy`. The library does not read the CLI's `config.json` or modify process environment variables.

### Discover local Java runtimes

```sh
jartender java
jartender java list
jartender java --table
jartender java list --table
jartender java --no-system --path /path/to/jdks
```

Search locations follow the Python implementation: `JAVA_HOME`, `PATH`, system JDK directories, SDKMAN/asdf, user JDKs, PrismLauncher and the current directory's `java/`. macOS also checks Homebrew; Linux checks common installation paths and Snap; Windows checks vendor directories and both registry views under the user and machine hives. Traversal is depth-limited, with canonical Java paths deduplicated. Additional `--path` entries may name executables, JDK homes or installation directories.

Probes run `java -XshowSettings:properties -version`, never a server or compiler. Each probe defaults to a three-second deadline and retains at most 64 KiB per output stream. Failures are reported without hiding working runtimes. JSON is the default (`--json` remains supported). Use `--table` to show version, major, vendor, architecture and executable path. JSON returns `runtimes` and `skipped`, including `vendor_version`, `home` and `is_64bit`. Discovery performs no network access, JDK download, environment mutation or `java_list.json` writes. It does not automatically switch Java major versions.

### Install Java

Supported distributions are **Oracle JDK** (`oracle`, alias `official`), **official OpenJDK GA builds** (`openjdk`), **Mojang Java Runtime** (`mojang`, alias `minecraft`), **Eclipse Temurin** (`adoptium`, alias `temurin`), **Azul Zulu** (`zulu`, alias `azul`), **IBM Semeru Open** (`semeru`, aliases `ibm` / `openj9`) and **GraalVM Community** (`graalvm`, aliases `graal` / `graalvm-community`):

```sh
jartender java types                    # supported JVM distributions
jartender java types --table
jartender java version 21               # JVM types available for Java 21
jartender java version 21 --table
jartender java version 21 openjdk       # OpenJDK versions for Java 21
jartender java version 8 zulu --table   # MAJOR first, optional TYPE
jartender java versions oracle          # current public downloads
jartender java versions oracle 21       # public archive for a major
jartender java versions openjdk         # native GA archives, latest 10 entries
jartender java versions openjdk 21 --all --table
jartender java versions adoptium 21
jartender java versions zulu 21 --table
jartender java versions semeru 21
jartender java versions graalvm 21
jartender java versions mojang
jartender java install 25 oracle
jartender java install 21 openjdk
jartender java install 21 adoptium
jartender java install 21 zulu
jartender java install 21 semeru
jartender java install 21 graalvm
jartender java install 25 mojang
jartender java install 21.0.2 openjdk java21
jartender java install 25 oracle --dry-run
jartender --root /path/to/jdks java install 25 oracle
```

The default root is `./java/` in the calling directory, independent of server `config.json`. Names default to `<distribution>-<requested-version>-<os>-<arch>`. Native Linux/macOS/Windows and x86_64/aarch64 archives are selected automatically. Missing platform builds are errors, with no architecture or distribution substitution. `--root` selects the Java installation root for this command; existing server install syntax remains supported.

A major version selects the newest publicly downloadable package from that source; exact versions must match. Oracle prefers its currently linked public latest URL, falling back to the newest public archive build for that major when current updates require an account. The installed version is reported explicitly. OpenJDK uses jdk.java.net GA archives, excluding EA and third-party distributions. These archives do not promise Java 8 binaries or ongoing LTS updates for every major. Authenticated Oracle downloads are not automated.

Adoptium uses `api.adoptium.net/v3`, selecting `image_type=jdk` Temurin packages and verifying `package.checksum` (SHA-256). Zulu uses the Azul Metadata API, lists packages, then reads `sha256_hash` from the package detail. Semeru reads GitHub releases from `ibmruntimes/semeruN-binaries`, selects `ibm-semeru-open-jdk_<arch>_<os>_<version>.tar.gz` (`.zip` on Windows), and verifies the release asset's `sha256:` digest. GraalVM Community reads GitHub releases from `graalvm/graalvm-ce-builds`, selects `graalvm-community-jdk-<version>_<os>-<arch>_bin.tar.gz` (`.zip` on Windows, with no Windows aarch64 build), excludes drafts and prereleases, and verifies the release asset's `sha256:` digest; queries use the anonymous GitHub API and can hit rate limits. Mojang uses its official `java-runtime` manifest and installs a JRE as individual files (macOS under `jre.bundle/Contents/Home`, no `javac`), downloading 100+ small files concurrently in about 20 seconds.

Installation requires official checksums: SHA-256 for archive packages and per-file SHA-1 from the official manifest for Mojang. After extraction or writing, path/link/expanded-size checks and `release` version/OS/architecture checks apply; JDKs additionally require `java` and `javac`, while the Mojang JRE only requires `java`. Existing targets are never overwritten; failure and cancellation clean staging files. Installation does not execute the downloaded Java or change the registry, `JAVA_HOME` or `PATH`. JSON output includes `directory`, `home`, `java` and the actual `version`. Copy the executable path into per-server configuration as needed.

Default installations are discovered by `jartender java`; for custom roots, use `jartender java --no-system --path /path/to/jdks`. This separate discovery step executes version-only probes. Library hosts use `java::install::{Request, Distribution, resolve, install}`; callbacks can cancel with `ControlFlow::Break(())`, and hosts may explicitly select OS/architecture.

`java version <MAJOR> [TYPE]` with a TYPE is equivalent to `java versions <TYPE> <MAJOR>` (10 entries by default); use `java versions <TYPE> <MAJOR> --all` for the full list. `version` takes the major first, while `versions` takes the TYPE first.

Java queries return JSON arrays by default; use `--table` for tables. Versions are sorted newest first and filtered for the current OS/architecture. The default limit is 10; `--limit 0` or `--all` returns everything. `latest: true` marks the newest entry from that source; Oracle's mutable latest URL identifies only the major. Both `java list` and bare `java` discover local runtimes. The old `install java` entry point has been removed.

Library queries use `java::install::{providers, Provider, VersionQuery, AvailableVersion, versions}`. `Provider` supplies type metadata, available versions and package resolution. CLI type values and completion come from the same `providers()` registry. To add a built-in JVM distribution, extend `Distribution`, implement/register its provider, and reuse downloading, SHA-256 verification and extraction.


### Per-server launch configuration

Save defaults in `Servers/<name>/jartender.json`:

```json
{
  "java": "/path/to/jdk/bin/java",
  "jvm_args": ["-Xms1G", "-Xmx2G"],
  "server_args": ["--port", "25566"],
  "gui": false
}
```

Choose an executable path from `jartender java`. Optional `jar` and `args_file` fields select a launch target and are mutually exclusive; omit them for automatic discovery. Java may be an absolute path, a bare PATH executable such as `java`, or a path relative to the server directory such as `./runtime/bin/java`. Each array element is one argument, preserving spaces.

Precedence is CLI options, then server configuration, then existing defaults. Without either Java override, `JAVA_HOME` and `PATH` still apply. An explicit `--jar` or `--args-file` clears the other saved target; nonempty CLI JVM/program argument lists replace the corresponding saved array. Use `--gui=false` to override saved `gui: true`. Running does not save CLI options. Missing configuration preserves existing behavior; malformed JSON, unknown fields and conflicting fields are errors.

`list` does not read launch configuration. `run --dry-run` reads defaults and builds a plan without executing Java or writing configuration/EULA files. Library hosts explicitly call `config::ServerConfig::load(root, name)` and `apply(directory, &mut LaunchOptions)`; `save(root, name)` atomically replaces the configuration when requested. `prepare_launch` continues to use only its supplied options. Java discovery is available through `java::ScanOptions::system()` and `java::scan`; default empty `ScanOptions` with explicit paths gives hosts full control of discovery sources.

### Directories and legacy data

`--root` applies to `list`, `run`, `install`, and `java install` (before or after the subcommand); `versions`, `completions`, and local Java discovery (`java` / `java list`) do not use it, so pass `--path` for other Java locations. For example:

```sh
jartender --root /srv/minecraft list
jartender run survival --root /srv/minecraft
```

The root is chosen from `--root`, then `serverpath` in the calling directory's legacy `config.json`, then `./Servers`. Relative paths are resolved against the calling directory; `~` is supported. No config is required and legacy files are never rewritten. The executable's location does not affect resolution.

`list` reads top-level JARs in each immediate server directory, excluding filenames containing `installer` and skipping hidden directories. It does not require `eula.txt`, `libraries`, or `server.properties`. Types come from JAR main classes, with the Paperclip payload index distinguishing Paper, Purpur, Folia, and Leaves. Velocity and BungeeCord are recognized; other types stay `unknown`. Without a top-level JAR, standard Forge/NeoForge platform argument files are also discovered. Directories that cannot be read are skipped and reported on stderr, so the rest of the listing still succeeds. Listing never starts Java, accesses the network, or writes files.

### Running

```sh
jartender run survival --java "/path/to/jdk/bin/java"
jartender run survival --jar fabric-server-launch.jar
jartender run survival --jvm-arg=-Xms1G --jvm-arg=-Xmx2G -- --port 25566
jartender run survival --dry-run
```

Java selection: `--java`, then the server configuration's `java`, then `JAVA_HOME/bin/java`, then `java` on `PATH`. Jartender does not choose among installed JDKs or maintain `java_list.json`; use `--java` for another version. Explicit Java paths are resolved before changing directories.

A single candidate JAR is selected automatically. With multiple candidates, an existing selection in the calling directory's legacy `list.json` can disambiguate; stale absolute `jar_path` values are ignored. Otherwise `--jar FILE` is required. `list` follows the same rule: when the legacy inventory resolves the directory it reports the core type and `selected_jar`; otherwise the type is `ambiguous`. An explicit JAR always wins and must be a filename inside the server directory. No inventory maintenance is required.

Without a top-level JAR, Jartender checks `libraries/net/neoforged/neoforge/<version>/`, `libraries/net/minecraftforge/forge/<version>/`, and the older NeoForge `libraries/net/neoforged/forge/<version>/` for `unix_args.txt` (macOS/Linux) or `win_args.txt` (Windows). Multiple installed versions require an explicit selection:

```sh
jartender run neoforge-26.3 --args-file libraries/net/neoforged/neoforge/26.3.0.100/unix_args.txt
```

`--args-file` is relative to the server directory and mutually exclusive with `--jar`. Argument-file launches load an existing `user_jvm_args.txt`, then explicit JVM arguments, then the platform argument file. Java parses the files; Jartender never executes `run.sh` or `run.bat`. A top-level JAR remains the default launch target when present.

Recognized Velocity/BungeeCord proxies need no Minecraft EULA file and receive no automatic `nogui` argument. Their `--accept-eula` flag creates or changes no file. Minecraft cores and unknown JARs still require EULA consent.

Servers run in their own directory with inherited stdin, stdout, and stderr. On Unix, Java replaces Jartender, preserving its PID and native signal/exit behavior. On Windows, Jartender waits for Java and returns its exit code.

### Installation

The order is `install <MINECRAFT> <KIND> <KIND_VERSION> <NAME>`:

```sh
jartender install 1.21.11 folia latest     folia-survival
jartender install 1.21.8 leaves latest     leaves-survival
jartender install 26.3 purpur   latest     purpur-survival
jartender install 26.3 paper    latest     paper-survival
jartender install 26.3 fabric   latest     survival
jartender install 26.3 forge    66.1.0     survival
jartender install 26.3 neoforge 26.3.0.100 survival
```

**Arclight / Mohist / Banner / CatServer** download official executable launcher JARs through the shared staging, verification and publication flow. Installation runs no Java; each core prepares its runtime libraries on the first `run`. The EULA remains `false`, existing directories are never overwritten, and failed or cancelled downloads clean staging files.

```sh
jartender versions 1.21.1 arclight
jartender install 1.21.1 arclight arclight-forge-1.21.1-1.0.1-8ec9529.jar arclight-survival
jartender install 1.20.1 mohist latest mohist-survival
jartender install 1.20.1 banner latest banner-survival
jartender install 1.16.5 catserver latest cat-survival
jartender list --table
```

Arclight uses [official GitHub Releases](https://github.com/IzzelAliz/Arclight/releases). Version values are complete JAR filenames identifying Forge, NeoForge or Fabric. When `latest` has several platform choices, select a filename from `versions` and pass it as the third install argument. Drafts are excluded; prereleases require `--all` to appear in queries. Anonymous GitHub requests are limited to 60 per hour; set `GITHUB_TOKEN` or `GH_TOKEN` to raise the limit. Installs stop paging after the newest matching release only for the default `latest`; an explicit asset filename may live in an older release, so the search keeps paging. `versions` always keeps paging to list every build.

Mohist / Banner use the [current official API](https://mohistmc.com/mohistmc-api), sort by numeric build ID, and verify the official SHA-256. Install a build number or `latest` (`-` also works). This API has no stability channels: `stable: true` means a published build, not a stability rating; `--all` returns the same list. Banner uses the official Banner project, without silently substituting a renamed project or fork.

CatServer uses [official GitHub Releases](https://github.com/Luohuayu/CatServer/releases), matching Minecraft versions in asset names; legacy universal JARs use the release target branch. Explicit versions are complete asset filenames. GitHub asset SHA-256 digests are verified when published. For older Arclight/CatServer releases without digests, the CLI says so and the library emits `ChecksumUnavailable`. In that case, checks cover official asset size, the executable JAR, core identity and readable Minecraft version; no digest verification is claimed.

Discovery uses main classes, class entries or Fabric metadata, and reads Minecraft versions from embedded installation metadata or manifest attributes. Renaming a JAR does not change identification; unknown versions remain `null`. Installation rejects a mismatching core or a known mismatching Minecraft version. Library constructors are `InstallRequest::arclight`, `mohist`, `banner` and `catserver`, alongside `versions::query` and `discover`. These installs do not use `--java` / `--installer`; choose Java at `run` time.

**Folia** shares the [official PaperMC Fill v3 API](https://docs.papermc.io/misc/downloads-service/) and implementation with Paper, defaulting to the newest stable build for the requested Minecraft version. Query with `jartender versions 1.21.11 folia`; add `--all` to include Alpha/Beta. Install a build number or `latest` (`-` also works). Without stable builds, `latest` fails instead of selecting a prerelease or changing the Minecraft version; prereleases require an explicit build number. Downloads are checked against official size and SHA-256 metadata and the executable JAR manifest. Installation runs no Java and preserves `eula=false`. Library hosts use `InstallRequest::folia("name", "1.21.11")`; `list` and `run` recognize Folia.

**Leaves** uses the [official Leaves download API](https://api.leavesmc.org/v2/projects/leaves), defaulting to the newest `default` channel build for the requested Minecraft version. Query regular builds with `jartender versions 1.21.8 leaves`; add `--all` for experimental builds. Install a build number or `latest` (`-` also works); an explicit build number may select an experimental build. The official SHA-256 and executable JAR manifest are verified before publishing `server.jar` with `eula=false`, without running Java. Missing regular builds are an error; the Minecraft version is never changed automatically. Library hosts use `InstallRequest::leaves("name", "1.21.8")`. Both Leavesclip and Paperclip carrying a Leaves payload are identified as `leaves`.

**Purpur** uses the [official Purpur API](https://purpurmc.org/docs/purpur/). Query build numbers with `jartender versions 26.3 purpur`, then install a specific build or `latest` (`-` also works). The official latest value is resolved to a pinned build number; project, Minecraft version, build number and `SUCCESS` status are checked before downloading. The official MD5 is verified, then the shared Paper JAR validation and publication flow is used. Installation runs no Java and preserves `eula=false`; runtime preparation happens on first launch. Library hosts use `InstallRequest::purpur("name", "26.3")`.

Purpur does not provide stable/prerelease channels. Queries return its published build catalog, with the same results for `--all`. For compatibility with the existing JSON format, Purpur entries have `stable: true`; this is not an official stability rating. `recommended` marks the API's latest build. Installation still checks successful build status and MD5.

**Paper** uses the [official Fill v3 API](https://docs.papermc.io/misc/downloads-service/) and defaults to the newest stable build for the requested Minecraft version. Query build numbers with `jartender versions 26.3 paper`, then pass a build number or `latest` (`-` also works) as the third install argument. An explicit build number can select an unstable build. The returned download URL is used directly; size, SHA-256 and the executable JAR manifest are checked before publishing `server.jar` with `eula=false`. Installation runs no Java; Paperclip prepares runtime files on first launch. A missing stable build is an error; the Minecraft version is never changed automatically. Library hosts can use `InstallRequest::paper("name", "26.3")`.

**Fabric** uses the [official Fabric Meta API](https://github.com/FabricMC/fabric-meta). Defaults select the latest stable compatible Loader and latest stable installer. It prepares `server.jar` and `eula.txt` with `eula=false`, runs no Java, and generates no world; the launcher downloads the remaining server files on first launch.

**Forge / NeoForge** download the official installer and run it with `--installServer`. That needs Java and pulls roughly 100–200 MB of libraries from Maven, usually in 1–2 minutes. Forge defaults to the `recommended` build for the Minecraft version (`latest` when absent); NeoForge defaults to the newest stable build. The third positional is the loader or build version; pass `latest` or `-` for the default. Java is selected from `--java`, `JAVA_HOME/bin/java`, then `PATH`, and the major version is checked against Mojang metadata before downloading (26.x needs Java 25). Forge 1.7.10–1.9.4 publishes Maven coordinates with a trailing `-<MC version>`; installation falls back to them automatically.

Forge 1.16.5 and older install a top-level launcher JAR (`forge-<version>.jar`, `forge-<version>-universal.jar`, or `minecraftforge-universal-*.jar`) and no platform argument file. Installation accepts that legacy layout and `list` uses the launcher JAR as the entry point without picking the adjacent `minecraft_server.*.jar`. Forge 1.17 and newer produce `run.sh` / `run.bat`, `user_jvm_args.txt`, and a platform argument file, which `list` and `run` understand directly.

Installation generates no world and never accepts the EULA for you (`eula=false`). Existing directories are never overwritten. The installer download is SHA-1 checked when the repository publishes a checksum and capped at 64 MB; the installer process has a 30-minute timeout. Failures, checksum mismatches, and cancellation remove staging files. `--verbose` prints installer output. Forced termination can leave a hidden `.jartender-install-*` directory; remove it after confirming no install is active. Failed installs are not retried automatically.

Forge/NeoForge installation executes a downloaded third-party Java program. That is the security boundary difference from a Fabric install.


## Tab completion

Completion scripts are generated from the CLI definitions using `clap_complete`. Bash, Zsh, Fish, PowerShell and Elvish are supported. They complete subcommands, options, fixed values such as `arclight/mohist/banner/catserver/folia/leaves/purpur/paper/fabric/forge/neoforge`, and paths. Zsh/Bash support fixed positional values such as loader kinds; static generator coverage differs for Fish and other shells. Query online version numbers with `versions`; server names currently require manual entry.

With `jartender` on your `PATH`, enable completion in the current Zsh session:

```sh
autoload -Uz compinit && compinit
source <(jartender completions zsh)
```

Add these lines to `~/.zshrc` for future sessions; if `compinit` is already initialized, add only the second line. For Bash, use `source <(jartender completions bash)`; for Fish, use `jartender completions fish | source`. For PowerShell, use `jartender completions powershell | Out-String | Invoke-Expression`. See `jartender completions --help` for shell choices.

For example, `jartender ins<Tab>` completes `install`, and `jartender install 26.3 f<Tab>` offers `fabric` and `forge`. Generating scripts does not read configuration, use the network or start Java. Reload the script after command updates.

## Query available versions

```sh
jartender versions
jartender versions paper
jartender versions folia --table
jartender versions 1.21.4
jartender versions 1.21.4 --table
jartender versions 1.21.11 folia
jartender versions 1.21.8 leaves
jartender versions 26.3 purpur
jartender versions 26.3 paper
jartender versions 26.3 fabric
jartender versions 26.3 forge
jartender versions 1.21.4 neoforge
jartender versions --all

# 10 entries are shown by default; --limit changes that (0 lists everything)
jartender versions --limit 5
jartender versions 26.3 fabric --limit 20

# Include prereleases and remove the default limit
jartender versions --all

# JSON is the default; use --table for the aligned table
jartender versions 26.3 fabric --limit 10 --table
```

Queries default to stable versions. Minecraft uses Mojang's chronological version manifest; Fabric uses the compatible loader list, where upstream marks only the newest loader `stable`, so Jartender treats `+build` style releases as stable and hides only `-beta`/`-pre`/`-snapshot` prereleases unless `--all` is given. Forge/NeoForge query complete official Maven build listings, sorted numerically newest first. Forge results omit the Minecraft prefix, so every returned version can be passed directly as the third `install` positional. Paper builds use official stability channels and sort by build number, newest first.

`RECOMMENDED` means latest stable for Minecraft/Fabric/NeoForge/Paper/Folia/Leaves and the official recommended build for Forge. If Forge's supplementary promotion service is unavailable, the complete list remains usable without recommendation markers.

A single KIND argument lists the Minecraft versions that core supports, newest first, where `stable` marks official releases. A single MINECRAFT argument lists the cores that can install it: JSON is `[{"kind":"paper","stable":true,"latest":"232"}]` and the table columns are `KIND / STABLE / LATEST`; `stable=false` means only alpha/beta builds exist and the per-version query needs `--all`. No matching core returns `[]` with a stderr hint.

When `versions <MINECRAFT> <KIND>` finds nothing, the command prints the supported range: it suggests `--all` when the version exists but only has alpha/beta builds, and otherwise lists supported versions and points at `jartender versions <KIND>`.

The command prints the 10 newest entries by default. `--limit N` changes that (`--limit 0` lists everything), and `--all` includes prereleases and removes the default limit. When output is truncated, JSON mode reports it on stderr and table mode appends the total, so stdout stays machine-readable. JSON is the default output: an array of `version`, `stable`, and `recommended`. `--table` prints a table with the query context, and `--json` remains accepted as the default. No matches returns `[]` in JSON and `No matching versions found.` in table mode; network or metadata failures return a nonzero exit code.

Queries read remote metadata only; `config.json` is consulted solely for proxy settings, never for `serverpath`. They do not scan servers, download artifacts, or run Java. One positional is read as a KIND; two are read as `MINECRAFT KIND`. `install --help` includes the discovery commands.

The Rust API is `versions::query(&VersionQuery)`. Use `VersionQuery::default()` for Minecraft, or `VersionQuery::loader("26.3", InstallKind::Fabric)` for compatible loaders. Set `include_unstable` to include prereleases; Arclight/CatServer queries accept `github_token` to raise GitHub limits. `versions::supported_minecraft(kind, include_unstable, github_token)` returns the Minecraft versions a core supports, backing both the KIND-only query and the empty-result hints. `versions::kinds_for(minecraft, github_token)` returns `KindSupport { kind, stable, latest }` entries for the cores that can install a version, with `kinds_for_with_proxy` taking the proxy policy. Results are thread-transferable `Vec<versions::Version>` values; GUI hosts should run synchronous network queries on their worker threads.

## Scripting

`list` prints a JSON array by default. Use `--table` for the display-width-aligned `SERVER / TYPE / VERSION / MAP SIZE / EULA / LAUNCH` table and server count; `--json` is still accepted and matches the default. Table launch entries are compact summaries with argument-file path prefixes omitted. Use JSON or a launch dry run for complete paths.

The JSON array is name-sorted and contains `name`, `server_type`, `version`, `map_size_bytes`, `eula`, and `jars` (candidate filenames relative to the server directory). An optional `argument_files` array contains relative platform argument-file paths; an optional `selected_jar` appears when a legacy inventory resolves the directory. Multiple candidates for the default launch method are labeled `ambiguous`. An empty root returns `[]`; a missing or unreadable root is an error, while a single unreadable directory is reported on stderr and skipped.

`eula` is `accepted`, `not-accepted`, or `not-required` (recognized Velocity/BungeeCord proxies need no Minecraft EULA); the table shows `yes`, `no`, and `n/a`.

`version` is the Minecraft version from core metadata, or a proxy's own version; unavailable versions are `null` (`-` in the table). `map_size_bytes` sums regular-file lengths under the configured `level-name` (default `world`) and matching `_nether` / `_the_end` directories, including nested dimensions but excluding server JARs, libraries, and logs. The table uses binary size units. Ungenerated worlds show `0 B`; proxies, unreadable locations, or worlds outside the server directory show `null` / `-`. Traversal does not follow symlinks or read world contents. Sizes of running worlds are approximate snapshots.

`run --dry-run` prints JSON containing `directory`, `java`, `args`, `accept_eula`, `server_type`, and `requires_eula`. It never starts Java or writes files, even with `--accept-eula`.

Exit codes: `0` for success, `1` for operational errors, `2` for usage errors. After launch, Java's exit code is preserved; Unix also preserves signal termination. Jartender diagnostics go to stderr; list and dry-run results go to stdout. The running server owns its console streams.

## Use as a Rust library

The same crate provides a library and CLI. Disable the CLI dependency in a host launcher:

```toml
[dependencies]
jartender = { path = "../Jartender", default-features = false }
```

```rust,no_run
use jartender::{LaunchOptions, discover, prepare_launch};
use std::path::Path;

fn main() -> jartender::Result<()> {
    let root = Path::new("/srv/minecraft");
    let servers = discover(root)?;
    let options = LaunchOptions::new("survival", "/path/to/jdk/bin/java");
    let plan = prepare_launch(root, &options)?;
    plan.check_eula()?;
    let mut child = plan.to_command().spawn()?;
    let status = child.wait()?;
    Ok(())
}
```

- `discover` and `prepare_launch` only read files and return server records or a `LaunchPlan`. `scan` also reports directories it could not read (`skipped`) and accepts an optional legacy inventory path.
- `Server.eula` distinguishes `NotRequired` (recognized proxies), `Accepted`, and `NotAccepted`; `Server.selected_jar` is the candidate a legacy inventory would select.
- `LaunchPlan::to_command()` creates an unstarted `std::process::Command`. The host owns stdio, spawning, stopping, and process lifetime. Library calls never replace or exit the host.
- `LaunchPlan::check_eula()` applies the selected core's consent policy. After obtaining agreement, the host can explicitly call `plan.accept_eula()`. Recognized proxies neither require nor write EULA files. The standalone `check_eula(directory)` / `accept_eula(directory)` functions remain low-level file operations.
- `install::install` takes an `InstallRequest` (any `InstallKind`) and a progress callback, returning an `InstallOutcome`; `FabricRequest` / `install_fabric` remain as the Fabric-only entry point. Forge/NeoForge require an explicit Java in `InstallRequest::java`; the library spawns and captures the installer process. Arclight/CatServer installs can set `InstallRequest::github_token` to raise GitHub limits. Callbacks receive `InstallEvent` values, including `Installing`, `Output`, and `ChecksumUnavailable`. Return `ControlFlow::Break(())` to request cancellation and clean up staging files. Cancellation is checked between events, download chunks, and installer output lines; HTTP operations and the installer are timeout-bound.
- The public `Error` enum distinguishes ambiguous JARs, missing EULA consent, download failures, cancellation, and other errors. Results and errors can cross worker-thread boundaries.

The library requires absolute root and legacy inventory paths. Java must be absolute or a bare executable name. Launch planning does not read current-directory config, `JAVA_HOME`, or home-directory settings, and never prints to stdout/stderr. Configuration loading is explicit; `java::ScanOptions::system()` explicitly opts into environment and system discovery. A bare name such as `java` is searched on PATH when the host starts the command. Legacy inventories are used only when explicitly supplied through `LaunchOptions::legacy_list`.

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

This version supports JAR and Forge/NeoForge argument-file discovery and execution. Installation covers Fabric/Paper/Folia/Purpur/Leaves, Arclight/Mohist/Banner/CatServer, and the official Forge/NeoForge installers. `java install` downloads and verifies JDKs and JREs. Plugin management, world management, and a TUI are not implemented.

## License

[GNU GPL v3](LICENSE). Parts of the original Python implementation referenced [Prism Launcher](https://github.com/PrismLauncher/PrismLauncher) and [MultiMC](https://github.com/MultiMC/MultiMC5).
