import {spawnSync} from 'node:child_process';
import {mkdtempSync,writeFileSync,statSync} from 'node:fs';
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
console.log('PASS: analytic prism volumes, starter valid lofts, STEP/BREP roundtrips, nonplanar section rejection.');console.log(`CAD fixtures: ${dir}`);
