export const meta = {
  name: 'review-branch-demo', description: 'Review a branch',
  phases: [{title: 'Review'}, {title: 'Verify'}, {title: 'Synthesize'}],
};
phase('Review');
const reviews = await parallel(args.lenses.map(lens => () => agent(
  `Review the branch for ${lens} issues`,
  {schema: {type: 'object', properties: {findings: {type: 'array', items: {type: 'object', properties: {id: {type: 'string'}, claim: {type: 'string'}}, required: ['id', 'claim']}}}, required: ['findings']}}
)));
const findings = reviews.flatMap(review => review.findings);
phase('Verify');
const verified = await pipeline(findings, async finding => ({
  finding,
  verdict: await agent(`Verify finding ${finding.id}: ${finding.claim}`, {schema: {type: 'object', properties: {real: {type: 'boolean'}}, required: ['real']}}),
}));
const confirmed = verified.filter(item => item.verdict.real).map(item => item.finding.id);
const dropped = verified.filter(item => !item.verdict.real).map(item => item.finding.id);
phase('Synthesize');
const summary = await agent(`Summarise the confirmed findings: ${confirmed.join(', ')}`);
return {confirmed, dropped, summary};
