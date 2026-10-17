const q=(value,dimension)=>({literal:{value,dimension,unit:dimension==='length'?'millimeter':null}});
const vector=axis=>({components:Object.fromEntries(['x','y','z'].map((k,i)=>[k,q(axis[i],'scalar')]))});
export function validateLoftConstraint(c){
 if(!c||typeof c!=='object'||!c.magnitude)throw Error('Loft constraints need a direction mode and magnitude.');
 if(c.normal_to_profile!==undefined&&typeof c.normal_to_profile!=='boolean'||Boolean(c.direction)===Boolean(c.normal_to_profile))throw Error('Choose exactly one loft end direction or normal to profile.');
 const v=['x','y','z'].map(k=>c.direction?.literal?.[k]??c.direction?.components?.[k]?.literal);
 if(v.every(Boolean)&&(!v.every(q=>q.dimension==='scalar'&&Number.isFinite(q.value))||Math.hypot(...v.map(q=>q.value))===0))throw Error('Enter a finite nonzero loft end direction.');
 if(c.magnitude.literal){const v=c.magnitude.literal,n=v.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[v.unit]??NaN);if(v.dimension!=='length'||!Number.isFinite(n)||n<=0)throw Error('Loft constraint magnitude must be a positive length.');}
 return structuredClone(c);
}
export function createLoftEndControls(loft){
 const states=['start','end'].map(end=>{const saved=loft[`${end}_constraint`];return {end,saved,mode:saved?(saved.normal_to_profile?'normal':'tangent'):'free',axis:['x','y','z'].map((k,i)=>saved?.direction?.literal?.[k]?.value??saved?.direction?.components?.[k]?.literal?.value??(i===2?1:0)),directionSource:saved?.direction?'preserve':'literal',size:saved?.magnitude.literal?saved.magnitude.literal.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[saved.magnitude.literal.unit]??NaN):10,sizeSource:saved?'preserve':'literal'};});
 return {
 definition(){return Object.fromEntries(states.map(s=>[`${s.end}_constraint`,s.mode!=='free'?validateLoftConstraint({...(s.mode==='normal'?{normal_to_profile:true}:{direction:s.directionSource==='preserve'?s.saved.direction:vector(s.axis)}),magnitude:s.sizeSource==='preserve'?s.saved.magnitude:s.sizeSource.startsWith('parameter:')?{parameter:s.sizeSource.slice(10)}:q(s.size,'length')}):undefined]));},
 render({select,number,content,parameters,render}){
  for(const s of states){const label=s.end==='start'?'Start':'End';
   select(`${label} loft condition`,[['free','Free'],['tangent','Tangent direction'],['normal','Normal to profile']],s.mode,value=>{s.mode=value;render();});
   if(s.mode==='free')continue;
   if(s.mode==='tangent'){
   if(s.saved?.direction)select(`${label} loft direction source`,[['preserve','Keep existing direction formula'],['literal','Entered direction']],s.directionSource,value=>{s.directionSource=value;render();});
   for(const [i,key] of ['X','Y','Z'].entries())number(`${label} loft direction ${key}`,s.axis[i],value=>s.axis[i]=value).disabled=s.directionSource==='preserve';
   }
   const choices=[['literal','Entered magnitude'],...parameters.filter(p=>p.parameter_type?.scalar==='length').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];if(s.saved)choices.push(['preserve','Keep existing magnitude formula']);
   select(`${label} loft strength source`,choices,s.sizeSource,value=>{s.sizeSource=value;render();});
   number(`${label} loft strength (mm)`,s.sizeSource==='literal'?s.size:'',value=>s.size=value).disabled=s.sizeSource!=='literal';
  }
  const note=document.createElement('p');note.textContent='Directions follow increasing station order. Normal to profile follows plane edits; strength is a length per station interval. Smooth constrained lofts require corresponding edges and at most 64 stations. Start anchors use family millimetres: an existing corner, or any point on a full circle or ellipse. Saved formulas follow parameters; unmatched anchors are rejected. Preview checks every instance and shows tangent arrows at one-third strength, with full-strength labels. Directions must cross end planes; strengths that fold the loft backward are rejected.';content.append(note);
 }
 };
}
