import type { ComponentPropsWithoutRef } from "react"
import { twMerge } from "tailwind-merge"

interface HeadingProps extends ComponentPropsWithoutRef<"h2"> {
  as: "h2" | "h3" | "h4"
}

export function Heading({ as: Component, className, children, id, ...props }: HeadingProps) {
  return (
    <Component
      {...props}
      id={id}
      className={twMerge("not-typeset mt-8 mb-4 scroll-mt-24 font-medium", className)}
    >
      {id ? (
        <a
          href={`#${id}`}
          className="group/heading inline-flex scroll-mt-24 items-baseline gap-2 text-current no-underline"
          aria-label={`Link to ${String(children)}`}
        >
          <span>{children}</span>
          <span
            aria-hidden="true"
            className="text-muted-fg opacity-0 transition-opacity group-hover/heading:opacity-100"
          >
            #
          </span>
        </a>
      ) : (
        children
      )}
    </Component>
  )
}
