import { describe, expect, it } from "vitest";

import { REDACTED, isSuspiciousKey, redactFrame, redactUrl } from "./redact.js";

/**
 * These frames mirror the shapes the Hermes gateway actually dispatches, read
 * from `tui_gateway/server.py`. If a test here fails, a credential is one commit
 * away from being written to disk.
 */

describe("redactFrame", () => {
  it("withholds the plaintext sudo password", () => {
    const { frame, redactions } = redactFrame({
      jsonrpc: "2.0",
      id: 7,
      method: "sudo.respond",
      params: { request_id: "r-1", password: "hunter2" },
    });

    expect(JSON.stringify(frame)).not.toContain("hunter2");
    expect((frame as any).params.password).toBe(REDACTED);
    expect(redactions).toEqual([
      { path: "params.password", reason: "deny-set:sudo.respond" },
    ]);
  });

  it("withholds a secret value", () => {
    const { frame, redactions } = redactFrame({
      jsonrpc: "2.0",
      method: "secret.respond",
      params: { request_id: "r-2", value: "sk-live-abc123" },
    });

    expect(JSON.stringify(frame)).not.toContain("sk-live-abc123");
    expect(redactions).toHaveLength(1);
  });

  it("withholds a captured terminal buffer", () => {
    const { frame } = redactFrame({
      method: "terminal.read.respond",
      params: { request_id: "r-3", text: "$ cat ~/.env\nAPI_KEY=live" },
    });

    expect(JSON.stringify(frame)).not.toContain("live");
  });

  it("keeps non-credential fields on a denied method intact", () => {
    const { frame } = redactFrame({
      method: "sudo.respond",
      params: { request_id: "r-4", password: "s3cret" },
    });

    expect((frame as any).params.request_id).toBe("r-4");
  });

  it("leaves ordinary traffic completely untouched", () => {
    const original = {
      jsonrpc: "2.0",
      method: "event",
      params: {
        type: "assistant_delta",
        session_id: "s-1",
        payload: { text: "hello" },
      },
    };

    const { frame, redactions } = redactFrame(original);

    expect(frame).toEqual(original);
    expect(redactions).toEqual([]);
  });

  it("does not mutate the frame it was given", () => {
    const original = {
      method: "sudo.respond",
      params: { password: "hunter2" },
    };
    redactFrame(original);
    expect(original.params.password).toBe("hunter2");
  });

  it("catches a credential on an unknown method via the key-name net", () => {
    // Protocol drift: a method the deny-set has never heard of.
    const { frame, redactions } = redactFrame({
      method: "future.unknown.respond",
      params: { api_key: "should-not-survive" },
    });

    expect(JSON.stringify(frame)).not.toContain("should-not-survive");
    expect(redactions[0]?.reason).toBe("suspicious-key");
  });

  it("catches a credential nested at arbitrary depth", () => {
    const { frame } = redactFrame({
      method: "event",
      params: { payload: { config: { provider: { token: "deep-secret" } } } },
    });

    expect(JSON.stringify(frame)).not.toContain("deep-secret");
  });

  it("catches a credential inside an array", () => {
    const { frame } = redactFrame({
      method: "batch",
      params: { items: [{ ok: 1 }, { password: "in-an-array" }] },
    });

    expect(JSON.stringify(frame)).not.toContain("in-an-array");
  });

  it("survives frames that are not objects", () => {
    expect(redactFrame(null).frame).toBeNull();
    expect(redactFrame("plain string").frame).toBe("plain string");
    expect(redactFrame(42).frame).toBe(42);
  });
});

describe("redactUrl", () => {
  it("withholds the gateway attach token", () => {
    const url = redactUrl(
      "ws://127.0.0.1:8765/api/ws?token=abc123&session=s-1",
    );

    expect(url).not.toContain("abc123");
    expect(url).toContain("session=s-1");
  });

  it("leaves a clean url alone", () => {
    const url = "ws://127.0.0.1:8765/api/ws";
    expect(redactUrl(url)).toBe(url);
  });

  it("withholds anything it cannot parse rather than guessing", () => {
    expect(redactUrl("not a url")).toBe(REDACTED);
  });
});

describe("isSuspiciousKey", () => {
  it("matches the credential-shaped names", () => {
    for (const key of [
      "password",
      "passphrase",
      "apiKey",
      "api_key",
      "authToken",
      "secret",
    ]) {
      expect(isSuspiciousKey(key)).toBe(true);
    }
  });

  it("does not match ordinary protocol fields", () => {
    for (const key of ["session_id", "method", "params", "text", "type"]) {
      expect(isSuspiciousKey(key)).toBe(false);
    }
  });
});
