import { describe, expect, test } from 'bun:test'
import {
  type AgentStatus,
  composerPlaceholder,
  deliveryNote,
  preferredAgent,
  statusLabel,
} from './agents'

const live = (sessions = 1): AgentStatus => ({ connected: true, sessions, delivering: true })
const absent: AgentStatus = { connected: false, sessions: 0, delivering: false }
/** A repository with threads, but nothing running to drain the queue. */
const undelivered: AgentStatus = { connected: true, sessions: 1, delivering: false }

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

  test('does not call Codex connected, because nothing is listening', () => {
    // Codex is a queue, not a live channel. "Ready" is the strongest honest
    // word: a message can be left, not delivered on the spot.
    expect(statusLabel('codex', live())).toBe('Codex ready')
    expect(statusLabel('claude', live())).toBe('Claude connected')
  })

  test('counts sessions only when there is more than one', () => {
    expect(statusLabel('claude', live(3))).toBe('Claude connected · 3 sessions')
    expect(statusLabel('codex', live(2))).toBe('Codex ready · 2 threads')
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
  test('warns that Codex delivery is deferred', () => {
    expect(deliveryNote('codex', live())).toContain('when the session next runs')
  })

  test('says so when nothing is running to drain the queue', () => {
    // The message is kept either way; what changes is whether waiting for a
    // reply is reasonable.
    expect(deliveryNote('codex', undelivered)).toContain('not running')
  })

  test('says nothing for Claude, where the send is the delivery', () => {
    expect(deliveryNote('claude', live())).toBeNull()
  })

  test('says nothing when there is no session to send to at all', () => {
    // The placeholder already explains that; a second sentence would be noise.
    expect(deliveryNote('codex', absent)).toBeNull()
  })
})
