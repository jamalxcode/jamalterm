// Help → Keyboard Shortcuts (the About box itself is the Mac's own panel).
import { Version } from './backend'
import iconUrl from './icon.png'

export async function showAbout(onClose: () => void) {
  const box = document.getElementById('about')!
  const version = await Version()
  box.innerHTML = `<div class="dialog about"><img class="app-icon" src="${iconUrl}" alt="" width="96" height="96"><h2>jamalterm</h2>
    <p class="dim">Version ${version}</p>
    <p>A terminal with a free-floating cursor and AI built in.</p>
    <table class="keys">
      <tr><td><kbd>⌘↑</kbd> or click</td><td>browse past commands</td></tr>
      <tr><td><kbd>⏎</kbd> in browse mode</td><td>run that command again</td></tr>
      <tr><td><kbd>⌘I</kbd></td><td>ask AI</td></tr>
      <tr><td><kbd>⌘J</kbd></td><td>command from plain English</td></tr>
      <tr><td><kbd>⇧⌘P</kbd></td><td>command palette: every action and setting</td></tr>
      <tr><td><kbd>⌘D</kbd> <kbd>⇧⌘D</kbd></td><td>split right / down</td></tr>
      <tr><td><kbd>⌘T</kbd> <kbd>⌘N</kbd></td><td>new tab / new window</td></tr>
      <tr><td><kbd>⌘W</kbd> <kbd>⇧⌘W</kbd></td><td>close pane / tab</td></tr>
    </table>
    <button class="primary">OK</button></div>`
  box.hidden = false
  const close = () => {
    box.hidden = true
    onClose()
  }
  box.querySelector('button')!.addEventListener('click', close)
  box.onmousedown = (e) => e.target === box && close()
  ;(box.querySelector('button') as HTMLElement).focus()
}
