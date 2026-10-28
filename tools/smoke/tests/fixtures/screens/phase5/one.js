export const meta = { name: "one", description: "One-agent smoke fixture" };
return await agent("Reply with one word.", {
  tier: "survey",
  label: "one",
  toolsets: ["todo"],
  schema: {
    type: "object",
    required: ["word"],
    additionalProperties: false,
    properties: { word: { type: "string" } },
  },
});
