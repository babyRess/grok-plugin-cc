import { spawn, spawnSync } from "node:child_process";
import process from "node:process";

/**
 * @param {string} command
 * @param {string[]} [args]
 * @param {{ cwd?: string, env?: NodeJS.ProcessEnv, timeoutMs?: number }} [options]
 */
export function runCommand(command, args = [], options = {}) {
  try {
    const result = spawnSync(command, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      encoding: "utf8",
      timeout: options.timeoutMs ?? 60_000,
      shell: process.platform === "win32",
      windowsHide: true
    });
    return {
      status: result.status ?? 1,
      stdout: result.stdout ?? "",
      stderr: result.stderr ?? "",
      error: result.error ?? null
    };
  } catch (error) {
    return {
      status: 1,
      stdout: "",
      stderr: error instanceof Error ? error.message : String(error),
      error
    };
  }
}

/**
 * @param {string} command
 * @param {string[]} [versionArgs]
 * @param {{ cwd?: string }} [options]
 */
export function binaryAvailable(command, versionArgs = ["--version"], options = {}) {
  const result = runCommand(command, versionArgs, { ...options, timeoutMs: 15_000 });
  if (result.error && result.error.code === "ENOENT") {
    return { available: false, detail: `${command} not found on PATH` };
  }
  if (result.status !== 0 && !result.stdout.trim()) {
    return {
      available: false,
      detail: result.stderr.trim() || result.error?.message || `${command} failed`
    };
  }
  const detail = (result.stdout || result.stderr).trim().split(/\r?\n/)[0] || command;
  return { available: true, detail };
}

/**
 * Return true if `pid` is currently alive.
 * @param {number} pid
 */
export function isProcessAlive(pid) {
  if (!Number.isFinite(pid) || pid <= 0) {
    return false;
  }
  try {
    // signal 0: existence check, no delivery
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/**
 * @param {number} pid
 * @param {NodeJS.Signals | number} [signal]
 */
export function terminateProcessTree(pid, signal = "SIGTERM") {
  if (!Number.isFinite(pid) || pid <= 0) {
    return false;
  }

  try {
    if (process.platform === "win32") {
      spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true
      });
      return true;
    }
    process.kill(-pid, signal);
    return true;
  } catch {
    try {
      process.kill(pid, signal);
      return true;
    } catch {
      return false;
    }
  }
}

/**
 * Spawn a detached process and return immediately.
 * @param {string} command
 * @param {string[]} args
 * @param {{ cwd?: string, env?: NodeJS.ProcessEnv, logFile?: string }} [options]
 */
export function spawnDetached(command, args, options = {}) {
  const stdio = options.logFile
    ? ["ignore", "ignore", "ignore"]
    : ["ignore", "ignore", "ignore"];

  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env ?? process.env,
    detached: true,
    stdio,
    shell: process.platform === "win32",
    windowsHide: true
  });
  child.unref();
  return child;
}
