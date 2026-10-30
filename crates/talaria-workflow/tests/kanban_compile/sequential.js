export const meta = {name: 'sequential', description: 'Two awaited calls'};
phase('Work');
await agent('read the source', {label: 'read', profile: 'fixture'});
await agent('write the notes', {label: 'write', profile: 'fixture'});
