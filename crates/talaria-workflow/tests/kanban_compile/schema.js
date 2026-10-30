export const meta = {name: 'schema', description: 'A schema result'};
phase('Work');
await agent('name the word', {
  label: 'word',
  profile: 'fixture',
  schema: {type: 'object', properties: {word: {type: 'string'}}, required: ['word']},
});
