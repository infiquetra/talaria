/**
 * Append-only recorder for gateway traffic.
 *
 * Writes JSON Lines: one self-describing JSON object per line, appended and
 * never rewritten. The format is deliberately plain and documented in
 * `docs/formats/frame-log.md` rather than being whatever a serializer happened
 * to emit, so a corpus recorded today can be replayed by a Talaria written in
 * any language later.
 *
 * Every frame passes through the redaction boundary before it is written. That
 * ordering is the point of this module and must not be relaxed: see
 * `./redact.ts`.
 */

import { createWriteStream, mkdirSync, type WriteStream } from "node:fs";
import { dirname } from "node:path";

import { type Redaction, redactFrame, redactUrl } from "./redact.js";

/** Bumped when the record shape changes in a way a reader must notice. */
export const FRAME_LOG_VERSION = 1;

/** Which way a frame travelled. */
export type Direction = "in" | "out";

/** The first line of every log: what this file is and what produced it. */
interface HeaderRecord {
  kind: "header";
  version: number;
  startedAt: string;
  /** Connection endpoint, with any credential in the query string withheld. */
  endpoint: string;
}

/** Every subsequent line: one observed frame. */
interface FrameRecord {
  kind: "frame";
  /** Monotonic index within this file, starting at 1. */
  seq: number;
  /** ISO-8601 timestamp of observation, not of sending. */
  at: string;
  dir: Direction;
  frame: unknown;
  /** Present only when something was withheld, so a reader sees the hole. */
  redactions?: Redaction[];
  /** Present when the payload could not be parsed as JSON. */
  parseError?: string;
}

export interface RecorderStats {
  frames: number;
  redactions: number;
  parseErrors: number;
}

/**
 * Records frames to a JSON Lines file.
 *
 * Construction opens the file and writes the header immediately, so an empty
 * recording is still self-describing and distinguishable from a missing one.
 */
export class FrameRecorder {
  private readonly stream: WriteStream;
  private seq = 0;
  private redactionCount = 0;
  private parseErrorCount = 0;

  constructor(
    private readonly path: string,
    endpoint: string,
  ) {
    mkdirSync(dirname(path), { recursive: true });
    this.stream = createWriteStream(path, { flags: "a" });

    const header: HeaderRecord = {
      kind: "header",
      version: FRAME_LOG_VERSION,
      startedAt: new Date().toISOString(),
      endpoint: redactUrl(endpoint),
    };
    this.writeLine(header);
  }

  /**
   * Record one frame as received from, or sent to, the wire.
   *
   * Takes the raw text rather than a parsed object so that a frame Talaria
   * cannot parse is still recorded -- an unparseable frame is exactly the kind
   * of protocol drift the corpus exists to catch.
   */
  record(dir: Direction, raw: string): void {
    this.seq += 1;
    const at = new Date().toISOString();

    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch (error) {
      this.parseErrorCount += 1;
      // The unparseable payload is withheld rather than stored: it could not be
      // walked by the redaction boundary, so it cannot be shown to be safe.
      this.writeLine({
        kind: "frame",
        seq: this.seq,
        at,
        dir,
        frame: null,
        parseError: error instanceof Error ? error.message : String(error),
      } satisfies FrameRecord);
      return;
    }

    const { frame, redactions } = redactFrame(parsed);
    this.redactionCount += redactions.length;

    const record: FrameRecord = {
      kind: "frame",
      seq: this.seq,
      at,
      dir,
      frame,
    };
    if (redactions.length > 0) record.redactions = redactions;
    this.writeLine(record);
  }

  stats(): RecorderStats {
    return {
      frames: this.seq,
      redactions: this.redactionCount,
      parseErrors: this.parseErrorCount,
    };
  }

  /** Flush and close. Safe to call more than once. */
  async close(): Promise<void> {
    await new Promise<void>((resolve) => this.stream.end(resolve));
  }

  /** Where this recording is being written. */
  get filePath(): string {
    return this.path;
  }

  private writeLine(record: HeaderRecord | FrameRecord): void {
    this.stream.write(`${JSON.stringify(record)}\n`);
  }
}
