# Complete workflow examples

Use tiers only after roster validation. Each reader is told not to edit, and every code worker gets a separate worktree. Missing or failed agent results are handled explicitly.

## Review through three lenses

```js
export const meta = {name:'three-lens-review',description:'Review through independent lenses, verify findings, and synthesize'};
const input = args ?? {};
const findingSchema = {type:'object',properties:{findings:{type:'array',items:{type:'object',properties:{id:{type:'string'},file:{type:'string'},line:{type:'integer'},title:{type:'string'}},required:['id','file','line','title']}}},required:['findings']};
phase('Review');
const lenses = ['correctness','security','simplicity'];
const reviews = await parallel(lenses.map(lens => () => agent(`Review ${input.target ?? 'the change'} for ${lens}. Do not edit files.`,{label:`review:${lens}`,tier:'survey',schema:findingSchema,toolsets:['file'],isolation:'worktree'})));
const findings = reviews.flatMap(result => Array.isArray(result?.findings) ? result.findings : []);
phase('Verify');
const verified = await pipeline(findings, finding => agent(`Verify this finding without editing files: ${JSON.stringify(finding)}`,{label:`verify:${finding.id}`,tier:'judge',schema:{type:'object',properties:{verdict:{type:'string'},reason:{type:'string'}},required:['verdict','reason']},toolsets:['file'],isolation:'worktree'}));
phase('Synthesize');
return await agent(`Summarize the findings and verification results: ${JSON.stringify({findings,verified})}`,{label:'synthesize',tier:'judge',schema:{type:'object',properties:{summary:{type:'string'}},required:['summary']},toolsets:['todo']});
```

## Research and deduplicate leads

```js
export const meta = {name:'research-sweep',description:'Search independently and read each distinct lead once'};
const input = args ?? {};
phase('Search');
const searches = await parallel([1,2,3].map(i => () => agent(`Research ${input.topic ?? 'the requested topic'} from angle ${i}.`,{label:`search:${i}`,tier:'survey',schema:{type:'object',properties:{leads:{type:'array',items:{type:'object',properties:{ref:{type:'string'},why:{type:'string'}},required:['ref','why']}}},required:['leads']},toolsets:['web']})));
const leads = new Map();
for (const result of searches) for (const lead of Array.isArray(result?.leads) ? result.leads : []) if (typeof lead.ref === 'string') leads.set(lead.ref,lead);
phase('Read');
const reading = await pipeline([...leads.values()], lead => agent(`Read ${lead.ref} and explain its evidence for ${input.topic ?? 'the topic'}.`,{label:`read:${lead.ref}`,tier:'survey',schema:{type:'object',properties:{summary:{type:'string'}},required:['summary']},toolsets:['web']}));
return await agent(`Synthesize these research notes, preserving uncertainty: ${JSON.stringify(reading)}`,{label:'synthesize',tier:'judge',schema:{type:'object',properties:{summary:{type:'string'}},required:['summary']},toolsets:['todo']});
```

## Plan, build independently, and review

```js
export const meta = {name:'implementation-fanout',description:'Plan independent tasks, build them on branches, then review'};
const input = args ?? {};
phase('Plan');
const plan = await agent(`Plan independent tasks for ${input.goal ?? 'the requested change'}.`,{label:'plan',tier:'judge',schema:{type:'object',properties:{tasks:{type:'array',items:{type:'object',properties:{id:{type:'string'},goal:{type:'string'}},required:['id','goal']}}},required:['tasks']},toolsets:['todo']});
phase('Build');
const builds = await pipeline(Array.isArray(plan?.tasks) ? plan.tasks : [], task => agent(`Implement ${task.goal}. Run the tests and commit on your branch. Report your commit; do not merge other branches.`,{label:`build:${task.id}`,tier:'builder',schema:{type:'object',properties:{status:{type:'string'},commit:{type:'string'}},required:['status','commit']},toolsets:['file','terminal'],isolation:'worktree'}));
phase('Review');
return await agent(`Review these branch results without editing files: ${JSON.stringify(builds)}. Identify what still needs integration.`,{label:'review',tier:'judge',schema:{type:'object',properties:{summary:{type:'string'}},required:['summary']},toolsets:['file'],isolation:'worktree'});
```
