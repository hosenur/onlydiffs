/**
 * Renders the GitHub social preview: an HTML page screenshotted by headless
 * Chrome at exactly 1280x640, the size GitHub expects.
 *
 * The app's own icon and font are inlined as data URIs rather than linked, so
 * the page is self-contained and nothing depends on relative paths resolving
 * under `file://`. That is also what makes the card look like the product —
 * same platypus, same Paper Mono, same palette as the running app.
 *
 *   bun scripts/social-preview.mjs "Your tagline here"
 *   bun scripts/social-preview.mjs "First line | second line"
 *   bun scripts/social-preview.mjs "Tagline" --out build/social-preview.png
 *
 * A `|` forces a line break. Automatic wrapping breaks on width alone and will
 * happily split "a chat box" across two lines, which looks wrong on something
 * this deliberate — so decide the breaks yourself.
 *
 * Upload the result at github.com/<owner>/<repo>/settings under "Social
 * preview" — GitHub has no API for it, so that step stays manual.
 */
import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { tmpdir } from 'node:os'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

const CHROME_CANDIDATES = [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/Applications/Chromium.app/Contents/MacOS/Chromium',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
]

const args = process.argv.slice(2)
const outFlag = args.indexOf('--out')
const out = resolve(root, outFlag === -1 ? 'build/social-preview.png' : args[outFlag + 1])
const tagline = (outFlag === -1 ? args : args.slice(0, outFlag)).join(' ').trim()

if (!tagline) {
  console.error('usage: bun scripts/social-preview.mjs "<tagline>" [--out <path>]')
  process.exit(1)
}

const chrome = CHROME_CANDIDATES.find((path) => existsSync(path))
if (!chrome) {
  console.error(`no Chrome found. Looked in:\n  ${CHROME_CANDIDATES.join('\n  ')}`)
  process.exit(1)
}

const base64 = (path) => readFileSync(join(root, path)).toString('base64')
const icon = base64('build/icon.png')
const font = base64('src/fonts/PaperMono-Variable.woff2')

/*
 * `max-width` is in `ch`, so it guards against overflow on character count
 * rather than pixels. Wide enough (48ch) that a beat separated with `|` keeps
 * its own line — a three-beat tagline reads as three lines or not at all.
 */
const html = `<!doctype html><html><head><meta charset="utf-8"><style>
@font-face{font-family:"Paper Mono";src:url(data:font/woff2;base64,${font}) format("woff2");font-weight:100 800}
*{margin:0;padding:0;box-sizing:border-box}
html,body{width:1280px;height:640px}
body{background:#131419;display:flex;align-items:center;justify-content:center;
     font-family:"Paper Mono",ui-monospace,monospace;overflow:hidden;position:relative}
body::after{content:"";position:absolute;inset:0;
  background:radial-gradient(820px 400px at 50% 46%,rgba(255,255,255,.06),transparent 72%)}
.row{display:flex;align-items:center;gap:48px;position:relative;z-index:1}
.icon{width:196px;height:196px;flex:none;border-radius:44px;box-shadow:0 24px 60px rgba(0,0,0,.62)}
h1{font-size:78px;font-weight:600;color:#f2f2f0;letter-spacing:-.05em;line-height:1}
p{margin-top:20px;font-size:25px;font-weight:400;color:#9a9aa2;letter-spacing:-.012em;line-height:1.5;max-width:48ch;text-wrap:balance}
.rule{margin-top:26px;display:flex;gap:10px;font-size:19px;color:#63636b}
.add{color:#5fb87a}.del{color:#d2665c}
</style></head><body>
<div class="row">
  <img class="icon" src="data:image/png;base64,${icon}">
  <div>
    <h1>onlydiffs</h1>
    <p>${tagline
      .split('|')
      .map((line) => line.trim().replace(/&/g, '&amp;').replace(/</g, '&lt;'))
      .join('<br>')}</p>
    <div class="rule"><span class="add">+ staged</span><span>/</span><span class="del">- unstaged</span><span>/</span><span>untracked</span></div>
  </div>
</div>
</body></html>`

const page = join(tmpdir(), `onlydiffs-social-${Date.now()}.html`)
writeFileSync(page, html)
mkdirSync(dirname(out), { recursive: true })

execFileSync(
  chrome,
  [
    '--headless',
    '--disable-gpu',
    '--hide-scrollbars',
    // Without this you get a 2560x1280 retina capture, not the 1280x640 GitHub wants.
    '--force-device-scale-factor=1',
    '--window-size=1280,640',
    `--screenshot=${out}`,
    `file://${page}`,
  ],
  { stdio: 'ignore' }
)

console.log(`wrote ${out}`)
console.log(`tagline: ${tagline}`)
