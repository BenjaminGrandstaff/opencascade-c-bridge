import {geometryIds} from './constraint-controls.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
export function pointLinks(definition,point){return (definition.constraints??[]).filter(c=>c.spline_point?.point===point||c.weighted_point?.point===point||c.point_on_curve?.point===point);}
export function linkNamedPoint(definition,{point,mode,spline,fraction,first,second}){
  const target=definition.points?.find(p=>p.id===point);if(!target)throw new Error('Choose an authored named point.');
  let constraints;
  if(mode==='spline'){
    const source=definition.splines?.find(s=>s.id===spline);if(!source?.basis)throw new Error('Choose an authored explicit spline.');
    if(source.points.includes(point))throw new Error('A linked point cannot also be a source control pole.');
    if(!fraction||typeof fraction!=='object'||Array.isArray(fraction))throw new Error('Choose a fraction driver.');
    const literal=fraction.literal;if(literal&&(literal.dimension!=='scalar'||literal.unit!=null||!Number.isFinite(literal.value)||literal.value<0||literal.value>1))throw new Error('Use a spline parameter fraction from 0 to 1.');
    constraints=[{spline_point:{point,spline,fraction:structuredClone(fraction)}}];
  }else if(mode==='crossing'){
    const curves=geometryIds(definition,'curves');if(!curves.includes(first)||!curves.includes(second)||first===second)throw new Error('Choose two distinct authored or projected curves.');
    // A target used to define either curve would move its own source geometry.
    for(const group of ['lines','circles','arcs','ellipses','splines'])for(const curve of definition[group]??[])if([first,second].includes(curve.id)){
      const ids=group==='splines'?[...curve.points,curve.endpoints?.start,curve.endpoints?.end]:['start','end','center','rim','major','minor'].map(k=>curve[k]);
      if(ids.includes(point))throw new Error('A crossing point must be separate from the defining points of both curves.');
    }
    constraints=[{point_on_curve:{point,curve:first}},{point_on_curve:{point,curve:second}}];
  }else throw new Error('Choose a spline parameter or a two-curve crossing.');
  const next=structuredClone(definition),old=pointLinks(definition,point);
  next.constraints=(next.constraints??[]).filter((_,i)=>!old.includes(definition.constraints[i]));next.constraints.push(...constraints);
  next.points.find(p=>p.id===point).fixed=false;
  return next;
}
export function releasePointLink(definition,point){
  const old=pointLinks(definition,point);if(!old.length)throw new Error('This point has no saved link.');
  const next=structuredClone(definition);next.constraints=next.constraints.filter((_,i)=>!old.includes(definition.constraints[i]));return next;
}
export function createPointLinkControls({host,definition,parameters=[],changed,message,highlight=()=>{},initialPoint=null,initialOpen=false}){
  const section=document.createElement('details'),heading=document.createElement('summary');heading.textContent='Named point links';section.open=initialOpen;section.append(heading);host.append(section);
  const note=document.createElement('p');note.textContent='Keep a named point on an authored spline at a fixed parameter fraction, or constrain it to a two-curve crossing. Linking unfixes the point and replaces its existing point links. On periodic splines, fractions 0 and 1 name the same seam. Other dimensions remain active. Crossings use the current point seed; lines use their supporting line. Preview checks the solved geometry and every model variant.';section.append(note);
  const select=(label,choices)=>{const row=document.createElement('label');row.textContent=label;const e=document.createElement('select');e.setAttribute('aria-label',label);for(const [id,name]of choices)e.append(new Option(name,id));row.append(e);section.append(row);return e;};
  const point=select('Named point to link',(definition.points??[]).map(p=>[p.id,p.id]));if((definition.points??[]).some(p=>p.id===initialPoint))point.value=initialPoint;
  const mode=select('Point link mode',[['spline','At spline parameter'],['crossing','At two-curve crossing']]);
  const spline=select('Point link spline',(definition.splines??[]).filter(s=>s.basis).map(s=>[s.id,s.id]));
  const driver=select('Spline fraction driver',[['literal','Number'],['expression','Existing expression'],...parameters.filter(p=>p.parameter_type?.scalar==='scalar').map(p=>[p.id,p.id])]);
  const row=document.createElement('label');row.textContent='Spline parameter fraction';const fraction=document.createElement('input');fraction.type='number';fraction.step='any';fraction.value='.5';fraction.setAttribute('aria-label',row.textContent);row.append(fraction);section.append(row);
  const curves=geometryIds(definition,'curves').map(id=>[id,id]),first=select('Crossing first curve',curves),second=select('Crossing second curve',curves);if(curves.length>1)second.value=curves[1][0];
  const status=document.createElement('p');section.append(status);
  const button=(name,action)=>{const b=document.createElement('button');b.textContent=name;b.onclick=()=>{try{action();}catch(error){message(error.message);}};section.append(b);return b;};
  button('Link named point',()=>{const preserved=pointLinks(definition,point.value).find(c=>c.spline_point)?.spline_point.fraction;const value=driver.value==='expression'?preserved:driver.value==='literal'?scalar(String(fraction.value).trim()?Number(fraction.value):NaN):{parameter:driver.value};changed(linkNamedPoint(definition,{point:point.value,mode:mode.value,spline:spline.value,fraction:value,first:first.value,second:second.value}));message('Named point linked in the draft. Preview before applying; crossing constraints use the current point seed and may move free source geometry.');});
  const release=button('Remove named point link',()=>{changed(releasePointLink(definition,point.value));message('Point link removed from the draft. The point is free; stored coordinates are solver seeds. Preview before applying.');});
  button('Highlight named point link',()=>highlight([point.value,...pointLinks(definition,point.value).flatMap(c=>c.spline_point?[c.spline_point.spline]:c.point_on_curve?[c.point_on_curve.curve]:c.weighted_point.terms.map(t=>t.point))]));
  function update(){const links=pointLinks(definition,point.value);release.disabled=!links.length;status.textContent=links.length?links.map(c=>c.spline_point?`Follows ${c.spline_point.spline} at fraction ${JSON.stringify(c.spline_point.fraction)}.`:c.point_on_curve?`On ${c.point_on_curve.curve}.`:'Weighted source point link.').join(' '):'No saved point link.';const sample=mode.value==='spline';spline.parentElement&&(spline.parentElement.hidden=!sample);driver.parentElement&&(driver.parentElement.hidden=!sample);row.hidden=!sample||driver.value!=='literal';first.parentElement&&(first.parentElement.hidden=sample);second.parentElement&&(second.parentElement.hidden=sample);fraction.disabled=driver.value!=='literal';}
  function load(){const link=pointLinks(definition,point.value).find(c=>c.spline_point)?.spline_point;if(link){mode.value='spline';spline.value=link.spline;if(link.fraction.literal){driver.value='literal';fraction.value=String(link.fraction.literal.value);}else if(link.fraction.parameter)driver.value=link.fraction.parameter;else driver.value='expression';}else{const crossing=pointLinks(definition,point.value).filter(c=>c.point_on_curve);if(crossing.length===2){mode.value='crossing';first.value=crossing[0].point_on_curve.curve;second.value=crossing[1].point_on_curve.curve;}}update();}
  point.onchange=load;mode.onchange=update;driver.onchange=update;load();
  return {get open(){return section.open;},get selected(){return point.value;},inspect(id){if(!(definition.points??[]).some(p=>p.id===id))return false;point.value=id;section.open=true;load();return true;}};
}
