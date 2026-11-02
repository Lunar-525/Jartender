import { invoke, isTauri } from "@tauri-apps/api/core";

export const native = isTauri();
export function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!native)
    return Promise.reject(
      new Error("请使用 Tauri 桌面应用访问本地功能：npm run desktop"),
    );
  return invoke<T>(command, args);
}
export interface Server {
  name: string;
  server_type: string;
  version: string | null;
  map_size_bytes: number | null;
  eula: string;
  jars: string[];
  argument_files?: string[];
}
export interface Snapshot {
  settings: { root: string };
  servers: Server[];
  skipped: { path: string; reason: string }[];
  running: string[];
  installing: string[];
  logs: string[];
}
export interface Config {
  java: string | null;
  jvm_args: string[];
  server_args: string[];
  jar: string | null;
  args_file: string | null;
  gui: boolean | null;
}
export interface Runtime {
  path: string;
  version: string;
  vendor: string | null;
  arch: string | null;
}
export interface Version {
  version: string;
  recommended: boolean;
  stable: boolean;
}
export interface Plugin {
  name: string;
  bytes: number;
}
