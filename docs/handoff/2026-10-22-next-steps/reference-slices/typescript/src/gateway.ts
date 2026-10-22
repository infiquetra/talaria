// Attach to the local Hermes gateway: control socket (identify + one-use ticket), then WebSocket.
// The ticket is a credential; it is never logged or returned from this module.
import net from "node:net"
import os from "node:os"
import path from "node:path"
import { appendFileSync, writeFileSync } from "node:fs"
import { decodeFrame, type Frame, type GatewayEvent } from "./protocol"

export interface Identity {
  apiOrigin: string
  instanceId: string
  profileId: string
}

function controlCall(sock: string, req: object, timeoutMs = 5000): Promise<any> {
  return new Promise((resolve, reject) => {
    const c = net.createConnection(sock)
    let buf = ""
    const timer = setTimeout(() => {
      c.destroy()
      reject(new Error("control socket timeout"))
    }, timeoutMs)
    c.on("connect", () => c.write(JSON.stringify(req) + "\n"))
    c.on("data", (d) => {
      buf += d.toString("utf8")
      const i = buf.indexOf("\n")
      if (i >= 0) {
        clearTimeout(timer)
        c.end()
        try {
          resolve(JSON.parse(buf.slice(0, i)))
        } catch (e) {
          reject(e)
        }
      }
    })
    c.on("error", (e) => {
      clearTimeout(timer)
      reject(e)
    })
  })
}

export class GatewayClient {
  private ws!: WebSocket
  private nextId = 100
  private pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void; method: string }>()
  private t0 = performance.now()
  private recordPath?: string
  onEvent: (e: GatewayEvent, raw: string) => void = () => {}
  onFrame: (f: Frame, raw: string) => void = () => {}
  onClose: () => void = () => {}
  identity!: Identity
  readyAtMs = 0
  describe: any

  constructor(opts: { recordPath?: string } = {}) {
    this.recordPath = opts.recordPath
    if (this.recordPath) writeFileSync(this.recordPath, "")
  }

  private record(dir: "in" | "out", raw: string) {
    if (!this.recordPath) return
    appendFileSync(this.recordPath, JSON.stringify({ t: Math.round(performance.now() - this.t0), dir, raw }) + "\n")
  }

  async attach(home = path.join(os.homedir(), ".hermes")): Promise<void> {
    this.t0 = performance.now()
    const sock = path.join(home, "gateway.sock")
    const ident = await controlCall(sock, { verb: "identify", id: 1, protocol: 1 })
    const res = ident.result ?? ident
    const served = res.served_profiles ?? []
    const profileId: string =
      served.find((p: any) => p.home === home)?.profile_id ?? served[0]?.profile_id ?? home
    this.identity = { apiOrigin: res.api_origin, instanceId: res.instance_id, profileId }
    const grant = await controlCall(sock, {
      protocol: 1,
      id: 2,
      verb: "session-ticket",
      params: { profile_id: profileId, instance_id: res.instance_id, purpose: "interactive", scope: "profile" },
    })
    if (!grant.ok) throw new Error(`session-ticket refused: ${JSON.stringify(grant.error ?? {}).slice(0, 200)}`)
    const ticket: string = grant.result.ticket
    const url = res.api_origin.replace(/^http/, "ws") + "/api/ws"
    await new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(url, ["hermes-gateway-v1", "hermes-gateway-ticket." + ticket])
      this.ws = ws
      let ready = false
      ws.onerror = (e: any) => {
        if (!ready) reject(new Error("websocket error before gateway.ready"))
      }
      ws.onclose = () => {
        if (!ready) reject(new Error("websocket closed before gateway.ready"))
        for (const p of this.pending.values()) p.reject(new Error("connection closed"))
        this.pending.clear()
        this.onClose()
      }
      ws.onmessage = (m) => {
        const raw = typeof m.data === "string" ? m.data : Buffer.from(m.data as ArrayBuffer).toString("utf8")
        this.record("in", raw)
        const f = decodeFrame(raw)
        if (!ready && f.kind === "event" && f.type === "gateway.ready") {
          ready = true
          this.readyAtMs = performance.now() - this.t0
          resolve()
        }
        if (f.kind === "response") {
          const p = this.pending.get(Number(f.id))
          if (p) {
            this.pending.delete(Number(f.id))
            if (f.error) p.reject(new Error(`${p.method}: ${f.error.message} (${f.error.code}) ${JSON.stringify(f.error.data ?? null).slice(0, 300)}`))
            else p.resolve(f.result)
          }
        }
        this.onFrame(f, raw)
        if (f.kind === "event") this.onEvent(f, raw)
      }
    })
    this.describe = await this.call("runtime.describe", {})
  }

  call<T = any>(method: string, params: object): Promise<T> {
    const id = this.nextId++
    const msg = JSON.stringify({ jsonrpc: "2.0", id, method, params })
    this.record("out", msg)
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve, reject, method })
      this.ws.send(msg)
    })
  }

  close() {
    try {
      this.ws?.close()
    } catch {}
  }
}
