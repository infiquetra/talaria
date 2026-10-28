export const meta = {
  name: "fan-out-12",
  description: "Run twelve staffed agents in parallel asking for one short word each",
  phases: [],
};

const defineWorkflow = ({ run }) => run();

return await defineWorkflow({
  run: async () => parallel(Array.from({ length: 12 }, (_, index) => () =>
    agent("Reply with exactly one short word.", {
      label: "survey-" + (index + 1),
      tier: "survey",
      toolsets: ["file"],
    })
  )),
});
