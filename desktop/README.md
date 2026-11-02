# JarTender Desktop

基于当前 Figma 设计的 Tauri 2 + React 桌面应用。主界面按原稿的 85% 显示，小窗口会继续等比缩小；放大窗口不会将控件放大超过此比例。Rust 后端通过路径依赖直接复用仓库中的 `jartender` 库，原 CLI 独立保留。

## 运行

需要 Node.js 20.19+ / 22.12+ 和 Rust 稳定版；本项目使用 Vite 6。macOS 需安装 Xcode Command Line Tools；Windows 需要 MSVC 工具链和 WebView2；Linux 需要 Tauri 的 WebKitGTK 系统依赖，参见 [Tauri 官方环境准备](https://v2.tauri.app/start/prerequisites/)。

```sh
cd desktop
npm ci
npm run desktop
```

开发服务器固定为 `127.0.0.1:1420`。`npm run desktop` 自动启动开发服务器，不要同时手动运行 `npm run dev`。只预览界面时可运行 `npm run dev`；浏览器预览不提供本地文件和进程功能。

```sh
npm run bundle
```

macOS 产物位于 `src-tauri/target/release/bundle/macos/JarTender.app` 和 `src-tauri/target/release/bundle/dmg/`。应用未配置开发者签名和 Apple 公证。其他平台可通过 Tauri 的 `--bundles` 参数选择本机支持的包格式。

## 使用

- 开发模式首次使用仓库的 `Servers/`；打包应用默认使用系统应用数据目录下的 `Servers/`。在左上角设置面板中填写已有服务器根目录的绝对路径可切换，并持久化到系统应用配置目录中的 `settings.json`。切换目录时不能有运行或安装中的任务。
- 鼠标移到左下角当前实例卡片上会展开实例列表。在当前卡片或列表上滚动可浏览，滚动不会切换实例；左键点击列表卡片才会选择实例并刷新控制台。列表显示名称、版本、核心及运行/安装状态，定时刷新保留浏览位置；移开鼠标、点击其他区域或按 Escape 可收起。也支持方向键浏览。
- 「Manage Instances」自动发现已有 JAR 服务端和 Forge/NeoForge 参数文件，保留搜索和选择入口；加号可查询官方版本、指定构建并安装，下载日志显示在控制台中。安装不启动服务器、不代替用户接受 EULA；可请求取消，阻塞中的请求仍需等待库的超时或下一个进度事件。
- 第一个管理按钮读取和保存实例的 `jartender.json`，可探测本地 Java、选择启动入口、设置 JVM/服务端参数。每行是一个独立参数，包含空格的参数不会再次拆分。
- 「Launch Server」启动真实 Java 进程；首次启动要求在应用中阅读并确认 EULA。确认只作用于当前实例。运行时按钮显示「Server Running」，点击可正常停止或再次确认后强制终止。
- 控制台显示标准输出与标准错误，支持发送指令、自动滚动和展开。历史保留最近 1500 行，每行最多 8192 字符；Minecraft 自己的文件日志不受此限制。状态显示进程存活情况，不表示 Minecraft 已就绪或端口可连接。
- 插件面板列出已有 `plugins/*.jar`，提供目录入口；不下载或修改插件。文件夹按钮打开实例目录，日志按钮打开 Minecraft 的日志目录（尚未生成时会提示错误）。
- 回收按钮将停止的实例移入根目录的 `.jartender-trash/<时间>-<名称>`，保留所有文件。恢复时手动移回并恢复原文件夹名称。
- 关闭窗口或退出应用前，应用要求先停止服务端、完成或取消安装。被系统强杀或断电时仍遵循操作系统的进程行为。

## 验证

```sh
npm run build
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

测试使用本地短时子进程和临时目录验证输入/输出、进程回收、日志上限、路径及符号链接边界，不启动 Minecraft。

## 设计与资源

参考 [Jartender Figma 设计](https://www.figma.com/design/xO9ca81GJwIjbYx5SrG9c2/Jartender?node-id=1-1303)。该链接选中版本说明图层；主界面元素分别位于同页的 `2:20`、`24:30`、`1:11`、`1:24`、`1:118`、`1:148`、`1:1298`。

主界面使用透明、无系统边框的原生窗口。白色主体是内部画布，Title、实例选择、Manage / ＋、Launch 和日志区保留原稿相对白色主体的越界位置。窗口外围留出阴影空间，透明留白通过原生鼠标命中区域处理点击穿透；弹窗打开时整个窗口接收交互。

`public/design/` 保存从 Figma 下载的原始资源，`design-assets.json` 记录资源及对应图层。运行时没有临时 Figma URL。位图背景按设计的 570×570 画布和图层偏移组合；管理按钮保留原始 521×100 SVG，包括阴影，交互热区对应五个 80×80 按钮。实例版本、名称、日志及运行状态来自本地后端。

随包包含设计所用的 [BoutiqueBitmap9x9 3D 开源字体](https://github.com/scott0107000/BoutiqueBitmap9x9)，许可见 `public/fonts/OFL.txt`。`app-icon-bartender-simple.png` 是应用图标源，打包图标位于 `src-tauri/icons-bartender-simple/`。生成说明见 `app-icon-bartender-simple.md`。

文字按钮、英文说明及弹窗标题使用用户提供的 `Branch.otf`，随应用离线加载。像素标题和版本号继续使用 BoutiqueBitmap9x9 3D；控制台保留等宽字体。
