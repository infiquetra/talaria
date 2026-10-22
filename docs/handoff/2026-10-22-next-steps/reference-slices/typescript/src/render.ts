// Pure rendering: model blocks and state -> styled lines. No framework imports (ADR-style seam):
// ui.ts converts Lines to OpenTUI chunks, tests compare plain text.
import { truncate, wrapText, cellWidth } from "./text"
import type { State } from "./state"

export interface Style {
  fg?: string
  bg?: string
  bold?: boolean
  dim?: boolean
  italic?: boolean
}
export interface Span {
  text: string
  style?: Style
}
export type Line = Span[]

export type Block =
  | { kind: "user"; text: string }
  | { kind: "assistant"; lines: string[]; first: boolean }
  | { kind: "tool"; name: string; context: string; ok: boolean; durationS?: number; diff?: string }
  | { kind: "approval"; command: string; choice: string }
  | { kind: "info"; text: string }
  | { kind: "error"; text: string }
  | { kind: "turn"; text: string }

export const C = {
  accent: "#5fd7ff",
  ok: "#5fd75f",
  bad: "#ff5f5f",
  warn: "#ffd75f",
  dim: "#808080",
  add: "#5fd75f",
  del: "#ff5f5f",
  hunk: "#5fafff",
}

const S = (text: string, style?: Style): Span => ({ text, style })

export function plain(lines: Line[]): string[] {
  return lines.map((l) => l.map((s) => s.text).join(""))
}

export function fmtDuration(s: number): string {
  if (s < 10) return `${s.toFixed(1)}s`
  if (s < 60) return `${Math.round(s)}s`
  return `${Math.floor(s / 60)}m${Math.round(s % 60)}s`
}

export function renderDiffLine(l: string): Span {
  if (l.startsWith("+++") || l.startsWith("---")) return S(l, { dim: true })
  if (l.startsWith("@@")) return S(l, { fg: C.hunk })
  if (l.startsWith("+")) return S(l, { fg: C.add })
  if (l.startsWith("-")) return S(l, { fg: C.del })
  return S(l, { dim: true })
}

export function renderBlock(b: Block, width: number): Line[] {
  const w = Math.max(10, width)
  switch (b.kind) {
    case "user": {
      const out: Line[] = [[]]
      const rows = b.text.split("\n").flatMap((l) => wrapText(l, w - 2))
      rows.forEach((r, i) => out.push([S(i === 0 ? "› " : "  ", { fg: C.accent, bold: true }), S(r, { bold: true })]))
      return out
    }
    case "assistant": {
      const out: Line[] = []
      let first = b.first
      for (const l of b.lines) {
        const rows = wrapText(l, w - 2)
        for (const r of rows) {
          out.push([S(first ? "⏺ " : "  ", { fg: C.accent }), S(r)])
          first = false
        }
      }
      return out
    }
    case "tool": {
      const dur = b.durationS !== undefined ? ` ${fmtDuration(b.durationS)}` : ""
      const mark = b.ok ? "✓" : "✗"
      const tail = ` ${mark}${dur}`
      const head = `${b.name}: `
      const ctx = truncate(b.context.replace(/\s+/g, " "), Math.max(4, w - 2 - cellWidth(head) - cellWidth(tail)))
      const line: Line = [
        S("⏺ ", { fg: b.ok ? C.ok : C.bad }),
        S(head, { bold: true }),
        S(ctx, { dim: true }),
        S(` ${mark}`, { fg: b.ok ? C.ok : C.bad, bold: true }),
        S(dur, { dim: true }),
      ]
      const out: Line[] = [line]
      if (b.diff) {
        const dl = b.diff.replace(/\n$/, "").split("\n")
        const max = 14
        for (const l of dl.slice(0, max)) out.push([S("  "), renderDiffLine(truncate(l, w - 2))])
        if (dl.length > max) out.push([S(`  … ${dl.length - max} more lines`, { dim: true })])
      }
      return out
    }
    case "approval":
      return [
        [
          S("⏺ ", { fg: C.warn }),
          S("approval: ", { bold: true }),
          S(truncate(b.command, Math.max(4, w - 20)), { dim: true }),
          S(" → ", { dim: true }),
          S(b.choice, { fg: b.choice === "deny" ? C.bad : C.ok, bold: true }),
        ],
      ]
    case "info":
      return wrapText(b.text, w - 2).map((r) => [S("  " + r, { dim: true })])
    case "error":
      return wrapText(b.text, w - 2).map((r, i) => [S(i === 0 ? "✗ " : "  ", { fg: C.bad, bold: true }), S(r, { fg: C.bad })])
    case "turn":
      return [[S("  " + b.text, { dim: true })], []]
  }
}

export function statusLabel(st: State): string {
  switch (st.phase) {
    case "connecting": return "connecting"
    case "idle": return "ready"
    case "thinking": return "thinking"
    case "streaming": return "streaming"
    case "tool": return "running tool"
    case "waiting-approval": return "waiting for approval"
    case "done": return "done"
    case "interrupted": return "interrupted"
    case "error": return "error"
  }
}

export function elapsedS(st: State, now: number): number | undefined {
  if (st.turnStartMs === undefined) return undefined
  return Math.max(0, ((st.turnEndMs ?? now) - st.turnStartMs) / 1000)
}

function fmtTok(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

export function renderStatus(st: State, width: number, now: number): Line {
  const parts: string[] = [st.model || "(no model)", statusLabel(st)]
  const el = elapsedS(st, now)
  if (el !== undefined) parts.push(fmtDuration(el))
  if (st.usage?.total) parts.push(`${fmtTok(st.usage.total)} tok`)
  const busy = st.phase === "thinking" || st.phase === "streaming" || st.phase === "tool"
  const color = st.phase === "waiting-approval" ? C.warn : st.phase === "error" ? C.bad : busy ? C.accent : C.dim
  const text = truncate(parts.join(" · "), width - 2)
  return [S("● ", { fg: color }), S(text, { fg: color })]
}

export const MAX_TAIL_ROWS = 5

/** The small live region: reasoning/tool hints, streaming tail, approval panel, status, input. */
export function renderLive(st: State, width: number, now: number): Line[] {
  const w = Math.max(20, width)
  const out: Line[] = []
  for (const t of st.tools) {
    const el = Math.max(0, (now - t.startMs) / 1000)
    out.push([S("⏺ ", { fg: C.warn }), S(`${t.name}: `, { bold: true }), S(truncate(t.context.replace(/\s+/g, " "), w - 16 - cellWidth(t.name)), { dim: true }), S(` ${fmtDuration(el)}`, { dim: true })])
  }
  if (st.reasoningTail && st.phase !== "streaming") {
    out.push([S("  " + truncate(st.reasoningTail.replace(/\s+/g, " ").trim(), w - 2), { dim: true, italic: true })])
  }
  if (st.tail) {
    const rows = wrapText(st.tail, w - 2)
    rows.slice(-MAX_TAIL_ROWS).forEach((r, i) => out.push([S(i === 0 && !st.assistantOpen ? "⏺ " : "  ", { fg: C.accent }), S(r)]))
  }
  if (st.approval) {
    const a = st.approval
    out.push([S("┌ approval required", { fg: C.warn, bold: true }), S(a.description ? ` · ${a.description}` : "", { dim: true })])
    wrapText(a.command, w - 4).slice(0, 3).forEach((r) => out.push([S("│ ", { fg: C.warn }), S(r, { bold: true })]))
    a.choices.forEach((c, i) =>
      out.push([
        S("│ ", { fg: C.warn }),
        S(i === a.selected ? "▸ " : "  ", { fg: C.accent }),
        S(`${i + 1}. ${c}`, i === a.selected ? { fg: C.accent, bold: true } : {}),
      ]),
    )
    out.push([S("└ 1-" + a.choices.length + " or ↑/↓ + Enter · Ctrl+C interrupts", { fg: C.warn, dim: true })])
  }
  out.push(renderStatus(st, w, now))
  const prompt: Line = [S("› ", { fg: C.accent, bold: true })]
  if (st.approval) prompt.push(S("(answer the approval above)", { dim: true }))
  else {
    const maxIn = w - 4
    const shown = cellWidth(st.input) > maxIn ? "…" + st.input.slice(-(maxIn - 1)) : st.input
    prompt.push(S(shown), S("▌", { fg: C.accent }))
  }
  out.push(prompt)
  return out
}
