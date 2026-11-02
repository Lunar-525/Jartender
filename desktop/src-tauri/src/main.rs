#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use jartender::{LaunchOptions, config::ServerConfig, install, java, versions};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::{BufRead, BufReader, Read, Write},
    ops::ControlFlow,
    path::{Component, Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager, State};

type Result<T> = std::result::Result<T, String>;
const LOG_LIMIT: usize = 1500;

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    root: PathBuf,
}

struct Process {
    child: Child,
    input: ChildStdin,
}

#[derive(Default)]
struct Inner {
    processes: HashMap<String, Process>,
    logs: HashMap<String, Arc<Mutex<VecDeque<String>>>>,
    installs: HashMap<String, Arc<AtomicBool>>,
}

struct Desktop {
    settings: Mutex<Settings>,
    settings_file: PathBuf,
    inner: Mutex<Inner>,
}

#[derive(Clone, Copy, Deserialize)]
struct HitRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

impl HitRect {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && x < self.left + self.width && y >= self.top && y < self.top + self.height
    }
}

#[derive(Default)]
struct SurfaceInput {
    regions: Mutex<Vec<HitRect>>,
    stopped: AtomicBool,
}

#[tauri::command]
fn set_hit_regions(state: State<'_, Arc<SurfaceInput>>, regions: Vec<HitRect>) -> Result<()> {
    if regions.len() > 64
        || regions.iter().any(|r| {
            ![r.left, r.top, r.width, r.height]
                .iter()
                .all(|n| n.is_finite())
                || r.width < 0.0
                || r.height < 0.0
        })
    {
        return Err("无效的窗口交互区域".into());
    }
    *lock(&state.regions)? = regions;
    Ok(())
}

fn track_surface_input(window: tauri::WebviewWindow, input: Arc<SurfaceInput>) {
    // Native cursor polling also works while the transparent margin ignores mouse events.
    // DOM pointer-events alone cannot pass a click through to another application.
    std::thread::spawn(move || {
        let mut ignored = false;
        while !input.stopped.load(Ordering::Relaxed) {
            if let (Ok(cursor), Ok(origin), Ok(scale), Ok(regions)) = (
                window.cursor_position(),
                window.inner_position(),
                window.scale_factor(),
                input.regions.lock(),
            ) && !regions.is_empty()
            {
                let x = (cursor.x - f64::from(origin.x)) / scale;
                let y = (cursor.y - f64::from(origin.y)) / scale;
                let next = !regions.iter().any(|r| r.contains(x, y));
                if next != ignored && window.set_ignore_cursor_events(next).is_ok() {
                    ignored = next;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    });
}

impl Drop for Desktop {
    fn drop(&mut self) {
        if let Ok(inner) = self.inner.get_mut() {
            for cancel in inner.installs.values() {
                cancel.store(true, Ordering::Relaxed);
            }
            for process in inner.processes.values_mut() {
                let _ = process.child.kill();
                let _ = process.child.wait();
            }
        }
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    m.lock().map_err(err)
}
fn root(state: &Desktop) -> Result<PathBuf> {
    Ok(lock(&state.settings)?.root.clone())
}
fn validate_name(name: &str) -> Result<()> {
    let mut parts = Path::new(name).components();
    if name.is_empty()
        || name.starts_with('.')
        || name.contains(['/', '\\', '\0'])
        || !matches!(parts.next(), Some(Component::Normal(_)))
        || parts.next().is_some()
    {
        return Err("实例名称必须是单个目录名，不能以点开头".into());
    }
    Ok(())
}
fn server_dir(state: &Desktop, name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    let base = fs::canonicalize(root(state)?).map_err(err)?;
    let dir = fs::canonicalize(base.join(name)).map_err(err)?;
    if !dir.starts_with(&base) || dir == base || !dir.is_dir() {
        return Err("实例目录超出服务器根目录".into());
    }
    Ok(dir)
}
fn append(log: &Arc<Mutex<VecDeque<String>>>, line: String) {
    if let Ok(mut lines) = log.lock() {
        if lines.len() >= LOG_LIMIT {
            lines.pop_front();
        }
        lines.push_back(line.chars().take(8192).collect());
    }
}
// Read bounded chunks, including output without newlines; a server cannot grow a line indefinitely.
fn pipe(reader: impl Read + Send + 'static, log: Arc<Mutex<VecDeque<String>>>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut bytes = Vec::new();
            match reader.by_ref().take(8192).read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(_) => append(&log, String::from_utf8_lossy(&bytes).trim_end().to_string()),
            }
        }
    });
}

fn spawn_process(command: &mut Command, log: Arc<Mutex<VecDeque<String>>>) -> Result<Process> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(err)?;
    let input = child.stdin.take().ok_or("无法打开服务端输入流")?;
    if let Some(stdout) = child.stdout.take() {
        pipe(stdout, log.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        pipe(stderr, log);
    }
    Ok(Process { child, input })
}
fn reap(inner: &mut Inner) {
    let ended: Vec<_> = inner
        .processes
        .iter_mut()
        .filter_map(|(name, process)| match process.child.try_wait() {
            Ok(Some(status)) => Some((name.clone(), format!("[JarTender] 进程已退出：{status}"))),
            _ => None,
        })
        .collect();
    for (name, message) in ended {
        inner.processes.remove(&name);
        if let Some(log) = inner.logs.get(&name) {
            append(log, message);
        }
    }
}

#[derive(Serialize)]
struct Snapshot {
    settings: Settings,
    servers: Vec<jartender::Server>,
    skipped: Vec<jartender::Skipped>,
    running: Vec<String>,
    installing: Vec<String>,
    logs: Vec<String>,
}

#[tauri::command]
async fn snapshot(state: State<'_, Arc<Desktop>>, selected: Option<String>) -> Result<Snapshot> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = lock(&state.settings)?.clone();
        let discovery = jartender::scan(&settings.root, None).map_err(err)?;
        let mut inner = lock(&state.inner)?;
        reap(&mut inner);
        let logs = selected
            .and_then(|name| inner.logs.get(&name))
            .map(|log| lock(log).map(|l| l.iter().cloned().collect()))
            .transpose()?
            .unwrap_or_default();
        Ok(Snapshot {
            settings,
            servers: discovery.servers,
            skipped: discovery.skipped,
            running: inner.processes.keys().cloned().collect(),
            installing: inner.installs.keys().cloned().collect(),
            logs,
        })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn set_root(state: State<'_, Arc<Desktop>>, path: String) -> Result<()> {
    let mut inner = lock(&state.inner)?;
    reap(&mut inner);
    if !inner.processes.is_empty() || !inner.installs.is_empty() {
        return Err("请先停止所有服务端并等待安装完成".into());
    }
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("请输入绝对路径".into());
    }
    let path = fs::canonicalize(path).map_err(err)?;
    if !path.is_dir() {
        return Err("请选择一个已有目录".into());
    }
    let settings = Settings { root: path };
    let mut current = lock(&state.settings)?;
    let mut staged =
        tempfile::NamedTempFile::new_in(state.settings_file.parent().ok_or("设置路径无效")?)
            .map_err(err)?;
    staged
        .write_all(&serde_json::to_vec_pretty(&settings).map_err(err)?)
        .map_err(err)?;
    staged.persist(&state.settings_file).map_err(err)?;
    *current = settings;
    inner.logs.clear();
    Ok(())
}

#[tauri::command]
async fn scan_java() -> Result<java::Discovery> {
    tauri::async_runtime::spawn_blocking(|| java::scan(&java::ScanOptions::system()).map_err(err))
        .await
        .map_err(err)?
}

#[tauri::command]
fn load_config(state: State<'_, Arc<Desktop>>, name: String) -> Result<ServerConfig> {
    server_dir(&state, &name)?;
    Ok(ServerConfig::load(&root(&state)?, &name)
        .map_err(err)?
        .unwrap_or_default())
}

#[tauri::command]
fn save_config(state: State<'_, Arc<Desktop>>, name: String, config: ServerConfig) -> Result<()> {
    let mut inner = lock(&state.inner)?;
    reap(&mut inner);
    if inner.processes.contains_key(&name) {
        return Err("请先停止此服务端再修改启动配置".into());
    }
    server_dir(&state, &name)?;
    config.save(&root(&state)?, &name).map_err(err)
}

#[tauri::command]
async fn launch(state: State<'_, Arc<Desktop>>, name: String, accept_eula: bool) -> Result<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut inner = lock(&state.inner)?;
        reap(&mut inner);
        if inner.processes.contains_key(&name) || inner.installs.contains_key(&name) {
            return Err("实例已在运行或安装中".into());
        }
        let directory = server_dir(&state, &name)?;
        let base = root(&state)?;
        let mut options = LaunchOptions::new(&name, "java");
        if let Some(config) = ServerConfig::load(&base, &name).map_err(err)? {
            config.apply(&directory, &mut options).map_err(err)?;
        }
        let plan = jartender::prepare_launch(&base, &options).map_err(err)?;
        if accept_eula {
            plan.accept_eula().map_err(err)?;
        }
        plan.check_eula().map_err(err)?;
        let mut command = plan.to_command();
        let log = Arc::new(Mutex::new(VecDeque::new()));
        append(&log, format!("[JarTender] 正在启动 {name}"));
        let process = spawn_process(&mut command, log.clone())?;
        inner.logs.insert(name.clone(), log);
        inner.processes.insert(name, process);
        Ok(())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn send_command(state: State<'_, Arc<Desktop>>, name: String, command: String) -> Result<()> {
    if command.trim().is_empty() || command.len() > 4096 || command.contains(['\n', '\r', '\0']) {
        return Err("请输入一行有效指令（最多 4096 字节）".into());
    }
    let mut inner = lock(&state.inner)?;
    reap(&mut inner);
    let process = inner.processes.get_mut(&name).ok_or("实例未运行")?;
    writeln!(process.input, "{command}")
        .and_then(|_| process.input.flush())
        .map_err(err)?;
    if let Some(log) = inner.logs.get(&name) {
        append(log, format!("> {command}"));
    }
    Ok(())
}

#[tauri::command]
fn force_stop(state: State<'_, Arc<Desktop>>, name: String) -> Result<()> {
    let mut inner = lock(&state.inner)?;
    let process = inner.processes.get_mut(&name).ok_or("实例未运行")?;
    process.child.kill().map_err(err)?;
    process.child.wait().map_err(err)?;
    inner.processes.remove(&name);
    if let Some(log) = inner.logs.get(&name) {
        append(log, "[JarTender] 进程已强制终止".into());
    }
    Ok(())
}

fn kind(name: &str) -> Result<install::InstallKind> {
    install::InstallKind::ALL
        .iter()
        .copied()
        .find(|k| k.name().eq_ignore_ascii_case(name))
        .ok_or("未知核心类型".into())
}

#[tauri::command]
async fn query_versions(core: String, minecraft: Option<String>) -> Result<Vec<versions::Version>> {
    tauri::async_runtime::spawn_blocking(move || {
        let core = kind(&core)?;
        match minecraft.filter(|v| !v.is_empty()) {
            Some(mc) => versions::query(&versions::VersionQuery::loader(mc, core)).map_err(err),
            None => versions::supported_minecraft(core, false, None).map_err(err),
        }
    })
    .await
    .map_err(err)?
}

#[derive(Deserialize)]
struct InstallInput {
    name: String,
    core: String,
    minecraft: String,
    version: Option<String>,
    java: Option<PathBuf>,
}

#[tauri::command]
async fn install_server(state: State<'_, Arc<Desktop>>, input: InstallInput) -> Result<()> {
    validate_name(&input.name)?;
    let mut request =
        install::InstallRequest::new(kind(&input.core)?, &input.name, input.minecraft);
    request.version = input.version.filter(|v| !v.is_empty() && v != "latest");
    request.java = input.java.filter(|p| !p.as_os_str().is_empty());
    let (base, cancel, log) = {
        let mut inner = lock(&state.inner)?;
        let base = root(&state)?;
        if base.join(&input.name).exists() || inner.installs.contains_key(&input.name) {
            return Err("同名实例已存在或正在安装".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(VecDeque::new()));
        inner.installs.insert(input.name.clone(), cancel.clone());
        inner.logs.insert(input.name.clone(), log.clone());
        (base, cancel, log)
    };
    let state = state.inner().clone();
    let name = input.name;
    let name_for_job = name.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut last_percent = None;
        let outcome = install::install(&base, &request, |event| {
            if cancel.load(Ordering::Relaxed) {
                return ControlFlow::Break(());
            }
            match event {
                install::InstallEvent::Progress { downloaded, total } => {
                    let percent = total
                        .filter(|t| *t > 0)
                        .map(|t| downloaded.saturating_mul(100) / t);
                    if percent != last_percent {
                        append(
                            &log,
                            format!(
                                "[下载] {} MiB / {}",
                                downloaded / 1048576,
                                percent
                                    .map(|p| format!("{p}%"))
                                    .unwrap_or("未知大小".into())
                            ),
                        );
                        last_percent = percent;
                    }
                }
                install::InstallEvent::Output { line } => append(&log, line),
                event => append(&log, format!("[安装] {event:?}")),
            }
            ControlFlow::Continue(())
        })
        .map(|_| ())
        .map_err(err);
        append(
            &log,
            match &outcome {
                Ok(()) => "[JarTender] 安装完成".into(),
                Err(e) => format!("[JarTender] {e}"),
            },
        );
        (name_for_job, outcome)
    })
    .await;
    lock(&state.inner)?.installs.remove(&name);
    result.map_err(err)?.1
}

#[tauri::command]
fn cancel_install(state: State<'_, Arc<Desktop>>, name: String) -> Result<()> {
    lock(&state.inner)?
        .installs
        .get(&name)
        .ok_or("实例不在安装中")?
        .store(true, Ordering::Relaxed);
    Ok(())
}

#[derive(Serialize)]
struct Entry {
    name: String,
    bytes: u64,
}

#[tauri::command]
fn list_plugins(state: State<'_, Arc<Desktop>>, name: String) -> Result<Vec<Entry>> {
    let base = server_dir(&state, &name)?;
    let directory = base.join("plugins");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let directory = fs::canonicalize(directory).map_err(err)?;
    if !directory.starts_with(&base) {
        return Err("插件目录超出实例目录".into());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory).map_err(err)? {
        let entry = entry.map_err(err)?;
        let meta = entry.metadata().map_err(err)?;
        if meta.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
        {
            entries.push(Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                bytes: meta.len(),
            });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[tauri::command]
fn open_folder(state: State<'_, Arc<Desktop>>, name: String, folder: String) -> Result<()> {
    let base = server_dir(&state, &name)?;
    let path = match folder.as_str() {
        "server" => base.clone(),
        "plugins" | "logs" => base.join(&folder),
        _ => return Err("不支持的目录".into()),
    };
    let path = fs::canonicalize(path).map_err(err)?;
    if !path.starts_with(base) || !path.is_dir() {
        return Err("目录不存在或超出实例目录".into());
    }
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command.arg(path).spawn().map_err(err)?;
    Ok(())
}

#[tauri::command]
fn open_eula() -> Result<()> {
    const URL: &str = "https://aka.ms/MinecraftEULA";
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command.arg(URL).spawn().map_err(err)?;
    Ok(())
}

#[tauri::command]
fn archive_server(state: State<'_, Arc<Desktop>>, name: String) -> Result<()> {
    let mut inner = lock(&state.inner)?;
    reap(&mut inner);
    if inner.processes.contains_key(&name) || inner.installs.contains_key(&name) {
        return Err("请先停止实例".into());
    }
    let directory = server_dir(&state, &name)?;
    let trash = root(&state)?.join(".jartender-trash");
    fs::create_dir_all(&trash).map_err(err)?;
    let base = fs::canonicalize(root(&state)?).map_err(err)?;
    let trash = fs::canonicalize(trash).map_err(err)?;
    if !trash.starts_with(base) {
        return Err("回收目录超出根目录".into());
    }
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_nanos();
    fs::rename(directory, trash.join(format!("{time}-{name}"))).map_err(err)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let surface_input = Arc::new(SurfaceInput::default());
            app.manage(surface_input.clone());
            if let Some(window) = app.get_webview_window("main") {
                track_surface_input(window, surface_input);
            }
            let config = app.path().app_config_dir()?;
            fs::create_dir_all(&config)?;
            let settings_file = config.join("settings.json");
            let settings: Settings = if settings_file.exists() {
                serde_json::from_slice(&fs::read(&settings_file)?)?
            } else {
                let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Servers");
                let root = if cfg!(debug_assertions) && workspace.is_dir() {
                    fs::canonicalize(workspace)?
                } else {
                    app.path().app_data_dir()?.join("Servers")
                };
                fs::create_dir_all(&root)?;
                Settings { root }
            };
            app.manage(Arc::new(Desktop {
                settings: Mutex::new(settings),
                settings_file,
                inner: Mutex::new(Inner::default()),
            }));
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .state::<Arc<SurfaceInput>>()
                    .stopped
                    .store(true, Ordering::Relaxed);
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<Arc<Desktop>>();
                if let Ok(mut inner) = state.inner.lock() {
                    reap(&mut inner);
                    if !inner.processes.is_empty() || !inner.installs.is_empty() {
                        api.prevent_close();
                        let _ = window.emit("close-blocked", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            set_root,
            scan_java,
            load_config,
            save_config,
            launch,
            send_command,
            force_stop,
            query_versions,
            install_server,
            cancel_install,
            list_plugins,
            open_folder,
            archive_server,
            open_eula,
            set_hit_regions
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 JarTender")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let state = app.state::<Arc<Desktop>>();
                if let Ok(mut inner) = state.inner.lock() {
                    reap(&mut inner);
                    if !inner.processes.is_empty() || !inner.installs.is_empty() {
                        api.prevent_exit();
                        let _ = app.emit("close-blocked", ());
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_cannot_escape_root() {
        for bad in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            ".hidden",
            "/tmp",
            "a\0b",
        ] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        for good in ["survival", "我的服务器", "paper 1.21"] {
            assert!(validate_name(good).is_ok());
        }
    }
    #[test]
    fn console_has_bounded_history_and_line_length() {
        let log = Arc::new(Mutex::new(VecDeque::new()));
        for i in 0..2000 {
            append(&log, i.to_string());
        }
        append(&log, "x".repeat(16000));
        let lines = log.lock().unwrap();
        assert_eq!(lines.len(), LOG_LIMIT);
        assert_eq!(lines.front().unwrap(), "501");
        assert_eq!(lines.back().unwrap().len(), 8192);
    }

    #[cfg(unix)]
    #[test]
    fn child_console_accepts_input_captures_both_streams_and_reaps_exit() {
        let log = Arc::new(Mutex::new(VecDeque::new()));
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "echo ready; echo diagnostic >&2; while IFS= read -r line; do echo \"received: $line\"; [ \"$line\" = stop ] && exit 0; done"]);
        let mut process = spawn_process(&mut command, log.clone()).unwrap();
        writeln!(process.input, "say hello\nstop").unwrap();
        process.input.flush().unwrap();
        let mut inner = Inner::default();
        inner.processes.insert("test".into(), process);
        inner.logs.insert("test".into(), log.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            reap(&mut inner);
            let lines = log.lock().unwrap();
            let complete = inner.processes.is_empty()
                && lines.iter().any(|s| s == "received: say hello")
                && lines.iter().any(|s| s == "diagnostic");
            drop(lines);
            if complete {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "child process did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let lines = log.lock().unwrap();
        assert!(lines.iter().any(|s| s == "ready"));
        assert!(lines.iter().any(|s| s.contains("进程已退出")));
    }

    #[cfg(unix)]
    #[test]
    fn server_symlink_cannot_target_a_directory_outside_root() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("Servers");
        let outside = temp.path().join("outside");
        fs::create_dir(&base).unwrap();
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("escape")).unwrap();
        let desktop = Desktop {
            settings: Mutex::new(Settings { root: base }),
            settings_file: temp.path().join("settings.json"),
            inner: Mutex::new(Inner::default()),
        };
        assert!(server_dir(&desktop, "escape").is_err());
    }
}
