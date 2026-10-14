// Values come from native annotations; this panel never evaluates model expressions.
const signature=value=>JSON.stringify(value,(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])):v);
function parameterNames(link){const found=new Set(),queue=[link.near,link.maximum_distance];while(queue.length){const value=queue.pop();if(!value||typeof value!=='object')continue;if(typeof value.parameter==='string')found.add(value.parameter);queue.push(...Object.values(value).filter(v=>v&&typeof v==='object'));}return [...found].sort();}
export function crossingReviews(definition,scene,current=true){
 const native=new Map((scene?.annotations??[]).filter(a=>a.detail?.constraint?.crossing_point).map(a=>[a.detail.constraint.crossing_point.point,a]));
 return (definition.constraints??[]).flatMap(c=>{
  const link=c.crossing_point;if(!link)return [];
  const a=native.get(link.point),w=a?.detail?.crossing_window,match=signature(c)===signature(a?.detail?.constraint);
  const valid=current&&match&&a?.status==='passed'&&Array.isArray(w?.center)&&w.center.length===3&&w.center.every(Number.isFinite)&&w.center[2]===0&&Number.isFinite(w.radius)&&w.radius>0&&Number.isFinite(w.distance)&&w.distance>=0;
  return [{...link,window:valid?{...w,point:link.point}:null,drivers:match?(a?.parameters??parameterNames(link)):parameterNames(link)}];
 });
}
export function createCrossingReview({host,definition,review=()=>null,current=()=>false,show=()=>{},highlight=()=>{},inspect=()=>{},message=()=>{},initialOpen=false,initialFilter=''}){
 const section=document.createElement('details'),heading=document.createElement('summary');heading.textContent='Saved crossing windows';section.open=initialOpen;section.append(heading);host.append(section);
 const note=document.createElement('p');note.textContent='Review native search windows for selected crossings. Preview after draft changes to resolve hint and radius expressions. Purple circles show search areas; a center cross marks each saved hint.';section.append(note);
 const filter=document.createElement('input');filter.type='search';filter.value=initialFilter;filter.setAttribute('aria-label','Filter crossing windows');section.append(filter);
 const status=document.createElement('p'),list=document.createElement('div');section.append(status);
 const button=(text,action,parent=section)=>{const b=document.createElement('button');b.textContent=text;b.onclick=()=>{try{action();}catch(e){message(e.message);}};parent.append(b);return b;};
 let rows=[];
 const matching=()=>rows.filter(r=>`${r.point} ${r.first} ${r.second}`.toLowerCase().includes(filter.value.toLowerCase().trim()));
 const display=items=>{if(!current()) {refresh();message('Preview the current draft before showing crossing windows.');return;}const valid=items.filter(r=>r.window).slice(0,100);show(valid.map(r=>r.window));highlight([...new Set(valid.flatMap(r=>[r.point,r.first,r.second]))]);message(`Showing ${valid.length} native crossing window${valid.length===1?'':'s'}.`);};
 const all=button('Show matching crossing windows',()=>display(matching()));
 button('Clear crossing windows',()=>{show([]);highlight([]);});section.append(list);
 function refresh(){show([]);rows=crossingReviews(definition,review(),current());const matches=matching();status.textContent=`${rows.length} saved crossings · ${matches.length} matching. ${current()?'Native values from the current view.':'Preview required for the current draft.'}${matches.length>100?' Showing the first 100; filter to narrow the list.':''}`;all.disabled=!current()||!matches.some(r=>r.window);list.replaceChildren();for(const r of matches.slice(0,100)){const item=document.createElement('fieldset'),title=document.createElement('legend');title.textContent=r.point;item.append(title);const text=document.createElement('p');text.textContent=`${r.first} × ${r.second}. ${r.window?`Hint (${r.window.center[0].toPrecision(5)}, ${r.window.center[1].toPrecision(5)}) mm; radius ${r.window.radius.toPrecision(5)} mm; crossing distance ${r.window.distance.toPrecision(5)} mm.`:'Preview required to verify this window.'}`;item.append(text);const drivers=document.createElement('p');drivers.textContent=`Drivers: ${r.drivers.length?r.drivers.join(', '):'no parameter drivers'}.`;item.append(drivers);button(`Inspect crossing ${r.point}`,()=>inspect(r.point),item);const showOne=button(`Show window ${r.point}`,()=>display([r]),item);showOne.disabled=!r.window;list.append(item);}}
 filter.oninput=refresh;refresh();return {refresh,get open(){return section.open;},get filter(){return filter.value;}};
}
