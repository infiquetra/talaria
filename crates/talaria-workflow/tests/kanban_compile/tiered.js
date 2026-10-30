export const meta = {name: 'tiered', description: 'A tier call'};
phase('Work');
await agent('build the change', {label: 'build', profile: 'fixture', tier: 'builder'});
