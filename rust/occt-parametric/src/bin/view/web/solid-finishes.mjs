export const isEdgeFinish=kind=>kind==='fillet'||kind==='chamfer';
export function edgeFinishDefinition(kind,input,edges,size) {
  if(!isEdgeFinish(kind))throw Error('Choose fillet or chamfer.');
  if(!Array.isArray(edges)||edges.length===0||edges.some(edge=>!edge||typeof edge!=='object'||Array.isArray(edge)))throw Error('Pick at least one edge set.');
  if(size?.literal){const q=size.literal,mm=q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN);if(q.dimension!=='length'||!Number.isFinite(mm)||mm<=0)throw Error('Finishing size must be a positive length.');}
  if(!size||typeof size!=='object')throw Error('Enter a finishing size or choose a parameter.');
  return {input,edges:structuredClone(edges),[kind==='fillet'?'radius':'distance']:structuredClone(size)};
}
