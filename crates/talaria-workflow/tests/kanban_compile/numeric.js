export const meta = {name: 'numeric', description: 'A numeric schema field flows into the next prompt'};
phase('Work');
const row = await agent('measure the change', {
  label: 'measure',
  profile: 'fixture',
  schema: {type: 'object', properties: {n: {type: 'number'}}, required: ['n']},
});
await agent(`the count was ${row.n}`, {label: 'report', profile: 'fixture'});
