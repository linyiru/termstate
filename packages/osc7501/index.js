// OSC 7501, the Program Status Protocol (spec revision 0.4).
// https://www.superlogical.com/rex/docs/build/program-status
//
// A port of termstate-core (Rust). Both are checked against the same
// conformance vectors in spec-tests/; see docs/interpretations.md for the
// places where the spec leaves room.

export const SPEC_REVISION = "0.4";

export const LIMITS = Object.freeze({
  sequence: 4096,
  key: 16,
  msgEncoded: 2732,
  msgDecoded: 2048,
  titleEncoded: 256,
  titleDecoded: 192,
  app: 32,
  id: 128,
  idSegment: 32,
  idDepth: 8,
  records: 256,
  minRecords: 64,
});

export const STATES = Object.freeze(["idle", "working", "done", "blocked", "error"]);
export const KINDS = Object.freeze(["permission", "question", "auth"]);

const PREFIX = "\x1b]7501;";
const MAX_BODY = LIMITS.sequence - PREFIX.length - 1;
const TERMINATORS = { st: "\x1b\\", bel: "\x07" };

const NAME_RE = /^[A-Za-z0-9_.+-]+$/;
const VALUE_RE = /^[A-Za-z0-9_.,+/=-]*$/;
const KEY_RE = /^[a-z]+$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f-\u009f]/;

/** Error thrown by the encoder for input a terminal would discard. */
export class Osc7501Error extends Error {
  constructor(code, message) {
    super(message);
    this.name = "Osc7501Error";
    this.code = code;
  }
}

// ---------------------------------------------------------------- base64

const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const B64_VALUE = new Map([...B64].map((c, i) => [c, i]));

function b64Encode(bytes) {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const n = bytes.length - i;
    const acc = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    for (let j = 0; j < 4; j++) {
      out += j <= Math.min(n, 3) ? B64[(acc >> (18 - 6 * j)) & 63] : "=";
    }
  }
  return out;
}

/** Strict standard base64, padding optional, canonical trailing bits. */
function b64Decode(s) {
  const pad = s.length - s.replace(/=+$/, "").length;
  if (pad > 2) return null;
  const data = s.slice(0, s.length - pad);
  if (pad > 0 && s.length % 4 !== 0) return null;
  if (data.length % 4 === 1) return null;
  const out = [];
  for (let i = 0; i < data.length; i += 4) {
    const chunk = data.slice(i, i + 4);
    let acc = 0;
    for (let j = 0; j < chunk.length; j++) {
      const v = B64_VALUE.get(chunk[j]);
      if (v === undefined) return null;
      acc |= v << (18 - 6 * j);
    }
    const unused = chunk.length === 2 ? 0xffff : chunk.length === 3 ? 0xff : 0;
    if (acc & unused) return null;
    const bytes = [(acc >> 16) & 255, (acc >> 8) & 255, acc & 255];
    out.push(...bytes.slice(0, chunk.length - 1));
  }
  return Uint8Array.from(out);
}

const utf8 = new TextEncoder();
const utf8Strict = new TextDecoder("utf-8", { fatal: true });

// ---------------------------------------------------------------- parsing

function isSpace(c) {
  return c === " " || c === "\t";
}

function trim(s) {
  let a = 0;
  let b = s.length;
  while (a < b && isSpace(s[a])) a++;
  while (b > a && isSpace(s[b - 1])) b--;
  return s.slice(a, b);
}

/** Bytes as a string of code units 0..255, so lengths count bytes. */
function toByteString(body) {
  const bytes = typeof body === "string" ? utf8.encode(body) : body;
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return s;
}

function validId(v) {
  if (v.length === 0 || v.length > LIMITS.id) return false;
  const segs = v.split("/");
  return (
    segs.length <= LIMITS.idDepth &&
    segs.every((s) => s.length > 0 && s.length <= LIMITS.idSegment && NAME_RE.test(s))
  );
}

const RAW_LIMITS = {
  app: LIMITS.app,
  title: LIMITS.titleEncoded,
  msg: LIMITS.msgEncoded,
  id: LIMITS.id,
};

function decodeText(v, maxEncoded, maxDecoded) {
  if (v === undefined || v === "") return { ok: true, text: undefined };
  if (v.length > maxEncoded) return { ok: false, reason: "too-long" };
  const bytes = b64Decode(v);
  if (!bytes) return { ok: false, reason: "invalid-base64" };
  if (bytes.length > maxDecoded) return { ok: false, reason: "too-long" };
  let text;
  try {
    text = utf8Strict.decode(bytes);
  } catch {
    return { ok: false, reason: "invalid-utf8" };
  }
  if (CONTROL_RE.test(text)) return { ok: false, reason: "control-character" };
  return { ok: true, text };
}

function parseProgress(v) {
  if (v === undefined || !/^[0-9]+$/.test(v)) return undefined;
  const digits = v.replace(/^0+/, "");
  if (digits.length > 3) return undefined;
  const n = Number(digits || "0");
  return n <= 100 ? n : undefined;
}

const discard = (reason) => ({ type: "discard", reason });

/**
 * Parses the body of one OSC 7501 sequence (the bytes after `ESC ] 7501 ;`
 * and before the terminator), following the terminal-side rules.
 *
 * Returns `{type: "report", state, id?, kind?, progress?, app?, title?, msg?}`,
 * `{type: "clear", id?}`, `{type: "feature", states, kinds}`, or
 * `{type: "discard", reason}` when nothing from the report may be applied.
 */
export function parseBody(body) {
  const s = toByteString(body);
  if (s.length > MAX_BODY) return discard("sequence-too-long");

  const parts = s.split(":");
  const feature = trim(parts[0]) === "?";
  const fields = {};
  const caps = { states: [...STATES], kinds: [...KINDS] };

  for (const part of feature ? parts.slice(1) : parts) {
    const eq = part.indexOf("=");
    if (eq < 0) continue;
    const key = trim(part.slice(0, eq));
    const value = trim(part.slice(eq + 1));
    if (key.length > LIMITS.key) return discard("key-too-long");
    if (!KEY_RE.test(key)) continue;
    const goodValue = VALUE_RE.test(value);
    if (feature) {
      if (!goodValue) continue;
      const items = value.split(",").map(trim).filter(Boolean);
      if (key === "states") caps.states = STATES.filter((x) => items.includes(x));
      if (key === "kinds") caps.kinds = KINDS.filter((x) => items.includes(x));
      continue;
    }
    if (!goodValue) {
      if (key === "id") return discard("invalid-id");
      if (key in RAW_LIMITS && value.length > RAW_LIMITS[key]) return discard("too-long");
      continue;
    }
    if (key in RAW_LIMITS && value.length > RAW_LIMITS[key]) return discard("too-long");
    if (key === "id" && !validId(value)) return discard("invalid-id");
    fields[key] = value;
  }

  if (feature) return { type: "feature", ...caps };

  const title = decodeText(fields.title, LIMITS.titleEncoded, LIMITS.titleDecoded);
  if (!title.ok) return discard(title.reason);
  const msg = decodeText(fields.msg, LIMITS.msgEncoded, LIMITS.msgDecoded);
  if (!msg.ok) return discard(msg.reason);

  const state = fields.state;
  if (state === undefined) return discard("missing-state");
  if (state === "clear") return fields.id === undefined ? { type: "clear" } : { type: "clear", id: fields.id };
  if (!STATES.includes(state)) return discard("unknown-state");

  const out = { type: "report", state };
  if (fields.id !== undefined) out.id = fields.id;
  if (state === "blocked" && KINDS.includes(fields.kind)) out.kind = fields.kind;
  if (state === "working" || state === "blocked") {
    const p = parseProgress(fields.progress);
    if (p !== undefined) out.progress = p;
  }
  if (fields.app !== undefined && NAME_RE.test(fields.app)) out.app = fields.app;
  if (title.text !== undefined) out.title = title.text;
  if (msg.text !== undefined) out.msg = msg.text;
  return out;
}

// ---------------------------------------------------------------- encoding

function checkText(text, field, max) {
  const bytes = utf8.encode(text);
  if (bytes.length > max) throw new Osc7501Error("too-long", `${field} exceeds ${max} bytes`);
  if (CONTROL_RE.test(text)) {
    throw new Osc7501Error("control-character", `${field} contains a control character`);
  }
  return bytes;
}

/**
 * Encodes a report as a full escape sequence. Throws `Osc7501Error` for
 * anything a conforming terminal would discard. Keys the state ignores
 * (`kind` unless blocked, `progress` unless working or blocked) and empty
 * `title`/`msg` are dropped.
 *
 * @param {{state: string, id?: string, kind?: string, progress?: number,
 *          app?: string, title?: string, msg?: string}} report
 * @param {{terminator?: "st" | "bel"}} [options]
 */
export function encode(report, { terminator = "st" } = {}) {
  const { state } = report;
  if (!STATES.includes(state)) throw new Osc7501Error("unknown-state", `unknown state ${state}`);
  let out = `${PREFIX}state=${state}`;
  if (report.id !== undefined) {
    if (!validId(report.id)) throw new Osc7501Error("invalid-id", `invalid id ${report.id}`);
    out += `:id=${report.id}`;
  }
  if (report.kind !== undefined && state === "blocked") {
    if (!KINDS.includes(report.kind)) throw new Osc7501Error("unknown-kind", `unknown kind ${report.kind}`);
    out += `:kind=${report.kind}`;
  }
  if (report.progress !== undefined) {
    const p = report.progress;
    if (!Number.isInteger(p) || p < 0 || p > 100) {
      throw new Osc7501Error("invalid-progress", "progress must be an integer 0 to 100");
    }
    if (state === "working" || state === "blocked") out += `:progress=${p}`;
  }
  if (report.app !== undefined) {
    if (report.app.length > LIMITS.app || !NAME_RE.test(report.app)) {
      throw new Osc7501Error("invalid-app", "app must match [A-Za-z0-9_.+-]{1,32}");
    }
    out += `:app=${report.app}`;
  }
  if (report.title) out += `:title=${b64Encode(checkText(report.title, "title", LIMITS.titleDecoded))}`;
  if (report.msg) out += `:msg=${b64Encode(checkText(report.msg, "msg", LIMITS.msgDecoded))}`;
  out += terminatorBytes(terminator);
  if (out.length > LIMITS.sequence) throw new Osc7501Error("sequence-too-long", "sequence exceeds 4096 bytes");
  return out;
}

/** `state=clear` for `id` and everything beneath it, or every record. */
export function encodeClear(id, { terminator = "st" } = {}) {
  if (id !== undefined && !validId(id)) throw new Osc7501Error("invalid-id", `invalid id ${id}`);
  return `${PREFIX}state=clear${id === undefined ? "" : `:id=${id}`}${terminatorBytes(terminator)}`;
}

/** The feature-detection query, followed by `CSI c` as the spec recommends. */
export function query({ terminator = "st" } = {}) {
  return `${PREFIX}?${terminatorBytes(terminator)}\x1b[c`;
}

function terminatorBytes(t) {
  const s = TERMINATORS[t];
  if (!s) throw new Osc7501Error("invalid-terminator", `terminator must be "st" or "bel"`);
  return s;
}

/**
 * Makes arbitrary text safe for `msg` or `title`: control characters become
 * spaces, and text longer than `maxBytes` is cut at a character boundary and
 * ends with `…`.
 */
export function sanitize(text, maxBytes = LIMITS.msgDecoded) {
  const clean = String(text).replace(new RegExp(CONTROL_RE.source, "g"), " ");
  if (utf8.encode(clean).length <= maxBytes) return clean;
  const budget = maxBytes - 3;
  let out = "";
  let used = 0;
  for (const ch of clean) {
    const n = utf8.encode(ch).length;
    if (used + n > budget) break;
    out += ch;
    used += n;
  }
  return maxBytes >= 3 ? `${out}…` : out;
}
