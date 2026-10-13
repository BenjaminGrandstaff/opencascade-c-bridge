// Profiles are authored in local coordinates; native geometry resolves support.
const quantity = (value, dimension) => ({value,dimension,unit:dimension==='length'?'millimeter':null});
const scalar = value => ({literal:quantity(value,'scalar')});
const length = value => ({literal:quantity(value,'length')});
const vector = (values,dimension='scalar') => ({literal:Object.fromEntries(['x','y','z'].map((key,i)=>[key,quantity(values[i],dimension)]))});
const faces={'+z':[0,0,1],'-z':[0,0,-1],'+x':[1,0,0],'-x':[-1,0,0],'+y':[0,1,0],'-y':[0,-1,0]};
export function newSketchDefinition({id,shape='rectangle',width=20,height=12,plane='xy',offset=0,support='',face='+z'}) {
  if(!id?.trim())throw Error('Enter a sketch name.');
  if(!Number.isFinite(width)||width<=0||!Number.isFinite(offset)||(shape==='rectangle'&&(!Number.isFinite(height)||height<=0)))throw Error('Dimensions must be positive and offset must be finite.');
  if(!['rectangle','circle'].includes(shape))throw Error('Choose rectangle or circle.');
  let x=[1,0,0],y=[0,1,0],origin=[0,0,offset];
  if(plane==='xz'){y=[0,0,1];origin=[0,-offset,0];}
  else if(plane==='yz'){x=[0,1,0];y=[0,0,1];origin=[offset,0,0];}
  else if(!['xy','face'].includes(plane))throw Error('Choose a supported sketch plane.');
  let face_support;
  if(plane==='face'){
    if(!support||!faces[face])throw Error('Choose a support feature and face normal.');
    x=face.endsWith('x')?[0,1,0]:[1,0,0];origin=[0,0,0];
    const n=faces[face];y=[n[1]*x[2]-n[2]*x[1],n[2]*x[0]-n[0]*x[2],n[0]*x[1]-n[1]*x[0]];
    face_support={input:support,face:{normal_aligned:{direction:vector(faces[face]),minimum_dot:scalar(0.999999)}},offset:length(offset)};
  }
  const point=(id,x,y)=>({id,x:length(x),y:length(y),fixed:true});
  const definition={id,acceptance:'solved',origin:vector(origin,'length'),x_axis:vector(x),y_axis:vector(y),points:[],lines:[],circles:[],profile:[],constraints:[]};
  if(face_support)definition.face_support=face_support;
  if(shape==='circle'){
    definition.points=[point('C',0,0),point('R',width/2,0)];
    definition.circles=[{id:'circle',center:'C',rim:'R'}];definition.profile=['circle'];
  }else{
    definition.points=[point('A',-width/2,-height/2),point('B',width/2,-height/2),point('C',width/2,height/2),point('D',-width/2,height/2)];
    definition.lines=[['AB','A','B'],['BC','B','C'],['CD','C','D'],['DA','D','A']].map(([id,start,end])=>({id,start,end}));
    definition.profile=['AB','BC','CD','DA'];
  }
  return definition;
}

export function createSketchCreator({host,request,instance,preview,applied,restore,showSketch=()=>{}}){
  let data=null,busy=false,values;
  const section=document.createElement('section');section.className='linked-edit sketch-create';
  const launch=document.createElement('button');launch.textContent='New sketch';
  const content=document.createElement('div'),status=document.createElement('p');status.setAttribute('role','status');
  section.append(launch,content,status);host.prepend(section);
  const message=text=>status.textContent=text;
  function button(text,action){const e=document.createElement('button');e.textContent=text;e.onclick=async()=>{if(busy)return;try{await action();}catch(error){message(error.message);}};return e;}
  function field(label,type,value,changed){
    const row=document.createElement('label');row.textContent=label;
    const e=document.createElement('input');e.type=type;e.value=value;e.setAttribute('aria-label',label);if(type==='number')e.step='any';
    e.oninput=()=>{changed(type==='number'?(e.value.trim()?Number(e.value):NaN):e.value);message('New sketch draft changed. Preview before applying.');};
    row.append(e);content.append(row);return e;
  }
  function select(label,choices,value,changed){
    const row=document.createElement('label');row.textContent=label;const e=document.createElement('select');e.setAttribute('aria-label',label);
    for(const [key,text] of choices)e.append(new Option(text,key));e.value=value;e.onchange=()=>{changed(e.value);render();};row.append(e);content.append(row);
  }
  function render(){
    content.replaceChildren();
    const note=document.createElement('p');note.textContent='Family-wide profile centered at local zero. Starter points are fixed; edit geometry or add dimensions after creation. A face support follows its area center and oriented normal. It must resolve to exactly one planar face.';content.append(note);
    field('New sketch feature name','text',values.id,value=>values.id=value);
    select('Starter profile',[['rectangle','Rectangle'],['circle','Circle']],values.shape,value=>values.shape=value);
    field(values.shape==='circle'?'Diameter (mm)':'Width (mm)','number',values.width,value=>values.width=value);
    if(values.shape==='rectangle')field('Height (mm)','number',values.height,value=>values.height=value);
    select('Sketch plane',[['xy','XY (normal +Z)'],['xz','XZ (normal −Y)'],['yz','YZ (normal +X)'],['face','Planar face']],values.plane,value=>values.plane=value);
    if(values.plane==='face'){
      select('Support feature',data.features.map(f=>[f.id,f.id]),values.support,value=>values.support=value);
      select('Face normal',[['+z','+Z'],['-z','−Z'],['+x','+X'],['-x','−X'],['+y','+Y'],['-y','−Y']],values.face,value=>values.face=value);
    }
    field('Offset along plane normal (mm)','number',values.offset,value=>values.offset=value);
    content.append(button('Preview new sketch',()=>submit(false)),button('Apply verified new sketch',()=>submit(true)),button('Close new sketch draft',async()=>{data=null;content.replaceChildren();await restore();message('New sketch draft closed.');}));
  }
  async function submit(apply){
    const definition=newSketchDefinition(values);
    if(data.features.some(f=>f.id===values.id))throw Error('That feature name already exists.');
    busy=true;launch.disabled=true;content.inert=true;message('Resolving support, regenerating all variants and checking requirements…');
    try{
      const response=await request('/api/sketch-create',{method:'POST',body:JSON.stringify({version:data.version,instance:instance(),expected:data.features,feature:values.id,definition,apply})});
      if(apply){data=null;content.replaceChildren();await applied(response.state);showSketch(response.feature);message('Sketch created. Use Edit sketch geometry to draw or constrain it, or Sketch to solid to extrude it. Save writes the feature graph.');}
      else{if(response.annotations){await preview(response.annotations);showSketch(values.id);}message(response.can_apply?'New sketch preview passes. Apply repeats all checks.':`New sketch rejected: ${response.error}`);}
    }finally{busy=false;launch.disabled=false;content.inert=false;}
  }
  launch.onclick=async()=>{
    if(busy)return;
    try{data=await request('/api/features');let id='new-profile';for(let i=2;data.features.some(f=>f.id===id);i++)id=`new-profile-${i}`;
      values={id,shape:'rectangle',width:20,height:12,plane:'xy',offset:0,support:data.output??data.features.at(-1)?.id??'',face:'+z'};render();message('Choose a plane or planar support face.');}
    catch(error){message(error.message);}
  };
  return section;
}
