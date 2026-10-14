export function projectionRefinement(kind,value,multiplicity=1){
  if(kind==='insert_knot'){
    if(!Number.isFinite(value)||value<=0||value>=1||!Number.isInteger(multiplicity)||multiplicity<1||multiplicity>25)throw new Error('Use a knot fraction inside (0, 1) and multiplicity 1–25.');
    return {insert_knot:{fraction:value,multiplicity}};
  }
  if(kind==='elevate_degree'){
    if(!Number.isInteger(value)||value<1||value>25)throw new Error('Use a minimum degree from 1 to 25.');
    return {elevate_degree:{degree:value}};
  }
  throw new Error('Choose projected knot insertion or degree elevation.');
}
export function withProjectionRefinements(projection,operations){
  if(!['spline','chain'].includes(projection.kind))throw new Error('Refine an exact spline or chain projection.');
  if(!Array.isArray(operations)||operations.length>32)throw new Error('Use at most 32 projected refinement operations.');
  const checked=operations.map(operation=>{
    if(!operation||typeof operation!=='object'||Object.keys(operation).length!==1)throw new Error('Enter one refinement operation.');
    const [kind,value]=Object.entries(operation)[0];return projectionRefinement(kind,kind==='insert_knot'?value?.fraction:value?.degree,value?.multiplicity??1);
  });
  const next=structuredClone(projection);if(checked.length)next.refinements=checked;else delete next.refinements;return next;
}
export function createProjectionRefinementControls({host,projection,changed,message,highlight=()=>{}}){
  const section=document.createElement('fieldset'),legend=document.createElement('legend');legend.textContent=`Refine projected spline ${projection.id}`;section.append(legend);host.append(section);
  const note=document.createElement('p');note.textContent='Refinements replay on fresh source geometry. Fractions use the imported curve domain. Degree and knot multiplicity are minimum targets; already-satisfied targets keep the source basis. Native transformations allow up to 256 poles.';section.append(note);
  const operations=projection.refinements??[];
  const number=(label,value)=>{const row=document.createElement('label');row.textContent=`${label} · ${projection.id}`;const input=document.createElement('input');input.type='number';input.step='any';input.value=String(value);input.setAttribute('aria-label',row.textContent);row.append(input);section.append(row);return input;};
  const fraction=number('Projected knot fraction',.5),multiplicity=number('Projected knot multiplicity',1),degree=number('Projected minimum degree',3);multiplicity.step=degree.step='1';
  const value=input=>String(input.value).trim()?Number(input.value):NaN;
  const button=(label,action)=>{const b=document.createElement('button');b.textContent=label;b.onclick=()=>{try{action();}catch(error){message(error.message);}};section.append(b);};
  const update=ops=>{changed(withProjectionRefinements(projection,ops));message('Projected refinement changed in the draft. Preview checks fresh source geometry, references and dependent solids.');};
  button(`Insert projected knot · ${projection.id}`,()=>update([...operations,projectionRefinement('insert_knot',value(fraction),value(multiplicity))]));
  button(`Elevate projected degree · ${projection.id}`,()=>update([...operations,projectionRefinement('elevate_degree',value(degree))]));
  for(const [index,operation]of operations.entries()){
    const text=document.createElement('p');text.textContent=`${index+1}. ${operation.insert_knot?`Knot fraction ${operation.insert_knot.fraction}, minimum multiplicity ${operation.insert_knot.multiplicity??1}`:`Minimum degree ${operation.elevate_degree.degree}`}`;section.append(text);
    button(`Remove projection refinement ${projection.id} ${index+1}`,()=>update(operations.filter((_,i)=>i!==index)));
  }
  if(operations.length)button(`Clear projected refinement · ${projection.id}`,()=>update([]));
  button(`Highlight projected curve · ${projection.id}`,()=>highlight([projection.id]));return section;
}
