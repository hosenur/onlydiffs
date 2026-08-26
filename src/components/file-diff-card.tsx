import { Component, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { FileDiff } from '@pierre/diffs/react'
import { parseDiffFromFile } from '@pierre/diffs'
import type { FileDiffOptions, OnDiffLineClickProps } from '@pierre/diffs'
import { CheckIcon } from '@heroicons/react/16/solid'
import { DiffFindToolbar } from '@/components/diff-find-toolbar'
import { Badge } from '@/components/ui/badge'
import { useAddClaudeReference } from '@/lib/claude-message-draft'
import { useDiffLayout } from '@/lib/diff-layout'
import { getFileContents, runIpc, writeClipboardText } from '@/lib/ipc'
import { STATUS_LABEL, statusIntent } from '@/lib/status'
import type { FileChange, FullFileContents } from '@/types'

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
  const addClaudeReference = useAddClaudeReference()
  const [copied, setCopied] = useState<string | null>(null)
  const [shouldRender, setShouldRender] = useState(false)
  const [loaded, setLoaded] = useState<{
    file: FileChange
    contents: FullFileContents
  } | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const preview = useRef<HTMLDivElement | null>(null)
  const root = useRef<HTMLElement>(null)

  useEffect(() => () => clearTimeout(timer.current), [])

  useEffect(() => {
    if (shouldRender || file.error || file.binary) return

    const target = preview.current
    if (!target || typeof IntersectionObserver === 'undefined') {
      setShouldRender(true)
      return
    }

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry?.isIntersecting) return
        setShouldRender(true)
        observer.disconnect()
      },
      { rootMargin: '600px 0px' }
    )
    observer.observe(target)
    return () => observer.disconnect()
  }, [file.binary, file.error, shouldRender])

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
   * Added lines are the ones that exist in the file's current state, so they
   * are the only ones whose number is a real location you can jump to. Their
   * `lineNumber` is already the new-file number.
   */
  const onLineClick = useCallback(
    ({ lineType, lineNumber }: OnDiffLineClickProps) => {
      if (lineType !== 'change-addition') return
      const reference = `${file.path}:${lineNumber}`
      addClaudeReference(reference)
      void runIpc(writeClipboardText(reference)).then(
        () => {
          setCopied(reference)
          clearTimeout(timer.current)
          timer.current = setTimeout(() => setCopied(null), 1600)
        },
        () => setCopied(null)
      )
    },
    [addClaudeReference, file.path]
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
      ref={root}
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

          {copied ? (
            <Badge intent="success" className="ml-auto shrink-0 font-mono">
              <CheckIcon />
              {copied}
            </Badge>
          ) : (
            <Badge
              intent={file.staged ? 'primary' : 'outline'}
              className="ml-auto shrink-0"
            >
              {file.staged ? 'staged' : 'unstaged'}
            </Badge>
          )}
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
      {bare && <DiffFindToolbar rootRef={root} path={file.path} />}
    </section>
  )
}
