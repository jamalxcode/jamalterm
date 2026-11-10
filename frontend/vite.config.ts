import { defineConfig } from 'vite'

// Two pages: the terminal window (index.html) and the Settings window
// (settings.html).
export default defineConfig({
  build: {
    rollupOptions: {
      input: { main: 'index.html', settings: 'settings.html' },
    },
  },
})
