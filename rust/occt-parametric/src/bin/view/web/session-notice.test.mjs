import { test } from 'node:test';
import assert from 'node:assert/strict';
import { changeNotice } from './session-notice.mjs';

test('a shared-session change is named only while it is the shown version', () => {
  const change = { author: 'assistant', message: 'Thicker plate', version: 4 };
  assert.equal(changeNotice({ version: 4, last_change: change }), 'assistant changed the model: Thicker plate');
  assert.equal(changeNotice({ version: 5, last_change: change }), '');
  assert.equal(changeNotice({ version: 4, last_change: null }), '');
  assert.equal(changeNotice(undefined), '');
});
