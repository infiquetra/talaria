export const meta = {name: 'fanout', description: 'Three parallel calls and one synthesis'};
phase('Work');
const parts = await parallel([
  () => agent('lens one', {label: 'one', profile: 'fixture'}),
  () => agent('lens two', {label: 'two', profile: 'fixture'}),
  () => agent('lens three', {label: 'three', profile: 'fixture'}),
]);
await agent('write the synthesis', {label: 'synthesis', profile: 'fixture'});
void parts;
