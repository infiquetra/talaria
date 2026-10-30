export const meta = {name: 'seeded', description: 'Call count follows the capture clock'};
phase('Work');
const clock = now;
const stamp = clock();
if (stamp === 0) {
  await agent('only when the clock is at the origin', {label: 'origin', profile: 'fixture'});
}
await agent('always runs', {label: 'always', profile: 'fixture'});
