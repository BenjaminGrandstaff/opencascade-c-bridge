const keys=['x','y','z'],units={millimeter:1,centimeter:10,meter:1000,inch:25.4};
const point=values=>({components:Object.fromEntries(keys.map((k,i)=>[k,{literal:{value:values[i],dimension:'length',unit:'millimeter'}}]))});
export function validateSplitPoints(points){
 if(points===undefined)return [];
 if(!Array.isArray(points)||points.length>128)throw Error('A loft profile supports at most 128 split points.');
 for(const p of points){
  if(!p||typeof p!=='object'||Array.isArray(p))throw Error('Split points need family-space length vectors.');
  const values=keys.map(k=>p.literal?.[k]??p.components?.[k]?.literal);
  if(values.every(Boolean)&&!values.every(q=>q.dimension==='length'&&Number.isFinite(q.value)&&units[q.unit]))throw Error('Split points need finite length coordinates.');
 }
 return structuredClone(points);
}
export function createLoftSplitControls(saved=[]){
 const points=saved.map(p=>({saved:p,source:'preserve',values:keys.map(k=>{const q=p.literal?.[k]??p.components?.[k]?.literal;return q?q.value*units[q.unit]:0;})}));
 const expression=p=>p.source==='preserve'?p.saved:point(p.values);
 return {
  definition:()=>validateSplitPoints(points.map(expression)),
  render(label,{select,number,render,button,content,invalidate,pickSplit,checkpoint,message,profile},useStart){
   for(const [i,p] of points.entries()){
    const prefix=`${label} split ${i+1}`;
    if(p.saved)select(`${prefix} source`,[['preserve','Keep existing split formula'],['literal','Entered split point']],p.source,value=>{p.source=value;render();});
    if(p.source==='literal')for(const [j,key] of ['X','Y','Z'].entries())number(`${prefix} ${key} (mm)`,p.values[j],value=>p.values[j]=value);
    content.append(button(`Use ${prefix.toLowerCase()} as start`,()=>{invalidate();useStart(structuredClone(expression(p)));render();}));
    content.append(button(`Remove ${prefix.toLowerCase()}`,()=>{invalidate();points.splice(i,1);render();}));
   }
   if(pickSplit){
    const pick=button(`Pick ${label.toLowerCase()} split in canvas`,async()=>{
     invalidate();const current=checkpoint();
     const picked=await pickSplit(validateSplitPoints(points.map(expression)));
     if(!current()||!picked)return;
     if(picked.profile!==profile||!Array.isArray(picked.position_mm)||picked.position_mm.length!==3||!picked.position_mm.every(Number.isFinite))throw Error('The picked loft split does not match this profile.');
     if(points.length>=128)throw Error('A loft profile supports at most 128 split points.');
     invalidate();points.push({source:'literal',values:[...picked.position_mm]});render();message('Split selected at fixed family coordinates. Preview checks every instance.');
    });pick.disabled=points.length>=128;content.append(pick);
   }
   const add=button(`Add ${label.toLowerCase()} split point`,()=>{if(points.length>=128)throw Error('A loft profile supports at most 128 split points.');invalidate();points.push({source:'literal',values:[0,0,0]});render();});add.disabled=points.length>=128;content.append(add);
  }
 };
}
