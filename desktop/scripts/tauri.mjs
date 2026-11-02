import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";

// Desktop/GUI shells do not always inherit rustup's PATH from the login shell.
const cargoBin = join(homedir(), ".cargo", "bin");
const env = { ...process.env };
if (existsSync(cargoBin)) env.PATH = `${cargoBin}${delimiter}${env.PATH || ""}`;
const cli = fileURLToPath(
  new URL("../node_modules/@tauri-apps/cli/tauri.js", import.meta.url),
);
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], {
  stdio: "inherit",
  env,
});
child.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
