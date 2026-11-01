# Jartender

在终端中安装和运行 Minecraft 服务端。

[English](README_EN.md)

Rust 版只有三个命令：`list`、`run`、`install`。服务端目录保存实际状态，无需初始化或同步清单。原 Python 版本暂时保留，可通过 `python3 jartender.py` 使用。

## 构建

安装 [Rust 稳定版工具链](https://rustup.rs/)，然后执行：

```sh
cargo build --release --locked
./target/release/jartender --help
```

也可以用 `cargo install --path . --locked` 安装。Windows 构建结果为 `target/release/jartender.exe`。

运行服务端需要与其版本相容的 Java。Jartender 自身以及 `list`、`install` 命令无需 Python 或 Java。

## 使用

```sh
jartender list
jartender run survival
jartender install fabric --minecraft 1.21.4 --name survival
```

首次启动前，阅读 [Minecraft EULA](https://aka.ms/MinecraftEULA)，然后自行将服务端 `eula.txt` 中的值设为 `eula=true`。同意协议后，也可以显式执行：

```sh
jartender run survival --accept-eula
```

启动后直接输入服务端命令，例如 `stop`。Minecraft 图形控制台默认关闭；使用 `--gui` 启用。

### 目录与旧数据

所有命令支持 `--root DIR`，可以放在子命令前或后：

```sh
jartender --root /srv/minecraft list
jartender run survival --root /srv/minecraft
```

根目录按以下顺序确定：`--root`、当前工作目录中旧 `config.json` 的 `serverpath`、当前工作目录下的 `Servers`。相对路径相对于调用命令时的目录解析，支持 `~`。无需配置文件；旧配置不会被修改。二进制所在位置不影响目录选择。

`list` 检查各服务端目录顶层的 JAR，排除名称含 `installer` 的文件，跳过隐藏目录。无需已有 `eula.txt`、`libraries` 或 `server.properties`。核心类型来自 JAR manifest，无法确认时显示 `unknown`。扫描不启动 Java、不联网、不写文件。没有顶层 launcher JAR 的 Forge 参数文件布局暂不支持。

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

Java 按 `--java`、`JAVA_HOME/bin/java`、`PATH` 中的 `java` 顺序选择。不自动切换已安装的 JDK，也无需维护 `java_list.json`；需要其他版本时使用 `--java`。显式 Java 路径在切换工作目录前解析。

单个候选 JAR 会直接使用。多个候选时，当前目录旧 `list.json` 中仍有效的 `jar_name` 可以提供选择，旧绝对 `jar_path` 会忽略；其余情况要求 `--jar FILE`。显式 `--jar` 优先级最高，必须是服务端目录顶层的文件名。无需维护旧清单。

服务端在自己的目录中运行，输入、输出和错误流直接连接调用方。Unix 上 Java 替换 Jartender 进程，直接接管 PID、信号和退出状态；Windows 上等待 Java 并返回其退出码。

### Fabric 安装

安装使用 [Fabric 官方 Meta API](https://github.com/FabricMC/fabric-meta)，默认选择兼容的最新稳定 Loader 和最新稳定 installer。需要固定版本时：

```sh
jartender install fabric --minecraft 1.21.4 --loader 0.16.10 --installer 1.0.3 --name survival
```

安装只准备 `server.jar` 和 `eula.txt`（初始值为 `false`），不启动 Java、不生成世界。首次 `run` 时 Fabric launcher 继续下载服务端所需文件，因此需要网络。

同名目录不会覆盖。下载有超时和大小限制，失败或 JAR 无效时清理本次暂存文件。强制终止进程可能留下 `.jartender-install-*` 隐藏目录，确认没有安装正在进行后可以删除。安装不会自动重试。

## 脚本与退出码

`list --json` 输出按名字排序的数组，每项包含 `name`、`server_type` 和 `jars`。`jars` 是相对于服务端目录的候选文件名；多个候选时类型为 `ambiguous`。空目录输出 `[]`，根目录不存在或不可读时报错。

`run --dry-run` 输出 JSON，包含 `directory`、`java`、`args`、`accept_eula`；即使指定 `--accept-eula` 也不写文件。

退出码：`0` 为成功，`1` 为操作错误，`2` 为命令用法错误。启动成功后返回 Java 的退出码，Unix 上保留信号终止状态。Jartender 自身的诊断写入 stderr，列表和预演结果写入 stdout；运行期间服务端使用自己的控制台流。

## 作为 Rust 库使用

同一个 crate 同时提供库和 CLI。宿主启动器可关闭 CLI 依赖：

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

- `discover`、`prepare_launch` 只读文件，返回服务器列表和 `LaunchPlan`。
- `LaunchPlan::to_command()` 只创建尚未启动的 `std::process::Command`。宿主决定标准流、启动、停止和进程生命周期；库不会接管或退出宿主进程。
- `check_eula` 只检查协议状态；宿主取得用户同意后，可显式调用 `accept_eula` 写入协议状态。
- `install::install_fabric` 接收 `FabricRequest` 和进度回调，返回 `InstallOutcome`。回调接收 `InstallEvent`；返回 `ControlFlow::Break(())` 可以请求取消，取消或失败后清理暂存文件。取消在事件及下载块之间检查，阻塞网络请求由超时限制。
- 错误通过公开的 `Error` 枚举返回，可区分多个 JAR、未接受 EULA、下载失败、取消等情况；错误和结果支持在线程间传递。

库要求根目录和旧清单路径为绝对路径；Java 必须是绝对路径或单独的可执行文件名。库不读取当前目录配置、`JAVA_HOME` 或主目录，也不输出到 stdout/stderr。若传入 `java` 这样的单独名称，宿主实际启动命令时由系统搜索 PATH。旧清单仅在显式设置 `LaunchOptions::legacy_list` 时使用。

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

首版支持 JAR 服务端的发现、启动和 Fabric launcher 安装。Forge 安装、Java 下载、插件及世界管理、TUI 尚未实现。

## 许可证

[GNU GPL v3](LICENSE)。原 Python 实现部分参考了 [Prism Launcher](https://github.com/PrismLauncher/PrismLauncher) 和 [MultiMC](https://github.com/MultiMC/MultiMC5)。
