# Jartender

在终端中安装和运行 Minecraft 服务端。

图形桌面版使用 Tauri 2 构建，复用同一 Rust 核心。运行与打包说明见 [JarTender Desktop](desktop/README.md)。

[English](README_EN.md)

Rust 版提供 `list`、`run`、`install`、`versions`、`java` 和 `completions`。服务端目录保存实际状态，无需初始化或同步清单。原 Python 版本暂时保留，可通过 `python3 jartender.py` 使用。

## 构建

安装 [Rust 稳定版工具链](https://rustup.rs/)，然后执行：

```sh
cargo build --release --locked
./target/release/jartender --help
```

也可以用 `cargo install --path . --locked` 安装。Windows 构建结果为 `target/release/jartender.exe`。

运行服务端需要与其版本相容的 Java。Jartender 自身和 `list` 无需 Java；Fabric/Paper/Folia/Purpur/Leaves 及混合核心的 JAR 下载也不需要，Forge/NeoForge 安装会运行官方 installer，因此需要 Java。

## 使用

```sh
jartender list
jartender run survival
jartender install 26.3 fabric latest survival
```

首次启动前，阅读 [Minecraft EULA](https://aka.ms/MinecraftEULA)，然后自行将服务端 `eula.txt` 中的值设为 `eula=true`。同意协议后，也可以显式执行：

```sh
jartender run survival --accept-eula
```

启动后直接输入服务端命令，例如 `stop`。Minecraft 图形控制台默认关闭；使用 `--gui` 启用。

### 下载代理

使用全局参数临时设置代理，参数也可放在子命令后：

```sh
jartender --proxy http://127.0.0.1:7890 install 26.3 fabric latest survival
jartender --proxy socks5h://127.0.0.1:1080 java install 21 openjdk
jartender --proxy http://127.0.0.1:7890 versions paper
jartender --no-proxy java versions oracle
```

在调用命令时所在目录的 `config.json` 中添加 `proxy` 可保存默认值；可与现有 `serverpath` 共存，仅配置代理时服务端根目录仍默认为 `./Servers`：

```json
{
  "proxy": "http://127.0.0.1:7890"
}
```

优先级为 `--proxy` / `--no-proxy` → `config.json` → 环境变量。`--no-proxy` 强制直连；保存直连默认值可用 `{"no_proxy": true}`。这两个设置互斥。未显式配置时沿用 `ALL_PROXY`、`HTTPS_PROXY`、`HTTP_PROXY`（也支持小写名称），并遵守 `NO_PROXY`；显式代理会代理所有目标，不使用环境中的绕过列表。`--root` 不改变代理配置文件位置。

代理覆盖 Minecraft/核心/Java 版本查询、元数据、校验文件、服务端 JAR、installer、JDK 压缩包以及 Forge/NeoForge 安装器下载的依赖。支持 `http://`、`https://`、`socks5://`、`socks5h://`；用户名和密码可写在 URL 中，特殊字符需百分号编码。Forge/NeoForge 的 Java 安装器只支持无认证的 HTTP 或 SOCKS5 代理，不支持的组合会报错，不会自动直连。代理不可用时下载失败。此配置用于 Rust 版工具的查询和安装，不改变已启动 Minecraft 服务端、插件或旧 Python 版的网络设置。

库调用方通过 `network::ProxyConfig::{new, Direct, Environment}` 配置安装请求和版本查询的 `proxy` 字段；受支持 Minecraft 版本和 Java 类型查询分别提供 `versions::supported_minecraft_with_proxy`、`java::install::types_for_major_with_proxy`。库不读取 CLI 的 `config.json`，也不修改进程环境变量。

### 本地 Java 探测

```sh
jartender java
jartender java list
jartender java --table
jartender java list --table
# 只查指定 JDK、可执行文件或安装目录；--path 可重复
jartender java --no-system --path /path/to/jdks
```

搜索来源参考 Python 版：`JAVA_HOME`、`PATH`、系统 JDK 目录、SDKMAN/asdf、用户下载的 JDK、PrismLauncher 和当前目录的 `java/`；macOS 另检查 Homebrew，Linux 检查常用安装目录及 Snap，Windows 检查常用厂商目录和当前用户/系统注册表的 32/64 位视图。搜索按目录深度限制，并按真实 Java 路径去重；不递归扫描整个磁盘。

探测执行 `java -XshowSettings:properties -version`，不执行服务端或编译器。每次默认最多 3 秒，输出每个流最多保留 64 KiB；挂起和失败的可执行文件不会阻断其他结果。默认输出 JSON，`--json` 仍可显式使用；`--table` 切换为表格，列出版本、主版本、发行商、架构与路径。JSON 返回 `runtimes` 和 `skipped`，包含 `vendor_version`、`home`、`is_64bit` 等字段。不联网、不下载 JDK、不修改环境变量、不写 `java_list.json`。这一步只发现已安装的 Java，不自动替你选择其他主版本。

### 安装 Java

支持 **Oracle JDK**（`oracle`，别名 `official`）、**OpenJDK 官方 GA**（`openjdk`）、**Mojang Java Runtime**（`mojang`，别名 `minecraft`）、**Eclipse Temurin**（`adoptium`，别名 `temurin`）、**Azul Zulu**（`zulu`，别名 `azul`）、**IBM Semeru Open**（`semeru`，别名 `ibm` / `openj9`）和 **GraalVM Community**（`graalvm`，别名 `graal` / `graalvm-community`）：

```sh
jartender java types                    # 可安装的 JVM 类型
jartender java types --table
jartender java version 21               # 查询 Java 21 可安装的 JVM 类型
jartender java version 21 --table
jartender java version 21 openjdk       # 直接查 OpenJDK 21 的可用版本
jartender java version 8 zulu --table   # 主版本在前，TYPE 可选
jartender java versions oracle          # 当前公开下载
jartender java versions oracle 21       # 指定主版本的公开归档
jartender java versions openjdk         # 当前平台的 GA 归档，默认最多 10 条
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
# 精确版本与自定义目录名
jartender java install 21.0.2 openjdk java21
# 查看下载地址、SHA-256 和目标目录，不写文件
jartender java install 25 oracle --dry-run
# 自定义安装根目录
jartender --root /path/to/jdks java install 25 oracle
```

默认根目录是调用命令时的 `./java/`，与服务器 `config.json` 无关；默认目录名为 `<发行版>-<请求版本>-<系统>-<架构>`。自动选择本机 Linux/macOS/Windows 与 x86_64/aarch64 压缩包；上游没有对应包时直接报错，不切换架构或发行版。`--root` 对该命令指定 Java 安装根目录，服务器的旧安装语法保持不变。

主版本号选择该来源公开可下载的最新包；精确版本只匹配同一版本。Oracle 优先使用官网当前公开的 latest 地址；若当前更新只提供账号下载，则在对应主版本官方归档中选择最新公开包，安装结果会显示实际版本。OpenJDK 使用 jdk.java.net 的 GA 归档，排除 EA，不使用第三方发行版；归档并不保证提供 Java 8 或每个主版本的持续 LTS 更新。需登录的 Oracle 下载不会被自动处理。

Adoptium 使用 `api.adoptium.net/v3`，取 `image_type=jdk` 的 Temurin 包，用 `package.checksum` 校验 SHA-256；Zulu 使用 Azul Metadata API，先列包，再读取包详情里的 `sha256_hash`；Semeru 读取 `ibmruntimes/semeruN-binaries` 的 GitHub Release，选择 `ibm-semeru-open-jdk_<arch>_<os>_<version>.tar.gz`（Windows 为 `.zip`），用 release asset 的 `sha256:` digest 校验；GraalVM Community 读取 `graalvm/graalvm-ce-builds` 的 GitHub Release，按平台选择 `graalvm-community-jdk-<版本>_<系统>-<架构>_bin.tar.gz`（Windows 为 `.zip`，没有 Windows aarch64 包），排除草稿和预发布，用 release asset 的 `sha256:` digest 校验（查询走匿名 GitHub API，可能受速率限制）；Mojang 使用官方 `java-runtime` manifest，是逐文件的 JRE 安装（macOS 位于 `jre.bundle/Contents/Home`，不含 `javac`），会并发下载 100 多个小文件，通常 20 秒左右。

安装强制校验官方校验和：单包来源用 SHA-256，Mojang 用官方清单的逐文件 SHA-1。解压或落盘后检查路径、链接、展开大小及 `release` 中的版本、系统、架构；JDK 还要求 `java` / `javac`，Mojang JRE 只要求 `java`。已有目标不覆盖，失败或取消会清理暂存文件。安装不执行下载的 Java、不修改系统注册表或 `JAVA_HOME` / `PATH`。输出 JSON 包含 `directory`、`home`、`java` 和实际 `version`；把 `java` 路径填入服务端配置即可。

装在默认目录后可运行 `jartender java` 探测；自定义目录使用 `jartender java --no-system --path /path/to/jdks`。这个单独的探测步骤会运行版本命令。库接口是 `java::install::{Request, Distribution, resolve, install}`，进度回调返回 `ControlFlow::Break(())` 可取消；平台和架构也可由宿主显式设置。

`java version <MAJOR> [TYPE]` 在给出 TYPE 时等价于 `java versions <TYPE> <MAJOR>`（默认 10 条）；要列全部版本用 `java versions <TYPE> <MAJOR> --all`。注意 `version` 的参数顺序是「主版本在前」，`versions` 是「TYPE 在前」。

Java 查询默认输出 JSON 数组，`--table` 切换为表格；版本默认最多 10 条，`--limit 0` 或 `--all` 返回全部。版本按新到旧排序，仅列出当前系统和架构可用的安装包。`latest: true` 标记该来源最新的一项；Oracle 使用可变的 latest 地址，其版本字段只表示主版本。`java list` 与不带子命令的 `java` 都用于本地探测。旧的 `install java` 入口已移除。

库接口 `java::install::{providers, Provider, VersionQuery, AvailableVersion, versions}` 负责类型和版本查询。`Provider` 统一提供类型元数据、版本查询和安装包解析；CLI 的类型参数和补全来自 `providers()` 注册表。以后新增内置 JVM 类型时扩展 `Distribution`、实现提供者并注册，继续共用下载、SHA-256 校验和解压流程。


### 每个服务端的启动配置

在 `Servers/<名称>/jartender.json` 中保存启动默认值，例如：

```json
{
  "java": "/path/to/jdk/bin/java",
  "jvm_args": ["-Xms1G", "-Xmx2G"],
  "server_args": ["--port", "25566"],
  "gui": false
}
```

从 `jartender java` 结果中选择 Java 路径。还可配置 `jar` 或 `args_file`，两者互斥；不配置时继续自动探测启动入口。Java 可以是绝对路径、`java` 这样的 PATH 可执行名，或相对服务端目录的路径（例如 `./runtime/bin/java`）。参数使用 JSON 数组，每项对应一个独立参数，包含空格时也不会被重新拆分。

优先级为命令行参数 → 服务端配置 → 原默认值。Java 未指定时仍按 `JAVA_HOME`、`PATH` 选择。显式 `--jar` / `--args-file` 会覆盖配置中的另一种启动入口；命令行提供的 JVM 参数或 `--` 后参数会替换相应配置数组。`--gui=false` 可覆盖配置里的 `gui: true`。启动不会自动保存命令行选项；配置文件缺失时行为不变，格式错误、未知字段或互斥字段会明确报错。

`list` 不读取启动配置；`run --dry-run` 只读取配置与生成计划，不执行 Java、不修改配置或 EULA。库宿主通过 `config::ServerConfig::load(root, name)`、`apply(directory, &mut LaunchOptions)` 显式使用配置；需要保存时调用 `save(root, name)`，原子替换该文件。`prepare_launch` 本身仍只使用传入的 `LaunchOptions`。Java 发现对应 `java::ScanOptions::system()` 与 `java::scan`；使用默认空 `ScanOptions` 并显式填写 `paths` 可让宿主完全控制扫描来源。

### 目录与旧数据

`--root` 可用于 `list`、`run`、`install` 和 `java install`，可以放在子命令前或后。`versions`、`completions` 以及本地 Java 探测（`java` / `java list`）不使用服务器根目录；探测其他 Java 位置请用 `--path`。例如：

```sh
jartender --root /srv/minecraft list
jartender run survival --root /srv/minecraft
```

根目录按以下顺序确定：`--root`、当前工作目录中旧 `config.json` 的 `serverpath`、当前工作目录下的 `Servers`。相对路径相对于调用命令时的目录解析，支持 `~`。无需配置文件；旧配置不会被修改。二进制所在位置不影响目录选择。

`list` 检查各服务端目录顶层的 JAR，排除名称含 `installer` 的文件，跳过隐藏目录。无需已有 `eula.txt`、`libraries` 或 `server.properties`。核心类型来自 JAR 主类；Paperclip 的负载索引用于区分 Paper、Purpur、Folia 和 Leaves。Velocity、BungeeCord 也可识别，无法确认时显示 `unknown`。无顶层 JAR 时，还会检查 Forge/NeoForge 标准安装目录中的平台参数文件。无法读取的目录会被跳过并在 stderr 报告原因，不会中断其余结果。扫描不启动 Java、不联网、不写文件。

### 启动

```sh
# 明确指定 Java
jartender run survival --java "/path/to/jdk/bin/java"

# 多个 JAR 时明确选择
jartender run survival --jar fabric-server-launch.jar

# JVM 参数用 --jvm-arg，服务端参数放在 -- 后
jartender run survival --jvm-arg=-Xms1G --jvm-arg=-Xmx2G -- --port 25566

# 仅输出启动计划，不启动 Java、不修改 EULA
jartender run survival --dry-run
```

Java 按 `--java`、服务端 `jartender.json` 的 `java`、`JAVA_HOME/bin/java`、`PATH` 中的 `java` 顺序选择。不自动切换已安装的 JDK，也无需维护 `java_list.json`；需要其他版本时使用 `--java`。显式 Java 路径在切换工作目录前解析。

单个候选 JAR 会直接使用。多个候选时，当前目录旧 `list.json` 中仍有效的 `jar_name` 可以提供选择，旧绝对 `jar_path` 会忽略；其余情况要求 `--jar FILE`。`list` 使用同一规则：旧清单能消歧时类型标为对应核心并返回 `selected_jar`，否则显示 `ambiguous`。显式 `--jar` 优先级最高，必须是服务端目录顶层的文件名。无需维护旧清单。

没有顶层 JAR 时，自动使用 `libraries/net/neoforged/neoforge/<版本>/`、`libraries/net/minecraftforge/forge/<版本>/` 或旧版 NeoForge 的 `libraries/net/neoforged/forge/<版本>/` 下的 `unix_args.txt`（macOS/Linux）或 `win_args.txt`（Windows）。多个版本并存时要求显式选择：

```sh
jartender run neoforge-26.3 --args-file libraries/net/neoforged/neoforge/26.3.0.100/unix_args.txt
```

`--args-file` 与 `--jar` 互斥，路径相对于服务端目录。参数文件启动会先载入存在的 `user_jvm_args.txt`，然后加入显式 JVM 参数及平台参数文件；解析由 Java 完成。Jartender 不执行 `run.sh` 或 `run.bat`。存在顶层 JAR 时默认保持 JAR 启动方式。

识别出的 Velocity/BungeeCord 无需 Minecraft EULA 文件，也不会自动附加 `nogui`；对它们指定 `--accept-eula` 不会创建或修改协议文件。Minecraft 核心和未知 JAR 仍要求 EULA 同意。

服务端在自己的目录中运行，输入、输出和错误流直接连接调用方。Unix 上 Java 替换 Jartender 进程，直接接管 PID、信号和退出状态；Windows 上等待 Java 并返回其退出码。

### 安装

顺序是 `install <版本> <类型> <类型版本> <名字>`：

```sh
jartender install 1.21.11 folia latest     folia-survival
jartender install 1.21.8 leaves latest     leaves-survival
jartender install 26.3 purpur   latest     purpur-survival
jartender install 26.3 paper    latest     paper-survival
jartender install 26.3 fabric   latest     survival
jartender install 26.3 forge    66.1.0     survival
jartender install 26.3 neoforge 26.3.0.100 survival
```

**Arclight / Mohist / Banner / CatServer** 下载官方可执行启动 JAR，共用暂存、校验和目录发布流程；安装不运行 Java，首次 `run` 时由各核心准备自己的运行库。EULA 保持 `false`，不会覆盖已有目录，下载失败或取消会清理暂存文件。

```sh
jartender versions 1.21.1 arclight
jartender install 1.21.1 arclight arclight-forge-1.21.1-1.0.1-8ec9529.jar arclight-survival
jartender install 1.20.1 mohist latest mohist-survival
jartender install 1.20.1 banner latest banner-survival
jartender install 1.16.5 catserver latest cat-survival
jartender list --table
```

Arclight 使用 [官方 GitHub Releases](https://github.com/IzzelAliz/Arclight/releases)，版本列表保留完整 JAR 文件名，其中包含 Forge、NeoForge 或 Fabric 平台。`latest` 若遇到多个平台会要求明确选择；将查询结果的文件名作为第三个安装参数即可。默认排除 GitHub 预发布和草稿，`--all` 包含预发布。GitHub 匿名请求限额为 60 次/小时；设置 `GITHUB_TOKEN` 或 `GH_TOKEN` 可提高限额。安装只在默认 `latest` 时读取最新的匹配发布并停止翻页；显式文件名可能位于较旧发布，因此会继续翻页查找。`versions` 始终翻页以列出全部构建。

Mohist / Banner 使用 [现行官方 API](https://mohistmc.com/mohistmc-api)，按构建号排序，第三个参数可用构建号或 `latest`（`-` 也可），校验官方 SHA-256。API 没有稳定性通道，查询中的 `stable: true` 表示公布的构建，不是稳定性评级；`--all` 不改变结果。这里的 Banner 使用官方 Banner 项目，不替换成其他改名或分叉项目。

CatServer 使用 [官方 GitHub Releases](https://github.com/Luohuayu/CatServer/releases)，按 JAR 名中的 Minecraft 版本筛选；旧 `universal.jar` 用发布目标分支确认 Minecraft 版本。指定版本时同样使用查询返回的完整文件名。GitHub 资产提供 SHA-256 时会校验；旧 Arclight/CatServer 发布未提供校验和时，CLI 会说明、库会发出 `ChecksumUnavailable` 事件，此时只校验官方资产大小、可执行 JAR、核心类型和能读取的 Minecraft 版本，不声称做了摘要校验。

四种核心都通过 JAR 主类、类条目或 Fabric 元数据识别，Minecraft 版本从内置安装元数据或清单读取，改名后仍可探测；无法确认的版本为 `null`。安装发现核心类型或已知 Minecraft 版本不符时拒绝发布。库接口分别为 `InstallRequest::arclight`、`mohist`、`banner`、`catserver`，配合 `versions::query` 和 `discover` 使用；这些核心的安装不使用 `--java` / `--installer`，Java 路径在 `run` 时选择。

**Folia** 与 Paper 共用 [PaperMC 官方 Fill v3 API](https://docs.papermc.io/misc/downloads-service/)，默认选择指定 Minecraft 版本的最新稳定构建。用 `jartender versions 1.21.11 folia` 查询，添加 `--all` 包含 Alpha/Beta；第三个安装参数支持构建号或 `latest`（`-` 也可）。没有稳定构建时，`latest` 报错，不自动选择测试版或改变 Minecraft 版本；要安装测试版需显式指定构建号。下载校验官方文件大小、SHA-256 和 JAR 清单，安装不运行 Java，保留 `eula=false`。库接口为 `InstallRequest::folia("名称", "1.21.11")`，`list` 和 `run` 可识别 Folia。

**Leaves** 使用 [Leaves 官方下载 API](https://api.leavesmc.org/v2/projects/leaves)，默认选择指定 Minecraft 版本中 `default` 通道的最新构建。用 `jartender versions 1.21.8 leaves` 查询普通构建，添加 `--all` 包含实验构建。安装第三个参数支持构建号或 `latest`（`-` 也可），显式构建号允许选择实验版。下载后校验官方 SHA-256 和 JAR 清单，发布为 `server.jar`，EULA 初始为 `false`，不执行 Java。没有普通构建时直接报错；不切换 Minecraft 版本。库接口为 `InstallRequest::leaves("名称", "1.21.8")`。Leavesclip 与 Paperclip 中的 Leaves 负载都可识别为 `leaves`。

**Purpur** 使用 [Purpur 官方 API](https://purpurmc.org/docs/purpur/)。`jartender versions 26.3 purpur` 列出构建号，安装第三个参数支持指定构建号或 `latest`（`-` 也可）。默认解析官方 `latest` 后固定到具体构建号，检查项目、Minecraft 版本、构建号及 `SUCCESS` 状态，再下载并按官方 MD5 校验；随后共用 Paper 的 JAR 检查和目录发布流程。安装不运行 Java，保留 `eula=false`，首次启动时准备运行文件。库接口为 `InstallRequest::purpur("名称", "26.3")`。

Purpur API 不区分稳定版与测试版；查询返回官方公布的构建列表，`--all` 结果相同。兼容现有 JSON 格式，Purpur 项的 `stable` 固定为 `true`，不代表官方稳定性评级；`recommended` 标记 API 的 `latest`。安装时仍须通过构建成功状态和 MD5 校验。

**Paper** 使用 [Paper 官方 Fill v3 API](https://docs.papermc.io/misc/downloads-service/)，默认选择该 Minecraft 版本的最新稳定构建。`jartender versions 26.3 paper` 查询构建号；第三个安装参数可用构建号或 `latest`（`-` 也可）。显式构建号可选择非稳定构建。下载官方返回的地址，校验文件大小、SHA-256 和可执行 JAR 清单后发布为 `server.jar`，并写入 `eula=false`。安装不运行 Java；首次启动时 Paperclip 继续准备运行文件。没有稳定构建时直接报错，不切换 Minecraft 版本。库调用使用 `InstallRequest::paper("名称", "26.3")`。

**Fabric** 使用 [Fabric 官方 Meta API](https://github.com/FabricMC/fabric-meta)，默认选择兼容的最新稳定 Loader 和最新稳定 installer。安装只准备 `server.jar` 和 `eula.txt`（初始值为 `false`），不启动 Java、不生成世界；首次 `run` 时 launcher 继续下载服务端所需文件，因此需要网络。

**Forge / NeoForge** 下载官方 installer 并执行 `--installServer`。这一步需要 Java，并且会从 Maven 拉取约 100–200 MB 的 libraries，通常耗时 1–2 分钟。默认 Forge 取该 Minecraft 版本的 `recommended`（没有则 `latest`），NeoForge 取最新稳定构建；第三个位置参数就是类型版本（Fabric Loader / Forge 构建号 / NeoForge 构建号），传 `latest` 或 `-` 使用默认值。Java 按 `--java`、`JAVA_HOME/bin/java`、`PATH` 选择，安装前会用 Mojang 元数据核对主版本（26.x 需要 Java 25）。1.7.10–1.9.4 的 Forge Maven 坐标带 `-<MC 版本>` 后缀，安装会自动回退到该坐标。

1.16.5 及更早的 Forge 生成顶层启动 JAR（`forge-<版本>.jar`、`forge-<版本>-universal.jar` 或 `minecraftforge-universal-*.jar`），没有平台参数文件；安装同样接受这种旧布局，`list` 会把该 JAR 作为启动入口（同目录的 `minecraft_server.*.jar` 不会被误选）。1.17 及以后生成 `run.sh` / `run.bat`、`user_jvm_args.txt` 和平台参数文件，`list` 与 `run` 直接识别。

安装不生成世界，也不代替你接受 EULA（写入 `eula=false`）。同名目录不会覆盖。installer 下载在仓库提供校验和时验证 SHA-1，并有 64 MB 上限；安装器进程有 30 分钟超时，失败、校验不符或取消都会清理暂存文件。`--verbose` 输出安装器日志。强制终止可能留下 `.jartender-install-*` 隐藏目录，确认没有安装正在进行后可以删除。安装不会自动重试。

Forge/NeoForge 的 installer 是下载并执行的第三方 Java 程序，这是与 Fabric 安装的安全边界差异。


## Tab 补全

使用 `clap_complete` 从命令定义生成补全脚本，支持 Bash、Zsh、Fish、PowerShell 和 Elvish。可补全子命令、选项、`arclight/mohist/banner/catserver/folia/leaves/purpur/paper/fabric/forge/neoforge` 等固定值及路径；Zsh/Bash 支持核心类型等位置参数的固定值补全，Fish 等 shell 的静态生成器支持范围有所不同。在线版本号用 `versions` 查询，服务器名目前需手动输入。

确保 `jartender` 在 `PATH` 中后，当前 Zsh 会话运行：

```sh
autoload -Uz compinit && compinit
source <(jartender completions zsh)
```

将以上两行放入 `~/.zshrc` 可在新会话中启用；已经初始化 `compinit` 的配置只需加入第二行。Bash 使用 `source <(jartender completions bash)`，Fish 使用 `jartender completions fish | source`。PowerShell 使用 `jartender completions powershell | Out-String | Invoke-Expression`。其他 shell 可通过 `jartender completions --help` 查看。

例如输入 `jartender ins<Tab>` 补全 `install`，输入 `jartender install 26.3 f<Tab>` 列出 `fabric` 和 `forge`。生成脚本不会读取配置、联网或运行 Java；命令更新后重新加载脚本即可。

## 查询可安装版本

```sh
# Minecraft 正式版本，按官方时间顺序从新到旧
jartender versions

# 只给核心名：列出该核心支持的 Minecraft 版本
jartender versions paper
jartender versions folia --table

# 只给 Minecraft 版本：列出可安装它的核心/加载器
jartender versions 1.21.4
jartender versions 1.21.4 --table

# 指定 Minecraft 版本的兼容 Loader / 核心构建
jartender versions 1.21.11 folia
jartender versions 1.21.8 leaves
jartender versions 26.3 purpur
jartender versions 26.3 paper
jartender versions 26.3 fabric
jartender versions 26.3 forge
jartender versions 1.21.4 neoforge

# 默认只列最近 10 条；--limit 控制条数（0 表示全部）
jartender versions --limit 5
jartender versions 26.3 fabric --limit 20

# 包含快照/预发布并列出全部结果
jartender versions --all

# 默认输出 JSON；表格用 --table
jartender versions 26.3 fabric --limit 10 --table
```

默认只显示稳定版本。Minecraft 使用 Mojang 版本清单；Fabric 使用兼容 Loader 列表——上游只把最新 Loader 标记为 `stable`，Jartender 按版本号把 `+build` 等发布版都视为稳定，`-beta`/`-pre`/`-snapshot` 等预发布需要 `--all`；Forge/NeoForge 使用官方 Maven 完整构建清单，按版本数值从新到旧排序。Forge 查询会去掉版本号中的 Minecraft 前缀，结果可直接用于 `install` 的第三个参数。Paper 按官方通道筛选稳定构建，按构建号从大到小排列。

`RECOMMENDED` 对 Minecraft/Fabric/NeoForge/Paper/Folia/Leaves 表示最新稳定版，对 Forge 表示官方推荐构建。Forge 推荐服务不可用时仍返回完整列表，推荐标记留空。

只给 KIND 时返回该核心支持的 Minecraft 版本（从新到旧），`stable` 表示该版本是正式版。只给 MINECRAFT 时返回可安装该版本的核心/加载器列表，JSON 为 `[{"kind":"paper","stable":true,"latest":"232"}]`，表格列为 `KIND / STABLE / LATEST`；`stable=false` 表示该类型只有 alpha/beta 构建，需要在该版本的查询中加 `--all`。没有类型支持时返回 `[]` 并在 stderr 提示。

`versions <MINECRAFT> <KIND>` 没有结果时会提示支持范围：该版本存在但只有 alpha/beta 构建时提示使用 `--all`；完全不支持时列出支持的版本，并提示用 `jartender versions <KIND>` 查看完整列表。

默认输出最近 10 条，`--limit N` 调整条数（`--limit 0` 不限），`--all` 同时包含预发布并取消条数上限。被截断时 JSON 模式在 stderr 提示、表格模式在末尾提示总数，stdout 始终可直接解析。默认 JSON 数组，每项含 `version`、`stable`、`recommended`；`--table` 输出带查询上下文的表格，`--json` 仍接受（等价于默认行为）。没有匹配版本时 JSON 返回 `[]`，表格显示 `No matching versions found.`；网络或元数据错误返回非零退出码。

查询只联网读取元数据；`config.json` 只用于代理设置，不读取其中的 `serverpath`，也不扫描服务器、不下载核心、不运行 Java。只给一个位置参数时按 KIND 查询，给两个时按 `MINECRAFT KIND` 查询；`install --help` 也会提示查询方法。

Rust 库提供 `versions::query(&VersionQuery)`：`VersionQuery::default()` 查询 Minecraft，`VersionQuery::loader("26.3", InstallKind::Fabric)` 查询兼容 Loader；将 `include_unstable` 设为 `true` 包含非稳定版本；Arclight/CatServer 查询可用 `github_token` 提高 GitHub 限额。`versions::supported_minecraft(kind, include_unstable, github_token)` 返回某个核心支持的 Minecraft 版本，供「空结果提示」和只给 KIND 的查询使用；`versions::kinds_for(minecraft, github_token)` 返回可安装该版本的核心列表（`KindSupport { kind, stable, latest }`），带代理版本为 `kinds_for_with_proxy`。结果为 `Vec<versions::Version>`，可在线程间传递；同步网络查询应放到宿主的工作线程。

## 脚本与退出码

`list` 默认输出 JSON 数组；`--table` 输出按终端显示宽度对齐的 `SERVER / TYPE / VERSION / MAP SIZE / EULA / LAUNCH` 表格和服务器数量，`--json` 仍然接受（等价于默认行为）。表格中的启动入口显示摘要，参数文件路径省略前缀；完整路径可通过 JSON 或启动预演查看。

JSON 数组按名字排序，每项包含 `name`、`server_type`、`version`、`map_size_bytes`、`eula` 和 `jars`。`jars` 是相对于服务端目录的候选文件名；有参数文件时额外返回相对路径数组 `argument_files`，旧清单能消歧时额外返回 `selected_jar`。当前默认启动方式存在多个候选时，类型为 `ambiguous`。空目录输出 `[]`，根目录不存在或不可读时报错；单个目录不可读只会在 stderr 提示并跳过。

`eula` 为 `accepted`、`not-accepted` 或 `not-required`（识别出的 Velocity/BungeeCord 不需要 Minecraft EULA），表格中分别显示 `yes`、`no`、`n/a`。

`version` 为从核心元数据提取的 Minecraft 版本；代理端显示自身版本，无法识别时为 `null`（表格显示 `-`）。`map_size_bytes` 是 `server.properties` 中 `level-name`（默认 `world`）目录及同名 `_nether`、`_the_end` 目录内常规文件大小之和，包含目录内的维度数据，不包含 JAR、依赖或日志。表格按二进制单位显示容量。未生成世界为 `0 B`；代理端、无法读取或指向服务器目录外的世界为 `null` / `-`。统计不跟随符号链接，只读取元数据；运行中世界的大小是扫描时的近似快照。

`run --dry-run` 输出 JSON，包含 `directory`、`java`、`args`、`accept_eula`、`server_type`、`requires_eula`；即使指定 `--accept-eula` 也不写文件。

退出码：`0` 为成功，`1` 为操作错误，`2` 为命令用法错误。启动成功后返回 Java 的退出码，Unix 上保留信号终止状态。Jartender 自身的诊断写入 stderr，列表和预演结果写入 stdout；运行期间服务端使用自己的控制台流。

## 作为 Rust 库使用

同一个 crate 同时提供库和 CLI。宿主启动器可关闭 CLI 依赖：

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

- `discover`、`prepare_launch` 只读文件，返回服务器列表和 `LaunchPlan`。`scan` 额外返回无法读取的目录（`skipped`）并接受可选的旧清单路径。
- `Server` 的 `eula` 字段区分 `NotRequired`（识别出的代理端）、`Accepted` 和 `NotAccepted`；`selected_jar` 表示旧清单会选中的候选。
- `LaunchPlan::to_command()` 只创建尚未启动的 `std::process::Command`。宿主决定标准流、启动、停止和进程生命周期；库不会接管或退出宿主进程。
- `LaunchPlan::check_eula()` 根据已选核心检查协议状态；宿主取得用户同意后，可显式调用 `plan.accept_eula()`。识别出的代理端不检查或写入协议文件。原独立函数 `check_eula(directory)` / `accept_eula(directory)` 保留为直接操作文件的底层接口。
- `install::install` 接收 `InstallRequest`（`kind` 为 `InstallKind` 中的任一核心）和进度回调，返回 `InstallOutcome`；`FabricRequest` / `install_fabric` 保留为 Fabric 专用入口。Forge/NeoForge 必须在 `InstallRequest::java` 中显式提供 Java，安装器进程由库启动并捕获输出；Arclight/CatServer 可设置 `InstallRequest::github_token` 提高 GitHub 限额。回调接收 `InstallEvent`（含 `Installing`、`Output`、`ChecksumUnavailable`）；返回 `ControlFlow::Break(())` 可以请求取消，取消或失败后清理暂存文件。取消在事件、下载块和安装器输出行之间检查，阻塞网络请求与安装器由超时限制。
- 错误通过公开的 `Error` 枚举返回，可区分多个 JAR、未接受 EULA、下载失败、取消等情况；错误和结果支持在线程间传递。

库要求根目录和旧清单路径为绝对路径；Java 必须是绝对路径或单独的可执行文件名。启动计划不读取当前目录配置、`JAVA_HOME` 或主目录，也不输出到 stdout/stderr。配置由宿主显式加载；`java::ScanOptions::system()` 显式启用环境和系统目录探测。若传入 `java` 这样的单独名称，宿主实际启动命令时由系统搜索 PATH。旧清单仅在显式设置 `LaunchOptions::legacy_list` 时使用。

所有 API 都是同步的，GUI 宿主应把下载放在自己的工作线程上。库模式支持 Rust；其他语言仍可通过 CLI 集成，目前没有 C ABI 或语言绑定。完整宿主示例见 [examples/launcher.rs](examples/launcher.rs)。

## 开发

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --no-default-features
cargo doc --locked --no-default-features --no-deps
```

一个 crate，`lib.rs` 暴露核心接口，`main.rs` 负责命令行解析、默认配置、终端输出和进程接管。测试使用临时目录、模拟 Java 和本地 HTTP 服务，并覆盖外部 crate 视角的库调用；不需要真实 Minecraft 服务端。CI 配置覆盖 Windows、macOS 和 Linux，并检查完整 CLI 和纯库构建。

支持 JAR 服务端和 Forge/NeoForge 参数文件布局的发现与启动；安装覆盖 Fabric/Paper/Folia/Purpur/Leaves、Arclight/Mohist/Banner/CatServer 以及官方 Forge/NeoForge installer。`java install` 可下载并校验 JDK/JRE。插件管理、世界管理和 TUI 尚未实现。

## 许可证

[GNU GPL v3](LICENSE)。原 Python 实现部分参考了 [Prism Launcher](https://github.com/PrismLauncher/PrismLauncher) 和 [MultiMC](https://github.com/MultiMC/MultiMC5)。
