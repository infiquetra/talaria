#!/usr/bin/env bun
// slice: a minimal Claude-Code-style Hermes client. See README in REPORT.md.
import { writeFileSync } from "node:fs"
import { GatewayClient } from "./gateway"
import { decodeFrame, type Frame } from "./protocol"
import { replayFile } from "./replay"
import { initialState, isBusy, reduce, startTurn, answerApproval, moveApproval, setInput, noteInterrupt, type State, type Step } from "./state"
import { renderBlock, renderLive, plain } from "./render"
import { createPlainUi, createTuiUi, type Ui, type UiKey } from "./ui"

const bootMs = performance.now()

interface Args {
  prompt: string
  record?: string
  replay?: string
  plain: boolean
  cwd: string
  model: string
  provider: string
  approve?: string
  metrics?: string
  cols: number
  exitAfterTurn: boolean
  attachOnly: boolean
}

const APPROVAL_CHOICES = ["once", "session", "always", "deny"]

function parseArgs(argv: string[]): Args {
  const a: Args = { prompt: "", plain: false, cwd: process.cwd(), model: "minimax-m3.1-flash", provider: "ollama-cloud", cols: 100, exitAfterTurn: false, attachOnly: false }
  const rest: string[] = []
  for (let i = 0; i < argv.length; i++) {
    const v = argv[i]!
    const next = () => argv[++i] ?? ""
    if (v === "--record") a.record = next()
    else if (v === "--replay") a.replay = next()
    else if (v === "--plain") a.plain = true
    else if (v === "--cwd") a.cwd = next()
    else if (v === "--model") a.model = next()
    else if (v === "--provider") a.provider = next()
    else if (v === "--approve") a.approve = next()
    else if (v === "--metrics") a.metrics = next()
    else if (v === "--cols") a.cols = Number(next())
    else if (v === "--attach-only") a.attachOnly = true
    else if (v === "--exit-after-turn") a.exitAfterTurn = true
    else rest.push(v)
  }
  a.prompt = rest.join(" ")
  return a
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  if (args.approve !== undefined && !APPROVAL_CHOICES.includes(args.approve)) {
    process.stderr.write(`--approve must be one of: ${APPROVAL_CHOICES.join(", ")}\n`)
    process.exit(2)
  }

  if (args.replay) {
    const r = replayFile(args.replay, args.cols, { autoAnswer: args.approve ?? "once" })
    process.stdout.write(r.scrollback.join("\n") + "\n\n" + r.footer.join("\n") + "\n")
    return
  }

  const ui: Ui = args.plain || !process.stdout.isTTY ? createPlainUi() : await createTuiUi()
  let state: State = initialState(ui.cols(), args.model)
  const metrics: Record<string, number | string> = {}
  let submitAt = 0
  let turnNo = 0
  let gotFirstChar = false
  let gotFirstEvent = false
  let exiting = false
  let interruptedAt = 0

  const gw = new GatewayClient({ recordPath: args.record })

  const refresh = () => ui.setFooter(renderLive(state, ui.cols(), performance.now()))
  const apply = (step: Step) => {
    state = step.state
    for (const b of step.blocks) ui.commit(renderBlock(b, ui.cols()))
    refresh()
  }
  const showError = (text: string) => apply({ state: { ...state, phase: isBusy(state) ? "error" : state.phase }, blocks: [{ kind: "error", text }] })

  async function exit(code = 0) {
    if (exiting) return
    exiting = true
    clearInterval(ticker)
    try {
      if (state.sessionId && state.subscriptionId) {
        await Promise.race([gw.call("session.detach", { session_id: state.sessionId, subscription_id: state.subscriptionId, end_reason: "viewer_closed" }), new Promise((r) => setTimeout(r, 1500))])
      }
    } catch {}
    if (args.metrics) {
      metrics.rss_mb_at_exit = Math.round((process.memoryUsage().rss / 1048576) * 10) / 10
      writeFileSync(args.metrics, JSON.stringify(metrics, null, 1))
    }
    gw.close()
    await ui.destroy()
    process.exit(code)
  }

  async function submit(text: string) {
    if (!state.sessionId) return
    gotFirstChar = false
    gotFirstEvent = false
    turnNo++
    submitAt = performance.now()
    apply(startTurn(state, text, performance.now()))
    try {
      await gw.call("prompt.submit", { session_id: state.sessionId, text, submission_id: crypto.randomUUID() })
    } catch (e) {
      state = { ...state, phase: "error" }
      showError(String((e as Error).message))
    }
  }

  async function respond(index: number) {
    const a = state.approval
    if (!a) return
    const r = answerApproval(state, index)
    if (!r.choice) return
    state = r.state
    refresh()
    try {
      await gw.call("approval.respond", { session_id: state.sessionId, execution_generation: a.generation, prompt_id: a.promptId, choice: r.choice })
    } catch (e) {
      showError(String((e as Error).message))
    }
  }

  async function interrupt() {
    apply(noteInterrupt(state))
    try {
      // The gateway keeps an unanswered approval open for its full timeout; deny it so the stop lands now.
      const a = state.approval
      if (a && a.choices.includes("deny")) await respond(a.choices.indexOf("deny"))
      await gw.call("session.interrupt", { session_id: state.sessionId, execution_generation: state.generation })
    } catch (e) {
      showError(String((e as Error).message))
    }
  }

  gw.onEvent = (e) => {
    const now = performance.now()
    if (submitAt && !gotFirstEvent && e.type !== "sessions.changed" && e.type !== "session.info") {
      gotFirstEvent = true
      metrics[`first_event_ms_turn${turnNo}`] = Math.round(now - submitAt)
    }
    if (submitAt && !gotFirstChar && e.type === "message.delta" && typeof e.payload.text === "string" && e.payload.text.trim()) {
      gotFirstChar = true
      metrics[`first_char_ms_turn${turnNo}`] = Math.round(now - submitAt)
    }
    if (e.sessionId && state.sessionId && e.sessionId !== state.sessionId) return
    const wasBusy = isBusy(state)
    const step = reduce(state, e, now)
    apply(step)
    if (e.type === "approval.request" && args.approve && state.approval) {
      // Answer only on an exact match; anything else stays pending for a person.
      const i = state.approval.choices.indexOf(args.approve)
      if (i >= 0) void respond(i)
    }
    if (wasBusy && !isBusy(state)) {
      metrics.turns = turnNo
      if (args.exitAfterTurn) void exit(0)
    }
  }
  gw.onClose = () => {
    if (!exiting) {
      showError("gateway connection closed")
      setTimeout(() => void exit(1), 300)
    }
  }

  const ticker = setInterval(() => {
    if (isBusy(state)) refresh()
  }, 120)

  ui.onResize(() => {
    state = { ...state, cols: ui.cols() }
    refresh()
  })

  ui.onKey((k: UiKey) => {
    if (exiting) return
    if (k.ctrl && k.name === "c") {
      if (isBusy(state) && !interruptedAt) {
        interruptedAt = Date.now()
        void interrupt()
      } else void exit(0)
      return
    }
    if (state.approval) {
      if (k.name === "up") state = moveApproval(state, -1)
      else if (k.name === "down") state = moveApproval(state, 1)
      else if (k.name === "return" || k.name === "enter") void respond(state.approval.selected)
      else if (/^[1-9]$/.test(k.sequence)) void respond(Number(k.sequence) - 1)
      refresh()
      return
    }
    if (k.ctrl && k.name === "d" && !state.input) return void exit(0)
    if (k.ctrl && k.name === "u") state = setInput(state, "")
    else if (k.name === "return" || k.name === "enter") {
      const text = state.input.trim()
      if (!text || isBusy(state) || state.phase === "connecting") return
      if (text === "/exit" || text === "/quit") return void exit(0)
      interruptedAt = 0
      void submit(text)
      return
    } else if (k.name === "backspace") state = setInput(state, Array.from(state.input).slice(0, -1).join(""))
    else if (!k.ctrl && !k.meta && k.sequence && k.sequence.length >= 1 && k.sequence >= " " && !k.sequence.startsWith("\x1b")) state = setInput(state, state.input + k.sequence)
    refresh()
  })

  refresh()
  try {
    await gw.attach()
  } catch (e) {
    await ui.destroy()
    process.stderr.write(`attach failed: ${(e as Error).message}\n`)
    process.exit(2)
  }
  metrics.launch_to_ready_ms = Math.round(performance.now())
  metrics.attach_ms = Math.round(gw.readyAtMs)
  try {
    const snap = await gw.call("session.create", { source: "tui", cwd: args.cwd, model: args.model, provider: args.provider })
    state = { ...state, sessionId: snap.session_id, subscriptionId: snap.subscription_id, generation: snap.execution_generation ?? 0, phase: "idle" }
    metrics.session_create_done_ms = Math.round(performance.now())
    refresh()
  } catch (e) {
    showError(`session.create failed: ${(e as Error).message}`)
    return void exit(1)
  }
  if (args.attachOnly) return void exit(0)
  if (args.prompt) await submit(args.prompt)
}

main().catch((e) => {
  process.stderr.write(String(e?.stack ?? e) + "\n")
  process.exit(1)
})
