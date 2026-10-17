import {validateLoftConstraint} from './solid-loft-constraints.mjs';
// Ordered loft stations and hole tracks share the same station ordering.
export function loftDefinition(features,definition) {
  const {profiles,holes=[],ruled=false}=definition;
  if(!Array.isArray(profiles)||profiles.length<2||profiles.length>1000)throw Error('Choose 2–1000 loft stations.');
  if(!Array.isArray(holes)||holes.length>100||holes.some(track=>!Array.isArray(track)||track.length!==profiles.length))throw Error('Each hole track needs one profile at every loft station (at most 100 tracks).');
  const ids=[...profiles,...holes.flat()];
  if(ids.some(id=>!id||!features.some(f=>f.id===id)))throw Error('Choose an existing profile for every loft station and hole.');
  if(new Set(ids).size!==ids.length)throw Error('Loft profiles must be distinct across outer stations and hole tracks.');
  if(typeof ruled!=='boolean')throw Error('Choose smooth or ruled loft surfaces.');
  const constraints={};
  for(const key of ['start_constraint','end_constraint'])if(definition[key])constraints[key]=validateLoftConstraint(definition[key]);
  if(Object.keys(constraints).length&&(ruled||profiles.length>64))throw Error('Loft end constraints require smooth surfaces and at most 64 stations.');
  return {profiles:[...profiles],holes:holes.map(track=>[...track]),ruled,...constraints};
}
export function editLoftStations(definition,action,index=0) {
  const result=structuredClone(definition),tracks=[result.profiles,...(result.holes??[])],count=result.profiles.length;
  if(action==='add'){
    if(count>=1000)throw Error('A loft supports at most 1000 stations.');
    for(const track of tracks)track.push('');
  }else{
    if(!Number.isInteger(index)||index<0||index>=count)throw Error('Choose an existing loft station.');
    if(action==='remove'){
      if(count<=2)throw Error('A loft needs at least two stations.');
      for(const track of tracks)track.splice(index,1);
    }else if(action==='earlier'||action==='later'){
      const destination=index+(action==='earlier'?-1:1);
      if(destination<0||destination>=count)throw Error('That loft station cannot move further.');
      for(const track of tracks)[track[index],track[destination]]=[track[destination],track[index]];
    }else throw Error('Choose a loft station action.');
  }
  return result;
}
