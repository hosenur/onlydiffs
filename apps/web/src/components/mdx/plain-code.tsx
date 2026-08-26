"use client"

import { useCallback, useRef } from "react"
import { twMerge } from "tailwind-merge"
import { CopyButton } from "@/components/ui/copy-button"
import { useClipboard } from "@/hooks/use-clipboard"

export interface PreProps extends React.HTMLAttributes<HTMLElement> {
  ref?: React.Ref<HTMLElement>
  icon?: React.ReactNode
  allowCopy?: boolean | "false"
  "data-line-numbers"?: boolean
  "data-line-numbers-start"?: number
}

export const Pre = ({ className, ref, ...props }: React.ComponentProps<"pre">) => {
  return (
    <pre
      ref={ref}
      className={twMerge(
        "shiki w-full p-4 focus-visible:outline-hidden [&_code]:bg-transparent",
        className,
      )}
      {...props}
    >
      {props.children}
    </pre>
  )
}

export const PlainCode = ({
  className,
  title,
  allowCopy = true,
  icon,
  ref,
  style,
  "data-line-numbers": hasLineNumbers,
  "data-line-numbers-start": lineNumberStart,
  ...props
}: PreProps) => {
  const areaRef = useRef<HTMLDivElement>(null)
  const { copy, copied } = useClipboard()
  const canCopy = allowCopy !== false && allowCopy !== "false"
  const onCopy = useCallback(async () => {
    const pre = areaRef.current?.getElementsByTagName("pre").item(0)

    if (!pre) return

    const clone = pre.cloneNode(true) as HTMLElement
    for (const node of clone.querySelectorAll(".nd-copy-ignore")) {
      node.remove()
    }
    await copy(clone.textContent ?? "")
  }, [copy])

  return (
    <figure
      ref={ref}
      {...props}
      data-line-numbers={hasLineNumbers || undefined}
      data-line-numbers-start={lineNumberStart}
      style={{ ...style, "--code-block-line-number": lineNumberStart ?? 1 } as React.CSSProperties}
      className={twMerge(
        "not-typeset group relative my-6 max-w-4xl overflow-hidden rounded-lg border bg-shiki",
        className,
      )}
    >
      {title ? (
        <div className="flex w-full flex-row items-center gap-2 border-b px-4 py-2.5 text-sm/6">
          {typeof icon === "string" ? (
            <div
              className="text-muted-fg [&_svg]:size-3.5"
              dangerouslySetInnerHTML={{
                __html: icon,
              }}
            />
          ) : icon ? (
            <div className="text-muted-fg [&_svg]:size-3.5">{icon}</div>
          ) : null}
          <figcaption className="flex-1 truncate text-fg/80">{title}</figcaption>
          {canCopy ? (
            <CopyButton
              className="absolute top-1.5 right-1.5 z-2 grid size-10 place-content-center"
              onPress={onCopy}
              isCopied={copied}
            />
          ) : null}
        </div>
      ) : (
        canCopy && (
          <CopyButton
            className="absolute top-1.5 right-1.5 z-2"
            onPress={onCopy}
            isCopied={copied}
          />
        )
      )}

      <div ref={areaRef} className="scroll-fade scrollbar-thin w-full overflow-auto *:max-h-120">
        {props.children}
      </div>
    </figure>
  )
}
