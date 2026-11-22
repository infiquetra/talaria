export const meta = {name:'budget',description:'budget fixture'};
const first = await agent('review',{label:'review',model:'scripted-model',provider:'scripted'});
let second;
try { second = await agent('again',{label:'again',model:'scripted-model',provider:'scripted'}); } catch (e) { second = e.name; }
return [first, second];
