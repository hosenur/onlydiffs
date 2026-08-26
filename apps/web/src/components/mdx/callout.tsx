import { twJoin } from "tailwind-merge"

interface CalloutProps {
  title?: string
  children: React.ReactNode
  type?: "info" | "warning" | "danger" | "success" | "default"
}

export function Callout({ title, type = "default", children }: CalloutProps) {
  return (
    <div
      className={twJoin(
        "not-typeset inset-ring inset-ring-border my-6 rounded-lg px-4 py-3",
        type === "info" && "inset-ring-info-subtle-fg/20 bg-info-subtle text-info-subtle-fg",
        type === "danger" &&
          "inset-ring-danger-subtle-fg/20 bg-danger-subtle text-danger-subtle-fg",
        type === "success" &&
          "inset-ring-success-subtle-fg/20 bg-success-subtle text-success-subtle-fg",
        type === "warning" &&
          "inset-ring-warning-subtle-fg/20 bg-warning-subtle text-warning-subtle-fg",
        type === "default" && "bg-muted/50 text-secondary-fg dark:bg-muted",
      )}
    >
      {title ? <p className="mb-1 font-medium text-base">{title}</p> : null}
      <div
        className={twJoin("text-sm/6 [&>p]:my-0", title && type === "default" && "text-muted-fg")}
      >
        {children}
      </div>
    </div>
  )
}
