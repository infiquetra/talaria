# Workflow JavaScript

Start with a pure literal header: `export const meta = {name:'name', description:'purpose', whenToUse:'when', phases:[{title:'phase',detail:'detail',model:'tier'}]};`. Name and description are required. The body supports top-level await and return. Metadata is parsed without executing it.

| Host | Meaning |
|---|---|
| `agent(prompt, opts?)` | Promise for final string or JSON; failed or skipped calls yield null. |
| `cutShort()` | Array of `{label, steps, limit}` for each agent so far that Hermes stopped at its step cap, in the order their results arrived; empty when none. See below. |
| `parallel(thunks)` | Invoke function thunks in order; return their results in input order, with null for failures. |
| `pipeline(items, ...stages)` | Each item moves through function stages independently. A stage receives previous result, original item, index. |
| `phase(title)` | Change the current instance's phase and narrate it. |
| `log(message)` | Narration, cut at 4,000 characters. |
| `workflow(nameOrRef, args?)` | Run a saved name or `{scriptPath:'relative.workflow'}` as a child. Children cannot create grandchildren. |
| `now()` | Fixed run-start timestamp, restored on resume. |
| `random(key)` | Deterministic number in [0,1) for a nonempty string key and stored seed. |
| `args` | JSON input; undefined when omitted. |
| `budget` | `.total`, `.spent()`, `.remaining()` in tokens, including usage running agents have streamed. Tokens means every token except cache reads (cache writes count); a turn whose cache reads Hermes did not report counts whole. Missing total is null; remaining is Infinity. Once spend reaches the total, new agents and still-running agents reject with BudgetExhaustedError naming what was spent; catch it to return partial results. |
| `budget.money` | Same accessors in US dollars. |

Every execution option:

| Option | Meaning |
|---|---|
| `opts.label` | Nonblank label, at most 120 characters. Defaults from the prompt's first nonblank line. |
| `opts.phase` | Override this call's phase. |
| `opts.schema` | Plain JSON schema, root object with properties and optional required names. |
| `opts.tier` | Staffing tier from the roster. |
| `opts.model` | Explicit model; overrides tier resolution. |
| `opts.provider` | Explicit provider from the roster. |
| `opts.effort` | none, minimal, low, medium, high, xhigh, max, or ultra. |
| `opts.profile` | Explicit profile. |
| `opts.toolsets` | Nonempty array of distinct toolset names. |
| `opts.skills` | Nonempty array of distinct skill names. |
| `opts.isolation` | Exactly 'worktree'. |
| `opts.maxTurns` | Integer 1 through 10,000. Omitted means the agent has no step cap; the profile's own `agent.max_turns` does not apply to workflow agents. Leave it out (see "Step caps"). |
| `opts.stallMs` | Positive integer, accepted as a logged no-op. |

An agent can end three ways. It finishes and its reply is returned. It fails and the call yields null; the run then ends `completed_with_failures` and names the failed agents first. Or Hermes stops it at its step cap (the exit reason `max_iterations_reached(N/M)` with `completed: false`): the agent is cut short. A cut short agent is not failed and is not sent another turn. Its reply is kept and returned exactly as for a finished agent, so a script that ignores the mark behaves as it always did. The mark is visible three ways: an object or array reply has a hidden `cutShort` property, `{steps, limit}`, that `Object.keys`, spread and `JSON.stringify` do not show; a text reply cannot carry one, so use `cutShort()`, which lists every cut short agent by label; and the journal's `call_finished` record, the `agent_finished` event, the run's final `cut_short_agents` list and the workflow view all say "cut short (60 of 60 steps)". With `opts.schema`, a cut short reply is checked once; if it does not fit, the call fails without a correction turn. A run whose only problem is cut short agents still ends `completed`. Cut short replies are kept when a run is resumed.

There are no coding toolsets by default. Name toolsets explicitly: ["file"], ["file","terminal"], ["web"], or ["todo"]. A file or terminal agent needs a worktree; tell readers not to edit. The file toolset can write.

Await every agent that matters. Returning abandons pending calls. Labels and phases do not identify paid work: content, execution options, occurrence, and workflow path do. Resume restores arguments, timestamp and seed. Ordered replay reuses completed calls; changing content or budget-dependent branching can diverge. Use args, now(), and random(key); ambient clocks and randomness are forbidden.

No filesystem, sockets, modules, or timers exist in the script. console, fetch, require, and setTimeout are absent. Dynamic imports are refused. eval and Function remain in the same sandbox. Limits default to 256 MiB heap, 1 MiB stack, 1,000 agents including child calls and replays, 4,096 items per combinator, and 5 seconds of runnable JavaScript per turn. Waiting for a backend does not count as runnable time. Native heap and stack errors can be caught; uncaught exhaustion fails the run.

API errors include DeterminismError, WorkflowOptionError, SchemaError, AgentLimitError, ItemLimitError, BudgetExhaustedError, NestedWorkflowError, WorkflowNotFoundError, and WorkflowScriptError. Header refusals include MetaMissing, MetaNotLiteral, MetaField, and ImportNotAllowed. Cancellation and backend aborts end the run outside JavaScript. An agentType option is refused; use tier, toolsets, or skills.

## Step caps

Leave `maxTurns` out of every `agent()` call. The token budget is the only limit a run needs. An agent stopped at its step cap cannot make another tool call, so it cannot commit its work or report properly: it leaves edits uncommitted and its reply can be cut off. `workflow check` warns when a script sets `maxTurns` and names the labels. The warning is not a refusal, but treat it as a mistake to fix.

## Status words

Talaria checks every reply against `opts.schema` with a full JSON Schema validator and sends one correction turn on a mismatch, so `enum` is enforced. A schema field the script tests or branches on must list its allowed words with `enum`, for example `status: {enum: ['done','blocked']}`, and the prompt must name the same words: "Answer status with exactly one of: done, blocked." A plain `{type:'string'}` lets an agent answer "complete" where the script tests for "done"; in real runs that made whole phases silently not run. Test for each enum word you handle, and name or retry anything else. `workflow check` answers an `enum` field with its first word, so list the word that continues the work first and the trial run reaches further.

## Reading a failed agent

A failure reason that says the agent was interrupted or stopped (for example "stopped in Hermes (outcome 'interrupted')") means a person or Talaria stopped it, for instance a keypress in the workflow view. It is not a provider limit or a timeout. Report it, ask the developer what happened, and do not reroute models because of it.

## Worktree and new project rules

A worktree is a separate checkout, not a sandbox. Workers commit on their own branches; report their commits for integration. Start independent phases together, such as a playtest that does not need the reviewers' output.

Worktrees: a new worktree starts at the source checkout's committed HEAD when that agent starts; uncommitted files are not copied. Tell each isolated worker to work in its own folder with relative paths, create named branches there with `git switch -c` (branches are shared across worktrees, so the integrator merges them by name), and never `cd` or `git -C` into the source checkout or switch branches there. Never paste the source checkout's absolute path into a prompt; workers must not commit absolute checkout paths.

New project: worktree isolation fails outside a git repository. When the target folder is not inside one, create the empty folder, run with `--cwd <that folder>`, and make the script's first agent unisolated (no `isolation`). It does only `git init` and an initial commit (`--allow-empty` without a scaffold), or the shared scaffold. Every later agent uses isolation: 'worktree'.
