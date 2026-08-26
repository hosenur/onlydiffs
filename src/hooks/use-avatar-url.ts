import { useEffect, useState } from 'react'
import { cachedAvatarUrl, resolveAvatarUrl } from '@/lib/avatar'

/**
 * The avatar URL for a commit author, or null while it is being derived and
 * for authors that have none. Resolution is async only because hashing is;
 * anything already in the cache is returned on the first render.
 */
export function useAvatarUrl(email: string) {
  const [url, setUrl] = useState(() => cachedAvatarUrl(email) ?? null)

  useEffect(() => {
    const known = cachedAvatarUrl(email)
    if (known !== undefined) {
      setUrl(known)
      return
    }

    let active = true
    void resolveAvatarUrl(email).then((resolved) => {
      if (active) setUrl(resolved)
    })
    return () => {
      active = false
    }
  }, [email])

  return url
}
