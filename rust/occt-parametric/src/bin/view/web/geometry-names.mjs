import {geometryIds} from './constraint-controls.mjs';
export function geometryNames(definition){
 const used=new Set(['points','lines','circles','arcs','ellipses','splines','texts','patterns','projections','measurements'].flatMap(k=>(definition[k]??[]).map(e=>e.id)));
 for(const shape of definition.shapes??[])for(const value of Object.values(shape))if(value?.id)used.add(value.id);
 for(const group of ['points','curves'])for(const id of geometryIds(definition,group))used.add(id);
 for(const p of definition.projections??[])for(const item of [...(p.intersections??[]),...(p.junctions??[])]){if(item.id)used.add(`${p.id}:${item.id}`);if(item.point)used.add(item.point);}
 for(const op of definition.curve_operations??[])if(op.split){used.add(op.split.before);used.add(op.split.after);}
 return used;
}
export function freshName(definition,prefix='Crossing'){const used=geometryNames(definition);let i=1;while(used.has(`${prefix}${i}`))i++;return `${prefix}${i}`;}
export function validateName(definition,name){
 if(typeof name!=='string'||!name.trim()||name!==name.trim())throw new Error('Enter a new point name without leading or trailing spaces.');
 if(geometryNames(definition).has(name))throw new Error('That geometry name already exists. Choose an existing target or a new name.');
}
