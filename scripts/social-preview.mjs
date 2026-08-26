/**
 * Renders the GitHub social preview: an HTML page screenshotted by headless
 * Chrome at exactly 1280x640, the size GitHub expects.
 *
 * The app's own icon and fonts are inlined as data URIs rather than linked, so
 * the page is self-contained and nothing depends on relative paths resolving
 * under `file://`. That is also what makes the card look like the product —
 * same platypus, same Geist, same palette as the running app.
 *
 * The Claude mark (svgl.app) states compatibility, which is what the app
 * actually does. It is not a badge of endorsement and should not be dressed up
 * as one.
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
const sans = base64('src/fonts/Geist-Variable.woff2')
const mono = base64('src/fonts/GeistMono-Variable.woff2')
const claude = base64('src/assets/claude.svg')

/*
 * `max-width` is in `ch`, so it guards against overflow on character count
 * rather than pixels. Wide enough (48ch) that a beat separated with `|` keeps
 * its own line — a three-beat tagline reads as three lines or not at all.
 */
const html = `<!doctype html><html><head><meta charset="utf-8"><style>
@font-face{font-family:"Geist";src:url(data:font/woff2;base64,${sans}) format("woff2");font-weight:100 900}
@font-face{font-family:"Geist Mono";src:url(data:font/woff2;base64,${mono}) format("woff2");font-weight:100 900}
*{margin:0;padding:0;box-sizing:border-box}
html,body{width:1280px;height:640px}
body{background:#131419;display:flex;align-items:center;justify-content:center;
     font-family:"Geist",ui-sans-serif,system-ui,sans-serif;overflow:hidden;position:relative}
body::after{content:"";position:absolute;inset:0;
  background:radial-gradient(820px 400px at 50% 44%,rgba(255,255,255,.06),transparent 72%)}
.card{display:flex;flex-direction:column;align-items:center;gap:46px;position:relative;z-index:1}
.row{display:flex;align-items:center;gap:48px}
.icon{width:180px;height:180px;flex:none;border-radius:40px;box-shadow:0 24px 60px rgba(0,0,0,.62)}
h1{font-family:"Geist Mono",ui-monospace,monospace;font-size:74px;font-weight:600;color:#f2f2f0;letter-spacing:-.05em;line-height:1}
p{margin-top:18px;font-size:25px;font-weight:400;color:#9a9aa2;letter-spacing:-.011em;line-height:1.5;max-width:48ch;text-wrap:balance}
/* Horizontally centred under the card, not tucked beside the wordmark. */
.pairs{font-size:20px;font-weight:450;color:#7d7d86;letter-spacing:-.008em}
/* Sits inline immediately before the word it marks, not at the line start. */
.pairs img{width:22px;height:22px;vertical-align:-4px;margin-right:6px}
</style></head><body>
<div class="card">
  <div class="row">
    <img class="icon" src="data:image/png;base64,${icon}">
    <div>
      <h1>onlydiffs</h1>
      <p>${tagline
        .split('|')
        .map((line) => line.trim().replace(/&/g, '&amp;').replace(/</g, '&lt;'))
        .join('<br>')}</p>
    </div>
  </div>
  <div class="pairs">Pairs directly with your <img src="data:image/svg+xml;base64,${claude}" alt="">Claude session</div>
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
