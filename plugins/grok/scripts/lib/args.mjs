/**
 * Lightweight argv parser used by the Grok companion CLI.
 */

export function splitRawArgumentString(raw) {
  const text = String(raw ?? "").trim();
  if (!text) {
    return [];
  }

  const tokens = [];
  let current = "";
  let quote = null;

  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (quote) {
      if (ch === quote) {
        quote = null;
      } else if (ch === "\\" && i + 1 < text.length) {
        current += text[i + 1];
        i += 1;
      } else {
        current += ch;
      }
      continue;
    }

    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }

    if (/\s/.test(ch)) {
      if (current) {
        tokens.push(current);
        current = "";
      }
      continue;
    }

    current += ch;
  }

  if (current) {
    tokens.push(current);
  }
  return tokens;
}

/**
 * @param {string[]} argv
 * @param {{
 *   booleanOptions?: string[],
 *   valueOptions?: string[],
 *   aliasMap?: Record<string, string>
 * }} [config]
 */
export function parseArgs(argv, config = {}) {
  const booleanOptions = new Set(config.booleanOptions ?? []);
  const valueOptions = new Set(config.valueOptions ?? []);
  const aliasMap = config.aliasMap ?? {};

  /** @type {Record<string, string | boolean>} */
  const options = {};
  /** @type {string[]} */
  const positionals = [];

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === "--") {
      positionals.push(...argv.slice(i + 1));
      break;
    }

    if (!token.startsWith("-") || token === "-") {
      positionals.push(token);
      continue;
    }

    let key;
    let inlineValue;
    if (token.startsWith("--")) {
      const eq = token.indexOf("=");
      if (eq === -1) {
        key = token.slice(2);
      } else {
        key = token.slice(2, eq);
        inlineValue = token.slice(eq + 1);
      }
    } else {
      const short = token.slice(1);
      key = aliasMap[short] ?? short;
    }

    key = aliasMap[key] ?? key;

    if (booleanOptions.has(key)) {
      options[key] = true;
      continue;
    }

    if (valueOptions.has(key)) {
      if (inlineValue !== undefined) {
        options[key] = inlineValue;
        continue;
      }
      const next = argv[i + 1];
      if (next == null || next.startsWith("-")) {
        throw new Error(`Missing value for --${key}.`);
      }
      options[key] = next;
      i += 1;
      continue;
    }

    // Unknown flag: treat as boolean if no next value, else keep as positional-ish error soft-pass
    if (inlineValue !== undefined) {
      options[key] = inlineValue;
    } else if (argv[i + 1] != null && !argv[i + 1].startsWith("-")) {
      options[key] = argv[i + 1];
      i += 1;
    } else {
      options[key] = true;
    }
  }

  return { options, positionals };
}
