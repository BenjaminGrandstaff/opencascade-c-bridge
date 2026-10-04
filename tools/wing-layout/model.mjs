export const SCHEMA = 'occb-wing-layout-v1';
export function symmetricAirfoil(thickness=0.12, count=40) {
  const upper = [];
  for(let i=0;i<=count;i++) {
    const x=(1+Math.cos(Math.PI*i/count))/2;
    const z=5*thickness*(.2969*Math.sqrt(x)-.126*x-.3516*x*x+.2843*x**3-.1036*x**4);
    upper.push([x,z]);
  }
  return [...upper,...upper.slice(1,-1).reverse().map(([x,z])=>[x,-z])];
}
export function starter() {
  return {schema:SCHEMA,name:'Ho 229-inspired RC layout — illustrative geometry', spanMm:2000,
    notes:'All dimensions, profiles and twist below are illustrative assumptions. This is not a historical Ho 229 reconstruction or a flight stability result.',
    stations:[
      [0,700,0,0,0,.12], [.35,530,230,0,-1,.11], [.7,350,470,0,-2,.10], [1,180,650,0,-3,.08]
    ].map(([fraction,chordMm,leadingEdgeMm,heightMm,twistDeg,t])=>({fraction,chordMm,leadingEdgeMm,heightMm,twistDeg,pivotFraction:.25,zeroLiftDeg:null,
      airfoil:{name:`Symmetric ${Math.round(t*100)}% placeholder`,source:'Generated symmetric NACA four-digit thickness distribution, closed trailing edge; assumed, not historical.',points:symmetricAirfoil(t)},provenance:'Illustrative assumption; no verified Ho 229 station data.'}))};
}
export function validate(model) {
  if(model?.schema!==SCHEMA) throw Error('Unsupported project schema.');
  if(!Number.isFinite(model.spanMm)||model.spanMm<=0) throw Error('Full span must be positive.');
  if(!Array.isArray(model.stations)||model.stations.length<2||model.stations.length>200) throw Error('Use 2–200 stations.');
  model.stations.forEach((s,i)=>{
    for(const k of ['fraction','chordMm','leadingEdgeMm','heightMm','twistDeg','pivotFraction'])
      if(!Number.isFinite(s[k])) throw Error(`Station ${i+1}: ${k} must be finite.`);
    if(s.fraction<0||s.fraction>1||s.chordMm<=0||s.pivotFraction<0||s.pivotFraction>1||Math.abs(s.twistDeg)>=80) throw Error(`Station ${i+1}: invalid dimension, fraction, pivot or twist (must be within ±80°).`);
    if(i&&s.fraction<=model.stations[i-1].fraction) throw Error('Stations must have increasing span fractions.');
    if(s.zeroLiftDeg!==null&&!Number.isFinite(s.zeroLiftDeg)) throw Error('Zero-lift angle must be blank or a finite number.');
    const p=s.airfoil?.points;
    if(!Array.isArray(p)||p.length<6||p.length>10000||p.some(v=>!Array.isArray(v)||v.length!==2||v.some(n=>!Number.isFinite(n))||v[0]<-.01||v[0]>1.01||Math.abs(v[1])>1)) throw Error(`Station ${i+1}: invalid normalized airfoil coordinates.`);
    const area=p.reduce((a,v,j)=>{const q=p[(j+1)%p.length];return a+v[0]*q[1]-q[0]*v[1];},0);
    if(Math.abs(area)<1e-7) throw Error(`Station ${i+1}: airfoil has zero enclosed area.`);
    const le=p.reduce((b,v,j)=>v[0]<p[b][0]?j:b,0);
    if(le<2||le>p.length-3||p[0][0]<.9||p.at(-1)[0]<.9||p[le][0]>.01||
       p.slice(0,le+1).some((v,j,a)=>j&&v[0]>a[j-1][0]+1e-6)||
       p.slice(le).some((v,j,a)=>j&&v[0]<a[j-1][0]-1e-6))
      throw Error(`Station ${i+1}: expected normalized Selig surface ordering.`);
    checkSurfaces(p,le);
  });
  if(model.stations[0].fraction!==0||model.stations.at(-1).fraction!==1) throw Error('First and last stations must be at fractions 0 and 1.');
  return model;
}
// Selig order only: upper trailing edge to leading edge to lower trailing edge.
export function parseAirfoil(text,name='Imported airfoil') {
  const lines=text.trim().split(/\r?\n/); let points=[];
  for(const line of lines) {
    if(!line.trim()||/^\s*[#;]/.test(line)) continue;
    const match=line.trim().match(/^([-+\d.eE]+)[\s,]+([-+\d.eE]+)\s*$/);
    if(!match) {if(!points.length) continue;throw Error('Unexpected text inside airfoil coordinates.');}
    const pair=match.slice(1).map(Number);
    if(pair.some(n=>!Number.isFinite(n))) throw Error('Invalid airfoil coordinate.');
    if(pair[0]>1.01||pair[0]<-.01||Math.abs(pair[1])>1) throw Error('Use normalized Selig coordinates; Lednicer count headers are not supported.');
    if(!points.length||Math.hypot(pair[0]-points.at(-1)[0],pair[1]-points.at(-1)[1])>1e-10) points.push(pair);
  }
  if(points.length>1&&Math.hypot(points[0][0]-points.at(-1)[0],points[0][1]-points.at(-1)[1])<1e-10) points.pop();
  const le=points.reduce((best,p,i)=>p[0]<(points[best]?.[0]??Infinity)?i:best,0);
  if(points.length<6||le<2||le>points.length-3||points[0][0]<.9||points.at(-1)[0]<.9||points[le][0]>.01) throw Error('Expected Selig order: trailing edge → upper surface → leading edge → lower surface → trailing edge.');
  if(points.slice(0,le+1).some((p,i,a)=>i&&p[0]>a[i-1][0]+1e-6)||points.slice(le).some((p,i,a)=>i&&p[0]<a[i-1][0]-1e-6)) throw Error('Airfoil surface coordinates must progress monotonically in x.');
  checkSurfaces(points,le);
  return {name,source:`Imported Selig file: ${name}. Historical applicability and aerodynamic data unverified.`,points};
}
function interpolate(surface,x) {
  const direction=surface.at(-1)[0]>=surface[0][0]?1:-1;
  if((x-surface[0][0])*direction<=0)return surface[0][1];
  if((x-surface.at(-1)[0])*direction>=0)return surface.at(-1)[1];
  let lo=0,hi=surface.length-1;
  while(hi-lo>1){const mid=Math.floor((lo+hi)/2);if((x-surface[mid][0])*direction>=0)lo=mid;else hi=mid;}
  const a=surface[lo],b=surface[hi];
  return Math.abs(b[0]-a[0])<1e-12?a[1]:a[1]+(b[1]-a[1])*(x-a[0])/(b[0]-a[0]);
}
function checkSurfaces(points,le) {
  const upper=points.slice(0,le+1),lower=points.slice(le);
  // Compare at every breakpoint: linear segments cannot cross between these checks.
  for(const [x] of points) if(interpolate(upper,x)<interpolate(lower,x)-1e-8)
    throw Error('Expected upper surface before lower surface, without crossing surfaces.');
}
export function resample(points,n=40) {
  const le=points.reduce((b,p,i)=>p[0]<points[b][0]?i:b,0);
  const upper=points.slice(0,le+1),lower=points.slice(le);
  // Add the first trailing-edge point to close lower surface if the profile is closed.
  if(lower.at(-1)[0]<.999999) lower.push(points[0]);
  const result=[];
  for(let i=0;i<=n;i++){const x=(1+Math.cos(Math.PI*i/n))/2;result.push([x,interpolate(upper,x)]);}
  for(let i=n-1;i>0;i--){const x=(1+Math.cos(Math.PI*i/n))/2;result.push([x,interpolate(lower,x)]);}
  result[0][1]=(interpolate(upper,1)+interpolate(lower,1))/2;
  return result;
}
export function sectionPoints(model,s,side=1,n=40) {
  const angle=s.twistDeg*Math.PI/180, pivot=s.pivotFraction*s.chordMm;
  return resample(s.airfoil.points,n).map(([x,z])=>{
    const dx=x*s.chordMm-pivot,dz=z*s.chordMm;
    // X aft, Y right, Z up. Positive incidence raises the leading edge.
    return [s.leadingEdgeMm+pivot+dx*Math.cos(angle)+dz*Math.sin(angle),side*s.fraction*model.spanMm/2,s.heightMm-dx*Math.sin(angle)+dz*Math.cos(angle)];
  });
}
export function cadSections(model) {
  validate(model);
  return {schema:'occb-wing-sections-v1',units:'mm',axes:'X aft, Y right, Z up',name:model.name,notes:model.notes,
    approximation:'Polygon sections, 40 cosine intervals per airfoil surface, trailing edge closed at midpoint; ruled solid loft between stations. No stability analysis.',
    halves:[1,-1].map(side=>model.stations.map(s=>sectionPoints(model,s,side)))};
}
export function metrics(model) {
  validate(model);let area=0,c2=0;
  for(let i=1;i<model.stations.length;i++){
    const a=model.stations[i-1],b=model.stations[i],dy=(b.fraction-a.fraction)*model.spanMm/2;
    area+=dy*(a.chordMm+b.chordMm)/2;c2+=dy*(a.chordMm**2+a.chordMm*b.chordMm+b.chordMm**2)/3;
  }
  return {areaMm2:area*2,aspectRatio:model.spanMm**2/(area*2),meanAerodynamicChordMm:c2/area};
}
export function stationCsv(model) {
  validate(model);
  const quote=v=>'"'+String(v??'').replaceAll('"','""')+'"';
  const header=['fraction','y_mm','chord_mm','leading_edge_mm','height_mm','geometric_twist_deg','pivot_fraction','zero_lift_deg','airfoil','provenance'];
  return [header.join(','),...model.stations.map(s=>[s.fraction,s.fraction*model.spanMm/2,s.chordMm,s.leadingEdgeMm,s.heightMm,s.twistDeg,s.pivotFraction,s.zeroLiftDeg,s.airfoil.name,s.provenance].map(quote).join(','))].join('\n');
}
