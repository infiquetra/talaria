/**
 * `talaria record <url>` -- attach to a Hermes gateway and record every frame.
 *
 * This is the instrument, not the product. A recorded corpus is language- and
 * renderer-neutral, so it settles questions that would otherwise be argued:
 * which events actually arrive and in what order, whether a renderer holds up
 * under a real session replayed at speed, and whether a Hermes upgrade changed
 * the protocol.
 */

import { FrameRecorder } from "./recorder.js";
import { redactUrl } from "./redact.js";
import { attach } from "../transport/attach.js";

export interface RecordCommandOptions {
  url: string;
  out: string;
  /** Where progress and the summary are written. */
  log?: (line: string) => void;
  signal?: AbortSignal;
}

/** Default log path. Timestamped so successive runs never overwrite a corpus. */
export function defaultLogPath(now: Date = new Date()): string {
  const stamp = now.toISOString().replace(/[:.]/g, "-");
  return `.talaria/frames/${stamp}.jsonl`;
}

/**
 * Run the recorder to completion. Resolves with a process exit code: 0 when the
 * socket closed normally, 1 when it never attached.
 */
export async function runRecord(
  options: RecordCommandOptions,
): Promise<number> {
  const log =
    options.log ?? ((line: string) => process.stdout.write(`${line}\n`));
  const recorder = new FrameRecorder(options.out, options.url);

  log(`talaria record`);
  log(`  endpoint  ${redactUrl(options.url)}`);
  log(`  writing   ${recorder.filePath}`);
  log(``);

  const outcome = await attach({
    url: options.url,
    recorder,
    signal: options.signal,
    onOpen: () => log(`attached. recording frames, Ctrl-C to stop.`),
    onFrame: (count) => {
      // Rewrite one line rather than scrolling, so a long recording stays quiet.
      if (process.stdout.isTTY) process.stdout.write(`\r  frames: ${count}`);
    },
  });

  if (process.stdout.isTTY) process.stdout.write(`\n`);
  await recorder.close();

  const stats = recorder.stats();
  log(``);
  log(`  frames recorded   ${stats.frames}`);
  log(`  values withheld   ${stats.redactions}`);
  if (stats.parseErrors > 0) log(`  unparseable       ${stats.parseErrors}`);
  log(`  corpus            ${recorder.filePath}`);

  if (outcome.status === "failed") {
    log(``);
    log(`could not attach: ${outcome.error}`);
    log(``);
    log(`Talaria dials a gateway it did not launch, so one must be running.`);
    log(`Start the Hermes dashboard, then pass its websocket url:`);
    log(`  talaria record 'ws://127.0.0.1:<port>/api/ws?token=<token>'`);
    return 1;
  }

  log(``);
  log(`socket closed (${outcome.code}): ${outcome.reason}`);
  return 0;
}
