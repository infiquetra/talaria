// Pure state machine for one session: frames in, (new state, blocks to commit to scrollback) out.
// No I/O and no terminal framework. Time is passed in so replay is deterministic.
import type { Frame, GatewayEvent } from "./protocol"
import { wrapOffsets } from "./text"
import { MAX_TAIL_ROWS, type Block } from "./render"

export type Phase =
  | "connecting"
  | "idle"
  | "thinking"
  | "streaming"
  | "tool"
  | "waiting-approval"
  | "done"
  | "interrupted"
  | "error"

export interface ToolRun {
  id: string
  name: string
  context: string
  startMs: number
}

export interface Approval {
  promptId: string
  command: string
  description: string
  choices: string[]
  selected: number
  generation: number
  answered?: string
}

export interface State {
  phase: Phase
  cols: number
  model: string
  sessionId?: string
  subscriptionId?: string
  generation: number
  tail: string
  assistantOpen: boolean // an assistant message already has its leading marker in scrollback
  lastBlank: boolean
  deltasThisTurn: boolean
  reasoningTail: string
  tools: ToolRun[]
  approval?: Approval
  lastApproval?: Approval
  turnStartMs?: number
  turnEndMs?: number
  usage?: { total: number; input: number; output: number }
  statusText?: string
  input: string
  seen: number // events consumed
}

export interface Step {
  state: State
  blocks: Block[]
}

export function initialState(cols = 80, model = ""): State {
  return {
    phase: "connecting",
    cols,
    model,
    generation: 0,
    tail: "",
    assistantOpen: false,
    lastBlank: true,
    deltasThisTurn: false,
    reasoningTail: "",
    tools: [],
    input: "",
    seen: 0,
  }
}

const BUSY: Phase[] = ["thinking", "streaming", "tool", "waiting-approval"]
export const isBusy = (s: State) => BUSY.includes(s.phase)

/** Commit complete lines of assistant text; keeps `tail` as the unfinished remainder. */
function emitLines(s: State, lines: string[], blocks: Block[]) {
  const out: string[] = []
  for (const l of lines) {
    const blank = l.trim() === ""
    if (blank && (s.lastBlank || !s.assistantOpen)) continue // drop leading and doubled blank lines
    out.push(l)
    s.lastBlank = blank
  }
  if (out.length) {
    blocks.push({ kind: "assistant", lines: out, first: !s.assistantOpen })
    s.assistantOpen = true
  }
}

function flushTail(s: State, blocks: Block[]) {
  if (s.tail) emitLines(s, [s.tail], blocks)
  s.tail = ""
}

/** If the tail wraps to more rows than the live region shows, commit the surplus (stable) rows. */
function spillTail(s: State, blocks: Block[]) {
  const w = Math.max(10, s.cols - 2)
  const segs = wrapOffsets(s.tail, w)
  if (segs.length <= MAX_TAIL_ROWS) return
  const keep = segs.length - MAX_TAIL_ROWS + 1 // number of rows to commit
  const rows = segs.slice(0, keep).map((g) => s.tail.slice(g.start, g.end))
  const cut = segs[keep]!.start
  // rows from one logical line: first gets the marker, the rest continue it
  emitLines(s, rows.map((r) => r), blocks)
  s.tail = s.tail.slice(cut)
}

function ctxOf(p: Record<string, any>): string {
  const c = p.context ?? p.preview ?? p.args_text
  if (typeof c === "string" && c) return c
  const a = p.args
  if (a && typeof a === "object") {
    const first = Object.values(a).find((v) => typeof v === "string")
    if (typeof first === "string") return first
    return JSON.stringify(a)
  }
  return ""
}

export function startTurn(prev: State, text: string, nowMs: number): Step {
  const s: State = { ...prev, tail: "", assistantOpen: false, lastBlank: true, deltasThisTurn: false, reasoningTail: "", tools: [], approval: undefined, phase: "thinking", turnStartMs: nowMs, turnEndMs: undefined, statusText: undefined, input: "" }
  return { state: s, blocks: [{ kind: "user", text }] }
}

export function setInput(s: State, input: string): State {
  return { ...s, input }
}

export function moveApproval(s: State, delta: number): State {
  if (!s.approval) return s
  const n = s.approval.choices.length
  return { ...s, approval: { ...s.approval, selected: (s.approval.selected + delta + n) % n } }
}

export function answerApproval(s: State, index: number): { state: State; choice?: string } {
  const a = s.approval
  if (!a || index < 0 || index >= a.choices.length) return { state: s }
  const choice = a.choices[index]!
  return { state: { ...s, approval: { ...a, answered: choice } }, choice }
}

export function noteInterrupt(s: State): Step {
  return { state: s, blocks: [{ kind: "info", text: "interrupt requested" }] }
}

export function reduce(prev: State, frame: Frame, nowMs: number): Step {
  const blocks: Block[] = []
  if (frame.kind !== "event") return { state: prev, blocks }
  const e = frame as GatewayEvent
  const s: State = { ...prev, seen: prev.seen + 1 }
  const p = e.payload
  if (e.sessionId && !s.sessionId) s.sessionId = e.sessionId
  if (e.executionGeneration !== undefined) s.generation = e.executionGeneration

  switch (e.type) {
    case "gateway.ready":
      // Stay "connecting": the session is not usable until session.create returns (main sets idle).
      break
    case "session.info":
      if (typeof p.model === "string" && p.model) s.model = p.model
      break
    case "message.start":
      if (!isBusy(s)) s.phase = "thinking"
      if (s.turnStartMs === undefined || s.turnEndMs !== undefined) {
        s.turnStartMs = nowMs
        s.turnEndMs = undefined
      }
      break
    case "reasoning.delta":
      if (typeof p.text === "string" && p.text) s.reasoningTail = (s.reasoningTail + p.text).slice(-400)
      break
    case "thinking.delta":
      // Hermes uses this for spinner-word status ("(⌁) linking..."), not reasoning text.
      if (typeof p.text === "string" && p.text.trim()) s.statusText = p.text.trim()
      break
    case "message.delta": {
      if (p.text === null || p.text === undefined) {
        flushTail(s, blocks) // segment boundary
        break
      }
      if (typeof p.text !== "string" || p.text === "") break
      s.deltasThisTurn = true
      s.phase = s.approval ? "waiting-approval" : "streaming"
      s.reasoningTail = ""
      const joined = s.tail + p.text
      const parts = joined.split("\n")
      s.tail = parts.pop() ?? ""
      if (parts.length) emitLines(s, parts, blocks)
      spillTail(s, blocks)
      break
    }
    case "tool.start": {
      flushTail(s, blocks)
      s.assistantOpen = false
      const id = String(p.tool_id ?? p.tool_call_id ?? s.tools.length)
      s.tools = [...s.tools, { id, name: String(p.name ?? p.tool_name ?? "tool"), context: ctxOf(p), startMs: nowMs }]
      if (!s.approval) s.phase = "tool"
      break
    }
    case "tool.complete": {
      const id = String(p.tool_id ?? p.tool_call_id ?? "")
      const run = s.tools.find((t) => t.id === id)
      s.tools = s.tools.filter((t) => t.id !== id)
      const dur = typeof p.duration_s === "number" ? p.duration_s : run ? (nowMs - run.startMs) / 1000 : undefined
      const diff = typeof p.inline_diff === "string" && p.inline_diff ? p.inline_diff : undefined
      blocks.push({
        kind: "tool",
        name: String(p.name ?? p.tool_name ?? run?.name ?? "tool"),
        context: ctxOf(p) || run?.context || "",
        ok: p.is_error !== true,
        durationS: dur,
        diff,
      })
      s.assistantOpen = false
      if (!s.approval && s.phase === "tool") s.phase = s.tools.length ? "tool" : "thinking"
      break
    }
    case "approval.request": {
      flushTail(s, blocks)
      const choices: string[] = Array.isArray(p.choices) && p.choices.length ? p.choices.map(String) : ["once", "session", "always", "deny"]
      s.approval = {
        promptId: String(p.prompt_id ?? ""),
        command: String(p.command ?? ""),
        description: String(p.description ?? ""),
        choices,
        selected: 0,
        generation: typeof p.execution_generation === "number" ? p.execution_generation : s.generation,
      }
      s.phase = "waiting-approval"
      break
    }
    case "approval.settled": {
      const a = s.approval
      if (a && (!p.prompt_id || p.prompt_id === a.promptId)) {
        blocks.push({ kind: "approval", command: a.command, choice: a.answered ?? "settled" })
        s.approval = undefined
        s.phase = s.tools.length ? "tool" : "thinking"
      }
      break
    }
    case "status.update":
      if (typeof p.text === "string") s.statusText = p.text
      break
    case "session.usage": {
      const u = p.usage ?? p
      const input = Number(u.input ?? u.input_tokens ?? 0)
      const output = Number(u.output ?? u.output_tokens ?? 0)
      const total = Number(u.total ?? u.total_tokens ?? input + output)
      s.usage = { input, output, total }
      break
    }
    case "message.complete": {
      flushTail(s, blocks)
      if (!s.deltasThisTurn) {
        const text = typeof p.text === "string" ? p.text : typeof p.content === "string" ? p.content : ""
        if (text) emitLines(s, text.split("\n"), blocks)
      }
      s.tools = []
      s.approval = undefined
      s.turnEndMs = nowMs
      const status = String(p.status ?? "complete")
      const failed = p.error || (status !== "complete" && status !== "completed" && status !== "interrupted")
      if (failed) {
        blocks.push({ kind: "error", text: String(p.error ?? p.failure_reason ?? `turn ${status}`) })
        s.phase = "error"
      } else {
        s.phase = status === "interrupted" ? "interrupted" : "done"
      }
      const secs = s.turnStartMs !== undefined ? (nowMs - s.turnStartMs) / 1000 : undefined
      blocks.push({ kind: "turn", text: `${s.phase}${secs !== undefined ? ` in ${secs.toFixed(1)}s` : ""}${s.usage?.total ? ` · ${s.usage.total} tokens` : ""}` })
      s.assistantOpen = false
      break
    }
    case "error":
      blocks.push({ kind: "error", text: String(p.message ?? "error") })
      if (!isBusy(s)) s.phase = "error"
      break
    default:
      break
  }
  return { state: s, blocks }
}
