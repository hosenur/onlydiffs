/*
 * Avatars for commit authors. Git records only a name and an email, so the
 * picture has to be derived from the email. Sources are tried in order of how
 * likely they are to be the person's real photo, and every candidate is
 * preflighted — a URL that doesn't load is never handed to an <img>, because
 * WebKit paints a "?" placeholder over the initials underneath it.
 */

/** Rendered at 20px; ask for enough pixels to survive a retina display. */
const SIZE = 64

/** Both the numeric and the older name-only form of GitHub's noreply address. */
const GITHUB_NOREPLY = /^(?:(\d+)\+)?([^@]+)@users\.noreply\.github\.com$/

/**
 * TEMPORARY: every author we can't identify borrows this account's picture,
 * so the sidebar reads as a real timeline while the app is single-user. Drop
 * this and the initials take over as the fallback.
 */
const PLACEHOLDER = `https://github.com/hosenur.png?size=${SIZE}`

/** Resolved URLs keyed by normalised email. `null` = nothing loaded. */
const cache = new Map<string, string | null>()

/**
 * The unauthenticated search API allows ~10 requests a minute. Blowing past
 * that costs nothing but a 403, so on the first one stop asking for a while
 * and let the remaining authors fall through to the later sources.
 */
let searchPausedUntil = 0

function normalise(email: string | undefined) {
  return (email ?? '').trim().toLowerCase()
}

/** Resolves once the browser knows whether this URL is a usable image. */
function loads(url: string) {
  return new Promise<boolean>((resolve) => {
    const probe = new Image()
    probe.onload = () => resolve(probe.naturalWidth > 0)
    probe.onerror = () => resolve(false)
    probe.src = url
  })
}

/** The GitHub account that made commits under this email, if it's public. */
async function searchGithub(email: string): Promise<string | null> {
  if (Date.now() < searchPausedUntil) return null

  try {
    const response = await fetch(
      `https://api.github.com/search/users?q=${encodeURIComponent(email)}+in:email`,
      { headers: { Accept: 'application/vnd.github+json' } }
    )

    if (response.status === 403 || response.status === 429) {
      searchPausedUntil = Date.now() + 60_000
      return null
    }
    if (!response.ok) return null

    const body = (await response.json()) as { items?: { avatar_url?: string }[] }
    return body.items?.[0]?.avatar_url ?? null
  } catch {
    // Offline, or the request was blocked — neither is worth surfacing.
    return null
  }
}

/** Gravatar identifies an account by the SHA-256 of its normalised email. */
async function gravatar(email: string) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(email))
  const hash = Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('')
  // d=404 rather than a generated placeholder: a miss should fall through to
  // the next source, not stop here on a random identicon.
  return `https://gravatar.com/avatar/${hash}?s=${SIZE}&d=404`
}

async function candidates(email: string): Promise<string[]> {
  const noreply = GITHUB_NOREPLY.exec(email)
  if (noreply) {
    // The address already names the account; no lookup needed.
    return [
      noreply[1]
        ? `https://avatars.githubusercontent.com/u/${noreply[1]}?s=${SIZE}&v=4`
        : `https://github.com/${noreply[2]}.png?size=${SIZE}`,
      PLACEHOLDER,
    ]
  }

  const github = await searchGithub(email)
  return [...(github ? [github] : []), await gravatar(email), PLACEHOLDER]
}

/** Whatever is already known for this email, without touching the network. */
export function cachedAvatarUrl(email: string): string | null | undefined {
  return cache.get(normalise(email))
}

export async function resolveAvatarUrl(email: string): Promise<string | null> {
  const key = normalise(email)
  const known = cache.get(key)
  if (known !== undefined) return known

  let resolved: string | null = null
  for (const url of key ? await candidates(key) : [PLACEHOLDER]) {
    if (await loads(url)) {
      resolved = url
      break
    }
  }

  cache.set(key, resolved)
  return resolved
}

/** "Ada Lovelace" → "AL", "hosenur" → "ho". Falls back to a placeholder glyph. */
export function authorInitials(name: string | undefined) {
  const words = (name ?? '').trim().split(/\s+/).filter(Boolean)
  if (words.length === 0) return '?'
  if (words.length === 1) return words[0].slice(0, 2)
  return words[0][0] + words[words.length - 1][0]
}
