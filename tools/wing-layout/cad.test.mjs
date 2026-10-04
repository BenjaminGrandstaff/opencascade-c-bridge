import {spawnSync} from 'node:child_process';
import {mkdtempSync,writeFileSync,statSync,readFileSync,readdirSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import assert from 'node:assert/strict';
import {starter,cadSections} from './model.mjs';
const binary=process.argv[2];assert.ok(binary,'Pass the built occb-wing-cad executable as an argument.');
const dir=mkdtempSync(join(tmpdir(),'wing-cad-test-'));
function run(name,model){const input=join(dir,name+'.json'),output=join(dir,name+'.step');writeFileSync(input,JSON.stringify(model));const r=spawnSync(binary,[input,output],{encoding:'utf8'});return {...r,output};}
const rectangle=[[0,10],[50,10],[100,10],[100,5],[100,0],[50,0],[0,0],[0,5]];
const prism={schema:'occb-wing-sections-v1',units:'mm',halves:[1,-1].map(side=>[0,1000].map(y=>rectangle.map(([x,z])=>[x,y*side,z])))};
const a=run('analytic-prism',prism);assert.equal(a.status,0,a.stderr);assert.match(a.stdout,/Half 1: valid solid, volume 1000000.000 mm³/);assert.match(a.stdout,/Half 2: valid solid, volume 1000000.000 mm³/);assert.match(a.stdout,/roundtrips verified/);
const b=run('starter',cadSections(starter()));assert.equal(b.status,0,b.stderr);assert.match(b.stdout,/roundtrips verified/);assert.ok(statSync(b.output).size>1000);
const invalid=structuredClone(prism);invalid.halves[0][1][2][1]=999;const c=run('invalid-plane',invalid);assert.notEqual(c.status,0);assert.match(c.stderr,/constant-Y/);
// A project file builds the parametric family: smooth sections, stored requirements, a model document.
const volume=r=>Number(r.stdout.match(/Half 1: valid solid, volume ([\d.]+)/)[1]);
const p=run('starter-project',starter());assert.equal(p.status,0,p.stderr);
assert.match(p.stdout,/Requirement right\.single-solid passed/);assert.match(p.stdout,/Requirement left\.valid passed/);
assert.match(p.stdout,/smooth sections/);assert.match(p.stdout,/roundtrips verified/);
const document=JSON.parse(readFileSync(p.output.replace(/\.step$/,'.model.json'),'utf8'));
assert.ok(document.schema_version>=49);assert.equal(document.family.features.length,2);
// Smooth sections enclose the inscribed polygons, by well under one percent here.
const ratio=volume(p)/volume(b);assert.ok(ratio>1&&ratio<1.01,`smooth/polygon volume ratio ${ratio}`);
const broken=starter();broken.stations[1].fraction=0;const q=run('broken-project',broken);assert.notEqual(q.status,0);assert.match(q.stderr,/fractions must increase/);
// A build file adds a swept spar channel, elevons, and print segments, each checked by requirements.
const build={schema:'occb-wing-build-v1',spar:{chordFraction:.25,diameterMm:8,toStation:2},
  elevon:{fromFraction:.5,toFraction:.95,hingeFraction:.75,gapMm:1},segments:4,printer:{bedMm:[256,256,256],maxOverhangDeg:45}};
function runBuild(name,model,config){const input=join(dir,name+'.json'),settings=join(dir,name+'.build.json'),output=join(dir,name+'.step');
  writeFileSync(input,JSON.stringify(model));writeFileSync(settings,JSON.stringify(config));
  return {...spawnSync(binary,[input,'--build',settings,output],{encoding:'utf8'}),output};}
const s=runBuild('structure',starter(),build);assert.equal(s.status,0,s.stderr);
const parts=readdirSync(s.output.replace(/\.step$/,'.parts'));assert.equal(parts.length,10);
assert.ok(parts.every(part=>statSync(join(s.output.replace(/\.step$/,'.parts'),part)).size>1000));
assert.equal((s.stdout.match(/single-solid passed/g)||[]).length,12);assert.doesNotMatch(s.stdout,/single-solid FAILED/);
// Parts of one half add up to the half minus the spar channel (~40,100 mm³) and hinge gap (~8,000 mm³).
const half=volume(s);const right=[...s.stdout.matchAll(/Part right_\w+: volume ([\d.]+)/g)].reduce((sum,m)=>sum+Number(m[1]),0);
assert.ok(half-right>40000&&half-right<60000,`removed ${half-right} mm³`);
// The spar channel ends inside segment 2, leaving a ceiling that overhangs when printed upright.
assert.match(s.stdout,/right_segment_2\.overhang FAILED[^\n]*\n\s+at face \d+ \([\d.-]+, 700\.0,/);
const bad=runBuild('bad-build',starter(),{...build,segments:0});assert.notEqual(bad.status,0);assert.match(bad.stderr,/1–50 segments/);
console.log('PASS: analytic prism volumes, starter valid lofts, STEP/BREP roundtrips, nonplanar section rejection, parametric project export, structure build.');console.log(`CAD fixtures: ${dir}`);
