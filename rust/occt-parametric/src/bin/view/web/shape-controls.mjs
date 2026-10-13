import {dimensionNumber} from './constraint-controls.mjs';

const tools={
  slot:{field:'radius',label:'Slot radius (mm)',dimension:'length',positive:true},
  polygon:{field:'sides',label:'Polygon sides',integer:true},
  rectangle:{field:'angle',label:'Rectangle angle (degrees)',dimension:'scalar',angle:true,optional:true,defaultLabel:'Axis aligned'},
  conic:{field:'rho',label:'Conic rho',dimension:'scalar',optional:true,defaultLabel:'Default rho (0.5)'},
};
export function shapeCurveIds(shape){
  const [kind,s]=Object.entries(shape)[0];
  if(kind==='rectangle')return Array.from({length:4},(_,i)=>`${s.id}:side${i}`);
  if(kind==='slot')return ['right','end_cap','left','start_cap'].map(key=>`${s.id}:${key}`);
  if(kind==='polygon')return [...Array.from({length:s.sides},(_,i)=>`${s.id}:side${i}`),`${s.id}:circle`];
  return [s.id];
}
export function editShapeParameter(shape,value){
  const [kind,data]=Object.entries(shape)[0],tool=tools[kind];
  if(!tool)throw new Error('Unsupported shape parameter.');
  if(tool.integer){
    if(!Number.isInteger(value)||value<3||value>1000)throw new Error('Polygon sides must be an integer from 3 to 1000.');
  }else if(value===undefined){
    if(!tool.optional)throw new Error('Enter a driving value.');
  }else{
    if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Enter a driving value.');
    if(value.literal){
      const number=tool.dimension==='length'?dimensionNumber(value,'length'):value.literal.dimension==='scalar'?value.literal.value:null;
      if(number===null||!Number.isFinite(number))throw new Error('Enter a finite value in the correct units.');
      if(tool.positive&&number<=0)throw new Error('Slot radius must be positive.');
      if(kind==='conic'&&(number<=0||number>=1))throw new Error('Conic rho must lie strictly between 0 and 1.');
    }
  }
  const result=structuredClone(data);
  if(value===undefined)delete result[tool.field];else result[tool.field]=structuredClone(value);
  return {[kind]:result};
}

export function createShapeControls({host,definition,parameters=[],changed,message,highlight=()=>{},beginPicking=()=>{},initialSelection=null}){
  const shapes=(definition.shapes??[]).filter(shape=>tools[Object.keys(shape)[0]]);
  let selected=shapes.some(s=>Object.values(s)[0].id===initialSelection)?initialSelection:shapes[0]&&Object.values(shapes[0])[0].id,picking=false;
  if(!shapes.length)return {get selected(){return null;},pick:()=>false,cancelPick(){}};
  const section=document.createElement('section'),heading=document.createElement('h3');heading.textContent='Shape parameters';
  const select=document.createElement('select');select.setAttribute('aria-label','Shape to edit');
  for(const shape of shapes){const [kind,data]=Object.entries(shape)[0];select.append(new Option(`${data.id} · ${kind}`,data.id));}
  const pick=document.createElement('button');pick.textContent='Select shape from canvas';pick.onclick=()=>{beginPicking();picking=true;message('Click a shortcut boundary curve to select its shape. Escape cancels.');};
  const form=document.createElement('div'),note=document.createElement('p');note.textContent='Updates keep the shape name and point references. Ellipse axes use point coordinates or dimensions. Preview checks generated references and dependent solids.';
  section.append(heading,select,pick,note,form);host.append(section);
  function render(){
    picking=false;select.value=selected;form.replaceChildren();
    const shape=shapes.find(s=>Object.values(s)[0].id===selected),[kind,data]=Object.entries(shape)[0],tool=tools[kind],original=structuredClone(data[tool.field]);
    let source=original?.parameter?`parameter:${original.parameter}`:original?.literal||tool.integer?'literal':original?'preserve':'default',edited=false;
    const initialSource=source;
    const number=tool.integer?original:tool.angle?dimensionNumber(original,'angle'):tool.dimension==='length'?dimensionNumber(original,'length'):original?.literal?.dimension==='scalar'?original.literal.value:null;
    const row=document.createElement('label');row.textContent=tool.label;
    const input=document.createElement('input');input.type='number';input.step=tool.integer?'1':'any';input.value=number??(kind==='conic'?.5:kind==='slot'?1:0);input.setAttribute('aria-label',tool.label);
    input.disabled=row.hidden=source!=='literal';input.oninput=()=>edited=true;row.append(input);
    if(!tool.integer){
      const label=document.createElement('label');label.textContent='Shape driving value';const driver=document.createElement('select');driver.setAttribute('aria-label','Shape driving value');
      const choices=[['literal','Entered value'],...parameters.filter(p=>p.parameter_type?.scalar===tool.dimension).map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
      if(tool.optional)choices.push(['default',tool.defaultLabel]);if(original&&!original.literal&&!original.parameter)choices.push(['preserve','Keep existing formula']);
      if(source.startsWith('parameter:')&&!choices.some(([id])=>id===source))choices.push([source,`Existing parameter: ${original.parameter}`]);
      for(const [id,text]of choices)driver.append(new Option(text,id));driver.value=source;
      driver.onchange=()=>{source=driver.value;input.disabled=row.hidden=source!=='literal';};label.append(driver);form.append(label);
    }
    form.append(row);
    if(initialSource==='preserve'){const hint=document.createElement('p');hint.textContent='The existing formula stays intact until you explicitly replace it.';form.append(hint);}
    const update=document.createElement('button');update.textContent='Update shape parameter';
    update.onclick=()=>{try{
      const number=String(input.value).trim()?Number(input.value):NaN;
      const value=tool.integer?number:source==='default'?undefined:source==='preserve'||source===initialSource&&!edited?original:source.startsWith('parameter:')?{parameter:source.slice(10)}:{literal:{value:tool.angle?number*Math.PI/180:number,dimension:tool.dimension,unit:tool.dimension==='length'?'millimeter':null}};
      const next=editShapeParameter(shape,value),index=definition.shapes.indexOf(shape);
      if(index<0)throw new Error('Shape changed. Reload its controls.');
      definition.shapes[index]=next;changed();message('Shape parameter changed in the draft. Preview checks generated geometry and references.');
    }catch(error){message(error.message);}};form.append(update);
  }
  select.onchange=()=>{selected=select.value;render();highlight(shapeCurveIds(shapes.find(s=>Object.values(s)[0].id===selected)));};render();
  return {
    get selected(){return selected;},
    cancelPick(){if(picking){picking=false;message('Shape pick canceled. Form retained.');}},
    pick(id){
      if(!picking)return false;
      const shape=shapes.find(s=>shapeCurveIds(s).includes(id));
      if(!shape){message('Choose a shortcut boundary curve.');return true;}
      selected=Object.values(shape)[0].id;render();highlight(shapeCurveIds(shape));heading.scrollIntoView?.({block:'nearest'});message(`Selected shape ${selected}.`);return true;
    },
  };
}
