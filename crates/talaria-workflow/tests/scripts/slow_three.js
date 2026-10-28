export const meta = {name: 'slow-three', description: 'fixture'};
return await parallel([() => agent('slow 1'), () => agent('slow 2'), () => agent('slow 3')]);
