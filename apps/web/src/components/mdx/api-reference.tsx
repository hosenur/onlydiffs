"use client"

import type { PressEvent } from "react-aria-components"
import { twMerge } from "tailwind-merge"
import { Badge, type BadgeProps } from "@onlydiffs/ui/badge"
import { CopyButton } from "@/components/ui/copy-button"
import { useClipboard } from "@/hooks/use-clipboard"

type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE"

const methodIntents: Record<HttpMethod, NonNullable<BadgeProps["intent"]>> = {
  GET: "info",
  POST: "success",
  PUT: "warning",
  PATCH: "primary",
  DELETE: "danger",
}

interface ApiEndpointProps {
  method: HttpMethod
  path: string
  children?: React.ReactNode
  className?: string
}

export function ApiEndpoint({ method, path, children, className }: ApiEndpointProps) {
  return (
    <section
      className={twMerge("not-typeset my-6 overflow-hidden rounded-lg border bg-shiki", className)}
    >
      <div className="flex min-w-0 items-center gap-3 px-4 py-1">
        <Badge intent={methodIntents[method]} isCircle={false} className="shrink-0 font-mono">
          {method}
        </Badge>
        <code className="min-w-0 flex-1 truncate bg-transparent p-0 font-mono text-sm">{path}</code>
        <CopyButton className="-mr-2.5" text={path} aria-label={`Copy endpoint ${path}`} />
      </div>
      {children ? (
        <div className="border-t px-4 py-3 text-muted-fg text-sm/6">{children}</div>
      ) : null}
    </section>
  )
}

export function ApiParameters({ children }: { children: React.ReactNode }) {
  return (
    <div className="not-typeset my-6 overflow-hidden rounded-lg border bg-shiki">{children}</div>
  )
}

interface ApiParameterProps {
  name: string
  type: string
  location?: "path" | "query" | "header" | "body"
  required?: boolean
  defaultValue?: string
  children: React.ReactNode
}

export function ApiParameter({
  name,
  type,
  location,
  required = false,
  defaultValue,
  children,
}: ApiParameterProps) {
  return (
    <div className="border-b p-4 last:border-b-0">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <code className="bg-transparent p-0 font-mono font-semibold text-sm">{name}</code>
        <span className="font-mono text-primary-subtle-fg text-xs">{type}</span>
        {location ? <span className="text-muted-fg text-xs">{location}</span> : null}
        {required ? (
          <Badge isCircle intent="danger">
            required
          </Badge>
        ) : (
          <span className="text-muted-fg text-xs">optional</span>
        )}
      </div>
      <div className="mt-2 text-muted-fg text-sm/6">{children}</div>
      {defaultValue ? (
        <div className="mt-2 text-muted-fg text-xs">
          Default: <code className="font-mono text-fg">{defaultValue}</code>
        </div>
      ) : null}
    </div>
  )
}

interface ApiResponseProps {
  status: number | string
  description?: string
  children?: React.ReactNode
}

export function ApiResponse({ status, description, children }: ApiResponseProps) {
  const isSuccess = String(status).startsWith("2")

  return (
    <section className="not-typeset my-6 overflow-hidden rounded-lg border bg-shiki [&_figure_button]:hidden">
      <div className="flex items-center gap-3 border-b px-4 py-1">
        <Badge intent={isSuccess ? "success" : "danger"} isCircle={false} className="font-mono">
          {status}
        </Badge>
        {description ? <span className="text-sm">{description}</span> : null}
        <ApiResponseCopyButton />
      </div>
      {children ? (
        <div className="p-1 *:border-none *:p-0 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
          {children}
        </div>
      ) : null}
    </section>
  )
}

function ApiResponseCopyButton() {
  const { copy, copied } = useClipboard()

  const copyResponse = async (event: PressEvent) => {
    const section = (event.target as HTMLElement).closest("section")
    const code = section?.querySelector("pre")?.textContent

    if (code) {
      await copy(code)
    }
  }

  return (
    <CopyButton
      aria-label="Copy response"
      className="-mr-2.5 ml-auto shrink-0"
      isCopied={copied}
      onPress={copyResponse}
    />
  )
}
