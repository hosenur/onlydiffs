import { describe, expect, test } from 'bun:test'
import {
  type AgentStatus,
  composerPlaceholder,
  deliveryNote,
  preferredAgent,
  statusLabel,
} from './agents'

const live = (sessions = 1): AgentStatus => ({ connected: true, sessions })
const absent: AgentStatus = { connected: false, sessions: 0 }
/** A session is running but cannot be reached: Codex not attached to the
 *  daemon, or Claude started without its channel flag. */
const undelivered: AgentStatus = { connected: false, sessions: 1 }

describe('statusLabel', () => {
  test('keeps "not asked yet" apart from "nothing there"', () => {
    // Before the first probe answers, the bar must not claim there is no
    // session — it has not looked.
    expect(statusLabel('claude', null)).toBe('Checking for Claude…')
    expect(statusLabel('codex', null)).toBe('Checking for Codex…')
  })

  test('names the agent that is missing', () => {
    expect(statusLabel('claude', absent)).toBe('No Claude session')
    expect(statusLabel('codex', absent)).toBe('No Codex session')
  })

  test('reports a reachable session as connected', () => {
    expect(statusLabel('codex', live())).toBe('Codex connected')
    expect(statusLabel('claude', live())).toBe('Claude connected')
  })

  test('counts sessions only when there is more than one', () => {
    expect(statusLabel('claude', live(3))).toBe('Claude connected · 3 sessions')
    expect(statusLabel('codex', live(2))).toBe('Codex connected · 2 sessions')
  })

  test('a running but unreachable session is not called absent', () => {
    // The user can see their own session; claiming there is none would be a
    // claim they can immediately disprove.
    expect(statusLabel('codex', undelivered)).toBe('Codex session not connected')
    expect(statusLabel('claude', undelivered)).toBe('Claude session not connected')
  })
})

describe('preferredAgent', () => {
  test('honours a remembered choice that can still be sent to', () => {
    expect(preferredAgent('codex', { claude: live(), codex: live() })).toBe('codex')
  })

  test('falls past a remembered choice whose session has gone', () => {
    // Silently sending to the other one would be wrong; so would refusing to
    // send at all when something is available. Move, and let the picker show it.
    expect(preferredAgent('codex', { claude: live(), codex: absent })).toBe('claude')
  })

  test('picks the one that is there when nothing is remembered', () => {
    expect(preferredAgent(null, { claude: absent, codex: live() })).toBe('codex')
    expect(preferredAgent(null, { claude: live(), codex: absent })).toBe('claude')
  })

  test('keeps the remembered choice when neither is available', () => {
    // The bar still has to name something while it explains there is nothing
    // there, and the last choice is the least surprising thing to name.
    expect(preferredAgent('codex', { claude: absent, codex: absent })).toBe('codex')
    expect(preferredAgent(null, { claude: null, codex: null })).toBe('claude')
  })
})

describe('composerPlaceholder', () => {
  test('asks for the question when there is somewhere to send it', () => {
    expect(composerPlaceholder('codex', true)).toBe('What about this line?')
  })

  test('names the missing agent when there is not', () => {
    expect(composerPlaceholder('codex', false)).toBe('No Codex session')
    expect(composerPlaceholder('claude', false)).toBe('No Claude session')
  })
})

describe('deliveryNote', () => {
  test('gives the command that reattaches a stranded Codex session', () => {
    // A command they can paste beats a description of a command, and `-C` is
    // part of it: a `--remote` session started without it is filed under the
    // daemon's directory, not the repository's.
    expect(deliveryNote('codex', undelivered)).toBe(
      'Not reachable. Close it, then: codex resume --last --remote unix:// -C "$PWD"'
    )
  })

  test('gives the channel flag for a Claude session started without it', () => {
    // Without the flag Claude Code drops every channel message silently, so
    // the only useful thing to say is the launch command.
    expect(deliveryNote('claude', undelivered)).toBe(
      'Not reachable. Close it, then: claude --dangerously-load-development-channels server:onlydiffs'
    )
  })

  test('says nothing once the session can be reached', () => {
    expect(deliveryNote('codex', live())).toBeNull()
    expect(deliveryNote('claude', live())).toBeNull()
  })

  test('says nothing when no session is running at all', () => {
    // The placeholder already says that; a second sentence would be noise.
    expect(deliveryNote('codex', absent)).toBeNull()
    expect(deliveryNote('claude', absent)).toBeNull()
  })
})
