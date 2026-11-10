'use strict';
// Check persistence ordering and errors without requiring a browser or IndexedDB.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(require('node:path').join(__dirname, '../web/bootstrap.js'), 'utf8');
function fixture() {
  const syncs = [], dependencies = new Set(), statuses = [], events = {};
  const context = {
    Module: { setStatus: text => statuses.push(text) }, ENV: {}, IDBFS: {},
    FS: { mkdirTree() {}, mount() {}, syncfs: (read, callback) => syncs.push({ read, callback }) },
    addRunDependency: name => dependencies.add(name), removeRunDependency: name => dependencies.delete(name),
    console: { error() {} }, setInterval() {},
    document: { hidden: false, addEventListener: (name, callback) => { events[name] = callback; } },
  };
  vm.runInNewContext(source, context);
  context.Module.preRun[0]();
  return { context, syncs, dependencies, statuses, events };
}
const f = fixture();
assert.equal(f.context.ENV.FATHERLOAD_STATE_DIR, '/fatherload');
assert.equal(f.dependencies.size, 1, 'main must wait for restored storage');
assert.equal(f.syncs[0].read, true);
f.syncs.shift().callback(null);
assert.equal(f.dependencies.size, 0);
f.context.Module.fatherloadSaveDirty = true;
f.context.Module.flushFatherloadSaves();
assert.equal(f.syncs.length, 1);
f.context.Module.fatherloadSaveDirty = true;
f.context.Module.flushFatherloadSaves();
assert.equal(f.syncs.length, 1, 'no overlapping transactions');
f.syncs.shift().callback(null);
assert.equal(f.syncs.length, 1, 'a write during sync must get a follow-up sync');
f.syncs.shift().callback(new Error('quota exceeded'));
assert.equal(f.context.Module.fatherloadSaveDirty, true, 'failed writes remain retryable');
assert.equal(f.context.Module.fatherloadSaveBusy, false);
assert.match(f.statuses.at(-1), /Could not persist/);
f.context.Module.flushFatherloadSaves();
f.syncs.shift().callback(null);
assert.equal(f.context.Module.fatherloadSaveDirty, false);
f.context.document.hidden = true;
f.events.visibilitychange();
assert.equal(f.context.Module.fatherloadSuspendRequested, true);
const failed = fixture();
failed.syncs.shift().callback(new Error('storage blocked'));
assert.equal(failed.dependencies.size, 1, 'never run on an empty filesystem after a failed restore');
assert.match(failed.statuses.at(-1), /storage unavailable/);
console.log('PASS: restore gating, coalesced writes, retryable failures and suspension request.');
