#!/usr/bin/env bun

import { randomBytes, timingSafeEqual } from 'node:crypto'
import { chmodSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, join } from 'node:path'
import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js'

const MAX_MESSAGE_BYTES = 64 * 1024
const MAX_REPLY_BYTES = 512 * 1024
const REPLY_TIMEOUT_MS = 10 * 60 * 1000
const STATE_DIR = join(homedir(), '.onlydiffs', 'claude-channels')
const token = randomBytes(32).toString('hex')

interface PendingReply {
  promise: Promise<string | null>
  resolve: (reply: string | null) => void
  reply?: string
  timeout: ReturnType<typeof setTimeout>
}

const pendingReplies = new Map<string, PendingReply>()

function createPendingReply(messageId: string) {
  let resolve!: (reply: string | null) => void
  const promise = new Promise<string | null>((done) => {
    resolve = done
  })
  let pending!: PendingReply
  const timeout = setTimeout(() => {
    if (pendingReplies.get(messageId) === pending) pendingReplies.delete(messageId)
    resolve(null)
  }, REPLY_TIMEOUT_MS)
  pending = { promise, resolve, timeout }
  pendingReplies.set(messageId, pending)
}

function discardPendingReply(messageId: string) {
  const pending = pendingReplies.get(messageId)
  if (!pending) return
  pendingReplies.delete(messageId)
  clearTimeout(pending.timeout)
  pending.resolve(null)
}

const mcp = new Server(
  { name: 'onlydiffs', version: '0.2.0' },
  {
    capabilities: {
      experimental: { 'claude/channel': {} },
      tools: {},
    },
    instructions:
      'Messages from the local OnlyDiffs git diff viewer arrive as channel events with a message_id. ' +
      'Treat them as requests from the user working in this repository. Complete all work first, ' +
      'then call the reply tool exactly once with that message_id and your complete final response. ' +
      'Do not send partial or streaming replies through the tool.',
  }
)

mcp.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: 'reply',
      description: 'Return one complete final response to a OnlyDiffs message',
      inputSchema: {
        type: 'object',
        properties: {
          message_id: {
            type: 'string',
            description: 'The message_id attribute from the inbound OnlyDiffs channel event',
          },
          text: {
            type: 'string',
            description: 'The complete final response; do not send partial output',
          },
        },
        required: ['message_id', 'text'],
        additionalProperties: false,
      },
    },
  ],
}))

mcp.setRequestHandler(CallToolRequestSchema, async (request) => {
  if (request.params.name !== 'reply') {
    throw new Error(`unknown tool: ${request.params.name}`)
  }

  const args = request.params.arguments
  const messageId = typeof args?.message_id === 'string' ? args.message_id : ''
  const text = typeof args?.text === 'string' ? args.text.trim() : ''
  const pending = pendingReplies.get(messageId)

  if (!messageId || !pending) {
    return {
      content: [{ type: 'text', text: 'Unknown or expired OnlyDiffs message_id' }],
      isError: true,
    }
  }
  if (!text) {
    return {
      content: [{ type: 'text', text: 'Reply text cannot be empty' }],
      isError: true,
    }
  }
  if (Buffer.byteLength(text) > MAX_REPLY_BYTES) {
    return {
      content: [{ type: 'text', text: `Reply exceeds ${MAX_REPLY_BYTES} bytes` }],
      isError: true,
    }
  }
  if (pending.reply !== undefined) {
    return { content: [{ type: 'text', text: 'Reply was already delivered' }] }
  }

  pending.reply = text
  pending.resolve(text)
  return { content: [{ type: 'text', text: 'Complete reply delivered to OnlyDiffs' }] }
})

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
  // `/replies/:id` is a long poll, and the default is ten seconds — far short
  // of a real Claude turn. 255 is Bun's ceiling; the client re-opens the poll
  // when it is hit, and the pending reply outlives the connection either way.
  idleTimeout: 255,
  async fetch(request) {
    const url = new URL(request.url)

    if (request.method === 'GET' && url.pathname === '/health') {
      return Response.json({ ok: true, cwd: process.cwd() })
    }

    if (!isAuthorized(request)) {
      return new Response('unauthorized', { status: 401 })
    }

    if (request.method === 'GET' && url.pathname.startsWith('/replies/')) {
      const messageId = url.pathname.slice('/replies/'.length)
      const pending = pendingReplies.get(messageId)
      if (!messageId || !pending) {
        return new Response('unknown or expired message', { status: 404 })
      }

      const reply = pending.reply ?? (await pending.promise)
      if (reply === null) {
        return new Response('Claude did not return a reply before the timeout', { status: 504 })
      }

      pendingReplies.delete(messageId)
      clearTimeout(pending.timeout)
      return Response.json({ messageId, text: reply })
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
    createPendingReply(messageId)
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
      discardPendingReply(messageId)
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
