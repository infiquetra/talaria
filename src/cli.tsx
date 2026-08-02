#!/usr/bin/env node

import { render } from "ink";

import { App } from "./app.js";
import { defaultLogPath, runRecord } from "./record/command.js";

const [command, ...rest] = process.argv.slice(2);

if (command === "record") {
  const url = rest[0];

  if (!url) {
    process.stderr.write(
      [
        `usage: talaria record <gateway-websocket-url> [--out <path>]`,
        ``,
        `Attaches to a running Hermes gateway and records every protocol frame`,
        `to a JSON Lines file. Credentials are withheld before anything is`,
        `written; see src/record/redact.ts.`,
        ``,
      ].join("\n"),
    );
    process.exit(2);
  }

  const outFlag = rest.indexOf("--out");
  const out = outFlag >= 0 ? rest[outFlag + 1] : undefined;

  if (outFlag >= 0 && !out) {
    process.stderr.write(`--out needs a path\n`);
    process.exit(2);
  }

  const controller = new AbortController();
  process.on("SIGINT", () => controller.abort());

  const code = await runRecord({
    url,
    out: out ?? defaultLogPath(),
    signal: controller.signal,
  });
  process.exit(code);
} else {
  render(<App />);
}
