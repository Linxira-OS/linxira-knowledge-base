const { contextBridge } = require('electron')

// Marks the page as running inside the Electron shell so the shared UI can
// label itself. No other Node capability is exposed to the renderer.
contextBridge.exposeInMainWorld('__ELECTRON__', true)
