// The fun bits: a "finish line" moment when a long command ends, an optional
// soft sound, and a private stats card (Help → Your Day in jamalterm).
// Stats stay in this Mac's browser storage and keep only program names
// ("npm", "git"), never arguments or output.
import { el, duration, escapeHtml, stored, store } from './util'

export interface Finished {
  command: string
  ok: boolean
  exitCode: number
  ms: number
}

/** Long enough to deserve a moment. */
export const FINISH_AFTER_MS = 60_000

/** The program a command ran ("sudo npm run build" → "npm"). */
export function programOf(command: string): string {
  const words = command.trim().split(/\s+/).filter((w) => !/^\w+=/.test(w)) // skip FOO=bar
  const first = words[0] === 'sudo' || words[0] === 'time' ? words[1] : words[0]
  return (first ?? '').split('/').pop() ?? ''
}

// ---- Finish line ----------------------------------------------------------------

export function finishLine(f: Finished, sound: boolean) {
  document.getElementById('finish-line')?.remove()
  const prog = programOf(f.command) || 'command'
  const card = el('div', { id: 'finish-line', class: f.ok ? 'ok' : 'bad', role: 'status' },
    el('span', { class: 'mark', text: f.ok ? '✓' : '✕' }),
    el('span', {}, el('strong', { text: prog }), f.ok ? ` done in ${duration(f.ms)}` : ` failed after ${duration(f.ms)} (exit ${f.exitCode})`))
  document.body.append(card)
  setTimeout(() => card.classList.add('leaving'), 3600)
  setTimeout(() => card.remove(), 4200)
  if (sound) chime(f.ok)
}

/** Two soft rising notes for success, one low note for failure. */
export function chime(ok: boolean) {
  try {
    const ctx = new AudioContext()
    const notes = ok ? [660, 880] : [220]
    notes.forEach((freq, i) => {
      const o = ctx.createOscillator()
      const g = ctx.createGain()
      o.type = 'sine'
      o.frequency.value = freq
      const t0 = ctx.currentTime + i * 0.12
      g.gain.setValueAtTime(0, t0)
      g.gain.linearRampToValueAtTime(0.08, t0 + 0.02)
      g.gain.exponentialRampToValueAtTime(0.0001, t0 + 0.45)
      o.connect(g).connect(ctx.destination)
      o.start(t0)
      o.stop(t0 + 0.5)
    })
    setTimeout(() => void ctx.close(), 1200)
  } catch {
    /* no audio */
  }
}

// ---- Stats ----------------------------------------------------------------------

interface Day {
  commands: number
  failed: number
  ms: number
  longest?: { program: string; ms: number }
  programs: Record<string, number>
}

const KEY = 'jt-stats'
const today = (d = new Date()) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`

function load(): Record<string, Day> {
  try { return JSON.parse(stored(KEY) ?? '{}') } catch { return {} }
}

/** Counts a finished command (program name and timing only). */
export function recordStat(f: Finished) {
  const all = load()
  const k = today()
  const d: Day = all[k] ?? { commands: 0, failed: 0, ms: 0, programs: {} }
  const prog = programOf(f.command)
  d.commands++
  if (!f.ok) d.failed++
  d.ms += f.ms
  if (prog) d.programs[prog] = (d.programs[prog] ?? 0) + 1
  if (prog && (!d.longest || f.ms > d.longest.ms)) d.longest = { program: prog, ms: f.ms }
  all[k] = d
  // Two weeks is plenty.
  for (const day of Object.keys(all).sort().slice(0, -14)) delete all[day]
  store(KEY, JSON.stringify(all))
}

export function resetStats() {
  store(KEY, '{}')
}

/** The numbers for the card: today and the last 7 days. */
export function summary(now = new Date()) {
  const all = load()
  const t = all[today(now)] ?? { commands: 0, failed: 0, ms: 0, programs: {} }
  const week: Day = { commands: 0, failed: 0, ms: 0, programs: {} }
  for (let i = 0; i < 7; i++) {
    const d = all[today(new Date(now.getTime() - i * 86_400_000))]
    if (!d) continue
    week.commands += d.commands
    week.failed += d.failed
    week.ms += d.ms
    for (const [p, n] of Object.entries(d.programs)) week.programs[p] = (week.programs[p] ?? 0) + n
  }
  const top = Object.entries(week.programs).sort((a, b) => b[1] - a[1]).slice(0, 5)
  return { today: t, week, top }
}

export function showStats(onClose: () => void) {
  const box = document.getElementById('about')!
  const { today: t, week, top } = summary()
  const rate = (d: Day) => (d.commands ? `${Math.round(((d.commands - d.failed) / d.commands) * 100)}% worked` : '—')
  const bars = top.map(([p, n]) => {
    const w = Math.max(6, Math.round((n / (top[0]?.[1] || 1)) * 100))
    return `<tr><td><code>${escapeHtml(p)}</code></td><td class="bar"><i style="width:${w}%"></i></td><td>${n}</td></tr>`
  }).join('')
  box.innerHTML = `<div class="dialog stats"><h2>Your day in jamalterm</h2>
    <div class="stat-row">
      <div><b>${t.commands}</b><span>commands today</span></div>
      <div><b>${t.failed}</b><span>failed (${rate(t)})</span></div>
      <div><b>${t.longest ? duration(t.longest.ms) : '—'}</b><span>longest${t.longest ? `: <code>${escapeHtml(t.longest.program)}</code>` : ''}</span></div>
    </div>
    <h3>Last 7 days: ${week.commands} commands, ${rate(week)}</h3>
    ${bars ? `<table class="top">${bars}</table>` : '<p class="dim">Run a few commands and come back.</p>'}
    <p class="dim small">Kept only on this Mac: program names and timings, never what you typed after them or any output.</p>
    <div class="row"><button class="ghost reset">Reset</button><span class="grow"></span><button class="primary ok">OK</button></div></div>`
  box.hidden = false
  const close = () => { box.hidden = true; onClose() }
  box.querySelector('.ok')!.addEventListener('click', close)
  box.querySelector('.reset')!.addEventListener('click', () => { resetStats(); showStats(onClose) })
  box.onmousedown = (e) => e.target === box && close()
  ;(box.querySelector('.ok') as HTMLElement).focus()
}
