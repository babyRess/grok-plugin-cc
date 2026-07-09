import fs from "node:fs";
import path from "node:path";

/**
 * Walk up from cwd to find the nearest git root, else return cwd.
 * @param {string} cwd
 */
export function resolveWorkspaceRoot(cwd) {
  let current = path.resolve(cwd || process.cwd());
  while (true) {
    if (fs.existsSync(path.join(current, ".git"))) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return path.resolve(cwd || process.cwd());
    }
    current = parent;
  }
}
