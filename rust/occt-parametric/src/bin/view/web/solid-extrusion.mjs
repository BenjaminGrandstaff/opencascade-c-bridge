const length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
export const extrusionExtentMode = extent => typeof extent==='string'?extent:extent?.up_to_face?'up_to_face':extent?.up_to_next?'up_to_next':null;
export function extrusionExtent(mode,target,face='z:maximum',preservedFace=null) {
  if(['distance','symmetric'].includes(mode))return mode;
  if(!target)throw Error('Choose a limiting feature.');
  if(mode==='up_to_next')return {up_to_next:{target}};
  if(mode!=='up_to_face')throw Error('Choose an extrusion extent.');
  if(face==='preserve'){
    if(!preservedFace)throw Error('Choose a limiting face.');
    return {up_to_face:{target,face:structuredClone(preservedFace)}};
  }
  if(!/^[xyz]:(minimum|maximum)$/.test(face))throw Error('Choose a limiting face.');
  const [axis,extremum]=face.split(':');
  return {up_to_face:{target,face:{at_extreme:{axis,extremum,tolerance:length(1e-6)}}}};
}
export function checkedExtrusionExtent(features,extent,input,id) {
  const mode=extrusionExtentMode(extent);
  if(!['distance','symmetric','up_to_face','up_to_next'].includes(mode))throw Error('Choose a supported extrusion extent.');
  if(typeof extent==='string'&&!['distance','symmetric'].includes(extent))throw Error('Choose a limiting feature for geometric extents.');
  if(typeof extent!=='string'){
    if(Object.keys(extent).length!==1)throw Error('Choose one extrusion extent.');
    const limit=extent[mode];
    if(!features.some(f=>f.id===limit.target)||limit.target===input||limit.target===id)throw Error('Choose a separate existing limiting feature.');
    if(mode==='up_to_face'&&(!limit.face||typeof limit.face!=='object'||Array.isArray(limit.face)))throw Error('Choose a limiting face.');
  }
  return structuredClone(extent);
}
