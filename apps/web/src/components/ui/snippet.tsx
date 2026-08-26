"use client"

import { useSlottedContext } from "react-aria-components/slots"
import { type TabPanelProps, TabsContext, type TabsProps } from "react-aria-components/Tabs"
import { CopyButton } from "@/components/ui/copy-button"
import { Tab, TabList, type TabListProps, TabPanel, TabPanels, Tabs } from "@onlydiffs/ui/tabs"
import { cx } from "@/lib/primitive"

export const Snippet = ({ className, ...props }: TabsProps) => (
  <Tabs
    className={cx("not-typeset group w-full gap-0 overflow-hidden rounded-lg border", className)}
    {...props}
  />
)

export function SnippetTabsList<T extends object>({ className, ...props }: TabListProps<T>) {
  const { orientation = "horizontal" } = useSlottedContext(TabsContext) ?? {}
  return (
    <TabList
      className={cx(
        orientation === "horizontal" &&
          "flex-row gap-x-(--tab-list-gutter) rounded-(--tab-list-rounded) border-b px-4 py-(--tab-list-gutter)",
        "bg-muted",
        className,
      )}
      {...props}
    />
  )
}

export const SnippetTab = ({ className, ...props }: React.ComponentProps<typeof Tab>) => (
  <Tab className={cx("gap-1.5", className)} {...props} />
)

export const SnippetTabPanels = TabPanels

export function SnippetTabPanel({ className, children, ...props }: TabPanelProps) {
  const canCopy = typeof children === "string"

  return (
    <TabPanel className={cx("relative mt-0 px-4 py-3 text-sm", className)} {...props}>
      {(values) => (
        <>
          {typeof children === "function" ? (
            <pre className="overflow-x-auto whitespace-pre">{children(values)}</pre>
          ) : (
            <>
              <pre className="overflow-x-auto whitespace-pre pr-10">{children}</pre>
              {canCopy ? (
                <CopyButton className="absolute top-1.5 right-1.5" text={children} />
              ) : null}
            </>
          )}
        </>
      )}
    </TabPanel>
  )
}
