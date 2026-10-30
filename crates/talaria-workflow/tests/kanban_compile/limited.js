export const meta = {name: 'limited', description: 'An explicit turn cap'};
phase('Work');
await agent('stop soon', {label: 'short', profile: 'fixture', maxTurns: 2});
