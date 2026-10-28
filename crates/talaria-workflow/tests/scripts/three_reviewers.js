export const meta = {name: 'three-reviewers', description: 'Review three lenses'};
phase('Review');
return await parallel([
  () => agent('correctness', {label: 'correctness'}),
  () => agent('security', {label: 'security'}),
  () => agent('simplicity', {label: 'simplicity'}),
]);
