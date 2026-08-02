import { describe, expect, it } from "vitest";

import { formatPrompt } from "./app.js";

describe("formatPrompt", () => {
  it("normalizes whitespace", () => {
    expect(formatPrompt("  hello   Hermes  ")).toBe("hello Hermes");
  });

  it("provides a safe fallback for an empty prompt", () => {
    expect(formatPrompt("  ")).toBe("Ready");
  });
});
