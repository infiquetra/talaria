import { expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { replayText } from "../src/replay"
import { decodeFrame } from "../src/protocol"
import { initialState, reduce, answerApproval, moveApproval, startTurn } from "../src/state"

const fx = (n: string) => readFileSync(new URL(`./fixtures/${n}.jsonl`, import.meta.url), "utf8")
const ev = (type: string, payload: object, extra: object = {}) =>
  decodeFrame(JSON.stringify({ jsonrpc: "2.0", method: "event", params: { type, session_id: "s1", payload, execution_generation: 1, ...extra } }))

test("replay of a read-only tool turn: tool cell then assistant text then done", () => {
  const r = replayText(fx("turn-tool-readonly"), 80)
  expect(r.phases).toContain("tool")
  expect(r.phases).toContain("streaming")
  expect(r.phases[r.phases.length - 1]).toBe("done")
  const text = r.scrollback.join("\n")
  expect(text).toMatch(/⏺ search_files: .* ✓/)
  expect(text).toContain("The directory contains two Python files")
  expect(text.indexOf("search_files")).toBeLessThan(text.indexOf("The directory contains"))
})

test("replay of the approval turn: waits for approval, settles, completes", () => {
  const r = replayText(fx("turn-approval"), 80, { autoAnswer: "once" })
  expect(r.approvalsSeen).toBe(1)
  expect(r.phases).toContain("waiting-approval")
  expect(r.phases[r.phases.length - 1]).toBe("done")
  const text = r.scrollback.join("\n")
  expect(text).toContain("approval: rm -rf .pytest_cache → once")
  expect(text).toMatch(/⏺ terminal: rm -rf \.pytest_cache ✓ 4\.1s/)
  expect(text).toContain("Done")
})

test("full synthetic turn: start, deltas, tool, approval, complete", () => {
  let s = initialState(60, "m")
  const feed = (f: ReturnType<typeof ev>, t: number) => {
    const r = reduce(s, f, t)
    s = r.state
    return r.blocks
  }
  feed(ev("gateway.ready", {}), 0)
  expect(s.phase).toBe("connecting") // idle only once the session exists
  s = { ...s, phase: "idle" }
  s = startTurn(s, "go", 10).state
  expect(s.phase).toBe("thinking")
  feed(ev("message.start", {}), 20)
  const b1 = feed(ev("message.delta", { text: "Hello\nwor" }), 30)
  expect(b1).toEqual([{ kind: "assistant", lines: ["Hello"], first: true }])
  expect(s.tail).toBe("wor")
  expect(s.phase).toBe("streaming")
  const b2 = feed(ev("message.delta", { text: "ld" }), 40)
  expect(b2).toEqual([])
  const b3 = feed(ev("message.delta", { text: null }), 50) // segment boundary flushes the tail
  expect(b3).toEqual([{ kind: "assistant", lines: ["world"], first: false }])
  feed(ev("tool.start", { tool_id: "t1", name: "terminal", context: "ls -la" }), 60)
  expect(s.phase).toBe("tool")
  expect(s.tools).toHaveLength(1)
  feed(ev("approval.request", { prompt_id: "p1", command: "ls -la", choices: ["once", "deny"] }), 70)
  expect(s.phase).toBe("waiting-approval")
  s = moveApproval(s, 1)
  expect(s.approval!.selected).toBe(1)
  const ans = answerApproval(s, 0)
  expect(ans.choice).toBe("once")
  s = ans.state
  const b4 = feed(ev("approval.settled", { prompt_id: "p1" }), 80)
  expect(b4[0]).toMatchObject({ kind: "approval", choice: "once" })
  const b5 = feed(ev("tool.complete", { tool_id: "t1", name: "terminal", context: "ls -la", duration_s: 1.2, is_error: false }), 90)
  expect(b5[0]).toMatchObject({ kind: "tool", ok: true, durationS: 1.2 })
  const b6 = feed(ev("message.complete", { text: "Hello\nworld", status: "complete" }), 100)
  expect(b6.map((b) => b.kind)).toEqual(["turn"]) // text already streamed, not duplicated
  expect(s.phase).toBe("done")
})

test("message.complete text is used when no deltas streamed", () => {
  let s = initialState(60)
  s = startTurn(s, "x", 0).state
  const r = reduce(s, ev("message.complete", { text: "only final", status: "complete" }), 5)
  expect(r.blocks[0]).toEqual({ kind: "assistant", lines: ["only final"], first: true })
})

test("errors surface as error blocks and phase", () => {
  let s = initialState(60)
  s = startTurn(s, "x", 0).state
  const r = reduce(s, ev("message.complete", { status: "failed", error: "boom" }), 5)
  expect(r.blocks[0]).toEqual({ kind: "error", text: "boom" })
  expect(r.state.phase).toBe("error")
})

test("a long unbroken paragraph spills stable rows into scrollback", () => {
  let s = initialState(30)
  s = startTurn(s, "x", 0).state
  const words = Array.from({ length: 40 }, (_, i) => `word${i}`).join(" ")
  const r = reduce(s, ev("message.delta", { text: words }), 1)
  expect(r.blocks.length).toBe(1)
  expect(r.state.tail.length).toBeGreaterThan(0)
  expect(r.state.tail.length).toBeLessThan(words.length)
})

test("replay of the full live two-turn recording (read-only tool turn, then approval turn)", () => {
  const r = replayText(fx("live-two-turns"), 100, { autoAnswer: "once" })
  const text = r.scrollback.join("\n")
  expect(text).toContain("› What files are in this directory?")
  expect(text).toContain("› Run exactly this command: rm -rf .pytest_cache")
  expect(text).toContain("→ once")
  expect(r.approvalsSeen).toBe(1)
  expect(r.scrollback.filter((l) => /done in/.test(l))).toHaveLength(2)
  expect(r.phases[r.phases.length - 1]).toBe("done")
})
