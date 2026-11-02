export declare const SPEC_REVISION: "0.4";
export declare const LIMITS: Readonly<{
  sequence: 4096; key: 16; msgEncoded: 2732; msgDecoded: 2048; titleEncoded: 256;
  titleDecoded: 192; app: 32; id: 128; idSegment: 32; idDepth: 8; records: 256; minRecords: 64;
}>;

export type State = "idle" | "working" | "done" | "blocked" | "error";
export type Kind = "permission" | "question" | "auth";
export type Terminator = "st" | "bel";

export declare const STATES: readonly State[];
export declare const KINDS: readonly Kind[];

export interface Report {
  state: State;
  /** Record id, `segment("/" segment)*`; absent addresses the root record. */
  id?: string;
  /** Only meaningful with `blocked`. */
  kind?: Kind;
  /** 0..=100, only with `working` or `blocked`; absent is indeterminate. */
  progress?: number;
  /** Stable program identifier, `[A-Za-z0-9_.+-]{1,32}`. */
  app?: string;
  /** Short label, at most 192 UTF-8 bytes. */
  title?: string;
  /** One line of text, at most 2048 UTF-8 bytes. */
  msg?: string;
}

export type Message =
  | ({ type: "report" } & Report)
  | { type: "clear"; id?: string }
  | { type: "feature"; states: State[]; kinds: Kind[] }
  | { type: "discard"; reason: string };

export declare class Osc7501Error extends Error {
  readonly code: string;
}

/** Parses the bytes between `ESC ] 7501 ;` and the terminator. */
export declare function parseBody(body: string | Uint8Array): Message;
/** Encodes a report; throws `Osc7501Error` for anything a terminal would discard. */
export declare function encode(report: Report, options?: { terminator?: Terminator }): string;
export declare function encodeClear(id?: string, options?: { terminator?: Terminator }): string;
/** The feature-detection query followed by `CSI c`. */
export declare function query(options?: { terminator?: Terminator }): string;
/** Replaces control characters with spaces and truncates to `maxBytes` with `…`. */
export declare function sanitize(text: string, maxBytes?: number): string;
