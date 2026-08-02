/**
 * Attach to a running Hermes gateway over WebSocket.
 *
 * This is the standalone relationship: Talaria owns its own process lifetime and
 * dials a gateway it did not launch, rather than being exec'd as a child of
 * `hermes --tui`. Hermes already serves and tests this path -- the dashboard
 * mounts `/api/ws` and hands the socket to `tui_gateway.ws.handle_ws`, driving
 * the same dispatch surface the bundled terminal UI uses over stdio.
 *
 * Node 22 provides a global `WebSocket`, so this adds no dependency.
 */

import { FrameRecorder } from "../record/recorder.js";
import { redactUrl } from "../record/redact.js";

/** Why an attach attempt ended. Reported rather than thrown, so a caller can
 *  distinguish "never connected" from "connected then dropped". */
export type AttachOutcome =
  | { status: "closed"; code: number; reason: string }
  | { status: "failed"; error: string };

export interface AttachOptions {
  /** Gateway endpoint, e.g. `ws://127.0.0.1:8765/api/ws?token=...`. */
  url: string;
  /** Where to write the frame log. */
  recorder: FrameRecorder;
  /** Called after each recorded frame, for progress output. */
  onFrame?: (count: number) => void;
  /** Called once the socket opens. */
  onOpen?: () => void;
  /** Resolves the attach early, e.g. on Ctrl-C. */
  signal?: AbortSignal;
}

/**
 * Connect, record every frame until the socket closes, and resolve with why it
 * ended. Never rejects: a failure to connect is an outcome to report, not an
 * exception to swallow.
 */
export function attach(options: AttachOptions): Promise<AttachOutcome> {
  const { url, recorder, onFrame, onOpen, signal } = options;

  return new Promise<AttachOutcome>((resolve) => {
    let settled = false;
    const settle = (outcome: AttachOutcome): void => {
      if (settled) return;
      settled = true;
      resolve(outcome);
    };

    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch (error) {
      settle({ status: "failed", error: describe(error) });
      return;
    }

    signal?.addEventListener(
      "abort",
      () => {
        try {
          socket.close(1000, "client stopping");
        } catch {
          // Already closed; the close handler will settle.
        }
        settle({ status: "closed", code: 1000, reason: "stopped by operator" });
      },
      { once: true },
    );

    socket.addEventListener("open", () => onOpen?.());

    socket.addEventListener("message", (event: MessageEvent) => {
      recorder.record("in", asText(event.data));
      onFrame?.(recorder.stats().frames);
    });

    socket.addEventListener("error", () => {
      // The WebSocket error event carries no useful detail by design. The close
      // event that follows carries the code, so report the endpoint instead of
      // inventing a cause.
      settle({
        status: "failed",
        error: `could not attach to ${redactUrl(url)}`,
      });
    });

    socket.addEventListener("close", (event: CloseEvent) => {
      settle({
        status: "closed",
        code: event.code,
        reason: event.reason || "(none given)",
      });
    });
  });
}

/** Normalize a WebSocket payload to text without assuming its runtime type. */
function asText(data: unknown): string {
  if (typeof data === "string") return data;
  if (data instanceof ArrayBuffer) return Buffer.from(data).toString("utf8");
  if (ArrayBuffer.isView(data)) {
    return Buffer.from(data.buffer, data.byteOffset, data.byteLength).toString(
      "utf8",
    );
  }
  return String(data);
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
