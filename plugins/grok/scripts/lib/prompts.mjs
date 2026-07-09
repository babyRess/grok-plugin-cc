import fs from "node:fs";
import path from "node:path";

/**
 * @param {string} pluginRoot
 * @param {string} name  // without .md
 */
export function loadPromptTemplate(pluginRoot, name) {
  const filePath = path.join(pluginRoot, "prompts", `${name}.md`);
  return fs.readFileSync(filePath, "utf8");
}

/**
 * Replace {{KEY}} placeholders in a template.
 * @param {string} template
 * @param {Record<string, string>} values
 */
export function interpolateTemplate(template, values) {
  return String(template).replace(/\{\{([A-Z0-9_]+)\}\}/g, (_, key) => {
    return values[key] ?? "";
  });
}
