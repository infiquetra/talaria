export const meta = {name: 'dynamic', description: 'A result field decides a later call'};
phase('Work');
const r = await agent('count the items', {
  label: 'count',
  profile: 'fixture',
  schema: {type: 'object', properties: {count: {type: 'number'}}, required: ['count']},
});
if (r.count > 2) {
  await agent('handle the many items', {label: 'many', profile: 'fixture'});
}
await agent('close the count', {label: 'after', profile: 'fixture'});
