'use strict';
/* eslint-disable @typescript-eslint/no-require-imports */

const { app, BrowserWindow, shell } = require('electron');
const path = require('path');

// In development, load from the running Next.js dev server.
// In production (packaged), load from the static export bundled inside the app.
const isDev = !app.isPackaged;
const DEV_URL = process.env.ELECTRON_START_URL || 'http://localhost:3002';

function createWindow() {
  const win = new BrowserWindow({
    width: 1440,
    height: 960,
    minWidth: 1100,
    minHeight: 760,
    backgroundColor: '#050A14',
    autoHideMenuBar: true,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  if (isDev) {
    win.loadURL(DEV_URL);
    win.webContents.openDevTools({ mode: 'detach' });
  } else {
    // The static export is bundled alongside electron/main.cjs under out/
    win.loadFile(path.join(__dirname, '../out/index.html'));
  }

  // Open external links in the system browser rather than a new Electron window
  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: 'deny' };
  });
}

app.whenReady().then(() => {
  createWindow();

  // macOS: re-create window when dock icon is clicked and no windows are open
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

// Quit on all windows closed — except on macOS where apps typically stay open
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
