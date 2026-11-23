export const meta = {name:'one',description:'one agent, then what the script saw'};
const first = await agent('review',{label:'review',model:'scripted-model',provider:'scripted'});
return {first, capped: cutShort()};
