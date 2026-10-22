import { expect, test } from "bun:test"
import { plain, renderBlock, renderLive } from "../src/render"
import { initialState, startTurn, reduce } from "../src/state"
import { decodeFrame } from "../src/protocol"

const ev = (type: string, payload: object, extra: object = {}) =>
  decodeFrame(JSON.stringify({ jsonrpc: "2.0", method: "event", params: { type, session_id: "s1", payload, execution_generation: 1, ...extra } }))

test("snapshot: tool cell, diff and assistant block (60 cols)", () => {
  const lines = [
    ...renderBlock({ kind: "user", text: "fix the bug" }, 60),
    ...renderBlock({ kind: "tool", name: "terminal", context: "ls -la", ok: true, durationS: 1.2 }, 60),
    ...renderBlock({ kind: "tool", name: "patch", context: "calc.py", ok: true, durationS: 0.1, diff: "--- a/calc.py\n+++ b/calc.py\n@@ -1,2 +1,2 @@\n-    return a - b\n+    return a + b" }, 60),
    ...renderBlock({ kind: "assistant", lines: ["Fixed the sign in add()."], first: true }, 60),
  ]
  expect(plain(lines).join("\n")).toMatchSnapshot()
})

test("snapshot: live region while waiting for approval (60 cols)", () => {
  let s = initialState(60, "minimax-m3.1-flash")
  s = startTurn(s, "rm", 0).state
  s = reduce(s, ev("approval.request", { prompt_id: "p", command: "rm -rf .pytest_cache", description: "recursive delete", choices: ["once", "deny", "session", "always"] }), 2000).state
  expect(plain(renderLive(s, 60, 2400)).join("\n")).toMatchSnapshot()
})

test("diff lines carry add and delete colours", () => {
  const lines = renderBlock({ kind: "tool", name: "patch", context: "x", ok: true, diff: "-a\n+b" }, 40)
  const spans = lines.flat()
  expect(spans.find((s) => s.text === "+b")?.style?.fg).toBe("#5fd75f")
  expect(spans.find((s) => s.text === "-a")?.style?.fg).toBe("#ff5f5f")
})

test("failed tool shows a cross", () => {
  expect(plain(renderBlock({ kind: "tool", name: "terminal", context: "false", ok: false, durationS: 0.2 }, 60))[0]).toBe("⏺ terminal: false ✗ 0.2s")
})

test("input line shows the cursor and the status line shows model and state", () => {
  let s = initialState(50, "m1")
  s = { ...s, phase: "idle", input: "hi" }
  const out = plain(renderLive(s, 50, 0))
  expect(out[0]).toBe("● m1 · ready")
  expect(out[1]).toBe("› hi▌")
})
