// Offline replay: recorded frames -> decode -> state machine -> render, with no socket and no TTY.
import { readFileSync } from "node:fs"
import { decodeFrame, parseRecording } from "./protocol"
import { initialState, reduce, startTurn, answerApproval, type State } from "./state"
import { plain, renderBlock, renderLive, type Block } from "./render"

export interface ReplayResult {
  state: State
  blocks: Block[]
  scrollback: string[] // plain text of everything committed
  phases: string[]
  approvalsSeen: number
  footer: string[]
}

export function replayText(text: string, cols = 80, opts: { autoAnswer?: string } = {}): ReplayResult {
  const frames = parseRecording(text)
  let st = initialState(cols)
  const blocks: Block[] = []
  const scrollback: string[] = []
  const phases: string[] = []
  let approvals = 0
  for (const f of frames) {
    if (f.dir === "out") {
      // Our own prompt.submit is the only client-originated thing the screen shows.
      const m = JSON.parse(f.raw)
      if (m.method === "prompt.submit" && typeof m.params?.text === "string") {
        const step = startTurn(st, m.params.text, f.t)
        st = step.state
        if (phases[phases.length - 1] !== st.phase) phases.push(st.phase)
        for (const b of step.blocks) {
          blocks.push(b)
          scrollback.push(...plain(renderBlock(b, cols)))
        }
      }
      continue
    }
    const fr = decodeFrame(f.raw)
    const step = reduce(st, fr, f.t)
    st = step.state
    if (fr.kind === "event" && fr.type === "approval.request") {
      approvals++
      if (opts.autoAnswer && st.approval) {
        const i = st.approval.choices.indexOf(opts.autoAnswer)
        if (i >= 0) st = answerApproval(st, i).state
      }
    }
    if (phases[phases.length - 1] !== st.phase) phases.push(st.phase)
    for (const b of step.blocks) {
      blocks.push(b)
      scrollback.push(...plain(renderBlock(b, cols)))
    }
  }
  const lastT = frames.length ? frames[frames.length - 1]!.t : 0
  return { state: st, blocks, scrollback, phases, approvalsSeen: approvals, footer: plain(renderLive(st, cols, lastT)) }
}

export function replayFile(path: string, cols = 80, opts: { autoAnswer?: string } = {}): ReplayResult {
  return replayText(readFileSync(path, "utf8"), cols, opts)
}
