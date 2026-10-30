export const meta = {name: 'two-step', description: 'Two live steps'};
phase('Work');
const step1 = await agent('Reply with exactly this summary word: BANANA', {label: 'step1', profile: 'fixture'});
await agent(`The previous step produced: ${step1}. As your summary, write that word twice separated by one space.`, {label: 'step2', profile: 'fixture'});
