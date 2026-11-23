export const meta = {name:'restart',description:'one agent finishes, one fails and is restarted'};
const plan = await agent('plan',{label:'plan',model:'scripted-model',provider:'scripted'});
const review = await agent('review',{label:'review',model:'scripted-model',provider:'scripted'});
return {plan, review};
