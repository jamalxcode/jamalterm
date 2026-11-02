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

const screen = (page) => page.evaluate(() => document.querySelector('.group.active .pane.focused .xterm-rows')?.innerText ?? '')
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
  assert.equal(data.groups.length, 1)
  assert.equal(data.groups[0].tab.cwd, '/srv')

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

test('command palette finds and runs actions and flips settings', async () => {
  const page = await open()
  await page.evaluate(() => window.__emit('menu', 'palette'))
  await page.waitForSelector('#palette:not([hidden]) input')
  await page.keyboard.type('blink')
  assert.match(await page.locator('#palette li.on .title').innerText(), /Turn on: Blinking cursor/)
  await page.keyboard.press('Enter')
  await page.waitForTimeout(100)
  const saved = await page.evaluate(() => window.__calls.filter((c) => c[0] === 'SaveSettings').pop()?.[1])
  assert.equal(saved.cursorBlink, true)
  assert.equal(await page.locator('#palette').isHidden(), true)

  // An action: new tab.
  await page.waitForTimeout(200)
  await page.evaluate(() => window.__emit('menu', 'palette'))
  await page.keyboard.type('new tab')
  await page.keyboard.press('Enter')
  await page.waitForTimeout(200)
  assert.equal(await page.locator('.tab').count(), 2)

  // A setting without a switch opens Settings on its page.
  await page.waitForTimeout(200)
  await page.evaluate(() => window.__emit('menu', 'palette'))
  await page.keyboard.type('scrollback')
  await page.keyboard.press('Enter')
  await page.waitForSelector('#settings:not([hidden]) .settings-nav button.on')
  assert.match(await page.locator('.settings-nav button.on').innerText(), /Shell/)
  assert.equal(await page.locator('.settings-pages section[data-page="shell"] .flash').count(), 1)
  await page.close()
})

test('Settings has a sidebar with pages', async () => {
  const page = await open()
  await page.evaluate(() => window.__emit('menu', 'settings'))
  await page.waitForSelector('#settings:not([hidden]) .settings-nav')
  const pages = await page.locator('.settings-nav button').allInnerTexts()
  assert.deepEqual(pages.map((p) => p.replace(/^\S+\s*/, '').trim()), ['Appearance', 'Shell', 'AI', 'Privacy', 'Updates'])
  assert.equal(await page.locator('section[data-page="appearance"]').isVisible(), true)
  await page.click('.settings-nav button[data-page="privacy"]')
  assert.equal(await page.locator('section[data-page="appearance"]').isVisible(), false)
  assert.equal(await page.getByText('Keep a private problem log').isVisible(), true)
  await page.close()
})

test('split panes: ⌘D right, ⇧⌘D down, focus moves, ⌘W closes one pane, layout is saved', async () => {
  const page = await open()
  await type(page, 'echo left pane')
  await page.evaluate(() => window.__emit('menu', 'splitRight'))
  await page.waitForTimeout(300)
  assert.equal(await page.locator('.group.active .pane').count(), 2)
  assert.equal(await page.locator('.tab').count(), 1)
  assert.match(await page.locator('.tab.active').innerText(), /⊞2/)
  await type(page, 'echo right pane')
  assert.match(await screen(page), /right pane/)
  assert.doesNotMatch(await screen(page), /left pane/)

  // Side by side: the new pane is to the right of the first.
  const [a, b] = await page.locator('.group.active .pane').evaluateAll((ps) => ps.map((p) => p.getBoundingClientRect().toJSON()))
  assert.ok(b.left >= a.right - 2 && Math.abs(a.top - b.top) < 2, 'side by side')

  await page.waitForTimeout(200)
  await page.evaluate(() => window.__emit('menu', 'splitDown'))
  await page.waitForTimeout(300)
  const rects = await page.locator('.group.active .pane').evaluateAll((ps) => ps.map((p) => p.getBoundingClientRect().toJSON()))
  assert.equal(rects.length, 3)
  assert.ok(rects[2].top >= rects[1].bottom - 2 && Math.abs(rects[2].left - rects[1].left) < 2, 'third pane below the second')

  // Saved layout keeps the splits.
  await page.evaluate(() => window.dispatchEvent(new Event('beforeunload')))
  await page.waitForTimeout(100)
  const saved = JSON.parse(await page.evaluate(() => window.__fake.session))
  assert.equal(saved.version, 2)
  assert.equal(saved.groups[0].dir, 'row')
  assert.equal(saved.groups[0].b.dir, 'column')

  // Click the first pane to focus it; ⌘W closes just that pane.
  await page.locator('.group.active .pane').first().click()
  await page.waitForTimeout(100)
  assert.match(await screen(page), /left pane/)
  await page.evaluate(() => window.__emit('menu', 'closeTab'))
  await page.waitForTimeout(300)
  assert.equal(await page.locator('.group.active .pane').count(), 2)
  assert.equal(await page.locator('.tab').count(), 1)
  await page.close()
})

test('a saved split layout comes back after a restart', async () => {
  const session = JSON.stringify({ version: 2, active: 0, groups: [
    { dir: 'column', ratio: 0.3, a: { dir: 'row', ratio: 0.5, a: { tab: { cwd: '/a' } }, b: { tab: { cwd: '/b' } } }, b: { tab: { cwd: '/c' } } },
  ] })
  const page = await open({ session })
  await page.waitForTimeout(300)
  const rects = await page.locator('.group.active .pane').evaluateAll((ps) => ps.map((p) => p.getBoundingClientRect().toJSON()))
  assert.equal(rects.length, 3)
  // a | b on top (30%), c below across the whole width.
  assert.ok(Math.abs(rects[0].top - rects[1].top) < 2 && rects[1].left > rects[0].left)
  assert.ok(rects[2].top >= rects[0].bottom - 2 && rects[2].width > rects[0].width * 1.8)
  assert.ok(rects[0].height < rects[2].height, 'top row is the smaller part')
  await page.close()
})
