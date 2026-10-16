import {geometryIds,dimensionNumber} from './constraint-controls.mjs';
import {geometryNames} from './geometry-names.mjs';
const dataOf=pattern=>Object.values(pattern)[0];
const stamp=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])):v);
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
// O(G + P) catalogue filtering; later patterns cannot be sources of earlier ones.
export function patternGeometry(definition,group,index=(definition.patterns??[]).length){
 const forbidden=new Set((definition.patterns??[]).slice(index).map(p=>dataOf(p).id));
 const ids=geometryIds(definition,group);
 return [...new Set(ids)].filter(id=>!forbidden.has(id.split(':')[0]));
}
export function makePattern(definition,kind,fields,index=null){
 const patterns=definition.patterns??[],previous=index===null?null:patterns[index];
 if(!['mirror','linear','circular'].includes(kind))throw new Error('Choose a pattern type.');
 if(previous&&(Object.keys(previous)[0]!==kind||dataOf(previous).id!==fields.id))throw new Error('Existing patterns keep their name and type.');
 if(typeof fields.id!=='string'||!fields.id.trim()||fields.id!==fields.id.trim()||fields.id.includes(':'))throw new Error('Use a pattern name without colons or surrounding spaces.');
 if(!previous&&geometryNames(definition).has(fields.id))throw new Error('That geometry name already exists.');
 if(!previous&&patterns.length>=100)throw new Error('Use at most 100 sketch patterns.');
 const position=index??patterns.length,curves=new Set(patternGeometry(definition,'curves',position));
 if(!Array.isArray(fields.entities)||!fields.entities.length||fields.entities.length>1000||new Set(fields.entities).size!==fields.entities.length||fields.entities.some(id=>!curves.has(id)))throw new Error('Choose 1 to 1000 distinct known source curves from before this pattern.');
 const result={id:fields.id,entities:[...fields.entities]};
 const role=kind==='mirror'?'axis':kind==='linear'?'direction':'center';
 if(!patternGeometry(definition,kind==='circular'?'points':'lines',position).includes(fields[role]))throw new Error(`Choose a known ${role} from before this pattern.`);
 if(kind!=='circular'&&fields.entities.includes(fields[role]))throw new Error('A pattern cannot copy its own reference line.');
 result[role]=fields[role];
 if(kind==='mirror'){
  const points=new Set(patternGeometry(definition,'points',position)),on=fields.on_axis??[];
  if(!Array.isArray(on)||new Set(on).size!==on.length||on.some(id=>!points.has(id)))throw new Error('Choose distinct known on-axis points.');
  if(on.length)result.on_axis=[...on];
 }else{
  if(!Number.isInteger(fields.count)||fields.count<2||fields.count>1000)throw new Error('Pattern count must be an integer from 2 to 1000, including the source.');
  result.count=fields.count;
  const key=kind==='linear'?'spacing':'angle',value=fields[key];
  if(value!==undefined){
   if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Choose a pattern driver.');
   if(value.literal){const number=kind==='linear'?dimensionNumber(value,'length'):value.literal.dimension==='scalar'&&value.literal.unit==null?value.literal.value:null;if(number===null||!Number.isFinite(number))throw new Error('Use a finite pattern value in the correct units.');if(kind==='linear'&&number===0)throw new Error('Linear spacing must be nonzero.');}
   result[key]=structuredClone(value);
  }else if(kind==='linear')throw new Error('Enter a spacing driver.');
 }
 return {[kind]:result};
}
export function createPatternControls({host,definition,parameters=[],changed,message,highlight=()=>{},initialSelection=null,initialOpen=false}){
 const patterns=definition.patterns??[],section=document.createElement('details'),summary=document.createElement('summary');summary.textContent='Linked sketch patterns';section.open=initialOpen;section.append(summary);host.append(section);
 let selected=patterns.some(p=>dataOf(p).id===initialSelection)?initialSelection:'',kind='linear';
 const select=document.createElement('select');select.setAttribute('aria-label','Pattern to edit');select.append(new Option('New pattern',''));for(const p of patterns)select.append(new Option(`${dataOf(p).id} · ${Object.keys(p)[0]}`,dataOf(p).id));section.append(select);
 const form=document.createElement('div');section.append(form);
 function render(){
  form.replaceChildren();select.value=selected;
  const index=patterns.findIndex(p=>dataOf(p).id===selected),existing=index<0?null:patterns[index];if(existing)kind=Object.keys(existing)[0];
  const data=existing?structuredClone(dataOf(existing)):{id:'',entities:[],count:2,on_axis:[]};
  const note=document.createElement('p');note.textContent='Copies stay linked to source geometry. Count includes the source. Circular patterns default to equal spacing around a full turn. Negative linear spacing reverses direction. Preview verifies copied geometry and dependent profiles; profile and hole selections are edited separately.';form.append(note);
  function input(label,value,type='text'){const row=document.createElement('label');row.textContent=label;const e=document.createElement('input');e.type=type;e.value=value;e.setAttribute('aria-label',label);if(type==='number')e.step='any';row.append(e);form.append(row);return e;}
  function choice(label,choices,value,multiple=false){const row=document.createElement('label');row.textContent=label;const e=document.createElement('select');e.setAttribute('aria-label',label);e.multiple=multiple;if(multiple)e.size=6;const selectedIds=new Set(multiple?value:[]),known=new Set(choices.map(([id])=>id));for(const id of (multiple?value:[value]))if(id&&!known.has(id))choices.push([id,`${id} (Preview to refresh)`]);for(const [id,text]of choices){const option=new Option(text,id);if(multiple)option.selected=selectedIds.has(id);e.append(option);}if(!multiple)e.value=value;row.append(e);form.append(row);return e;}
  const type=choice('Pattern type',[['linear','Linear'],['circular','Circular'],['mirror','Mirror']],kind);type.disabled=Boolean(existing);type.onchange=()=>{kind=type.value;render();};
  const name=input('Pattern name',data.id);name.readOnly=Boolean(existing);
  const position=index<0?patterns.length:index,curves=patternGeometry(definition,'curves',position),points=patternGeometry(definition,'points',position),lines=patternGeometry(definition,'lines',position);
  const entities=choice('Pattern source curves',curves.map(id=>[id,id]),data.entities,true),role=kind==='mirror'?'axis':kind==='linear'?'direction':'center';
  const reference=choice(`Pattern ${role}`,(kind==='circular'?points:lines).map(id=>[id,id]),data[role]??(kind==='circular'?points:lines)[0]??'');
  let onAxis,count,driver,value,preserved;
  if(kind==='mirror'){
   onAxis=choice('Shared points on mirror axis',points.map(id=>[id,id]),data.on_axis??[],true);
   const hint=document.createElement('p');hint.textContent='Select source points that must be shared on the axis, including any axis endpoints used by mirrored curves. Native Preview rejects invalid axis selections.';form.append(hint);
  }else{
   count=input('Pattern count (including source)',data.count,'number');const key=kind==='linear'?'spacing':'angle';preserved=data[key];
   let source=preserved?.parameter?`parameter:${preserved.parameter}`:preserved?.literal?'literal':preserved?'preserve':kind==='circular'?'default':'literal';
   const choices=[['literal','Entered value'],...parameters.filter(p=>p.parameter_type?.scalar===(kind==='linear'?'length':'scalar')).map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
   if(kind==='circular')choices.push(['default','Equal full turn']);if(source==='preserve')choices.push(['preserve','Keep existing formula']);if(source.startsWith('parameter:')&&!choices.some(([id])=>id===source))choices.push([source,`Existing parameter: ${preserved.parameter}`]);
   driver=choice('Pattern value driver',choices,source);value=input(kind==='linear'?'Pattern spacing (mm)':'Pattern step angle (degrees)',dimensionNumber(preserved,kind==='linear'?'length':'angle')??(kind==='linear'?10:30),'number');
   driver.onchange=()=>{value.disabled=value.parentElement.hidden=driver.value!=='literal';};driver.onchange();
  }
  const selectedValues=e=>Array.from(e.selectedOptions,o=>o.value);
  const show=document.createElement('button');show.textContent='Highlight pattern sources';show.onclick=()=>highlight([...selectedValues(entities),reference.value]);form.append(show);
  const submit=document.createElement('button');submit.textContent=existing?'Update sketch pattern':'Add sketch pattern';submit.onclick=()=>{try{
   const fields={id:name.value,entities:selectedValues(entities),[role]:reference.value};
   if(kind==='mirror')fields.on_axis=selectedValues(onAxis);else{fields.count=Number(count.value);fields[kind==='linear'?'spacing':'angle']=driver.value==='default'?undefined:driver.value==='preserve'?preserved:driver.value.startsWith('parameter:')?{parameter:driver.value.slice(10)}:kind==='linear'?length(String(value.value).trim()?Number(value.value):NaN):scalar(String(value.value).trim()?Number(value.value)*Math.PI/180:NaN);}
   const next=makePattern(definition,kind,fields,existing?index:null);if(existing)definition.patterns[index]=next;else(definition.patterns??=[]).push(next);selected=fields.id;changed();message('Pattern updated in the draft. Preview to generate copies and refresh their references.');
  }catch(error){message(error.message);}};form.append(submit);
  if(existing){const remove=document.createElement('button');remove.textContent='Remove sketch pattern';remove.onclick=()=>{definition.patterns.splice(index,1);selected='';changed();message('Pattern removed from the draft. Preview checks dependent references.');};form.append(remove);}
 }
 select.onchange=()=>{selected=select.value;render();};render();
 return {get selected(){return selected;},get open(){return section.open;},inspect(expected){const pattern=patterns.find(p=>dataOf(p).id===dataOf(expected)?.id);if(!pattern||stamp(pattern)!==stamp(expected))return false;selected=dataOf(pattern).id;section.open=true;render();section.scrollIntoView?.({block:'nearest'});return true;}};
}
