// UI tests: the real jamalterm interface in Chromium with a fake Go side
// (tests/fake-wails.js). Run: npm run build && node --test tests/
import { test, before, after } from 'node:test'
import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { extname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chromium } from 'playwright'

const dist = fileURLToPath(new URL('../dist/', import.meta.url))
const fake = await readFile(new URL('./fake-wails.js', import.meta.url), 'utf8')
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.ttf': 'font/ttf', '.woff2': 'font/woff2', '.woff': 'font/woff' }
let server, base, browser

before(async () => {
  server = createServer(async (req, res) => {
    const path = join(dist, decodeURIComponent(req.url.split('?')[0]) === '/' ? 'index.html' : decodeURIComponent(req.url.split('?')[0]))
    try {
      res.writeHead(200, { 'Content-Type': types[extname(path)] || 'application/octet-stream' })
      res.end(await readFile(path))
    } catch {
      res.writeHead(404).end()
    }
  })
  await new Promise((r) => server.listen(0, '127.0.0.1', r))
  base = `http://127.0.0.1:${server.address().port}/`
  browser = await chromium.launch()
})
after(async () => {
  await browser?.close()
  server?.close()
})

async function open(opts = {}) {
  const ctx = await browser.newContext({ viewport: { width: 1200, height: 700 } })
  await ctx.grantPermissions(['clipboard-read', 'clipboard-write'], { origin: base })
  const page = await ctx.newPage()
  await page.addInitScript((o) => {
    window.__settings = o.settings
    window.__session = o.session
  }, opts)
  await page.addInitScript(fake)
  await page.goto(base)
  await page.waitForFunction(() => document.querySelectorAll('.tab').length > 0)
  await page.waitForTimeout(300)
  return page
}

const screen = (page) => page.evaluate(() => document.querySelector('.pane.active .xterm-rows')?.innerText ?? '')
async function type(page, text) {
  await page.keyboard.type(text)
  await page.keyboard.press('Enter')
  await page.waitForTimeout(150)
}
const lastWrite = (page) => page.evaluate(() => window.__calls.filter((c) => c[0] === 'Write').map((c) => c[2]).join(''))

test('runs commands in a tab', async () => {
  const page = await open()
  await type(page, 'echo hello world')
  assert.match(await screen(page), /hello world/)
  await page.close()
})

test('↑ at an empty prompt starts browse mode and ⏎ re-runs the command', async () => {
  const page = await open()
  await type(page, 'echo first')
  await type(page, 'echo second')
  await page.keyboard.press('ArrowUp')
  await page.waitForTimeout(100)
  assert.equal(await page.isVisible('#hint'), true, 'hint bar shows in browse mode')
  await page.keyboard.press('Alt+ArrowUp')
  await page.keyboard.press('Alt+ArrowUp')
  await page.waitForTimeout(100)
  assert.match(await page.textContent('#hint'), /echo first/)
  await page.evaluate(() => (window.__calls.length = 0))
  await page.keyboard.press('Enter')
  await page.waitForTimeout(150)
  assert.equal(await lastWrite(page), '\x05\x15echo first\r', 'clears the prompt line, then runs')
  assert.equal(await page.isVisible('#hint'), false, 'back to the prompt')
  await page.close()
})

test('re-running a command from another folder warns first, ⌥⏎ runs it there', async () => {
  const page = await open()
  await type(page, 'echo here')
  await type(page, 'cd /tmp')
  await page.keyboard.press('ArrowUp')
  await page.keyboard.press('Alt+ArrowUp')
  await page.keyboard.press('Alt+ArrowUp')
  await page.waitForTimeout(100)
  await page.keyboard.press('Enter')
  await page.waitForTimeout(100)
  assert.match(await page.textContent('#hint'), /ran in.*you're in/)
  await page.evaluate(() => (window.__calls.length = 0))
  await page.keyboard.press('Alt+Enter')
  await page.waitForTimeout(150)
  assert.equal(await lastWrite(page), "\x05\x15cd '/home/u' && echo here\r")
  await page.close()
})

test('vi prompt keys clear the line with Esc cc', async () => {
  const page = await open({ settings: { promptKeys: 'vi' } })
  await type(page, 'echo vi')
  await page.keyboard.press('ArrowUp')
  await page.keyboard.press('Alt+ArrowUp')
  await page.evaluate(() => (window.__calls.length = 0))
  await page.keyboard.press('Enter')
  await page.waitForTimeout(150)
  assert.equal(await lastWrite(page), '\x1bccecho vi\r')
  await page.close()
})

test('tabs: new tab, numbers, failed command shows ✕', async () => {
  const page = await open()
  await type(page, 'false')
  await page.evaluate(() => window.runtime.EventsEmit('menu', 'newTab'))
  await page.waitForTimeout(300)
  const tabs = await page.$$eval('.tab', (els) => els.map((e) => ({ cls: e.className, num: e.querySelector('.num')?.textContent, state: e.querySelector('.state')?.textContent })))
  assert.equal(tabs.length, 2)
  assert.deepEqual(tabs.map((t) => t.num), ['1', '2'])
  assert.match(tabs[1].cls, /active/)
  assert.match(tabs[0].cls, /failed/)
  assert.equal(tabs[0].state, '✕')
  await page.close()
})

test('OSC 52 copies to the clipboard', async () => {
  const page = await open()
  await type(page, 'osc52 copied-from-the-shell')
  await page.waitForTimeout(200)
  assert.equal(await page.evaluate(() => navigator.clipboard.readText()), 'copied-from-the-shell')
  await page.close()
})

test('AI panel streams an answer and shows the model and tokens', async () => {
  const page = await open()
  await page.evaluate(() => window.runtime.EventsEmit('menu', 'aiPanel'))
  await page.waitForSelector('#ai-panel:not([hidden])')
  await page.fill('#ai-panel textarea', 'what happened?')
  await page.keyboard.press('Enter')
  await page.waitForSelector('.msg-meta')
  assert.match(await page.textContent('.msg.assistant'), /Here you go\./)
  assert.match(await page.textContent('.msg-meta'), /claude-opus-5-5 · 1\.2k in · 34 out/)
  await page.close()
})

test('AI asking to run a command waits for the user to click Run', async () => {
  const page = await open()
  await page.evaluate(() => (window.__fake.aiTool = { command: 'echo from-ai', run: true }))
  await page.evaluate(() => window.runtime.EventsEmit('menu', 'aiPanel'))
  await page.fill('#ai-panel textarea', 'run hello')
  await page.keyboard.press('Enter')
  await page.waitForSelector('.tool-card.ask')
  assert.doesNotMatch(await lastWrite(page), /echo from-ai/, 'nothing runs before the click')
  await page.click('.tool-card button.primary')
  await page.waitForTimeout(400)
  assert.match(await lastWrite(page), /echo from-ai\r/)
  const result = await page.evaluate(() => window.__calls.find((c) => c[0] === 'AIToolResult'))
  assert.match(result[2], /Exit code 0/)
  await page.close()
})

test('tabs are saved and come back after a restart', async () => {
  const page = await open()
  await type(page, 'cd /srv')
  await type(page, 'echo remember-me')
  await page.waitForTimeout(3300) // saved every 3 s
  const session = await page.evaluate(() => window.__fake.session)
  await page.close()
  const data = JSON.parse(session)
  assert.equal(data.tabs.length, 1)
  assert.equal(data.tabs[0].cwd, '/srv')

  const again = await open({ session })
  assert.match(await screen(again), /remember-me[\s\S]*restored from your last session/)
  const started = await again.evaluate(() => window.__calls.find((c) => c[0] === 'Start'))
  assert.equal(started[4], '/srv', 'the shell starts in the saved folder')
  await again.close()
})

test('window opacity makes the background see-through', async () => {
  const page = await open({ settings: { opacity: 60 } })
  const bg = await page.evaluate(() => getComputedStyle(document.querySelector('#terminals')).backgroundColor)
  assert.match(bg, /rgba\(21, 22, 30, 0\.6\)/)
  await page.close()
})

test('Copy Diagnostics shows the report first and copies only on Copy', async () => {
  const page = await open()
  await page.evaluate(() => window.__emit('menu', 'diagnostics'))
  await page.waitForSelector('#diagnostics textarea')
  assert.match(await page.inputValue('#diagnostics textarea'), /Version:\s+test/)
  await page.evaluate(() => navigator.clipboard.writeText('before'))
  await page.click('#diagnostics button:has-text("Close")')
  assert.equal(await page.evaluate(() => navigator.clipboard.readText()), 'before')
  await page.waitForTimeout(200) // the menu ignores repeats within 150 ms
  await page.evaluate(() => window.__emit('menu', 'diagnostics'))
  await page.click('#diagnostics button:has-text("Copy")')
  assert.match(await page.evaluate(() => navigator.clipboard.readText()), /jamalterm diagnostics/)
  assert.equal(await page.locator('#diagnostics').count(), 0)
  await page.close()
})

test('script errors in the window go to the problem log', async () => {
  const page = await open()
  await page.evaluate(() => setTimeout(() => { throw new Error('boom here') }, 0))
  await page.waitForTimeout(100)
  const logged = await page.evaluate(() => window.__calls.filter((c) => c[0] === 'LogProblem'))
  assert.equal(logged.length, 1)
  assert.match(logged[0][2], /boom here/)
  await page.close()
})
