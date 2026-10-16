import {geometryNames} from './geometry-names.mjs';
import {geometryIds} from './constraint-controls.mjs';
const literal = value => ({literal:{value,dimension:'length',unit:'millimeter'}});
export function nextId(definition, prefix) {
  const used=geometryNames(definition);
  let i=1;while(used.has(`${prefix}${i}`))i++;return `${prefix}${i}`;
}
export function snapPosition(position, definition, solved={}, grid=1, tolerance=0, preferredPoint=null) {
  if (!position.every(Number.isFinite) || !Number.isFinite(grid) || grid<0) throw new Error('Use finite coordinates and a nonnegative grid spacing.');
  let best=null;
  const authored=new Map((definition.points??[]).map(p=>[p.id,p]));
  for(const id of geometryIds(definition,'points')){
    const point=authored.get(id);
    const xy=solved[id] ?? (point?.x?.literal?.unit==='millimeter'&&point?.y?.literal?.unit==='millimeter'?[point.x.literal.value,point.y.literal.value]:null);
    if(!Array.isArray(xy)||xy.length<2||(!Number.isFinite(xy[0])||!Number.isFinite(xy[1])))continue;const distance=Math.hypot(xy[0]-position[0],xy[1]-position[1]);
    if(distance<=tolerance && (!best||id===preferredPoint||(best.id!==preferredPoint&&distance<best.distance)))best={id,xy:xy.slice(0,2),distance};
  }
  return best?{point:best.id,xy:best.xy}:{point:null,xy:position.map(value=>grid?Math.round(value/grid)*grid:value)};
}
export function arcEnd(clicks) {
  const [center,start,end]=clicks;
  if(!end)throw new Error('Choose center, start, then end direction.');
  const radius=Math.hypot(start.xy[0]-center.xy[0],start.xy[1]-center.xy[1]),distance=Math.hypot(end.xy[0]-center.xy[0],end.xy[1]-center.xy[1]);
  if(radius<=1e-9||distance<=1e-9)throw new Error('An arc needs a positive radius and end direction.');
  if(end.point&&Math.abs(radius-distance)>1e-6)throw new Error('The picked endpoint must lie on the arc radius. Choose an unsnapped end direction instead.');
  const xy=end.point?end.xy:[center.xy[0]+(end.xy[0]-center.xy[0])*radius/distance,center.xy[1]+(end.xy[1]-center.xy[1])*radius/distance];
  if(Math.hypot(xy[0]-start.xy[0],xy[1]-start.xy[1])<=1e-9)throw new Error('Arc endpoints must be distinct. Use Circle for a full circle.');
  return {point:end.point,xy};
}
function axis(clicks){
  const [a,b,c]=clicks;if(!c)throw new Error('Choose the axis endpoints, then the width.');
  const dx=b.xy[0]-a.xy[0],dy=b.xy[1]-a.xy[1],radius=Math.hypot(dx,dy);
  if(radius<=1e-9)throw new Error('Choose distinct axis endpoints.');
  const u=[dx/radius,dy/radius],v=[-u[1],u[0]],offset=[c.xy[0]-a.xy[0],c.xy[1]-a.xy[1]],across=offset[0]*v[0]+offset[1]*v[1];
  if(Math.abs(across)<=1e-9)throw new Error('Choose a positive width away from the axis.');
  return {u,v,radius,across,along:offset[0]*u[0]+offset[1]*u[1]};
}
export function ellipseMinor(clicks){
  const {v,radius,across,along}=axis(clicks),[center,,minor]=clicks;
  if(Math.abs(across)>radius+1e-9)throw new Error('The minor radius must not exceed the major radius.');
  if(minor.point&&Math.abs(along)>1e-9)throw new Error('The picked minor-axis point must lie perpendicular to the major axis. Choose an unsnapped width instead.');
  const width=Math.sign(across)*Math.min(Math.abs(across),radius);
  return {point:minor.point,xy:minor.point?minor.xy:[center.xy[0]+v[0]*width,center.xy[1]+v[1]*width]};
}
export function drawPrimitive(definition, kind, clicks, {fixed=true, profile=false,clockwise=false,closed=false,sides=6}={}) {
  const reusablePoints=new Set(geometryIds(definition,'points'));
  const candidate=structuredClone(definition),created=[];
  function point(click){
    if(click.point && reusablePoints.has(click.point))return click.point;
    const id=nextId(candidate,'P');(candidate.points??=[]).push({id,x:literal(click.xy[0]),y:literal(click.xy[1]),fixed});reusablePoints.add(id);created.push(id);return id;
  }
  function line(start,end){const id=nextId(candidate,'L');(candidate.lines??=[]).push({id,start,end});return id;}
  if(clicks.some(click=>!click.xy.every(Number.isFinite)))throw new Error('Drawing coordinates must be finite.');
  if(kind==='point'){point(clicks[0]);return {definition:candidate,created};}
  const [a,b]=clicks;
  if(!b || Math.hypot(a.xy[0]-b.xy[0],a.xy[1]-b.xy[1])<=1e-9)throw new Error('Choose two distinct locations.');
  let boundary=[];
  if(kind==='line'){const start=point(a),end=point(b);if(start===end)throw new Error('A line needs distinct endpoints.');boundary=[line(start,end)];}
  else if(kind==='ellipse'){
    const minorClick=ellipseMinor(clicks),center=point(a),major=point(b),minor=point(minorClick);
    if(new Set([center,major,minor]).size!==3)throw new Error('Ellipse center and axis points must be distinct.');
    const id=nextId(candidate,'E');(candidate.ellipses??=[]).push({id,center,major,minor});boundary=[id];
  }
  else if(kind==='slot'){
    const radius=Math.abs(axis(clicks).across),start=point(a),end=point(b),id=nextId(candidate,'Slot');
    if(start===end)throw new Error('A slot needs distinct cap centers.');
    (candidate.shapes??=[]).push({slot:{id,start,end,radius:literal(radius)}});boundary=[id];
  }
  else if(kind==='polygon'){
    if(!Number.isInteger(sides)||sides<3||sides>1000)throw new Error('A regular polygon needs 3 to 1000 integer sides.');
    const center=point(a),vertex=point(b),id=nextId(candidate,'Polygon');
    if(center===vertex)throw new Error('A polygon needs a positive circumradius.');
    (candidate.shapes??=[]).push({polygon:{id,center,vertex,sides}});boundary=[id];
  }
  else if(kind==='arc'){
    const endClick=arcEnd(clicks),center=point(a),start=point(b),end=point(endClick);
    if(new Set([center,start,end]).size!==3)throw new Error('Arc center and endpoints must be distinct.');
    const id=nextId(candidate,'A');(candidate.arcs??=[]).push({id,center,start,end,clockwise});boundary=[id];
  }
  else if(kind==='spline'){
    if(clicks.length<(closed?3:2))throw new Error(`A ${closed?'closed':'open'} spline needs at least ${closed?3:2} distinct points.`);
    for(let i=0;i<clicks.length;i++)for(let j=0;j<i;j++)if(Math.hypot(clicks[i].xy[0]-clicks[j].xy[0],clicks[i].xy[1]-clicks[j].xy[1])<=1e-9)throw new Error('Spline interpolation points must be distinct. Use Close spline instead of repeating the first click.');
    const points=clicks.map(point);if(new Set(points).size!==points.length)throw new Error('Spline interpolation points must be distinct.');
    if(closed)points.push(points[0]);const id=nextId(candidate,'S');(candidate.splines??=[]).push({id,points});boundary=[id];
  }
  else if(kind==='circle'){const center=point(a),rim=point(b);if(center===rim)throw new Error('A circle needs a positive radius.');const id=nextId(candidate,'C');(candidate.circles??=[]).push({id,center,rim});boundary=[id];}
  else if(kind==='rectangle'){
    if(Math.abs(a.xy[0]-b.xy[0])<=1e-9||Math.abs(a.xy[1]-b.xy[1])<=1e-9)throw new Error('A rectangle needs nonzero width and height.');
    const ids=[point(a),point({xy:[b.xy[0],a.xy[1]]}),point(b),point({xy:[a.xy[0],b.xy[1]]})];
    boundary=ids.map((id,i)=>line(id,ids[(i+1)%4]));
    for(let i=0;i<4;i++)(candidate.constraints??=[]).push({[i%2?'vertical':'horizontal']:{line:boundary[i]}});
  } else throw new Error('Unsupported drawing tool.');
  if(profile&&(['circle','rectangle','ellipse','slot','polygon'].includes(kind)||kind==='spline'&&closed)){
    if(candidate.holes?.length||candidate.regions?.length||candidate.region_discovery||candidate.region_edits?.length||candidate.profile_operations?.length)throw new Error('This sketch has region or profile edits. Use its profile controls to choose the new outline.');
    candidate.profile=boundary;
  }
  return {definition:candidate,created:[...created,...boundary]};
}
export function primitiveGhost(kind, clicks, cursor, {clockwise=false,closed=false,sides=6}={}) {
  if(!cursor)return [];
  if(kind==='point'||!clicks.length)return [[...cursor.xy,0]];
  const a=clicks[0].xy,b=cursor.xy;
  if(kind==='polygon'){
    if(!Number.isInteger(sides)||sides<3||sides>1000)return [[...a,0],[...b,0]];
    const dx=b[0]-a[0],dy=b[1]-a[1];return Array.from({length:sides+1},(_,i)=>{const angle=i*2*Math.PI/sides;return [a[0]+dx*Math.cos(angle)-dy*Math.sin(angle),a[1]+dx*Math.sin(angle)+dy*Math.cos(angle),0];});
  }
  if((kind==='ellipse'||kind==='slot')&&clicks.length>=2){
    try{
      const end=clicks[1].xy;
      if(kind==='ellipse'){
        const minor=ellipseMinor([clicks[0],clicks[1],cursor]).xy;
        return Array.from({length:65},(_,i)=>{const t=i*Math.PI/32;return [a[0]+(end[0]-a[0])*Math.cos(t)+(minor[0]-a[0])*Math.sin(t),a[1]+(end[1]-a[1])*Math.cos(t)+(minor[1]-a[1])*Math.sin(t),0];});
      }
      const {u,v,across}=axis([clicks[0],clicks[1],cursor]),r=Math.abs(across);
      const at=(center,t)=>[center[0]+r*(u[0]*Math.cos(t)+v[0]*Math.sin(t)),center[1]+r*(u[1]*Math.cos(t)+v[1]*Math.sin(t)),0];
      return [at(a,-Math.PI/2),...Array.from({length:33},(_,i)=>at(end,-Math.PI/2+i*Math.PI/32)),...Array.from({length:33},(_,i)=>at(a,Math.PI/2+i*Math.PI/32))];
    }catch{return [[...clicks[1].xy,0],[...b,0]];}
  }
  if(kind==='spline'){const points=[...clicks,cursor].map(c=>[...c.xy,0]);if(closed&&points.length>=3)points.push(points[0]);return points;}
  if(kind==='arc'&&clicks.length>=2){
    try{
      const start=clicks[1].xy,end=arcEnd([clicks[0],clicks[1],cursor]).xy,r=Math.hypot(start[0]-a[0],start[1]-a[1]),first=Math.atan2(start[1]-a[1],start[0]-a[0]);
      let span=Math.atan2(end[1]-a[1],end[0]-a[0])-first;
      if(clockwise){while(span>=0)span-=2*Math.PI;}else{while(span<=0)span+=2*Math.PI;}
      return Array.from({length:65},(_,i)=>[a[0]+r*Math.cos(first+span*i/64),a[1]+r*Math.sin(first+span*i/64),0]);
    }catch{return [[...clicks[1].xy,0],[...b,0]];}
  }
  if(kind==='circle'){const radius=Math.hypot(b[0]-a[0],b[1]-a[1]);return Array.from({length:65},(_,i)=>[a[0]+radius*Math.cos(i*Math.PI/32),a[1]+radius*Math.sin(i*Math.PI/32),0]);}
  if(kind==='rectangle')return [[a[0],a[1],0],[b[0],a[1],0],[b[0],b[1],0],[a[0],b[1],0],[a[0],a[1],0]];
  return [[...a,0],[...b,0]];
}
export function createCanvasTools({host,getDraft,setDraft,changed,message,ghost,dragPoint,modeChanged=()=>{}}) {
  let mode='select',grid=1,fixed=true,profile=false,clockwise=false,closed=false,sides=6,clicks=[],gesture=null,pendingPaths=[];
  const section=document.createElement('section');section.className='canvas-tools';
  const heading=document.createElement('h3');heading.textContent='Canvas tools';section.append(heading);
  const label=document.createElement('label');label.textContent='Tool';const select=document.createElement('select');select.setAttribute('aria-label','Canvas tool');
  for(const name of ['select','drag','point','line','circle','rectangle','arc','spline','ellipse','slot','polygon'])select.append(new Option(name[0].toUpperCase()+name.slice(1),name));label.append(select);section.append(label);
  const note=document.createElement('p');note.textContent='Draw with clicks. Drag free points. Shift-drag empty space to pan. Escape cancels. New geometry stays in your local draft until Preview.';section.append(note);
  function input(caption,type,value,change){const row=document.createElement('label');row.textContent=caption;const e=document.createElement('input');e.type=type;e.setAttribute('aria-label',caption);if(type==='checkbox')e.checked=value;else{e.value=value;e.min=0;e.step='any';}e.onchange=()=>change(type==='checkbox'?e.checked:e.value.trim()?Number(e.value):NaN);row.append(e);section.append(row);return row;}
  input('Snap grid (mm; 0 disables)','number',grid,value=>{if(Number.isFinite(value)&&value>=0)grid=value;else message('Grid spacing must be finite and nonnegative.');});
  input('Fix new points','checkbox',fixed,value=>fixed=value);input('Use new closed curve / rectangle as outer profile','checkbox',profile,value=>profile=value);
  const clockwiseRow=input('Clockwise arc','checkbox',clockwise,value=>clockwise=value),closeRow=input('Close spline','checkbox',closed,value=>{closed=value;finish.disabled=mode!=='spline'||clicks.length<(closed?3:2);});
  const sidesRow=input('Polygon sides (3–1000)','number',sides,value=>{sides=value;if(!Number.isInteger(value)||value<3||value>1000)message('A regular polygon needs 3 to 1000 integer sides.');});
  const finish=document.createElement('button');finish.textContent='Finish spline';finish.disabled=true;section.append(finish);
  const instructions=document.createElement('p');instructions.textContent='Arc: center, start, end direction. Ellipse: center, major-axis end, minor width. Slot: cap centers, then half-width. Polygon: center, first vertex. Spline: interpolation points, then Finish spline. Preview builds native geometry.';section.append(instructions);
  const snapStatus=document.createElement('p');snapStatus.setAttribute('data-snap-status','');snapStatus.setAttribute('aria-live','polite');section.append(snapStatus);
  host.append(section);
  function paint(path=[]){ghost([...pendingPaths,...(path.length?[path]:[])]);}
  function cancel(){clicks=[];gesture=null;snapStatus.textContent='';finish.disabled=true;paint();}
  function reviewed(){pendingPaths=[];cancel();}
  function showOptions(){clockwiseRow.hidden=mode!=='arc';closeRow.hidden=finish.hidden=mode!=='spline';sidesRow.hidden=mode!=='polygon';}showOptions();
  select.onchange=()=>{cancel();mode=select.value;showOptions();modeChanged(mode);message(mode==='select'?'Select geometry for dimensions and constraints.':`Canvas ${mode} tool active.`);};
  function snap(event){const click=snapPosition(event.position,gesture?{...getDraft(),points:getDraft().points.filter(p=>p.id!==gesture.point)}:getDraft(),event.scene.points,grid,event.tolerance,event.point);const text=click.point?`Snap: ${click.point}`:'';if(snapStatus.textContent!==text)snapStatus.textContent=text;return click;}
  function commit(points){
    const result=drawPrimitive(getDraft(),mode,points,{fixed,profile,clockwise,closed,sides});
    setDraft(result.definition);pendingPaths.push(primitiveGhost(mode,points.slice(0,-1),points.at(-1),{clockwise,closed,sides}));cancel();changed();message('Geometry drawn into the draft. Preview to solve and check it.');
  }
  finish.onclick=()=>{if(mode!=='spline')return;try{commit(clicks);}catch(error){message(error.message);}};
  return {
    get mode(){return mode;},cancel,reviewed,
    selectMode(){cancel();mode='select';select.value=mode;showOptions();modeChanged(mode);},
    down(event){
      if(mode==='select'||event.shiftKey)return false;
      try{
        if(mode==='drag'){
          if(!event.point){message('Choose a free authored point to drag.');return true;}
          const p=getDraft().points.find(p=>p.id===event.point);
          if((getDraft().constraints??[]).some(c=>c.weighted_point?.point===event.point)){message('This point follows a weighted link. Edit its source points or weights.');return true;}
          if(!p||p.fixed||!p.x?.literal||!p.y?.literal){message('This point is fixed, generated, or formula-driven. Use its driving controls instead.');return true;}
          gesture={point:p.id,start:event.position};return true;
        }
        const click=snap(event);
        if(mode==='spline'){
          if(clicks.some(c=>Math.hypot(c.xy[0]-click.xy[0],c.xy[1]-click.xy[1])<=1e-9)){message('Choose a distinct interpolation point; use Close spline to close the curve.');return true;}
          clicks.push(click);finish.disabled=clicks.length<(closed?3:2);paint(clicks.map(c=>[...c.xy,0]));message(`${clicks.length} spline points staged. Finish spline commits them to the draft.`);
        }
        else if(mode==='point'||clicks.length>=(['arc','ellipse','slot'].includes(mode)?2:1)){commit([...clicks,click]);}
        else if(['arc','ellipse','slot'].includes(mode)&&clicks.length===1){if(Math.hypot(clicks[0].xy[0]-click.xy[0],clicks[0].xy[1]-click.xy[1])<=1e-9)throw new Error('Choose distinct axis endpoints.');clicks.push(click);paint(primitiveGhost(mode,[clicks[0]],click));message(mode==='arc'?'Choose the arc end direction.':'Choose the width perpendicular to the axis.');}
        else{clicks=[click];paint(primitiveGhost(mode,clicks,click));message('Choose the second location.');}
        return true;
      }catch(error){message(error.message);return true;}
    },
    move(event){if(mode==='select'||event.shiftKey)return false;try{const cursor=snap(event);paint(gesture?[[...gesture.start,0],[...cursor.xy,0]]:primitiveGhost(mode,clicks,cursor,{clockwise,closed,sides}));}catch(error){message(error.message);}return true;},
    async up(event){if(!gesture)return;const pending=gesture;gesture=null;const target=snapPosition(event.position,{...getDraft(),points:getDraft().points.filter(p=>p.id!==pending.point)},event.scene.points,grid,event.tolerance,event.point).xy;paint();if(Math.hypot(target[0]-pending.start[0],target[1]-pending.start[1])<1e-8)return;await dragPoint(pending.point,target);},
  };
}
