const units={millimeter:1,centimeter:10,meter:1000,inch:25.4};
const xyz=['x','y','z'];
const point=values=>({components:Object.fromEntries(xyz.map((key,i)=>[key,{literal:{value:values[i],dimension:'length',unit:'millimeter'}}]))});
export function validateLoftCorrespondence(values,ids,constrained){
 if(values===undefined)return {};
 if(!values||typeof values!=='object'||Array.isArray(values))throw Error('Loft correspondence must map profile IDs to alignment settings.');
 const result=Object.create(null);
 for(const [id,value] of Object.entries(values)){
  if(!ids.includes(id))throw Error('Loft correspondence references a profile outside its station tracks.');
  if(!value||typeof value!=='object'||Array.isArray(value)||Object.keys(value).some(k=>!['start_point','reverse'].includes(k))||value.reverse!==undefined&&typeof value.reverse!=='boolean')throw Error('Choose a valid loft start anchor and traversal direction.');
  if(value.start_point!==undefined){
   if(!value.start_point||typeof value.start_point!=='object')throw Error('Choose a loft start anchor.');
   const literal=xyz.map(k=>value.start_point.literal?.[k]??value.start_point.components?.[k]?.literal);
   if(literal.every(Boolean)&&!literal.every(q=>q.dimension==='length'&&Number.isFinite(q.value)&&units[q.unit]))throw Error('Loft start anchors need finite length coordinates.');
  }
  if(value.start_point!==undefined||value.reverse)result[id]=structuredClone(value);
 }
 if(Object.keys(values).length&&!constrained)throw Error('Manual loft correspondence requires smooth surfaces and an end constraint.');
 return Object.fromEntries(Object.entries(result));
}
export function createLoftCorrespondenceControls(loft){
 const states=new Map();
 const state=id=>{
  if(!states.has(id)){
   const saved=loft.correspondence?.[id];
   states.set(id,{saved,source:saved?.start_point?'preserve':'default',reverse:saved?.reverse??false,values:xyz.map(k=>{const q=saved?.start_point?.literal?.[k]??saved?.start_point?.components?.[k]?.literal;return q?q.value*units[q.unit]:0;})});
  }
  return states.get(id);
 };
 return {
  definition(loft){
   const result=Object.create(null);
   for(const id of [...loft.profiles,...loft.holes.flat()]){
    const s=state(id),anchor=s.source==='preserve'?s.saved.start_point:s.source==='literal'?point(s.values):undefined;
    if(anchor||s.reverse)result[id]={...(anchor?{start_point:structuredClone(anchor)}:{}),...(s.reverse?{reverse:true}:{})};
   }
   return Object.keys(result).length?{correspondence:Object.fromEntries(Object.entries(result))}:{correspondence:undefined};
  },
  render(id,label,{select,number,render,button,content,pickAnchor,invalidate,checkpoint,message}){
   if(!id)return;
   const s=state(id);
   select(`${label} traversal`,[['forward','Original'],['reverse','Reversed']],s.reverse?'reverse':'forward',value=>s.reverse=value==='reverse');
   const choices=[['default','Original start'],['literal','Entered start point']];if(s.saved?.start_point)choices.push(['preserve','Keep existing start formula']);
   select(`${label} start anchor`,choices,s.source,value=>{s.source=value;render();});
   if(pickAnchor)content.append(button(`Pick ${label.toLowerCase()} start in canvas`,async()=>{
    invalidate();const current=checkpoint();
    const picked=await pickAnchor(id);
    if(!current()||!picked)return;
    if(picked.profile!==id||!Array.isArray(picked.position_mm)||picked.position_mm.length!==3||!picked.position_mm.every(Number.isFinite))throw Error('The picked loft start does not match this profile.');
    invalidate();s.values=[...picked.position_mm];s.source='literal';render();message('Start anchor selected at fixed family coordinates. Preview checks every instance.');
   }));
   if(s.source==='literal')for(const [i,key] of ['X','Y','Z'].entries())number(`${label} start ${key} (mm)`,s.values[i],value=>s.values[i]=value);
  }
 };
}
