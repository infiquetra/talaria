// OpenTUI glue: split-footer screen mode. Finished output goes to the terminal's own scrollback via
// writeToScrollback; only the small footer (live region) is redrawn.
import { createCliRenderer, createTextAttributes, RGBA, StyledText, TextRenderable, type CliRenderer, type KeyEvent } from "@opentui/core"
import type { Line, Span } from "./render"

type Chunk = StyledText["chunks"][number]

const colorCache = new Map<string, RGBA>()
function rgba(hex: string): RGBA {
  let c = colorCache.get(hex)
  if (!c) colorCache.set(hex, (c = RGBA.fromHex(hex)))
  return c
}

function chunk(s: Span): Chunk {
  const st = s.style
  const c: Chunk = { __isChunk: true, text: s.text }
  if (!st) return c
  if (st.fg) c.fg = rgba(st.fg)
  if (st.bg) c.bg = rgba(st.bg)
  const attrs = createTextAttributes({ bold: st.bold, dim: st.dim, italic: st.italic })
  if (attrs) c.attributes = attrs
  return c
}

export function toStyled(lines: Line[]): StyledText {
  const chunks: Chunk[] = []
  lines.forEach((l, i) => {
    for (const s of l) if (s.text) chunks.push(chunk(s))
    if (i < lines.length - 1) chunks.push({ __isChunk: true, text: "\n" })
  })
  if (!chunks.length) chunks.push({ __isChunk: true, text: " " })
  return new StyledText(chunks)
}

export interface UiKey {
  name: string
  ctrl: boolean
  meta: boolean
  sequence: string
}

export interface Ui {
  cols(): number
  commit(lines: Line[]): void
  setFooter(lines: Line[]): void
  onKey(cb: (k: UiKey) => void): void
  onResize(cb: () => void): void
  destroy(): Promise<void>
}

export async function createTuiUi(): Promise<Ui> {
  const renderer: CliRenderer = await createCliRenderer({
    screenMode: "split-footer",
    footerHeight: 3,
    externalOutputMode: "capture-stdout",
    exitOnCtrlC: false,
    useMouse: false,
    consoleMode: "disabled",
    clearOnShutdown: true,
  })
  return uiOnRenderer(renderer)
}

/** Builds the Ui on any split-footer renderer (the real one, or the framework's test renderer). */
export function uiOnRenderer(renderer: CliRenderer): Ui {
  const footer = new TextRenderable(renderer, { id: "footer", content: " ", width: "100%", height: 3, wrapMode: "none", truncate: true })
  renderer.root.add(footer)
  let height = 3
  const keyCbs: Array<(k: UiKey) => void> = []
  renderer.keyInput.on("keypress", (k: KeyEvent) => {
    for (const cb of keyCbs) cb({ name: k.name, ctrl: k.ctrl, meta: k.meta, sequence: k.sequence })
  })
  renderer.keyInput.on("paste", (p) => {
    const text = new TextDecoder().decode(p.bytes)
    for (const ch of text.replace(/\r?\n/g, " ")) for (const cb of keyCbs) cb({ name: ch, ctrl: false, meta: false, sequence: ch })
  })
  return {
    cols: () => renderer.width,
    commit(lines) {
      if (!lines.length) return
      const styled = toStyled(lines)
      renderer.writeToScrollback((ctx) => ({
        root: new TextRenderable(ctx.renderContext, { content: styled, width: ctx.width, height: lines.length, wrapMode: "none" }),
        width: ctx.width,
        height: lines.length,
        startOnNewLine: true,
        trailingNewline: true,
      }))
    },
    setFooter(lines) {
      const n = Math.max(2, lines.length)
      if (n !== height) {
        height = n
        footer.height = n
        renderer.footerHeight = n
      }
      footer.content = toStyled(lines)
      renderer.requestRender()
    },
    onKey: (cb) => void keyCbs.push(cb),
    onResize: (cb) => void renderer.on("resize", cb),
    async destroy() {
      renderer.destroy()
    },
  }
}

/** Non-TTY fallback and test double: committed lines go to stdout as plain text. */
export function createPlainUi(write: (s: string) => void = (s) => process.stdout.write(s)): Ui {
  return {
    cols: () => 100,
    commit(lines) {
      for (const l of lines) write(l.map((s) => s.text).join("") + "\n")
    },
    setFooter() {},
    onKey() {},
    onResize() {},
    async destroy() {},
  }
}
