// Frame decoding for the Hermes gateway-v1 JSON-RPC-over-WebSocket protocol.
// Pure: no I/O, no framework imports. Used identically by the live path and --replay.

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json }

export interface GatewayEvent {
  kind: "event"
  type: string
  sessionId?: string
  seq?: number
  executionGeneration?: number
  payload: Record<string, any>
  params: Record<string, any>
}
export interface RpcResponse {
  kind: "response"
  id: number | string
  result?: any
  error?: { code: number; message: string; data?: unknown }
}
export interface RpcOther {
  kind: "other"
  raw: unknown
}
export interface DecodeError {
  kind: "invalid"
  reason: string
}
export type Frame = GatewayEvent | RpcResponse | RpcOther | DecodeError

export function decodeFrame(raw: string): Frame {
  let msg: any
  try {
    msg = JSON.parse(raw)
  } catch (e) {
    return { kind: "invalid", reason: `not JSON: ${raw.slice(0, 60)}` }
  }
  if (msg === null || typeof msg !== "object" || Array.isArray(msg)) return { kind: "invalid", reason: "not an object" }
  if (msg.method === "event" && msg.params && typeof msg.params === "object") {
    const p = msg.params
    return {
      kind: "event",
      type: String(p.type ?? ""),
      sessionId: typeof p.session_id === "string" ? p.session_id : undefined,
      seq: typeof p.seq === "number" ? p.seq : undefined,
      executionGeneration: typeof p.execution_generation === "number" ? p.execution_generation : undefined,
      payload: p.payload && typeof p.payload === "object" ? p.payload : {},
      params: p,
    }
  }
  if (msg.id !== undefined && ("result" in msg || "error" in msg)) {
    return { kind: "response", id: msg.id, result: msg.result, error: msg.error }
  }
  return { kind: "other", raw: msg }
}

/** JSONL record: one received frame with a millisecond offset from connect. */
export interface RecordedFrame {
  t: number
  dir: "in" | "out"
  raw: string
}

export function parseRecording(text: string): RecordedFrame[] {
  const out: RecordedFrame[] = []
  for (const line of text.split("\n")) {
    if (!line.trim()) continue
    const r = JSON.parse(line)
    if (typeof r.raw === "string") out.push({ t: Number(r.t ?? 0), dir: r.dir === "out" ? "out" : "in", raw: r.raw })
  }
  return out
}
