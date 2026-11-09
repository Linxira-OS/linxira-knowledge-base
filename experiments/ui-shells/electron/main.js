const { app, BrowserWindow } = require('electron')
const path = require('node:path')

app.setAppUserModelId('dev.lkb.shell.electron-prototype')

// --headless keeps the window off-screen (offscreen rendering, no visible
// window, no focus change). Used only by the benchmark harness.
const headless = process.argv.includes('--headless')

function createWindow() {
  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 600,
    backgroundColor: '#f8fafc',
    title: 'Layered KB',
    show: !headless,
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      offscreen: headless,
      backgroundThrottling: !headless,
    },
  })
  win.loadFile(path.join(__dirname, 'renderer', 'index.html'))
}

app.whenReady().then(() => {
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  app.quit()
})
