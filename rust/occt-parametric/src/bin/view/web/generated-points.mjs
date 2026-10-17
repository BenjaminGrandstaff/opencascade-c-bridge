// Core-generated names are inspection metadata and never part of a saved definition.
const bindings=new WeakMap(),groups=['lines','circles','arcs','ellipses','splines'];
const signature=value=>JSON.stringify(value??[],(_,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,v[k]])):v);
const generators=definition=>signature([definition.shapes??[],definition.patterns??[],definition.projections??[]]);
export function captureGeneratedPoints(definition,ids,curves={}){
 if(!Array.isArray(ids)||ids.length>40000||ids.some(id=>typeof id!=='string'||!id))return null;
 if(!curves||typeof curves!=='object'||Array.isArray(curves))return null;
 const entries=groups.map(group=>[group,curves[group]??[]]);
 if(entries.some(([,names])=>!Array.isArray(names)||names.some(id=>typeof id!=='string'||!id))||entries.reduce((n,[,names])=>n+names.length,0)>40000)return null;
 return {curves:Object.fromEntries(entries.map(([group,names])=>[group,[...new Set(names)]])),generators:generators(definition),sources:groups.map(group=>[group,new Map((definition[group]??[]).map(entity=>[entity.id,signature(entity)]))]),ids:[...new Set(ids)]};
}
function compatible(definition,snapshot){
 if(!snapshot||generators(definition)!==snapshot.generators)return false;
 for(const [group,previous]of snapshot.sources){const present=new Map((definition[group]??[]).map(e=>[e.id,e]));for(const [id,stamp]of previous)if(!present.has(id)||signature(present.get(id))!==stamp)return false;}
 return true;
}
export function bindGeneratedPoints(definition,snapshot){
 bindings.delete(definition);if(!compatible(definition,snapshot))return false;bindings.set(definition,snapshot);return true;
}
export function generatedReferenceBinding(definition){const snapshot=bindings.get(definition);return compatible(definition,snapshot)?snapshot:null;}
export function generatedPointIds(definition){return generatedReferenceBinding(definition)?.ids??[];}

export function generatedCurveIds(definition,groups){const snapshot=bindings.get(definition);return compatible(definition,snapshot)?groups.flatMap(group=>snapshot.curves[group]??[]):[];}

// Carry verified names across a local clone only while its generators/sources agree.
export function inheritGeneratedReferences(source,target){return bindGeneratedPoints(target,bindings.get(source));}
