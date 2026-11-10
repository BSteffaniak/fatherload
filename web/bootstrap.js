// Loaded before Emscripten starts. Only this directory is persisted; no desktop paths are used.
Module.preRun = Module.preRun || [];
Module.preRun.push(function () {
  FS.mkdirTree('/fatherload');
  FS.mount(IDBFS, {}, '/fatherload');
  ENV.FATHERLOAD_STATE_DIR = '/fatherload';
  addRunDependency('fatherload-saves');
  FS.syncfs(true, function (error) {
    if (error) {
      Module.setStatus('Browser storage unavailable. Enable site storage and reload before playing.');
      console.error('Unable to restore Fatherload saves', error);
      return; // Do not run against an empty filesystem and overwrite existing saves.
    }
    removeRunDependency('fatherload-saves');
  });
});
// Coalesce writes; never run overlapping IndexedDB syncs.
Module.fatherloadSaveDirty = false;
Module.fatherloadSaveBusy = false;
Module.flushFatherloadSaves = function () {
  if (!Module.fatherloadSaveDirty || Module.fatherloadSaveBusy) return;
  Module.fatherloadSaveDirty = false;
  Module.fatherloadSaveBusy = true;
  Module.setStatus('Saving in this browser…');
  FS.syncfs(false, function (error) {
    Module.fatherloadSaveBusy = false;
    if (error) {
      Module.fatherloadSaveDirty = true;
      console.error('Unable to persist Fatherload saves', error);
      Module.setStatus('Could not persist saves. Keep this tab open and retry saving.');
    }
    if (!error) Module.setStatus('Saved in this browser.');
    if (!error && Module.fatherloadSaveDirty) Module.flushFatherloadSaves();
  });
};
setInterval(function () { Module.flushFatherloadSaves(); }, 1000);
document.addEventListener('visibilitychange', function () {
  if (document.hidden) {
    Module.fatherloadSuspendRequested = true;
    Module.flushFatherloadSaves();
  }
});
