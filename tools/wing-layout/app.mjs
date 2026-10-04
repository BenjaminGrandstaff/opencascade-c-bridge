import {starter,validate,metrics,sectionPoints,cadSections,stationCsv,parseAirfoil} from './model.mjs';
const $=id=>document.getElementById(id);let model=starter(),selected=0,importIndex=0,yaw=.55,pitch=.55;
const fields=['fraction','chordMm','leadingEdgeMm','heightMm','twistDeg','pivotFraction','zeroLiftDeg'];
function download(name,data,type='application/json'){const url=URL.createObjectURL(new Blob([data],{type}));const a=document.createElement('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
function guard(fn){try{fn();$('error').textContent='';}catch(e){$('error').textContent=e.message;}}
function table(){
 $('span').value=model.spanMm;$('notes').value=model.notes;
 const body=$('stations');body.replaceChildren();
 model.stations.forEach((s,i)=>{
  const row=document.createElement('tr');if(i===selected)row.className='selected';
  row.onclick=()=>{selected=i;for(const [j,r]of [...body.children].entries())r.classList.toggle('selected',j===i);guard(draw);};
  fields.forEach(k=>{const td=document.createElement('td'),input=document.createElement('input');input.type='number';input.step=k==='fraction'||k==='pivotFraction'?'0.01':'0.1';input.value=s[k]??'';input.setAttribute('aria-label',`Station ${i+1} ${k}`);
   input.oninput=()=>{s[k]=k==='zeroLiftDeg'&&input.value===''?null:input.value===''?NaN:Number(input.value);s.provenance='User-edited station; historical applicability unverified.';guard(draw);};td.append(input);row.append(td);});
  const p=document.createElement('td');p.className='profile';const name=document.createElement('span');name.textContent=s.airfoil.name;p.append(name);
  const imp=document.createElement('button');imp.textContent='Import .dat';imp.onclick=()=>{importIndex=i;$('foil-file').click();};p.append(imp);row.append(p);
  const td=document.createElement('td');if(i>0&&i<model.stations.length-1){const b=document.createElement('button');b.textContent='Remove';b.onclick=()=>{model.stations.splice(i,1);selected=Math.min(selected,model.stations.length-1);table();guard(draw);};td.append(b);}row.append(td);body.append(row);
 });
}
const line=(p,color,width=1,dash='')=>`<polyline points="${p.map(v=>v.join(',')).join(' ')}" fill="none" stroke="${color}" stroke-width="${width}" ${dash?`stroke-dasharray="${dash}"`:''}/>`;
function draw(){
 validate(model);const m=metrics(model);$('metrics').textContent=`Projected area ${(m.areaMm2/1e6).toFixed(3)} m²    •    Aspect ratio ${m.aspectRatio.toFixed(2)}    •    Mean aerodynamic chord ${m.meanAerodynamicChordMm.toFixed(1)} mm`;
 const scale=Math.min(570/model.spanMm,150/Math.max(...model.stations.map(s=>s.leadingEdgeMm+s.chordMm)));
 const plan=(s,side,rear)=>[310+side*s.fraction*model.spanMm/2*scale,20+(s.leadingEdgeMm+(rear?s.chordMm:0))*scale];
 let planSvg='';for(const side of [1,-1]){
  const p=[...model.stations.map(s=>plan(s,side,false)),...model.stations.toReversed().map(s=>plan(s,side,true))];planSvg+=`<polygon points="${p.map(v=>v.join(',')).join(' ')}" fill="#294751" stroke="#81c6bc"/>`;
  model.stations.forEach((s,i)=>planSvg+=line([plan(s,side,false),plan(s,side,true)],i===selected?'#ffc67d':'#5d8390',i===selected?3:1));
 }$('planform').innerHTML=planSvg;
 const incidence=model.stations.map(s=>s.zeroLiftDeg===null?null:s.twistDeg-s.zeroLiftDeg);
 const vals=[0,...model.stations.map(s=>s.twistDeg),...incidence.filter(v=>v!==null)];const lo=Math.min(...vals)-1,hi=Math.max(...vals)+1;
 const chart=(s,v)=>[45+s.fraction*545,175-(v-lo)/(hi-lo)*145];
 let plot=line([[45,30],[45,175],[590,175]],'#627681');
 plot+=line(model.stations.map(s=>chart(s,s.twistDeg)),'#96ddcc',2);
 for(let i=1;i<model.stations.length;i++)if(incidence[i]!==null&&incidence[i-1]!==null)plot+=line([chart(model.stations[i-1],incidence[i-1]),chart(model.stations[i],incidence[i])],'#ffc67d',2,'5 4');
 plot+=`<text x="3" y="36" fill="#a8bec8">${hi.toFixed(1)}°</text><text x="3" y="175" fill="#a8bec8">${lo.toFixed(1)}°</text><text x="45" y="201" fill="#a8bec8">Root</text><text x="565" y="201" fill="#a8bec8">Tip</text>`;$('twist').innerHTML=plot;
 const s=model.stations[selected],p=s.airfoil.points.map(([x,z])=>[40+x*540,100-z*540]);$('profile').innerHTML=line([...p,p[0]],'#96ddcc',2)+line([[40,100],[580,100]],'#526b77');$('profile-source').textContent=`${s.airfoil.name}\n${s.airfoil.source}\nStation provenance: ${s.provenance}`;
 renderWing();
}
function renderWing(){
 const canvas=$('wing'),rect=canvas.getBoundingClientRect(),dpr=devicePixelRatio||1;canvas.width=Math.round(rect.width*dpr);canvas.height=Math.round(rect.height*dpr);const ctx=canvas.getContext('2d');ctx.scale(dpr,dpr);ctx.clearRect(0,0,rect.width,rect.height);
 const all=[1,-1].map(side=>model.stations.map(s=>sectionPoints(model,s,side)));
 const centerX=(Math.min(...all.flat(2).map(p=>p[0]))+Math.max(...all.flat(2).map(p=>p[0])))/2;
 const raw=p=>{const x=p[0]-centerX,y=p[1],z=p[2];const u=y*Math.cos(yaw)-x*Math.sin(yaw),depth=x*Math.cos(yaw)+y*Math.sin(yaw);return [u,depth*Math.sin(pitch)-z*Math.cos(pitch),depth*Math.cos(pitch)+z*Math.sin(pitch)];};
 const projected=all.map(half=>half.map(ps=>ps.map(raw))),flat=projected.flat(2),maxX=Math.max(...flat.map(p=>Math.abs(p[0]))),maxY=Math.max(...flat.map(p=>Math.abs(p[1]))),scale=Math.min((rect.width-30)/(2*maxX||1),(rect.height-30)/(2*maxY||1));
 const screen=p=>[rect.width/2+p[0]*scale,rect.height/2+p[1]*scale];let faces=[];
 for(const half of projected)for(let i=1;i<half.length;i++)for(let j=0;j<half[i].length;j++){const k=(j+1)%half[i].length,ps=[half[i-1][j],half[i][j],half[i][k],half[i-1][k]];faces.push({ps,depth:ps.reduce((a,p)=>a+p[2],0)/4});}
 faces.sort((a,b)=>b.depth-a.depth);ctx.fillStyle='#326472';ctx.strokeStyle='#477783';ctx.lineWidth=.4;
 for(const f of faces){ctx.beginPath();f.ps.forEach((p,i)=>{const [x,y]=screen(p);i?ctx.lineTo(x,y):ctx.moveTo(x,y);});ctx.closePath();ctx.fill();ctx.stroke();}
 for(const half of projected)half.forEach((ps,i)=>{ctx.beginPath();[...ps,ps[0]].forEach((p,j)=>{const [x,y]=screen(p);j?ctx.lineTo(x,y):ctx.moveTo(x,y);});ctx.strokeStyle=i===selected?'#ffc67d':'#81c6bc';ctx.lineWidth=i===selected?2:1;ctx.stroke();});
}
$('span').oninput=()=>{model.spanMm=Number($('span').value);guard(draw);};$('notes').oninput=()=>model.notes=$('notes').value;
$('reset').onclick=()=>{model=starter();selected=0;table();guard(draw);};
$('save').onclick=()=>guard(()=>{validate(model);download('wing-project.json',JSON.stringify(model,null,2));});
$('csv').onclick=()=>guard(()=>download('wing-stations.csv',stationCsv(model),'text/csv'));
$('cad').onclick=()=>guard(()=>download('wing-sections.json',JSON.stringify(cadSections(model))));
$('open').onchange=async()=>{try{const file=$('open').files[0];if(!file)return;const next=JSON.parse(await file.text());validate(next);model=next;selected=0;table();guard(draw);}catch(e){$('error').textContent=e.message;}finally{$('open').value='';}};
$('foil-file').onchange=async()=>{try{const file=$('foil-file').files[0];if(!file)return;const airfoil=parseAirfoil(await file.text(),file.name);const next=structuredClone(model);next.stations[importIndex].airfoil=airfoil;validate(next);model=next;selected=importIndex;table();guard(draw);}catch(e){$('error').textContent=e.message;}finally{$('foil-file').value='';}};
$('add').onclick=()=>guard(()=>{validate(model);let i=1;for(let j=2;j<model.stations.length;j++)if(model.stations[j].fraction-model.stations[j-1].fraction>model.stations[i].fraction-model.stations[i-1].fraction)i=j;const a=model.stations[i-1],b=model.stations[i],s=structuredClone(a);for(const k of fields)s[k]=k==='zeroLiftDeg'?(a[k]===null||b[k]===null?null:(a[k]+b[k])/2):(a[k]+b[k])/2;s.provenance='Interpolated geometry; airfoil copied from adjoining inner station. Assumed.';model.stations.splice(i,0,s);selected=i;table();draw();});
let drag=null;$('wing').onpointerdown=e=>{drag=[e.clientX,e.clientY];$('wing').setPointerCapture(e.pointerId);};$('wing').onpointermove=e=>{if(!drag)return;yaw+=(e.clientX-drag[0])*.008;pitch=Math.max(-1.5,Math.min(1.5,pitch+(e.clientY-drag[1])*.008));drag=[e.clientX,e.clientY];guard(draw);};$('wing').onpointerup=()=>drag=null;$('wing').onpointercancel=()=>drag=null;
window.addEventListener('resize',()=>guard(draw));table();guard(draw);window.wingLayout={getProject:()=>structuredClone(model)};
