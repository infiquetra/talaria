// Feeds a JSON array of raw receive-only frame strings through the
// TypeScript reference recorder (src/record/recorder.ts) and prints the
// resulting frame-log v1 file's parsed JSON lines as a JSON array on stdout.
//
// Invoked as a subprocess by tests/recorder/test_equivalence.py (AE6, R28)
// via `node_modules/.bin/tsx run_ts_recorder.mjs`, so the equivalence
// harness runs the real reference implementation rather than a
// reimplementation of it.
//
// stdin:  {"endpoint": "<url>", "frames": ["<raw1>", "<raw2>", ...]}
// stdout: [{...header record...}, {...frame record...}, ...]

import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { FrameRecorder } from "../../../src/record/recorder.js";

async function main() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  const input = JSON.parse(Buffer.concat(chunks).toString("utf8"));

  const dir = mkdtempSync(join(tmpdir(), "talaria-ts-equiv-"));
  const outPath = join(dir, "out.jsonl");
  const recorder = new FrameRecorder(outPath, input.endpoint);

  for (const raw of input.frames) {
    recorder.record("in", raw);
  }
  await recorder.close();

  const text = readFileSync(outPath, "utf8");
  const lines = text
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line));
  process.stdout.write(JSON.stringify(lines));

  rmSync(dir, { recursive: true, force: true });
}

main().catch((err) => {
  process.stderr.write(String(err?.stack ?? err));
  process.exit(1);
});
