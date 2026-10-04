import test from 'node:test';
import assert from 'node:assert/strict';
import {starter,validate,metrics,sectionPoints,parseAirfoil,cadSections,symmetricAirfoil,resample} from './model.mjs';
const close=(a,b)=>assert.ok(Math.abs(a-b)<1e-8,`${a} != ${b}`);
test('rectangular wing area, AR and MAC match analytic values',()=>{
 const m=starter();m.spanMm=1000;m.stations=[structuredClone(m.stations[0]),structuredClone(m.stations.at(-1))];
 m.stations.forEach(s=>s.chordMm=200);const v=metrics(m);close(v.areaMm2,200000);close(v.aspectRatio,5);close(v.meanAerodynamicChordMm,200);
 m.stations[1].chordMm=100;const t=metrics(m);close(t.areaMm2,150000);close(t.meanAerodynamicChordMm,155.55555555555554);
});
test('positive incidence raises LE, preserves pivot, and mirrors only Y',()=>{
 const m=starter(),s=m.stations[1];s.twistDeg=10;s.airfoil.points=symmetricAirfoil(.12);const right=sectionPoints(m,s),left=sectionPoints(m,s,-1);
 right.forEach((p,i)=>{close(p[0],left[i][0]);close(p[1],-left[i][1]);close(p[2],left[i][2]);});
 const le=right[40];assert.ok(le[2]>s.heightMm);close(le[0],s.leadingEdgeMm+s.pivotFraction*s.chordMm*(1-Math.cos(Math.PI/18)));
 close(Math.hypot(right[0][0]-le[0],right[0][2]-le[2]),s.chordMm);
});
test('Selig parsing preserves profile and rejects count-header formats and reversed surfaces',()=>{
 const points=symmetricAirfoil(.12);const foil=parseAirfoil('Example\n'+points.map(p=>p.join(' ')).join('\n'));assert.equal(foil.points.length,points.length);
 assert.throws(()=>parseAirfoil('Example\n40 40\n0 0'),/Lednicer/);
 assert.throws(()=>parseAirfoil(points.toReversed().map(p=>p.join(' ')).join('\n')),/upper surface|Selig order/);
});
test('CAD sections preserve station planes, common polygon sampling and project roundtrip',()=>{
 const m=validate(JSON.parse(JSON.stringify(starter()))),cad=cadSections(m);assert.equal(cad.halves.length,2);
 for(const [i,half] of cad.halves.entries())for(const [j,section]of half.entries()){
  assert.equal(section.length,80);section.forEach(p=>{assert.ok(p.every(Number.isFinite));close(p[1],(i===0?1:-1)*m.stations[j].fraction*m.spanMm/2);});
 }
});
test('invalid station order, unknown alpha and dimensions are rejected',()=>{
 const m=starter();m.stations[1].fraction=0;assert.throws(()=>validate(m),/increasing/);
 m.stations[1].fraction=.35;m.stations[1].zeroLiftDeg=NaN;assert.throws(()=>validate(m),/Zero-lift/);
 m.stations[1].zeroLiftDeg=null;m.spanMm=0;assert.throws(()=>validate(m),/span/);
});
test('finite trailing-edge gap is closed at its midpoint',()=>{
 const points=[[1,.02],[.75,.09],[.25,.08],[0,0],[.25,-.04],[.75,-.03],[1,-.01]];
 const foil=parseAirfoil(points.map(p=>p.join(' ')).join('\n'));
 const sampled=resample(foil.points);close(sampled[0][0],1);close(sampled[0][1],.005);close(sampled[40][1],0);
 const crossing=structuredClone(points);crossing[5][1]=.15;assert.throws(()=>parseAirfoil(crossing.map(p=>p.join(' ')).join('\n')),/crossing/);
});
