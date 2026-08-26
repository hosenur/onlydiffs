#!/usr/bin/env bun

import { randomBytes, timingSafeEqual } from 'node:crypto'
import { chmodSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'

const MAX_MESSAGE_BYTES = 64 * 1024
const STATE_DIR = join(homedir(), '.onlydiffs', 'claude-channels')
const token = randomBytes(32).toString('hex')

const mcp = new Server(
  { name: 'onlydiffs', version: '0.2.0' },
  {
    capabilities: {
      experimental: { 'claude/channel': {} },
    },
    instructions:
      'Messages from the local OnlyDiffs git diff viewer arrive as channel events. ' +
      'Treat them as requests from the user working in this repository. The channel is ' +
      'one-way: act on the message in this session as you normally would. There is no ' +
      'reply tool and nothing is waiting on a response.',
  }
)

await mcp.connect(new StdioServerTransport())

function isAuthorized(request: Request) {
  const supplied = Buffer.from(request.headers.get('authorization') ?? '')
  const expected = Buffer.from(`Bearer ${token}`)
  return supplied.length === expected.length && timingSafeEqual(supplied, expected)
}

let sequence = 0
const server = Bun.serve({
  hostname: '127.0.0.1',
  port: 0,
  async fetch(request) {
    const url = new URL(request.url)

    if (request.method === 'GET' && url.pathname === '/health') {
      return Response.json({ ok: true, cwd: process.cwd() })
    }

    if (!isAuthorized(request)) {
      return new Response('unauthorized', { status: 401 })
    }

    if (request.method !== 'POST' || url.pathname !== '/messages') {
      return new Response('not found', { status: 404 })
    }

    const bytes = await request.arrayBuffer()
    if (bytes.byteLength > MAX_MESSAGE_BYTES) {
      return new Response('message is too large', { status: 413 })
    }

    const content = new TextDecoder().decode(bytes).trim()
    if (!content) {
      return new Response('message is empty', { status: 400 })
    }

    const messageId = `onlydiffs-${Date.now()}-${++sequence}`
    try {
      await mcp.notification({
        method: 'notifications/claude/channel',
        params: {
          content,
          meta: {
            repository: basename(process.cwd()),
            message_id: messageId,
          },
        },
      })
      return Response.json({ messageId }, { status: 202 })
    } catch (error) {
      process.stderr.write(
        `onlydiffs channel: failed to forward message: ${error instanceof Error ? error.message : String(error)}\n`
      )
      return new Response('channel transport is unavailable', { status: 503 })
    }
  },
})

mkdirSync(STATE_DIR, { recursive: true, mode: 0o700 })
chmodSync(STATE_DIR, 0o700)
const registrationPath = join(STATE_DIR, `${process.pid}.json`)
writeFileSync(
  registrationPath,
  `${JSON.stringify({
    schemaVersion: 1,
    pid: process.pid,
    cwd: process.cwd(),
    port: server.port,
    token,
    startedAt: Date.now(),
  })}\n`,
  { mode: 0o600 }
)

function cleanup() {
  rmSync(registrationPath, { force: true })
}

process.on('exit', cleanup)
for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => {
    cleanup()
    server.stop(true)
    process.exit(0)
  })
}

process.stderr.write(`onlydiffs channel: listening for ${process.cwd()} on 127.0.0.1:${server.port}\n`)
