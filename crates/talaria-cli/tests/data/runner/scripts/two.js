export const meta = {name:'two',description:'resume fixture'};
const first=await agent('first',{label:'first',model:'scripted-model',provider:'scripted'});
const second=await agent('second',{label:'second',model:'scripted-model',provider:'scripted'});
return [first,second];
