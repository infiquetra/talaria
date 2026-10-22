// Width-aware word wrapping with source offsets, so streaming code can cut a tail precisely.
export function cellWidth(s: string): number {
  return Bun.stringWidth(s)
}

export interface Seg {
  start: number
  end: number // exclusive; excludes the break whitespace
  next: number // where the following segment starts (after skipped break whitespace)
}

/** Greedy word wrap of a single logical line (no "\n"). Always returns at least one segment. */
export function wrapOffsets(text: string, width: number): Seg[] {
  width = Math.max(1, width)
  const segs: Seg[] = []
  let w = 0
  let i = 0
  const chars = Array.from(text)
  // map code point index -> UTF-16 offset
  const offs: number[] = []
  {
    let o = 0
    for (const c of chars) {
      offs.push(o)
      o += c.length
    }
    offs.push(o)
  }
  let lineStartCp = 0
  let lastSpaceCp = -1
  for (i = 0; i < chars.length; i++) {
    const c = chars[i]!
    const cw = cellWidth(c)
    if (c === " ") {
      lastSpaceCp = i
    }
    if (w + cw > width && c !== " ") {
      if (lastSpaceCp > lineStartCp) {
        segs.push({ start: offs[lineStartCp]!, end: offs[lastSpaceCp]!, next: offs[lastSpaceCp + 1]! })
        // recompute width of chars after the space up to i
        lineStartCp = lastSpaceCp + 1
        w = 0
        for (let k = lineStartCp; k < i; k++) w += cellWidth(chars[k]!)
        lastSpaceCp = -1
      } else {
        segs.push({ start: offs[lineStartCp]!, end: offs[i]!, next: offs[i]! })
        lineStartCp = i
        w = 0
        lastSpaceCp = -1
      }
    }
    w += cw
  }
  segs.push({ start: offs[lineStartCp]!, end: offs[chars.length]!, next: offs[chars.length]! })
  return segs
}

export function wrapText(text: string, width: number): string[] {
  return wrapOffsets(text, width).map((s) => text.slice(s.start, s.end))
}

export function truncate(text: string, width: number): string {
  if (cellWidth(text) <= width) return text
  let out = ""
  let w = 0
  for (const c of text) {
    const cw = cellWidth(c)
    if (w + cw > width - 1) break
    out += c
    w += cw
  }
  return out + "…"
}
