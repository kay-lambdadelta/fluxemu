// This function works around a deficiency in Firefox, which otherwise causes the emulator to freeze

addToLibrary({
  _wasmfs_opfs_open_shared_access__deps: [
    "$wasmfsOPFSFileHandles",
    "$wasmfsOPFSAccessHandles",
    "$wasmfsOPFSProxyFinish",
  ],
  _wasmfs_opfs_open_shared_access: async (ctx, fileID, accessIDPtr) => {
    // This means "fall back to a Blob" which causes freezes on Firefox, so we try to avoid it if possible
    let accessID = -1;
    let fileHandle = wasmfsOPFSFileHandles.get(fileID);

    try {
      let accessHandle = await fileHandle.createSyncAccessHandle({
        mode: "readwrite-unsafe",
      });

      accessID = wasmfsOPFSAccessHandles.allocate(accessHandle);
    } catch (e) {}

    // prettier-ignore
    {{{ makeSetValue('accessIDPtr', 0, 'accessID', 'i32') }}}

    wasmfsOPFSProxyFinish(ctx);
  },
});
