export const meta = {name: 'flaky-loop', description: 'Find flaky tests'};
const found = [];
let empty = 0;
let rounds = 0;
while (empty < 2) {
  rounds += 1;
  const answer = await agent(`Find new flaky tests, round ${rounds}`, {
    effort: 'xhigh',
    schema: {type: 'object', properties: {new: {type: 'array', items: {type: 'string'}}}, required: ['new']},
  });
  const fresh = answer.new.filter(name => !found.includes(name));
  found.push(...fresh);
  empty = fresh.length ? 0 : empty + 1;
  log(`Round ${rounds}: ${fresh.length} new`);
}
return {found, rounds};
