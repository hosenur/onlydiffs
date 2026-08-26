import type { ReactNode } from "react"
import { twJoin, twMerge } from "tailwind-merge"
import { Badge, type BadgeProps } from "@onlydiffs/ui/badge"

export function Changelog({ children, className }: React.ComponentProps<"ol">) {
  return (
    <ol className={twMerge("not-typeset my-8 list-none space-y-0 p-0", className)}>{children}</ol>
  )
}

type ChangelogStatus = "new" | "improved" | "fixed" | "breaking"

interface ChangelogEntryProps {
  children: ReactNode
  date: string
  title: string
  version?: string
  status?: ChangelogStatus
  className?: string
}

const statusIntents: Record<ChangelogStatus, NonNullable<BadgeProps["intent"]>> = {
  new: "success",
  improved: "info",
  fixed: "primary",
  breaking: "danger",
}

function formatDate(date: string) {
  return new Intl.DateTimeFormat("en", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(date))
}

export function ChangelogEntry({
  children,
  date,
  title,
  version,
  status,
  className,
}: ChangelogEntryProps) {
  return (
    <li
      className={twMerge(
        "group relative grid gap-x-8 pb-12 sm:grid-cols-[8rem_minmax(0,1fr)] sm:last:pb-0",
        className,
      )}
    >
      <time
        dateTime={date}
        className="mb-3 block pt-0.5 text-muted-fg text-sm/6 tabular-nums sm:mb-0 sm:text-right"
      >
        {formatDate(date)}
      </time>

      <div className="relative border-border border-l pl-7 sm:pl-8">
        <span
          aria-hidden="true"
          className="-left-1.25 absolute top-1.5 size-2.5 rounded-full border-2 border-bg bg-muted-fg ring-1 ring-border transition-colors group-hover:bg-primary-subtle-fg"
        />

        <div className="flex flex-wrap items-center gap-2">
          <h3 className="font-semibold text-base/6 text-fg">{title}</h3>
          {version ? (
            <Badge intent="outline" isCircle={false} className="font-mono">
              {version}
            </Badge>
          ) : null}
          {status ? (
            <Badge intent={statusIntents[status]} isCircle={false} className="capitalize">
              {status}
            </Badge>
          ) : null}
        </div>

        <div
          className={twJoin(
            "mt-3 text-muted-fg text-sm/6",
            "[&_a]:text-primary-subtle-fg [&_a]:underline [&_a]:underline-offset-4",
            "[&_code]:rounded-sm [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-fg [&_code]:text-xs",
            "[&_li]:pl-1 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:space-y-1 [&_ol]:pl-5",
            "[&_p:first-child]:mt-0 [&_p:last-child]:mb-0 [&_p]:my-3",
            "[&_ul]:my-3 [&_ul]:list-disc [&_ul]:space-y-1 [&_ul]:pl-5",
          )}
        >
          {children}
        </div>
      </div>
    </li>
  )
}
