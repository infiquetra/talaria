import { expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { decodeFrame, parseRecording } from "../src/protocol"

const fx = (n: string) => readFileSync(new URL(`./fixtures/${n}.jsonl`, import.meta.url), "utf8")

test("decodes every recorded frame without invalid results", () => {
  for (const name of ["turn-tool-readonly", "turn-approval"]) {
    const frames = parseRecording(fx(name)).filter((f) => f.dir === "in")
    expect(frames.length).toBeGreaterThan(10)
    for (const f of frames) expect(decodeFrame(f.raw).kind).not.toBe("invalid")
  }
})

test("event frames expose type, session, seq and generation", () => {
  const frames = parseRecording(fx("turn-approval")).filter((f) => f.dir === "in").map((f) => decodeFrame(f.raw))
  const ready = frames[0]!
  expect(ready.kind).toBe("event")
  if (ready.kind === "event") expect(ready.type).toBe("gateway.ready")
  const req = frames.find((f) => f.kind === "event" && f.type === "approval.request")
  expect(req).toBeDefined()
  if (req?.kind === "event") {
    expect(req.payload.command).toBe("rm -rf .pytest_cache")
    expect(req.payload.choices).toEqual(["once", "deny", "session", "always"])
    expect(req.executionGeneration).toBe(1)
    expect(typeof req.seq).toBe("number")
  }
})

test("responses are decoded with id and result", () => {
  const f = decodeFrame('{"jsonrpc":"2.0","id":101,"result":{"ok":true}}')
  expect(f).toEqual({ kind: "response", id: 101, result: { ok: true }, error: undefined })
  const e = decodeFrame('{"jsonrpc":"2.0","id":5,"error":{"code":4001,"message":"invalid_params"}}')
  expect(e.kind === "response" && e.error?.code).toBe(4001)
})

test("garbage is reported, not thrown", () => {
  expect(decodeFrame("{nope").kind).toBe("invalid")
  expect(decodeFrame("[1]").kind).toBe("invalid")
})
