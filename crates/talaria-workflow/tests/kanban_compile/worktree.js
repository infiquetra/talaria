export const meta = {name: 'worktree', description: 'A worktree workspace'};
phase('Work');
await agent('edit the repo', {label: 'edit', profile: 'fixture', isolation: 'worktree'});
