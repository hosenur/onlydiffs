import { Component, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { FileDiff } from '@pierre/diffs/react'
import { parseDiffFromFile } from '@pierre/diffs'
import type { FileDiffOptions, OnDiffLineClickProps } from '@pierre/diffs'
import { Badge } from '@onlydiffs/ui/badge'
import { useDiffLayout } from '@/lib/diff-layout'
import { useLineReference } from '@/lib/line-reference'
import { getFileContents, runIpc } from '@/lib/ipc'
import { STATUS_LABEL, statusIntent } from '@/lib/status'
import type { FileChange, FullFileContents } from '@/types'

/** How far outside the viewport a card starts loading. */
const MARGIN = 600

const contentRequests = new WeakMap<FileChange, Promise<FullFileContents>>()

function loadFileContents(file: FileChange) {
  const existing = contentRequests.get(file)
  if (existing) return existing

  const request = runIpc(
    getFileContents({
      path: file.path,
      oldPath: file.oldPath,
      status: file.status,
      staged: file.staged,
    })
  )
  contentRequests.set(file, request)
  void request.catch(() => contentRequests.delete(file))
  return request
}

/** A renderer failure should stay isolated to one file. */
class DiffBoundary extends Component<
  { children: ReactNode; path: string },
  { error: Error | null }
> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  render() {
    if (this.state.error) {
      return (
        <p className="p-5 text-center text-muted-fg">
          Couldn't render {this.props.path} — {this.state.error.message}
        </p>
      )
    }
    return this.props.children
  }
}

interface FileDiffCardProps {
  file: FileChange
  /**
   * Drops the card chrome — header, border, rounding — for the single-file
   * route, where the breadcrumbs name the file and the diff owns the pane.
   */
  bare?: boolean
}

function CompleteFileDiff({
  file,
  contents,
  options,
}: {
  file: FileChange
  contents: FullFileContents
  options: FileDiffOptions<undefined>
}) {
  const fileDiff = useMemo(
    () =>
      parseDiffFromFile(
        contents.oldContents === null
          ? null
          : {
              name: file.oldPath ?? file.path,
              contents: contents.oldContents,
            },
        contents.newContents === null
          ? null
          : {
              name: file.path,
              contents: contents.newContents,
            }
      ),
    [contents.newContents, contents.oldContents, file.oldPath, file.path]
  )

  return <FileDiff fileDiff={fileDiff} options={options} />
}

export function FileDiffCard({ file, bare = false }: FileDiffCardProps) {
  const { options } = useDiffLayout()
  const { select } = useLineReference()
  const [shouldRender, setShouldRender] = useState(false)
  const [loaded, setLoaded] = useState<{
    file: FileChange
    contents: FullFileContents
  } | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const preview = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (shouldRender || file.error || file.binary) return

    // The single-file route renders exactly one card and it is always in view.
    // Waiting on an observer there buys nothing and can strand the card: if the
    // pane has not been laid out when the observer starts, the first entry is
    // not intersecting, and nothing afterwards forces a re-check.
    if (bare) {
      setShouldRender(true)
      return
    }

    const target = preview.current
    if (!target || typeof IntersectionObserver === 'undefined') {
      setShouldRender(true)
      return
    }

    const near = () => {
      const box = target.getBoundingClientRect()
      return box.top < window.innerHeight + MARGIN && box.bottom > -MARGIN
    }

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry?.isIntersecting) return
        setShouldRender(true)
        observer.disconnect()
      },
      { rootMargin: `${MARGIN}px 0px` }
    )
    observer.observe(target)

    // Same stranding hazard on the list route, where switching files fast means
    // observers are created mid-layout. Re-check by hand once, rather than
    // trusting the observer to notice a change that already happened.
    const recheck = setTimeout(() => {
      if (near()) {
        setShouldRender(true)
        observer.disconnect()
      }
    }, 400)

    return () => {
      observer.disconnect()
      clearTimeout(recheck)
    }
  }, [bare, file.binary, file.error, shouldRender])

  useEffect(() => {
    if (!shouldRender || file.error || file.binary) return

    let active = true
    setLoadError(null)
    void loadFileContents(file).then(
      (contents) => {
        if (active) setLoaded({ file, contents })
      },
      (error: unknown) => {
        if (active) setLoadError(error instanceof Error ? error.message : String(error))
      }
    )
    return () => {
      active = false
    }
  }, [file, file.binary, file.error, shouldRender])

  /*
   * Any line can be pointed at. Deleted lines carry their old-file number,
   * which still identifies the line being discussed even though it is no
   * longer in the file.
   */
  const onLineClick = useCallback(
    ({ lineNumber }: OnDiffLineClickProps) => select(file.path, lineNumber),
    [select, file.path]
  )

  // Must stay referentially stable: the renderer diffs its options by value and
  // re-renders the whole file when they change.
  const clickableOptions = useMemo<FileDiffOptions<undefined>>(
    // The renderer draws its own filename bar; with our header gone that would
    // be the only thing naming the file, and the breadcrumbs already do it.
    () => ({ ...options, onLineClick, disableFileHeader: bare }),
    [options, onLineClick, bare]
  )

  const contents = loaded?.file === file ? loaded.contents : null

  return (
    <section
      className={bare ? undefined : 'overflow-hidden rounded-lg border bg-overlay'}
    >
      {!bare && (
        <div className="flex items-center gap-2 border-b bg-navbar px-3 py-2">
          <Badge
            intent={statusIntent(file.status)}
            isCircle={false}
            className="w-5 shrink-0 justify-center font-mono"
            title={file.status}
          >
            {STATUS_LABEL[file.status]}
          </Badge>
          <span className="truncate font-mono text-xs">
            {file.oldPath && <span className="text-muted-fg">{file.oldPath} → </span>}
            {file.path}
          </span>

          <Badge
            intent={file.staged ? 'primary' : 'outline'}
            className="ml-auto shrink-0"
          >
            {file.staged ? 'staged' : 'unstaged'}
          </Badge>
        </div>
      )}

      {file.error ? (
        <p className="whitespace-pre-wrap p-5 font-mono text-danger-subtle-fg">{file.error}</p>
      ) : file.binary ? (
        <p className="p-5 text-center text-muted-fg">Binary file — no preview.</p>
      ) : !shouldRender ? (
        <div
          ref={preview}
          className="flex min-h-32 items-center justify-center text-xs text-muted-fg"
        >
          Loading file when it enters view…
        </div>
      ) : loadError ? (
        <p className="whitespace-pre-wrap p-5 font-mono text-danger-subtle-fg">
          Couldn't load {file.path} — {loadError}
        </p>
      ) : contents === null ? (
        <div className="flex min-h-32 items-center justify-center text-xs text-muted-fg">
          Loading complete file…
        </div>
      ) : (
        <DiffBoundary path={file.path}>
          <CompleteFileDiff file={file} contents={contents} options={clickableOptions} />
        </DiffBoundary>
      )}
    </section>
  )
}
