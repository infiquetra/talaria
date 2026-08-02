/**
 * Redaction boundary for recorded gateway traffic.
 *
 * Every frame passes through here before it reaches disk. This is deliberately
 * the first module written in the recorder: the Hermes gateway carries
 * credentials in plaintext on ordinary client-to-server frames, and a recording
 * that captures them cannot be cleaned up afterwards once it is hash-chained.
 *
 * Verified against the Hermes gateway source (`tui_gateway/server.py`):
 *
 *     @method("sudo.respond")   -> _respond(rid, params, "password")
 *     @method("secret.respond") -> _respond(rid, params, "value")
 *
 * The request side is safe -- `sudo.request` is emitted with an empty payload --
 * so the exposure is entirely in the direction Talaria writes.
 *
 * Redactions are recorded rather than silently applied, so a reader of the
 * corpus sees that something was withheld instead of a clean-looking hole.
 */

/** A value withheld from the recording. The original never reaches disk. */
export const REDACTED = "[redacted]";

/** One withheld value, reported alongside the frame it was removed from. */
export interface Redaction {
  /** Dotted path to the removed value, e.g. `params.password`. */
  path: string;
  /** Why it was removed -- either the explicit rule or the key-name net. */
  reason: string;
}

/**
 * Explicit deny-set, keyed by JSON-RPC method name.
 *
 * Each entry lists the `params` keys whose values must never be written.
 * Derived by reading the gateway's `_respond` dispatch rather than by guessing
 * at names.
 */
const DENY_BY_METHOD: Record<string, readonly string[]> = {
  "sudo.respond": ["password"],
  "secret.respond": ["value"],
  // The serialized terminal buffer is arbitrary captured screen content and can
  // contain anything the operator's terminal was displaying, including output
  // from an unrelated program.
  "terminal.read.respond": ["text"],
};

/**
 * Defence in depth against protocol drift.
 *
 * The deny-set above is exact and will go stale the first time Hermes adds a
 * credential-bearing method. Any parameter whose *name* looks like a secret is
 * withheld regardless of which method carried it, so a new method leaks nothing
 * before anyone notices it exists.
 */
const SUSPICIOUS_KEY =
  /pass(word|phrase)|secret|token|credential|api[-_]?key|private[-_]?key|authorization/i;

/** True when a key name alone is reason enough to withhold its value. */
export function isSuspiciousKey(key: string): boolean {
  return SUSPICIOUS_KEY.test(key);
}

/** Strip credentials from a URL's query string so it is safe to record. */
export function redactUrl(url: string): string {
  try {
    const parsed = new URL(url);
    let changed = false;
    for (const key of [...parsed.searchParams.keys()]) {
      if (isSuspiciousKey(key)) {
        parsed.searchParams.set(key, REDACTED);
        changed = true;
      }
    }
    return changed ? parsed.toString() : url;
  } catch {
    // Not a parseable URL. Withhold it rather than record an unknown string that
    // may itself be a credential.
    return REDACTED;
  }
}

/** The result of passing one frame through the boundary. */
export interface RedactResult<T> {
  /** A copy safe to write. The input is never mutated. */
  frame: T;
  /** What was withheld, in the order encountered. Empty when nothing was. */
  redactions: Redaction[];
}

/**
 * Withhold every credential-bearing value in a decoded JSON-RPC frame.
 *
 * Walks the whole frame rather than only the known paths, so a credential
 * nested inside a batch or an unexpected envelope shape is still caught.
 */
export function redactFrame(frame: unknown): RedactResult<unknown> {
  const redactions: Redaction[] = [];
  const method = readMethod(frame);
  const denied = method ? (DENY_BY_METHOD[method] ?? []) : [];

  const walk = (value: unknown, path: string): unknown => {
    if (Array.isArray(value)) {
      return value.map((item, index) => walk(item, `${path}[${index}]`));
    }
    if (value === null || typeof value !== "object") {
      return value;
    }

    const out: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(
      value as Record<string, unknown>,
    )) {
      const childPath = path ? `${path}.${key}` : key;

      // An explicit rule for this method, matched at the `params` level.
      if (
        denied.includes(key) &&
        (path === "params" || path.startsWith("params["))
      ) {
        redactions.push({ path: childPath, reason: `deny-set:${method}` });
        out[key] = REDACTED;
        continue;
      }

      // The key-name net, applied at any depth.
      if (isSuspiciousKey(key)) {
        redactions.push({ path: childPath, reason: "suspicious-key" });
        out[key] = REDACTED;
        continue;
      }

      out[key] = walk(child, childPath);
    }
    return out;
  };

  return { frame: walk(frame, ""), redactions };
}

/** Read the JSON-RPC method name, tolerating any frame shape. */
function readMethod(frame: unknown): string | undefined {
  if (frame === null || typeof frame !== "object") return undefined;
  const method = (frame as { method?: unknown }).method;
  return typeof method === "string" ? method : undefined;
}
