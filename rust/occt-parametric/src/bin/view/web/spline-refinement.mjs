import {profileIds} from './profile-controls.mjs';
// Rational refinement uses affine combinations of homogeneous poles. Keeping
// coefficients relative to source poles avoids recursively expanding formulas.
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const unit=e=>e?.literal?.dimension==='scalar'&&e.literal.unit==null&&e.literal.value===1;
const multiply=(a,b)=>unit(a)?b:unit(b)?a:{multiply:[a,b]};
function sum(terms){if(terms.length===1)return terms[0];const mid=Math.floor(terms.length/2);return {add:[sum(terms.slice(0,mid)),sum(terms.slice(mid))]};}
const blend=(a,b,t)=>a.map((v,i)=>(1-t)*v+t*b[i]);
function insert(rows,knots,degree,value){
  const last=rows.length-1,k=knots.findLastIndex(u=>u<=value),s=knots.filter(u=>u===value).length;
  if(s>=degree)throw new Error('Internal knot multiplicity already equals the degree.');
  const next=Array(rows.length+1);
  for(let i=0;i<=k-degree;i++)next[i]=rows[i];
  for(let i=k-s;i<=last;i++)next[i+1]=rows[i];
  for(let i=k-degree+1;i<=k-s;i++){const t=(value-knots[i])/(knots[i+degree]-knots[i]);next[i]=blend(rows[i-1],rows[i],t);}
  return {rows:next,knots:[...knots.slice(0,k+1),value,...knots.slice(k+1)]};
}
export function refineSpline(definition,splineId,{action,knot,degree,transformation}){
  const source=definition.splines?.find(s=>s.id===splineId),basis=source?.basis;
  if(!basis)throw new Error('Exact refinement requires an explicit B-spline.');
  if(!transformation&&(basis.periodic||basis.non_clamped))throw new Error('Native refinement is required for periodic and non-clamped bases.');
  const poles=source.points.map(id=>definition.points?.find(p=>p.id===id));
  if(poles.some(p=>!p||!p.fixed))throw new Error('Exact refinement requires fixed authored poles; free or projected poles need their native solved geometry.');
  const p=basis.degree,n=poles.length,periodic=Boolean(basis.periodic),nonClamped=Boolean(basis.non_clamped);
  if(!Number.isInteger(p)||p<1||p>25||n<p+1||n>256||basis.knots.length<2||basis.knots.length!==basis.multiplicities.length||basis.knots.some((k,i)=>!Number.isFinite(k)||i>0&&k<=basis.knots[i-1])||basis.multiplicities.some((m,i)=>!Number.isInteger(m)||m<1||m>(i===0||i===basis.knots.length-1?p+1:p))||periodic&&nonClamped||!periodic&&!nonClamped&&(basis.multiplicities[0]!==p+1||basis.multiplicities.at(-1)!==p+1)||periodic&&(basis.multiplicities[0]!==basis.multiplicities.at(-1)||basis.multiplicities[0]>p)||basis.multiplicities.reduce((a,b)=>a+b,0)!==n+(periodic?basis.multiplicities.at(-1):p+1))throw new Error('Use a valid basis with at most 256 control poles.');
  if(nonClamped&&(!source.endpoints||[source.endpoints.start,source.endpoints.end].some(id=>!profileIds(definition,'points').includes(id)||source.points.includes(id))))throw new Error('Keep known native endpoint references separate from control poles.');
  if(basis.weights?.length&&basis.weights.length!==n)throw new Error('Each pole needs one weight.');
  let rows=poles.map((_,i)=>poles.map((_,j)=>i===j?1:0)),knots=basis.knots.flatMap((k,i)=>Array(basis.multiplicities[i]).fill(k)),target=p;
  if(transformation){
    const t=transformation,expected=action==='insert_knot'?p:degree;
    if(!['insert_knot','elevate_degree'].includes(action)||t.degree!==expected||!Array.isArray(t.coefficients)||!t.coefficients.length||t.coefficients.length>256||action==='insert_knot'&&t.coefficients.length!==n+1||action==='elevate_degree'&&(!Number.isInteger(degree)||degree<=p||degree>25)||t.coefficients.some(row=>!Array.isArray(row)||row.length!==n||row.some(v=>!Number.isFinite(v)||v<0)||Math.abs(row.reduce((a,b)=>a+b,0)-1)>1e-10)||!Array.isArray(t.knots)||t.knots.length<2||t.knots.some((k,i)=>!Number.isFinite(k)||i>0&&k<=t.knots[i-1])||!Array.isArray(t.multiplicities)||t.multiplicities.length!==t.knots.length||t.multiplicities.some((m,i)=>!Number.isInteger(m)||m<1||m>t.degree+(!periodic&&(i===0||i===t.knots.length-1)?1:0))||t.multiplicities.reduce((a,b)=>a+b,0)!==t.coefficients.length+(periodic?t.multiplicities.at(-1):t.degree+1))throw new Error('Native refinement returned an inconsistent transformation.');
    rows=t.coefficients;target=t.degree;knots=t.knots.flatMap((k,i)=>Array(t.multiplicities[i]).fill(k));
  }else if(action==='insert_knot'){
    if(!Number.isFinite(knot)||knot<=knots[0]||knot>=knots.at(-1))throw new Error('Insert a finite knot strictly inside the curve domain.');
    ({rows,knots}=insert(rows,knots,p,knot));
  }else if(action==='elevate_degree'){
    if(!Number.isInteger(degree)||degree<=p||degree>25)throw new Error('Elevated degree must be an integer above the current degree and at most 25.');
    const spans=basis.knots.length-1;
    if(spans*degree+1>256)throw new Error('Refinement exceeds the 256-pole limit.');
    for(const k of basis.knots.slice(1,-1))while(knots.filter(u=>u===k).length<p)({rows,knots}=insert(rows,knots,p,k));
    const elevated=[];
    for(let span=0;span<spans;span++){
      let arc=rows.slice(span*p,span*p+p+1);
      for(let d=p;d<degree;d++){const next=[arc[0]];for(let i=1;i<=d;i++)next.push(blend(arc[i],arc[i-1],i/(d+1)));next.push(arc.at(-1));arc=next;}
      elevated.push(...arc.slice(span?1:0));
    }
    rows=elevated;target=degree;knots=basis.knots.flatMap((k,i)=>Array(i===0||i===basis.knots.length-1?degree+1:degree).fill(k));
  }else throw new Error('Choose knot insertion or degree elevation.');
  if(rows.length>256)throw new Error('Refinement exceeds the 256-pole limit.');
  const next=structuredClone(definition),spline=next.splines.find(s=>s.id===splineId),weights=basis.weights?.length?basis.weights:poles.map(()=>scalar(1));
  const names=new Set(['points','lines','circles','arcs','ellipses','splines','shapes','texts','patterns','projections','measurements'].flatMap(key=>(definition[key]??[]).map(e=>e.id)));
  for(const kind of ['points','curves'])for(const id of profileIds(definition,kind))names.add(id);
  let serial=1;const name=()=>{let id;do{id=`${splineId}_refined_${serial++}`;}while(names.has(id));names.add(id);return id;};
  const newWeights=[];spline.points=rows.map(row=>{
    const terms=row.map((c,i)=>({c,i})).filter(t=>t.c!==0),original=terms.length===1&&terms[0].c===1?terms[0].i:null;
    if(original!==null){newWeights.push(structuredClone(weights[original]));return source.points[original];}
    const weighted=terms.map(({c,i})=>multiply(scalar(c),structuredClone(weights[i]))),weight=sum(weighted);
    const coordinate=axis=>{const numerator=sum(terms.map(({i},j)=>multiply(weighted[j],structuredClone(poles[i][axis]))));return unit(weight)?numerator:{divide:[numerator,weight]};};
    const id=name();next.points.push({id,x:coordinate('x'),y:coordinate('y'),fixed:true});newWeights.push(weight);return id;
  });
  const unique=[...new Set(knots)];Object.assign(spline.basis,{degree:target,knots:unique,multiplicities:unique.map(k=>knots.filter(u=>u===k).length),weights:weights.every(unit)?[]:newWeights});
  if(JSON.stringify(next).length>1000000)throw new Error('Refinement exceeds the expression-size limit.');
  return next;
}
