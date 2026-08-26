"use client"

import { DocumentIcon, FolderIcon, FolderOpenIcon } from "@heroicons/react/24/outline"

import { twMerge } from "tailwind-merge"
import {
  Tree,
  TreeContent,
  TreeItem,
  type TreeItemProps,
  type TreeProps,
} from "@onlydiffs/ui/tree"

interface FileTreeProps extends Omit<TreeProps<object>, "children" | "className"> {
  children: React.ReactNode
  className?: string
}

export function FileTree({ className, children, ...props }: FileTreeProps) {
  return (
    <Tree
      aria-label="File structure"
      selectionMode="none"
      className={twMerge(
        "not-typeset my-6 gap-y-0.5 rounded-lg border p-2 font-mono dark:bg-muted",
        className,
      )}
      {...props}
    >
      {children}
    </Tree>
  )
}

interface FileTreeItemProps
  extends Omit<TreeItemProps<object>, "children" | "className" | "textValue"> {
  name: string
  children?: React.ReactNode
  className?: string
}

export function FileTreeItem({ name, children, className, ...props }: FileTreeItemProps) {
  return (
    <TreeItem
      textValue={name}
      className={twMerge("rounded-md px-2 py-1 text-fg", className)}
      {...props}
    >
      <TreeContent className="font-mono text-[13px]/6">
        {({ hasChildItems, isExpanded }) => (
          <>
            {hasChildItems ? (
              isExpanded ? (
                <FolderOpenIcon className="text-primary-subtle-fg" />
              ) : (
                <FolderIcon className="text-primary-subtle-fg" />
              )
            ) : (
              <DocumentIcon className="text-muted-fg" />
            )}
            <span className="truncate">{name}</span>
          </>
        )}
      </TreeContent>
      {children}
    </TreeItem>
  )
}
