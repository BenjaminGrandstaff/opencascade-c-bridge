import {geometryIds} from './constraint-controls.mjs';
let sourceListId=0;
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
// O(G + T) validation and temporary space for G known points and <=256 terms.
export function validateWeightedTerms(definition,target,terms){
 const sources=new Set(geometryIds(definition,'points'));
 if(!Array.isArray(terms)||!terms.length||terms.length>256)throw new Error('Use 1 to 256 weighted source terms.');
 for(const term of terms){
  if(!sources.has(term?.point)||term.point===target)throw new Error('Choose a known source point separate from the target.');
  const weight=term.weight;
  if(!weight||typeof weight!=='object'||Array.isArray(weight))throw new Error('Choose a weight driver.');
  if(weight.literal){const q=weight.literal;if(q.dimension!=='scalar'||q.unit!=null||!Number.isFinite(q.value)||q.value<=0)throw new Error('Weights must be positive finite scalars.');}
 }
 return structuredClone(terms);
}
// Source rows share one datalist: O(G + T) controls, avoiding T copies of G source options.
// Scalar-parameter menus are bounded by 256 rows; native Preview validates their values.
export function createWeightedTermControls({host,definition,parameters=[],message}){
 const section=document.createElement('div');host.append(section);
 const note=document.createElement('p');note.textContent='The target follows sum(weight × source) / sum(weight). Equal weights give an average or midpoint. Weights must be positive scalars. Preview checks expression values, cycles and dependent sources.';
 const sources=document.createElement('datalist');sources.id=`weighted-source-options-${++sourceListId}`;
 const list=document.createElement('div'),add=document.createElement('button');add.textContent='Add weighted source';section.append(note,sources,list,add);
 let target='',terms=[];
 function render(){
  list.replaceChildren();
  for(const [index,term]of terms.entries()){
   const row=document.createElement('fieldset'),title=document.createElement('legend');title.textContent=`Weighted source ${index+1}`;row.append(title);
   function select(label,choices,value,change){const l=document.createElement('label');l.textContent=label;const e=document.createElement('select');e.setAttribute('aria-label',label);for(const [id,text]of choices)e.append(new Option(text,id));e.value=value;e.onchange=()=>change(e.value);l.append(e);row.append(l);return e;}
   const sourceLabel=document.createElement('label');sourceLabel.textContent=`Weighted source point ${index+1}`;
   const source=document.createElement('input');source.setAttribute('aria-label',sourceLabel.textContent);source.setAttribute('list',sources.id);source.value=term.point;source.oninput=()=>term.point=source.value;sourceLabel.append(source);row.append(sourceLabel);
   const preserved=structuredClone(term.weight);
   let driver=preserved?.literal?'literal':preserved?.parameter?`parameter:${preserved.parameter}`:'preserve';
   const choices=[['literal','Entered weight'],...parameters.filter(p=>p.parameter_type?.scalar==='scalar').map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
   if(preserved&&!preserved.literal&&!preserved.parameter)choices.push(['preserve','Keep existing formula']);
   if(driver.startsWith('parameter:')&&!choices.some(([id])=>id===driver))choices.push([driver,`Existing parameter: ${preserved.parameter}`]);
   const label=document.createElement('label');label.textContent=`Source weight ${index+1}`;const value=document.createElement('input');value.type='number';value.step='any';value.value=preserved?.literal?.value??1;value.setAttribute('aria-label',label.textContent);value.disabled=label.hidden=driver!=='literal';
   select(`Weight driver ${index+1}`,choices,driver,next=>{driver=next;value.disabled=label.hidden=next!=='literal';term.weight=next==='preserve'?structuredClone(preserved):next.startsWith('parameter:')?{parameter:next.slice(10)}:scalar(String(value.value).trim()?Number(value.value):NaN);});
   value.oninput=()=>term.weight=scalar(String(value.value).trim()?Number(value.value):NaN);label.append(value);row.append(label);
   const remove=document.createElement('button');remove.textContent=`Remove weighted source ${index+1}`;remove.onclick=()=>{terms.splice(index,1);render();};row.append(remove);list.append(row);
  }
  add.disabled=terms.length>=256;
 }
 add.onclick=()=>{if(terms.length>=256){message('Use at most 256 weighted source terms.');return;}const ids=geometryIds(definition,'points').filter(id=>id!==target),used=new Set(terms.map(t=>t.point));terms.push({point:ids.find(id=>!used.has(id))??ids[0]??'',weight:scalar(1)});render();};
 return {get terms(){return structuredClone(terms);},set hidden(value){section.hidden=value;},load(point,existing){target=point;terms=structuredClone(existing??[]);sources.replaceChildren();for(const id of geometryIds(definition,'points'))if(id!==target)sources.append(new Option(id,id));render();}};
}
