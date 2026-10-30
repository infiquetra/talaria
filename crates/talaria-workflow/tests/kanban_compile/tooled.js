export const meta = {name: 'tooled', description: 'An explicit toolset'};
phase('Work');
await agent('read with tools', {label: 'tools', profile: 'fixture', toolsets: ['files']});
