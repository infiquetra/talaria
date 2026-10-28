# Workflow JavaScript

Start with a pure literal header: `export const meta = {name:'name', description:'purpose', whenToUse:'when', phases:[{title:'phase',detail:'detail',model:'tier'}]};`. Name and description are required. The body supports top-level await and return. Metadata is parsed without executing it.

| Host | Meaning |
|---|---|
| `agent(prompt, opts?)` | Promise for final string or JSON; failed or skipped calls yield null. |
| `parallel(thunks)` | Invoke function thunks in order; return their results in input order, with null for failures. |
| `pipeline(items, ...stages)` | Each item moves through function stages independently. A stage receives previous result, original item, index. |
| `phase(title)` | Change the current instance's phase and narrate it. |
| `log(message)` | Narration, cut at 4,000 characters. |
| `workflow(nameOrRef, args?)` | Run a saved name or `{scriptPath:'relative.js'}` as a child. Children cannot create grandchildren. |
| `now()` | Fixed run-start timestamp, restored on resume. |
| `random(key)` | Deterministic number in [0,1) for a nonempty string key and stored seed. |
| `args` | JSON input; undefined when omitted. |
| `budget` | `.total`, `.spent()`, `.remaining()` in tokens. Missing total is null; remaining is Infinity. |
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
| `opts.maxTurns` | Integer 1 through 10,000. Omitted uses the profile policy. |
| `opts.stallMs` | Positive integer, accepted as a logged no-op. |

There are no coding toolsets by default. Name toolsets explicitly: ["file"], ["file","terminal"], ["web"], or ["todo"]. A file or terminal agent needs a worktree; tell readers not to edit. The file toolset can write.

Await every agent that matters. Returning abandons pending calls. Labels and phases do not identify paid work: content, execution options, occurrence, and workflow path do. Resume restores arguments, timestamp and seed. Ordered replay reuses completed calls; changing content or budget-dependent branching can diverge. Use args, now(), and random(key); ambient clocks and randomness are forbidden.

No filesystem, sockets, modules, or timers exist in the script. console, fetch, require, and setTimeout are absent. Dynamic imports are refused. eval and Function remain in the same sandbox. Limits default to 256 MiB heap, 1 MiB stack, 1,000 agents including child calls and replays, 4,096 items per combinator, and 5 seconds of runnable JavaScript per turn. Waiting for a backend does not count as runnable time. Native heap and stack errors can be caught; uncaught exhaustion fails the run.

API errors include DeterminismError, WorkflowOptionError, SchemaError, AgentLimitError, ItemLimitError, BudgetExhaustedError, NestedWorkflowError, WorkflowNotFoundError, and WorkflowScriptError. Header refusals include MetaMissing, MetaNotLiteral, MetaField, and ImportNotAllowed. Cancellation and backend aborts end the run outside JavaScript. An agentType option is refused; use tier, toolsets, or skills.
