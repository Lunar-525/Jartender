import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import {
  call,
  native,
  type Snapshot,
  type Config,
  type Runtime,
  type Version,
  type Plugin,
} from "./api";
import "./style.css";
import { InstanceSwitcher, type PickerInstance } from "./InstanceSwitcher";
import { INSTANCE_SURFACE } from "./instanceMotion";

const design = (file: string) => `/design/${file}`;
const cores = [
  "Fabric",
  "Paper",
  "Folia",
  "Purpur",
  "Leaves",
  "Forge",
  "NeoForge",
  "Arclight",
  "Mohist",
  "Banner",
  "CatServer",
];
const empty: Snapshot = {
  settings: { root: "" },
  servers: [],
  skipped: [],
  running: [],
  installing: [],
  logs: [],
};
const defaultConfig: Config = {
  java: null,
  jvm_args: [],
  server_args: [],
  jar: null,
  args_file: null,
  gui: false,
};
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
const bytes = (n: number | null) =>
  n === null
    ? "—"
    : n < 1048576
      ? `${(n / 1024).toFixed(1)} KiB`
      : `${(n / 1048576).toFixed(1)} MiB`;
type Modal =
  | "settings"
  | "instances"
  | "install"
  | "config"
  | "plugins"
  | "archive"
  | "eula"
  | "stop"
  | "close"
  | "force-stop"
  | null;

function FabricBackground() {
  const layers: [string, number, number][] = [
    ["08bad.svg", 71.25, 35.625],
    ["1f970.svg", 126.23575, 71.25],
    ["b1cef.svg", 320.625, 106.875],
    ["2757a.svg", 356.25, 233.72795],
    ["363ee.svg", 285, 335.05554],
    ["85a3e.svg", 232.382, 427.5],
    ["2e42e.svg", 249.375, 178.125],
    ["e4fe2.svg", 213.75, 285],
    ["7cef0.svg", 106.875, 320.625],
    ["742f4.svg", 106.875, 356.25],
    ["e9d4b.svg", 463.125, 249.375],
    ["52a5d.svg", 320.625, 142.5],
  ];
  return (
    <div className="fabric-background" aria-hidden="true">
      <img src={design("651fc.png")} width="570" height="570" />
      {layers.map(([file, left, top]) => (
        <img key={file} src={design(file)} style={{ left, top }} />
      ))}
    </div>
  );
}

function App() {
  const [viewport, setViewport] = useState({
    width: window.innerWidth,
    height: window.innerHeight,
  });
  const surfaceScale = Math.min(
    0.85,
    viewport.width / INSTANCE_SURFACE.width,
    viewport.height / INSTANCE_SURFACE.height,
  );
  useEffect(() => {
    const resize = () =>
      setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const [data, setData] = useState<Snapshot>(empty);
  const [selected, setSelected] = useState(
    localStorage.getItem("jartender-selected") || "",
  );
  const selectedRef = useRef(selected);
  const [modal, setModal] = useState<Modal>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(native);
  const [command, setCommand] = useState("");
  const [follow, setFollow] = useState(true);
  const [expanded, setExpanded] = useState(false);
  const [rootPath, setRootPath] = useState("");
  const [config, setConfig] = useState<Config>(defaultConfig);
  const [java, setJava] = useState<Runtime[]>([]);
  const [javaLoading, setJavaLoading] = useState(false);
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [filter, setFilter] = useState("");
  const [draft, setDraft] = useState({
    name: "",
    core: "Fabric",
    minecraft: "",
    version: "",
    java: "",
  });
  const [versions, setVersions] = useState<Version[]>([]);
  const [builds, setBuilds] = useState<Version[]>([]);
  const [versionLoading, setVersionLoading] = useState(false);
  const [eulaConsent, setEulaConsent] = useState(false);
  const consoleRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const server = data.servers.find((s) => s.name === selected);
  const running = data.running.includes(selected);
  const installing = data.installing.includes(selected);

  useEffect(() => {
    if (!native) return;
    const frame = requestAnimationFrame(() => {
      const regions = modal
        ? [{ left: 0, top: 0, width: viewport.width, height: viewport.height }]
        : Array.from(
            document.querySelectorAll(
              ".white-frame,.titlebar,.settings-button,.selected-instance,.instance-rail[data-interactive='true'],.picker-actions,.console-wrap,.console-expand,.management,.launch-button,.toast",
            ),
          ).map((element) => {
            const { left, top, width, height } =
              element.getBoundingClientRect();
            return { left, top, width, height };
          });
      void call("set_hit_regions", { regions }).catch(() => {});
    });
    return () => cancelAnimationFrame(frame);
  }, [viewport, modal, expanded, error, notice, pickerOpen, data.servers.length, data.installing.length]);

  const select = (name: string) => {
    selectedRef.current = name;
    setSelected(name);
    localStorage.setItem("jartender-selected", name);
  };
  const refresh = useCallback(async () => {
    if (!native) return;
    const requested = selectedRef.current;
    const result = await call<Snapshot>("snapshot", {
      selected: requested || null,
    });
    if (selectedRef.current !== requested) return;
    setData(result);
    setLoading(false);
    if (
      !selectedRef.current ||
      ![...result.servers.map((s) => s.name), ...result.installing].includes(
        selectedRef.current,
      )
    ) {
      const name = result.servers[0]?.name || result.installing[0] || "";
      selectedRef.current = name;
      setSelected(name);
    }
  }, []);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        await refresh();
      } catch (e) {
        if (active) {
          setError(message(e));
          setLoading(false);
        }
      }
      if (active) timer = setTimeout(poll, 2000);
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [refresh]);
  useEffect(() => {
    if (follow && consoleRef.current)
      consoleRef.current.scrollTop = consoleRef.current.scrollHeight;
  }, [data.logs, selected, follow]);
  useEffect(() => {
    if (modal) setPickerOpen(false);
    if (modal && !dialogRef.current?.open) dialogRef.current?.showModal();
    else if (!modal) dialogRef.current?.close();
  }, [modal]);
  useEffect(() => {
    if (!native) return;
    const subscription = listen("close-blocked", () => setModal("close"));
    return () => {
      void subscription.then((fn) => fn());
    };
  }, []);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 5000);
    return () => clearTimeout(timer);
  }, [notice]);

  const pickerInstances: PickerInstance[] = [
    ...data.servers.map((server) => ({
      name: server.name,
      version: server.version,
      kind: server.server_type,
      status: data.running.includes(server.name)
        ? ("running" as const)
        : ("stopped" as const),
    })),
    ...data.installing
      .filter((name) => !data.servers.some((server) => server.name === name))
      .map((name) => ({
        name,
        version: null,
        kind: "Installing",
        status: "installing" as const,
      })),
  ];
  const chooseInstance = (name: string) => {
    select(name);
    setData((previous) => ({ ...previous, logs: [] }));
    void refresh().catch((error) => setError(message(error)));
  };

  const perform = async (
    action: () => Promise<unknown>,
    success?: string,
    close = false,
  ) => {
    setError("");
    setBusy(true);
    try {
      await action();
      if (success) setNotice(success);
      if (close) setModal(null);
      await refresh();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  const open = async (next: Modal) => {
    setPickerOpen(false);
    setError("");
    setNotice("");
    setModal(next);
    if (next === "settings") setRootPath(data.settings.root);
    if (next === "config") {
      setConfig(defaultConfig);
      await perform(async () =>
        setConfig(await call<Config>("load_config", { name: selected })),
      );
    }
    if (next === "plugins") {
      setPlugins([]);
      await perform(async () =>
        setPlugins(await call<Plugin[]>("list_plugins", { name: selected })),
      );
    }
  };
  const scanJava = async () => {
    setJavaLoading(true);
    setError("");
    try {
      const found = await call<{ runtimes: Runtime[] }>("scan_java");
      setJava(found.runtimes);
      if (!found.runtimes.length)
        setNotice("未发现 Java，请填写已安装 Java 的绝对路径。");
    } catch (e) {
      setError(message(e));
    } finally {
      setJavaLoading(false);
    }
  };
  const query = async (build: boolean) => {
    setVersionLoading(true);
    setError("");
    try {
      const found = await call<Version[]>("query_versions", {
        core: draft.core,
        minecraft: build ? draft.minecraft : null,
      });
      if (build) setBuilds(found);
      else setVersions(found);
      if (!found.length) setNotice("没有匹配的稳定版本。");
    } catch (e) {
      setError(message(e));
    } finally {
      setVersionLoading(false);
    }
  };
  const launch = () => {
    if (!server) return;
    if (server.eula === "not-accepted") {
      setEulaConsent(false);
      setModal("eula");
      return;
    }
    void perform(() => call("launch", { name: selected, acceptEula: false }));
  };
  const install = async (event: React.FormEvent) => {
    event.preventDefault();
    setError("");
    const name = draft.name.trim();
    select(name);
    setModal(null);
    try {
      await call("install_server", {
        input: {
          ...draft,
          name,
          minecraft: draft.minecraft.trim(),
          version: draft.version || null,
          java: draft.java || null,
        },
      });
      setNotice(`${name} 安装完成`);
      await refresh();
    } catch (e) {
      setError(message(e));
      await refresh().catch(() => {});
    }
  };
  const send = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!command.trim()) return;
    await perform(async () => {
      await call("send_command", { name: selected, command: command.trim() });
      setCommand("");
    });
  };
  const folder = (which: string) =>
    void perform(() => call("open_folder", { name: selected, folder: which }));
  const title = modal
    ? {
        settings: "应用设置",
        instances: "Manage Instances",
        install: "添加实例",
        config: "启动配置",
        plugins: "插件",
        archive: "移入回收目录",
        eula: "Minecraft EULA",
        stop: "停止服务端",
        close: "还有任务正在运行",
        "force-stop": "强制终止服务端",
      }[modal]
    : "";

  return (
    <main className="app">
      <div
        className="design-surface"
        style={{ transform: `translate(-50%, -50%) scale(${surfaceScale})` }}
      >
        <div className="design-artboard">
        <div className="white-frame" aria-hidden="true">
          <FabricBackground />
        </div>
        <header
          className="titlebar"
          data-tauri-drag-region
          onDoubleClick={() => {
            if (native) void getCurrentWindow().toggleMaximize();
          }}
        >
          <img className="title-line" src={design("66c08.svg")} alt="" />
          <h1 data-tauri-drag-region>JarTender</h1>
          <div className="window-controls">
            <button
              aria-label="最小化"
              onClick={() => {
                if (native) void getCurrentWindow().minimize();
              }}
            >
              <img src={design("331a6.svg")} alt="" />
            </button>
            <button
              aria-label="关闭"
              onClick={() => {
                if (native) void getCurrentWindow().close();
              }}
            >
              <img src={design("6772f.svg")} alt="" />
            </button>
          </div>
        </header>
        <aside className="sidebar">
          <div className="about">
            <p>Jartender Version 0.1.0 Beta</p>
            <p>Tauri desktop · Rust core</p>
            <p>
              This project is not affiliated in
              <br />
              any way with Mojang AB.
            </p>
          </div>
          <button
            className="settings-button asset-button"
            aria-label="应用设置"
            onClick={() => void open("settings")}
          >
            <img src={design("6d453.svg")} alt="" />
          </button>
          <div className="instance-picker">
            <InstanceSwitcher
              instances={pickerInstances}
              selected={selected}
              open={pickerOpen && !modal}
              loading={loading}
              disabled={busy}
              onOpenChange={setPickerOpen}
              onSelect={chooseInstance}
              onEmpty={() => void open("install")}
            />
            <div className="picker-actions">
              <button
                className="asset-button manage-button"
                onClick={() => void open("instances")}
                aria-label="管理实例"
              >
                <img src={design("b06d8.svg")} alt="Manage Instances" />
              </button>
              <button
                className="asset-button add-button"
                onClick={() => void open("install")}
                aria-label="添加实例"
              >
                <img src={design("3dbdf.svg")} alt="" />
              </button>
            </div>
          </div>
        </aside>
        <section
          className={`workspace ${expanded ? "expanded" : ""}`}
          aria-label="服务端控制台"
        >
          <div className="console-wrap">
            <img
              className="console-background"
              src={design("56b83.svg")}
              alt=""
            />
            <div
              className="console"
              ref={consoleRef}
              role="log"
              aria-label="控制台输出"
              tabIndex={0}
            >
              {data.logs.length ? (
                data.logs.map((line, i) => (
                  <div
                    key={i}
                    className={
                      /error|exception|失败/i.test(line)
                        ? "log-error"
                        : /warn|警告/i.test(line)
                          ? "log-warn"
                          : ""
                    }
                  >
                    {line || "\u00a0"}
                  </div>
                ))
              ) : (
                <div className="console-empty">
                  <span className="console-prompt">&gt;_</span>
                  <p>
                    {!native
                      ? "浏览器预览"
                      : loading
                        ? "正在读取实例…"
                        : server
                          ? "控制台已就绪"
                          : "欢迎使用 JarTender"}
                  </p>
                  <small>
                    {!native
                      ? "运行 npm run desktop 以使用本地功能"
                      : server
                        ? "启动服务端后，运行日志将显示在这里。"
                        : "添加一个实例，或在设置中选择服务器目录。"}
                  </small>
                </div>
              )}
            </div>
            <form className="command-bar" onSubmit={send}>
              <img src={design("573b8.svg")} alt="" />
              <span>&gt;</span>
              <input
                aria-label="服务端指令"
                placeholder={running ? "输入服务端指令…" : "等待服务端启动…"}
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                disabled={!running || busy}
                maxLength={4096}
              />
              <button
                type="submit"
                disabled={!running || busy || !command.trim()}
                aria-label="发送指令"
              >
                ↵
              </button>
            </form>
            <button
              className="console-expand asset-button"
              aria-label={expanded ? "还原控制台" : "展开控制台"}
              onClick={() => setExpanded(!expanded)}
            >
              <img src={design("2f952.svg")} alt="" />
              <span>{expanded ? "↙" : "↗"}</span>
            </button>
          </div>
          <div className="console-status">
            <span>
              <i className={running ? "online" : ""} />
              {installing
                ? "正在安装"
                : running
                  ? "进程运行中"
                  : server
                    ? "已停止"
                    : "未选择实例"}
            </span>
            <label>
              <input
                type="checkbox"
                checked={follow}
                onChange={(e) => setFollow(e.target.checked)}
              />{" "}
              自动滚动
            </label>
          </div>
          <div className="bottom-actions">
            <div className="management">
              <img src={design("2de31.svg")} alt="" />
              {(
                [
                  "启动配置",
                  "插件",
                  "打开实例目录",
                  "打开日志目录",
                  "移入回收目录",
                ] as const
              ).map((label, i) => (
                <button
                  key={label}
                  aria-label={label}
                  title={label}
                  disabled={!server || busy || (i === 4 && running)}
                  onClick={() => {
                    if (i === 0) void open("config");
                    if (i === 1) void open("plugins");
                    if (i === 2) folder("server");
                    if (i === 3) folder("logs");
                    if (i === 4) void open("archive");
                  }}
                />
              ))}
            </div>
            <button
              className={`launch-button ${running ? "is-running" : ""}`}
              disabled={(!server && !installing) || busy || loading}
              onClick={() =>
                installing
                  ? void perform(
                      () => call("cancel_install", { name: selected }),
                      "正在取消，请等待下载请求结束",
                    )
                  : running
                    ? setModal("stop")
                    : launch()
              }
              aria-label={
                installing ? "取消安装" : running ? "停止服务端" : "启动服务端"
              }
            >
              {running ? (
                <img src={design("0ad58.svg")} alt="Server Running" />
              ) : (
                <span>
                  {installing
                    ? "Cancel Installation"
                    : busy
                      ? "Please Wait"
                      : "Launch Server"}
                </span>
              )}
            </button>
          </div>
        </section>
        </div>
      </div>
      {error && (
        <div className="toast error" role="alert">
          <span>{error}</span>
          <button onClick={() => setError("")} aria-label="关闭错误">
            ×
          </button>
        </div>
      )}
      {notice && (
        <div className="toast notice" role="status">
          {notice}
        </div>
      )}
      <dialog
        ref={dialogRef}
        onCancel={(e) => {
          if (busy) e.preventDefault();
          else setModal(null);
        }}
        onClick={(e) => {
          if (e.target === e.currentTarget && !busy) setModal(null);
        }}
      >
        <div className="dialog-content">
          <div className="dialog-heading">
            <h2>{title}</h2>
            <button
              aria-label="关闭面板"
              disabled={busy}
              onClick={() => setModal(null)}
            >
              ×
            </button>
          </div>
          {error && (
            <p className="inline-error" role="alert">
              {error}
            </p>
          )}
          {modal === "settings" && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void perform(
                  () => call("set_root", { path: rootPath }),
                  "服务器目录已保存",
                  true,
                );
              }}
            >
              <label>
                服务器根目录
                <input
                  required
                  value={rootPath}
                  placeholder="/absolute/path/to/Servers"
                  onChange={(e) => setRootPath(e.target.value)}
                />
              </label>
              <p className="hint">
                选择已有的绝对路径。目录中的 JAR 和 Forge
                参数文件会自动识别；切换目录前请停止所有任务。
              </p>
              <button className="primary" disabled={busy}>
                保存目录
              </button>
              <hr />
              <h3>Java 环境</h3>
              <button
                type="button"
                onClick={() => void scanJava()}
                disabled={javaLoading}
              >
                {javaLoading ? "正在探测…" : "探测本机 Java"}
              </button>
              {java.map((r) => (
                <div className="runtime" key={r.path}>
                  <strong>
                    {r.vendor || "Java"} {r.version}
                  </strong>
                  <code>{r.path}</code>
                </div>
              ))}
              <p className="hint">
                Java 可执行文件可在每个实例的启动配置中单独选择。
              </p>
            </form>
          )}
          {modal === "instances" && (
            <>
              <div className="list-toolbar">
                <input
                  aria-label="搜索实例"
                  placeholder="搜索名称、版本或核心…"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                />
                <button onClick={() => void perform(refresh)}>刷新</button>
                <button
                  className="primary"
                  onClick={() => void open("install")}
                >
                  ＋ 添加
                </button>
              </div>
              <div className="instance-list">
                {data.servers
                  .filter((s) =>
                    `${s.name} ${s.version} ${s.server_type}`
                      .toLowerCase()
                      .includes(filter.toLowerCase()),
                  )
                  .map((s) => (
                    <button
                      className={`instance-row ${s.name === selected ? "active" : ""}`}
                      key={s.name}
                      onClick={() => {
                        chooseInstance(s.name);
                        setModal(null);
                      }}
                    >
                      <span>
                        <strong>{s.name}</strong>
                        <small>
                          {s.server_type} · {s.version || "未知版本"}
                        </small>
                      </span>
                      <span>
                        <small>{bytes(s.map_size_bytes)}</small>
                        <small>
                          {data.running.includes(s.name)
                            ? "● 运行中"
                            : "○ 已停止"}
                        </small>
                      </span>
                    </button>
                  ))}
                {data.installing
                  .filter((name) =>
                    name.toLowerCase().includes(filter.toLowerCase()),
                  )
                  .map((name) => (
                    <button
                      className="instance-row"
                      key={name}
                      onClick={() => {
                        chooseInstance(name);
                        setModal(null);
                      }}
                    >
                      <span>
                        <strong>{name}</strong>
                        <small>正在安装 · 查看进度或取消</small>
                      </span>
                    </button>
                  ))}
                {!data.servers.length && !data.installing.length && (
                  <p className="empty-state">
                    还没有实例。添加一个服务端，或在应用设置中选择现有目录。
                  </p>
                )}
              </div>
              <p className="hint path">
                {data.settings.root || "尚未连接本地后端"}
              </p>
              {data.skipped.map((s) => (
                <p className="inline-error" key={s.path}>
                  {s.path}：{s.reason}
                </p>
              ))}
            </>
          )}
          {modal === "install" && (
            <form onSubmit={(e) => void install(e)}>
              <label>
                实例名称
                <input
                  required
                  maxLength={100}
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                  placeholder="survival"
                  pattern="[^./\\\\][^/\\\\]*"
                />
              </label>
              <label>
                服务端核心
                <select
                  value={draft.core}
                  onChange={(e) => {
                    setDraft({
                      ...draft,
                      core: e.target.value,
                      minecraft: "",
                      version: "",
                    });
                    setVersions([]);
                    setBuilds([]);
                  }}
                >
                  {cores.map((c) => (
                    <option key={c}>{c}</option>
                  ))}
                </select>
              </label>
              <label>
                Minecraft 版本
                <div className="field-action">
                  <input
                    required
                    list="minecraft-versions"
                    value={draft.minecraft}
                    onChange={(e) => {
                      setDraft({
                        ...draft,
                        minecraft: e.target.value,
                        version: "",
                      });
                      setBuilds([]);
                    }}
                    placeholder="例如 1.21.4"
                  />
                  <button
                    type="button"
                    disabled={versionLoading}
                    onClick={() => void query(false)}
                  >
                    {versionLoading ? "查询中…" : "查询版本"}
                  </button>
                </div>
              </label>
              <datalist id="minecraft-versions">
                {versions.map((v) => (
                  <option key={v.version} value={v.version} />
                ))}
              </datalist>
              <label>
                核心构建 / Loader 版本
                <div className="field-action">
                  <input
                    list="core-builds"
                    value={draft.version}
                    onChange={(e) =>
                      setDraft({ ...draft, version: e.target.value })
                    }
                    placeholder="留空使用最新稳定版本"
                  />
                  <button
                    type="button"
                    disabled={versionLoading || !draft.minecraft}
                    onClick={() => void query(true)}
                  >
                    查询构建
                  </button>
                </div>
              </label>
              <datalist id="core-builds">
                {builds.map((v) => (
                  <option key={v.version} value={v.version}>
                    {v.recommended ? "推荐" : ""}
                  </option>
                ))}
              </datalist>
              {["Forge", "NeoForge"].includes(draft.core) && (
                <>
                  <label>
                    安装器使用的 Java
                    <input
                      required
                      value={draft.java}
                      onChange={(e) =>
                        setDraft({ ...draft, java: e.target.value })
                      }
                      placeholder="Java 可执行文件绝对路径或 java"
                    />
                  </label>
                  <p className="hint">此核心会下载并运行官方 Java 安装器。</p>
                </>
              )}
              <p className="hint">
                将查询官方来源并校验下载文件。同名目录不会覆盖，安装失败会清理暂存文件。
              </p>
              <button
                className="primary"
                disabled={
                  !native || busy || data.installing.includes(draft.name.trim())
                }
              >
                下载并安装
              </button>
            </form>
          )}
          {modal === "config" && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void perform(
                  () =>
                    call("save_config", {
                      name: selected,
                      config: {
                        ...config,
                        jvm_args: config.jvm_args.filter((a) => a.length > 0),
                        server_args: config.server_args.filter(
                          (a) => a.length > 0,
                        ),
                      },
                    }),
                  "启动配置已保存",
                  true,
                );
              }}
            >
              <p className="hint">
                {selected} · 每行一个参数，包含空格的参数会原样保留。
              </p>
              <label>
                Java 可执行文件
                <div className="field-action">
                  <input
                    list="java-runtimes"
                    value={config.java || ""}
                    placeholder="java（使用 PATH）"
                    onChange={(e) =>
                      setConfig({ ...config, java: e.target.value || null })
                    }
                  />
                  <button
                    type="button"
                    disabled={javaLoading}
                    onClick={() => void scanJava()}
                  >
                    {javaLoading ? "探测中…" : "探测 Java"}
                  </button>
                </div>
              </label>
              <datalist id="java-runtimes">
                {java.map((r) => (
                  <option key={r.path} value={r.path}>
                    {r.vendor} {r.version}
                  </option>
                ))}
              </datalist>
              <label>
                JVM 参数
                <textarea
                  value={config.jvm_args.join("\n")}
                  placeholder={"-Xms1G\n-Xmx2G"}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      jvm_args: e.target.value.split("\n"),
                    })
                  }
                />
              </label>
              <label>
                服务端参数
                <textarea
                  value={config.server_args.join("\n")}
                  placeholder="每行一个参数"
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      server_args: e.target.value.split("\n"),
                    })
                  }
                />
              </label>
              <label>
                启动 JAR
                <select
                  value={config.jar || ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      jar: e.target.value || null,
                      args_file: null,
                    })
                  }
                >
                  <option value="">自动识别</option>
                  {server?.jars.map((j) => (
                    <option key={j}>{j}</option>
                  ))}
                </select>
              </label>
              {!!server?.argument_files?.length && (
                <label>
                  Forge / NeoForge 参数文件
                  <select
                    value={config.args_file || ""}
                    onChange={(e) =>
                      setConfig({
                        ...config,
                        args_file: e.target.value || null,
                        jar: null,
                      })
                    }
                  >
                    <option value="">自动识别</option>
                    {server.argument_files.map((f) => (
                      <option key={f}>{f}</option>
                    ))}
                  </select>
                </label>
              )}
              <label className="check">
                <input
                  type="checkbox"
                  checked={!!config.gui}
                  onChange={(e) =>
                    setConfig({ ...config, gui: e.target.checked })
                  }
                />{" "}
                显示 Minecraft 图形控制台
              </label>
              <button className="primary" disabled={busy || running}>
                保存配置
              </button>
              {running && <p className="hint">停止服务端后可保存配置。</p>}
            </form>
          )}
          {modal === "plugins" && (
            <>
              <p className="hint">{selected} / plugins</p>
              {plugins.map((p) => (
                <div className="plugin-row" key={p.name}>
                  <code>{p.name}</code>
                  <small>{bytes(p.bytes)}</small>
                </div>
              ))}
              {!plugins.length && (
                <p className="empty-state">
                  没有找到插件 JAR。Fabric / Forge 模组位于 mods
                  目录，可通过实例目录访问。
                </p>
              )}
              <button onClick={() => folder("server")}>打开实例目录</button>
              {!!plugins.length && (
                <button onClick={() => folder("plugins")}>打开插件目录</button>
              )}
            </>
          )}
          {modal === "archive" && (
            <>
              <p>
                将「{selected}」移到服务器根目录下的{" "}
                <code>.jartender-trash</code>。世界、配置和插件均会保留。
              </p>
              <p className="hint">
                恢复时，将回收目录中的实例文件夹移回根目录并恢复名称。
              </p>
              <button
                className="danger"
                disabled={busy}
                onClick={() =>
                  void perform(
                    () => call("archive_server", { name: selected }),
                    "实例已移入回收目录",
                    true,
                  )
                }
              >
                移入回收目录
              </button>
            </>
          )}
          {modal === "eula" && (
            <>
              <p>
                启动「{selected}」前，请阅读{" "}
                <a
                  href="https://aka.ms/MinecraftEULA"
                  target="_blank"
                  rel="noreferrer"
                  onClick={(e) => {
                    if (native) {
                      e.preventDefault();
                      void perform(() => call("open_eula"));
                    }
                  }}
                >
                  Minecraft 最终用户许可协议
                </a>
                。
              </p>
              <label className="check">
                <input
                  type="checkbox"
                  checked={eulaConsent}
                  onChange={(e) => setEulaConsent(e.target.checked)}
                />{" "}
                我已阅读并同意 Minecraft EULA
              </label>
              <p className="hint">
                确认后将此实例的 eula.txt 设置为 eula=true，并启动服务端。
              </p>
              <button
                className="primary"
                disabled={!eulaConsent || busy}
                onClick={() =>
                  void perform(
                    () => call("launch", { name: selected, acceptEula: true }),
                    undefined,
                    true,
                  )
                }
              >
                同意并启动
              </button>
            </>
          )}
          {modal === "stop" && (
            <>
              <p>向「{selected}」发送 stop 指令，等待世界保存完成。</p>
              <div className="dialog-actions">
                <button
                  className="primary"
                  disabled={busy}
                  onClick={() =>
                    void perform(
                      () =>
                        call("send_command", {
                          name: selected,
                          command: "stop",
                        }),
                      "已发送 stop，请等待服务端退出",
                      true,
                    )
                  }
                >
                  正常停止
                </button>
                <button
                  className="danger"
                  disabled={busy}
                  onClick={() => setModal("force-stop")}
                >
                  强制停止
                </button>
              </div>
            </>
          )}
          {modal === "force-stop" && (
            <>
              <p>强制终止「{selected}」可能丢失尚未保存的世界数据。</p>
              <button
                className="danger"
                disabled={busy}
                onClick={() =>
                  void perform(
                    () => call("force_stop", { name: selected }),
                    "服务端已强制停止",
                    true,
                  )
                }
              >
                确认强制终止
              </button>
            </>
          )}
          {modal === "close" && (
            <>
              <p>请先停止运行的服务端并等待安装完成，再关闭应用。</p>
              {data.running.map((name) => (
                <div className="plugin-row" key={name}>
                  <strong>{name}</strong>
                  <button
                    disabled={busy}
                    onClick={() =>
                      void perform(
                        () => call("send_command", { name, command: "stop" }),
                        `已向 ${name} 发送 stop`,
                      )
                    }
                  >
                    正常停止
                  </button>
                </div>
              ))}
              {data.installing.map((name) => (
                <div className="plugin-row" key={name}>
                  <strong>{name} · 安装中</strong>
                  <button
                    onClick={() =>
                      void perform(
                        () => call("cancel_install", { name }),
                        "正在取消安装",
                      )
                    }
                  >
                    取消安装
                  </button>
                </div>
              ))}
              <button
                className="primary"
                disabled={!!data.running.length || !!data.installing.length}
                onClick={() => {
                  if (native) void getCurrentWindow().close();
                }}
              >
                关闭应用
              </button>
            </>
          )}
        </div>
      </dialog>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
