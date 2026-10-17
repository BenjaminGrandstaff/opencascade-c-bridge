import test from 'node:test';
import assert from 'node:assert/strict';
import { instanceBlock, instanceOptions } from './sketch-studio-instances.mjs';

test('instances are listed with their overrides', () => {
  assert.deepEqual(instanceOptions([{ id: 'part', overrides: [] }, { id: 'wide', overrides: ['depth', 'width'] }, { id: 'one', overrides: ['width'] }]), [
    { value: 'part', label: 'part', title: 'Family defaults' },
    { value: 'wide', label: 'wide · 2 overrides', title: 'Overrides: depth, width' },
    { value: 'one', label: 'one · 1 override', title: 'Overrides: width' },
  ]);
  assert.deepEqual(instanceOptions(undefined), []);
});

test('the instance changes only without pending requests or draft edits', () => {
  assert.equal(instanceBlock({ hasDraftHistory: () => false, draft: null }), null);
  assert.equal(instanceBlock({ hasDraftHistory: () => false, draft: { feature: 'side', dirty: false } }), null, 'an untouched draft is discarded');
  assert.match(instanceBlock({ hasDraftHistory: () => true, draft: { feature: 'side' } }), /draft of side/);
  assert.match(instanceBlock({ hasDraftHistory: () => false, draft: { feature: 'side', dirty: true } }), /draft of side/);
  assert.match(instanceBlock({ modelPending: true }), /Wait/);
});
