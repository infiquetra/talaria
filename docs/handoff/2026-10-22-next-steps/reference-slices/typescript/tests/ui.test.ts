import { expect, test } from "bun:test"
import { createTestRenderer } from "@opentui/core/testing"
import { uiOnRenderer } from "../src/ui"
import { renderBlock, renderLive } from "../src/render"
import { initialState, startTurn, reduce } from "../src/state"
import { decodeFrame } from "../src/protocol"

const ev = (type: string, payload: object) =>
  decodeFrame(JSON.stringify({ jsonrpc: "2.0", method: "event", params: { type, session_id: "s", execution_generation: 1, payload } }))

test("OpenTUI split-footer: finished blocks go to scrollback, the footer holds the live region", async () => {
  const t = await createTestRenderer({ width: 60, height: 12, screenMode: "split-footer", footerHeight: 3, externalOutputMode: "capture-stdout" })
  const ui = uiOnRenderer(t.renderer)
  let s = initialState(60, "minimax-m3.1-flash")
  s = { ...s, phase: "idle" }
  const step = startTurn(s, "hello there", 0)
  for (const b of step.blocks) ui.commit(renderBlock(b, 60))
  s = step.state
  s = reduce(s, ev("tool.start", { tool_id: "t", name: "terminal", context: "ls -la" }), 100).state
  s = reduce(s, ev("message.delta", { text: "partial answer" }), 200).state
  ui.setFooter(renderLive(s, 60, 900))
  await t.flush()
  const scrollback = t.externalOutput.takeText()
  expect(scrollback).toContain("› hello there")
  const frame = t.captureCharFrame()
  expect(frame).toContain("partial answer")
  expect(frame).toContain("minimax-m3.1-flash · streaming")
  expect(frame).not.toContain("hello there") // already in scrollback, not redrawn
  expect(frame.split("\n").filter((l) => l.trim()).map((l) => l.trimEnd()).join("\n")).toMatchSnapshot()
  await t.dispose()
})
