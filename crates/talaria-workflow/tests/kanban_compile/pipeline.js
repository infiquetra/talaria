export const meta = {name: 'pipeline', description: 'A later prompt embeds the earlier result'};
phase('Work');
const draft = await agent('draft the note', {label: 'draft', profile: 'fixture'});
await agent(`Revise this draft: ${draft}`, {label: 'revise', profile: 'fixture'});
