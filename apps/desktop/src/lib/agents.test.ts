import { describe, expect, test } from 'bun:test'
import {
  type AgentStatus,
  composerPlaceholder,
  deliveryNote,
  preferredAgent,
  statusLabel,
} from './agents'

const live = (sessions = 1): AgentStatus => ({ connected: true, sessions, delivering: true })
/** A stranded session whose thread is known, so the fix can name it. */
const stranded: AgentStatus = {
  connected: false,
  sessions: 1,
  delivering: false,
  thread: '01a06c34-0a13-7fa2-bda5-08b36460d602',
}
const absent: AgentStatus = { connected: false, sessions: 0, delivering: false }
/** A session is running, but not attached to the daemon, so unreachable. */
const undelivered: AgentStatus = { connected: false, sessions: 1, delivering: false }

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

  test('a running but unreachable Codex session is not called absent', () => {
    // The user can see their own session; claiming there is none would be a
    // claim they can immediately disprove.
    expect(statusLabel('codex', undelivered)).toBe('Codex session not connected')
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
  test('names the thread in the command that fixes a stranded session', () => {
    // A command they can paste beats a description of a command.
    expect(deliveryNote('codex', stranded)).toBe(
      'Not reachable. Close it, then: codex resume 01a06c34-0a13-7fa2-bda5-08b36460d602 --remote unix://'
    )
  })

  test('falls back to a plain instruction when no thread exists yet', () => {
    // Before its first turn a session has no thread to name.
    expect(deliveryNote('codex', undelivered)).toContain('codex --remote unix://')
  })

  test('says nothing once the session can be reached', () => {
    // Delivery is immediate now; there is nothing left to explain.
    expect(deliveryNote('codex', live())).toBeNull()
  })

  test('says nothing for Claude, where the send is the delivery', () => {
    expect(deliveryNote('claude', live())).toBeNull()
  })

  test('says nothing when no session is running at all', () => {
    // The placeholder already says that; a second sentence would be noise.
    expect(deliveryNote('codex', absent)).toBeNull()
  })
})
