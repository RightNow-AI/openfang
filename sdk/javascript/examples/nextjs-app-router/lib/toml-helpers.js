/**
 * lib/toml-helpers.js
 *
 * Helpers for reading and rewriting TOML manifest strings.
 * extractTomlField and extractTomlMultiline use a minimal character scanner
 * that correctly handles TOML escape sequences (including \" inside
 * triple-quoted strings) and the TOML 1.0 four/five-consecutive-quote edge
 * case.  patchTomlName rewrites a single line and remains regex-based.
 *
 * Scope: only the fields used by agent manifests (single-line strings and
 * triple-quoted multi-line strings).  Table arrays, inline tables, and
 * non-string value types are not needed and are not handled.
 */

/**
 * Extract a single-line quoted string field from a TOML string.
 * Returns '' if the field is absent or uses multi-line syntax.
 *
 * @param {string} toml
 * @param {string} field  - bare field name, e.g. 'name'
 * @returns {string}
 */
export function extractTomlField(toml, field) {
  if (!toml) return '';
  const m = toml.match(new RegExp(`^${field}\\s*=\\s*"([^"]*)"`, 'm'));
  return m ? m[1] : '';
}

/**
 * Extract a multi-line basic string (triple-quoted) from a TOML string,
 * or fall back to single-line extraction via extractTomlField.
 *
 * Uses a character scanner instead of regex so that:
 *   - escape sequences such as \" are processed correctly
 *   - multiline values containing literal \"\"\" (which is impossible in
 *     valid TOML but previously caused silent truncation) are handled safely
 *   - the TOML 1.0 §2.4.5 four/five-consecutive-quote rule is respected
 *
 * @param {string} toml
 * @param {string} field
 * @returns {string}
 */
export function extractTomlMultiline(toml, field) {
  if (!toml) return '';

  // Locate the field's triple-quote opening
  const re = new RegExp(`^${field}\\s*=\\s*"""`, 'm');
  const start = re.exec(toml);
  if (!start) return extractTomlField(toml, field);

  let i = start.index + start[0].length;

  // TOML spec: a newline immediately after the opening """ is trimmed
  if (toml[i] === '\r') i++;
  if (toml[i] === '\n') i++;

  let result = '';
  while (i < toml.length) {
    const ch = toml[i];

    if (ch === '\\') {
      // Process escape sequence
      const esc = toml[i + 1];
      if (esc === '"')  { result += '"';  i += 2; continue; }
      if (esc === '\\') { result += '\\'; i += 2; continue; }
      if (esc === 'n')  { result += '\n'; i += 2; continue; }
      if (esc === 't')  { result += '\t'; i += 2; continue; }
      if (esc === 'r')  { result += '\r'; i += 2; continue; }
      if (esc === '\n' || (esc === '\r' && toml[i + 2] === '\n')) {
        // Line-ending backslash: trim all following whitespace
        i += esc === '\r' ? 3 : 2;
        while (i < toml.length && ' \t\r\n'.includes(toml[i])) i++;
        continue;
      }
      result += ch; i++; continue;
    }

    // Check for closing delimiter or TOML 1.0 four/five-quote sequences
    if (ch === '"' && toml[i + 1] === '"' && toml[i + 2] === '"') {
      if (toml[i + 3] === '"') {
        if (toml[i + 4] === '"') {
          // Five consecutive quotes: "" is content, """ closes
          result += '""';
          i += 5;
        } else {
          // Four consecutive quotes: " is content, """ closes
          result += '"';
          i += 4;
        }
      } else {
        // Standard closing """
        i += 3;
      }
      break;
    }

    result += ch;
    i++;
  }

  return result.trim();
}

/**
 * Return a copy of `toml` with the first `name = "..."` line replaced.
 * Escapes backslashes and double-quotes in `newName`.
 *
 * @param {string} toml
 * @param {string} newName
 * @returns {string}
 */
export function patchTomlName(toml, newName) {
  const safe = newName.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  return toml.replace(/^name\s*=\s*"[^"]*"/m, `name = "${safe}"`);
}
