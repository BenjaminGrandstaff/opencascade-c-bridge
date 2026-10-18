// Sheet-metal parts in the solid editor: the engine's sheet_metal strip
// (flanges and bends, hems, edge flanges with reliefs, cutouts) and its
// linked sheet_metal_flat blank. Every value is entered, linked to a
// parameter, or kept as its saved formula; unchanged values write back in
// their saved form.
const quantity=(value,dimension='length',unit='millimeter')=>({literal:{value,dimension,unit}});
const mm=q=>q?.dimension==='length'?q.value*({millimeter:1,centimeter:10,meter:1000,inch:25.4}[q.unit]??NaN):NaN;
const DEGREE=Math.PI/180;
const KINDS={
  length:{read:mm,make:v=>quantity(v),dimension:'length',unit:'mm'},
  angle:{read:q=>q?.dimension==='scalar'?q.value/DEGREE:NaN,make:v=>quantity(v*DEGREE,'scalar',null),dimension:'scalar',unit:'degrees'},
  factor:{read:q=>q?.dimension==='scalar'?q.value:NaN,make:v=>quantity(v,'scalar',null),dimension:'scalar',unit:''},
};

/** A value's editor state: entered, a parameter, or a kept formula. */
function field(saved,kind,fallback) {
  const k=KINDS[kind];
  if(!saved)return {kind,source:'literal',value:fallback,saved:null,original:null};
  if(saved.parameter)return {kind,source:`parameter:${saved.parameter}`,value:fallback,saved:null,original:null};
  const value=saved.literal?k.read(saved.literal):NaN;
  return Number.isFinite(value)?{kind,source:'literal',value,saved:null,original:{expression:saved,value}}:{kind,source:'preserve',value:fallback,saved,original:null};
}
function expression(f,what,check=v=>v>0) {
  if(f.source==='preserve')return structuredClone(f.saved);
  if(f.source.startsWith('parameter:'))return {parameter:f.source.slice(10)};
  if(!Number.isFinite(f.value)||!check(f.value))throw Error(`Enter a valid ${what}.`);
  if(f.original&&f.original.value===f.value)return structuredClone(f.original.expression);
  return KINDS[f.kind].make(f.value);
}
function readVector(value,dimension) {
  const values=['x','y','z'].map(key=>value?.literal?.[key]??value?.components?.[key]?.literal);
  return values.every(v=>v?.dimension===dimension)?values.map(v=>dimension==='length'?mm(v):v.value):null;
}
function vectorState(saved,dimension,fallback) {
  const values=saved?readVector(saved,dimension):null;
  return {dimension,values:values??[...fallback],saved:saved&&!values?saved:null,original:values?{expression:saved,values:[...values]}:null};
}
function vectorExpression(v,what) {
  if(v.saved)return structuredClone(v.saved);
  if(v.values.length!==3||!v.values.every(Number.isFinite))throw Error(`Enter a finite ${what}.`);
  if(v.dimension==='scalar'&&Math.hypot(...v.values)===0)throw Error(`Enter a nonzero ${what}.`);
  if(v.original&&v.original.values.every((x,i)=>x===v.values[i]))return structuredClone(v.original.expression);
  return {components:Object.fromEntries(['x','y','z'].map((key,i)=>[key,quantity(v.values[i],v.dimension,v.dimension==='length'?'millimeter':null)]))};
}

const hemFields=(hem)=>({
  kind:hem?.kind??'none',side:hem?.side??'positive',
  gap:field(hem?.gap,'length',1),length:field(hem?.length,'length',8),
  radius:field(hem?.inside_radius,'length',1),angle:field(hem?.angle_radians,'angle',hem?.kind==='teardrop'?290:270),
});
function hemDefinition(h,where) {
  if(h.kind==='none')return undefined;
  const side=h.side;
  if(h.kind==='open')return {kind:'open',side,gap:expression(h.gap,`${where} hem gap`),length:expression(h.length,`${where} hem length`)};
  if(h.kind==='closed')return {kind:'closed',side,length:expression(h.length,`${where} hem length`)};
  return {kind:h.kind,side,inside_radius:expression(h.radius,`${where} hem radius`),angle_radians:expression(h.angle,`${where} hem angle (degrees)`,v=>v>180&&v<360)};
}

/** Editor state for a sheet-metal part (the folded feature's definition)
 * and its linked flat blank, if any. */
export function readSheetMetal(definition,flat=null) {
  const d=definition??{};
  const flanges=d.flanges??[quantity(40),quantity(30)];
  const bends=d.bends??[{angle_radians:quantity(Math.PI/2,'scalar',null),inside_radius:quantity(2)}];
  return {
    origin:vectorState(d.origin,'length',[0,0,0]),start:vectorState(d.start_direction,'scalar',[1,0,0]),widthAxis:vectorState(d.width_axis,'scalar',[0,1,0]),
    width:field(d.width??quantity(30),'length',30),thickness:field(d.thickness??quantity(2),'length',2),
    flanges:flanges.map(f=>field(f,'length',20)),
    bends:bends.map(b=>({angle:field(b.angle_radians,'angle',90),radius:field(b.inside_radius,'length',2)})),
    startHem:hemFields(d.start_hem),endHem:hemFields(d.end_hem),
    edges:(d.edge_flanges??[]).map(e=>({id:e.id,flange:e.flange,edge:e.edge,start:field(e.start,'length',0),length:field(e.length,'length',20),
      angle:field(e.angle_radians,'angle',90),radius:field(e.inside_radius,'length',1),height:field(e.height,'length',10),
      relief:e.relief_depth?field(e.relief_depth,'length',3):null})),
    cutouts:(d.cutouts??[]).map(c=>({id:c.id,flange:c.flange,along:field(c.along,'length',10),across:field(c.across,'length',10),shape:c.shape?.kind??'circle',
      diameter:field(c.shape?.diameter,'length',4),length:field(c.shape?.length,'length',8),width:field(c.shape?.width,'length',4)})),
    flat:{enabled:Boolean(flat)||!definition,existing:flat?.id??null,id:flat?.id??'',k:field(flat?.operation?.sheet_metal_flat?.neutral_factor??quantity(0.44,'scalar',null),'factor',0.44),
      table:flat?.operation?.sheet_metal_flat?.bend_table??null},
  };
}

/** The sheet_metal operation and, when wanted, its flat blank operation. */
export function sheetMetalOperations(state,id) {
  const definition={
    origin:vectorExpression(state.origin,'sheet origin'),width_axis:vectorExpression(state.widthAxis,'width direction'),start_direction:vectorExpression(state.start,'start direction'),
    width:expression(state.width,'sheet width'),thickness:expression(state.thickness,'sheet thickness'),
    flanges:state.flanges.map((f,i)=>expression(f,`flange ${i+1} length`)),
    bends:state.bends.map((b,i)=>({angle_radians:expression(b.angle,`bend ${i+1} angle (between −180° and 180°, not 0)`,v=>v!==0&&Math.abs(v)<180),inside_radius:expression(b.radius,`bend ${i+1} inside radius`)})),
  };
  if(state.flanges.length<1||state.bends.length!==state.flanges.length-1)throw Error('A sheet needs one bend between each pair of flanges.');
  const ids=new Set();
  const unique=(key,what)=>{if(!key?.trim()||ids.has(key))throw Error(`${what} need unique names.`);ids.add(key);};
  if(state.cutouts.length)definition.cutouts=state.cutouts.map(c=>{
    unique(c.id,'Cutouts');
    const shape=c.shape==='circle'?{kind:'circle',diameter:expression(c.diameter,`cutout ${c.id} diameter`)}:{kind:c.shape,length:expression(c.length,`cutout ${c.id} length`),width:expression(c.width,`cutout ${c.id} width`)};
    return {id:c.id,flange:c.flange,along:expression(c.along,`cutout ${c.id} position along`,Number.isFinite),across:expression(c.across,`cutout ${c.id} position across`),shape};
  });
  const start=hemDefinition(state.startHem,'start'),end=hemDefinition(state.endHem,'end');
  if(start)definition.start_hem=start;
  if(end)definition.end_hem=end;
  ids.clear();
  if(state.edges.length)definition.edge_flanges=state.edges.map(e=>{
    unique(e.id,'Edge flanges');
    if(!(Number.isInteger(e.flange)&&e.flange>=0&&e.flange<state.flanges.length))throw Error(`Edge flange ${e.id} names a flange the sheet does not have.`);
    const edge={id:e.id,flange:e.flange,edge:e.edge,start:expression(e.start,`edge flange ${e.id} start`,v=>v>=0),length:expression(e.length,`edge flange ${e.id} length`),
      angle_radians:expression(e.angle,`edge flange ${e.id} angle (between −180° and 180°, not 0)`,v=>v!==0&&Math.abs(v)<180),inside_radius:expression(e.radius,`edge flange ${e.id} radius`),height:expression(e.height,`edge flange ${e.id} height`)};
    if(e.relief)edge.relief_depth=expression(e.relief,`edge flange ${e.id} relief depth`);
    return edge;
  });
  const sheet={sheet_metal:{definition}};
  if(!state.flat.enabled)return {sheet};
  const flatId=state.flat.existing??(state.flat.id?.trim()||`${id}-flat`);
  if(flatId===id)throw Error('Name the flat blank feature, different from the sheet.');
  const flat={id:flatId,operation:{sheet_metal_flat:{input:id,neutral_factor:expression(state.flat.k,'neutral factor K (0 to 1)',v=>v>=0&&v<=1),...(state.flat.table?{bend_table:structuredClone(state.flat.table)}:{})}}};
  return {sheet,flat};
}

/** The rows after the editor's name row. */
export function renderSheetMetal({state,parameters,select,number,button,content,render,invalidate}) {
  const note=document.createElement('p');
  note.textContent='A strip of flanges joined by bends, extruded across its width from the origin along the start direction. Positive bend angles turn toward width × start. Lengths are on the midplane between bends; the flat blank unfolds it at the neutral factor K.';
  content.append(note);
  const heading=text=>{const h=document.createElement('h4');h.textContent=text;content.append(h);};
  const rows=(label,f)=>{
    const list=parameters.filter(p=>p.parameter_type?.scalar===KINDS[f.kind].dimension);
    if(list.length||f.source!=='literal'){
      const choices=[['literal','Entered'],...list.map(p=>[`parameter:${p.id}`,`Parameter: ${p.id}`])];
      if(f.source.startsWith('parameter:')&&!choices.some(([key])=>key===f.source))choices.push([f.source,`Existing parameter: ${f.source.slice(10)}`]);
      if(f.saved)choices.push(['preserve','Keep existing formula']);
      select(`${label} source`,choices,f.source,value=>{f.source=value;render();});
    }
    const unit=KINDS[f.kind].unit;
    number(unit?`${label} (${unit})`:label,f.value,value=>f.value=value).disabled=f.source!=='literal';
  };
  const vectorRows=(label,v)=>{
    if(v.saved){const kept=document.createElement('p');kept.textContent=`${label} keeps its formula.`;content.append(kept,button(`Enter ${label.toLowerCase()} instead`,()=>{v.saved=null;invalidate();render();}));return;}
    for(const [i,key] of ['X','Y','Z'].entries())number(`${label} ${key}${v.dimension==='length'?' (mm)':''}`,v.values[i],value=>v.values[i]=value);
  };
  heading('Placement and stock');
  vectorRows('Sheet origin',state.origin);vectorRows('Start direction',state.start);vectorRows('Width direction',state.widthAxis);
  rows('Sheet width',state.width);rows('Sheet thickness',state.thickness);
  heading('Flanges and bends');
  state.flanges.forEach((f,i)=>{
    rows(`Flange ${i+1} length`,f);
    if(i<state.bends.length){rows(`Bend ${i+1} angle`,state.bends[i].angle);rows(`Bend ${i+1} inside radius`,state.bends[i].radius);}
  });
  content.append(button('Add flange and bend',()=>{state.bends.push({angle:field(null,'angle',90),radius:field(null,'length',2)});state.flanges.push(field(null,'length',20));invalidate();render();}));
  if(state.flanges.length>1)content.append(button('Remove last flange and bend',()=>{state.flanges.pop();state.bends.pop();invalidate();render();}));
  for(const [where,hem] of [['Start',state.startHem],['End',state.endHem]]){
    heading(`${where} hem`);
    select(`${where} hem`,[['none','None'],['open','Open'],['closed','Closed (flat)'],['rolled','Rolled'],['teardrop','Teardrop']],hem.kind,value=>{hem.kind=value;if(value==='teardrop'&&hem.angle.source==='literal'&&hem.angle.value===270)hem.angle.value=290;render();});
    if(hem.kind==='none')continue;
    select(`${where} hem side`,[['positive','Positive side'],['negative','Negative side']],hem.side,value=>hem.side=value);
    if(hem.kind==='open')rows(`${where} hem gap`,hem.gap);
    if(hem.kind==='open'||hem.kind==='closed')rows(`${where} hem length`,hem.length);
    if(hem.kind==='rolled'||hem.kind==='teardrop'){rows(`${where} hem inside radius`,hem.radius);rows(`${where} hem angle`,hem.angle);}
  }
  heading('Edge flanges');
  state.edges.forEach((e,i)=>{
    select(`Edge flange ${i+1} base flange`,state.flanges.map((_,k)=>[String(k),`Flange ${k+1}`]),String(e.flange),value=>e.flange=Number(value));
    select(`Edge flange ${i+1} edge`,[['far','Far width edge'],['origin','Origin width edge']],e.edge,value=>e.edge=value);
    rows(`Edge flange ${i+1} start`,e.start);rows(`Edge flange ${i+1} length`,e.length);rows(`Edge flange ${i+1} angle`,e.angle);
    rows(`Edge flange ${i+1} inside radius`,e.radius);rows(`Edge flange ${i+1} height`,e.height);
    select(`Edge flange ${i+1} relief`,[['none','No relief'],['relief','Relieve the bends it reaches']],e.relief?'relief':'none',value=>{e.relief=value==='relief'?field(null,'length',3):null;render();});
    if(e.relief)rows(`Edge flange ${i+1} relief depth`,e.relief);
    content.append(button(`Remove edge flange ${i+1}`,()=>{state.edges.splice(i,1);invalidate();render();}));
  });
  content.append(button('Add edge flange',()=>{state.edges.push({id:`edge-${state.edges.length+1}`,flange:0,edge:'far',start:field(null,'length',0),length:field(null,'length',10),angle:field(null,'angle',90),radius:field(null,'length',1),height:field(null,'length',10),relief:null});invalidate();render();}));
  heading('Cutouts');
  state.cutouts.forEach((c,i)=>{
    select(`Cutout ${i+1} flange`,state.flanges.map((_,k)=>[String(k),`Flange ${k+1}`]),String(c.flange),value=>c.flange=Number(value));
    select(`Cutout ${i+1} shape`,[['circle','Circle'],['rectangle','Rectangle'],['slot','Slot']],c.shape,value=>{c.shape=value;render();});
    rows(`Cutout ${i+1} along`,c.along);rows(`Cutout ${i+1} across`,c.across);
    if(c.shape==='circle')rows(`Cutout ${i+1} diameter`,c.diameter);else{rows(`Cutout ${i+1} length`,c.length);rows(`Cutout ${i+1} width`,c.width);}
    content.append(button(`Remove cutout ${i+1}`,()=>{state.cutouts.splice(i,1);invalidate();render();}));
  });
  content.append(button('Add cutout',()=>{state.cutouts.push({id:`cutout-${state.cutouts.length+1}`,flange:0,along:field(null,'length',10),across:field(null,'length',10),shape:'circle',diameter:field(null,'length',4),length:field(null,'length',8),width:field(null,'length',4)});invalidate();render();}));
  heading('Flat blank');
  if(state.flat.existing){const kept=document.createElement('p');kept.textContent=`Flat blank ${state.flat.existing} follows this sheet.${state.flat.table?' Its bend table is kept.':''}`;content.append(kept);}
  else select('Flat blank',[['add','Add a linked flat blank'],['none','No flat blank']],state.flat.enabled?'add':'none',value=>{state.flat.enabled=value==='add';render();});
  if(state.flat.enabled){
    if(!state.flat.existing){
      const row=document.createElement('label');row.textContent='Flat blank feature name';const input=document.createElement('input');input.value=state.flat.id;input.setAttribute('aria-label','Flat blank feature name');input.oninput=()=>{invalidate();state.flat.id=input.value;};row.append(input);content.append(row);
    }
    rows('Neutral factor K',state.flat.k);
  }
}
