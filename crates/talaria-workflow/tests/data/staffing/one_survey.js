export const meta = {
  name: "one-survey",
  description: "Run one staffed agent asking for one short word",
  phases: [],
};

const defineWorkflow = ({ run }) => run();

return await defineWorkflow({
  run: async () => agent("Reply with exactly one short word.", {
    tier: "survey",
    toolsets: ["file"],
  }),
});
