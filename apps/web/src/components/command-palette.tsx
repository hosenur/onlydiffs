"use client"

import { BookOpenIcon, HashtagIcon, HomeIcon, ServerStackIcon } from "@heroicons/react/24/outline"
import {
  CommandMenu,
  CommandMenuItem,
  CommandMenuLabel,
  CommandMenuList,
  type CommandMenuProps,
  CommandMenuSearch,
  CommandMenuSection,
} from "@/components/ui/command-menu"

import { flattenSidebar } from "@/lib/docs"
import type { PageTreeRoot } from "@/types/content"

export function CommandPalette({
  isOpen,
  tree,
  onOpenChange,
}: Pick<CommandMenuProps, "isOpen" | "onOpenChange"> & { tree?: PageTreeRoot }) {
  const menus = tree ? flattenSidebar(tree.children) : []
  return (
    <CommandMenu isOpen={isOpen} onOpenChange={onOpenChange} shortcut="k" size="sm">
      <CommandMenuSearch placeholder="Quick search..." />
      <CommandMenuList>
        <CommandMenuSection aria-label="Pages">
          <CommandMenuItem textValue="Home" href="/" onAction={() => onOpenChange?.(false)}>
            <HomeIcon />
            <CommandMenuLabel>Home</CommandMenuLabel>
          </CommandMenuItem>
          <CommandMenuItem textValue="Docs" href="/docs" onAction={() => onOpenChange?.(false)}>
            <BookOpenIcon />
            <CommandMenuLabel>Docs</CommandMenuLabel>
          </CommandMenuItem>
          <CommandMenuItem
            textValue="SSH repositories"
            href="/docs/remote-repositories/connect-an-ssh-host"
            onAction={() => onOpenChange?.(false)}
          >
            <ServerStackIcon />
            <CommandMenuLabel>SSH repositories</CommandMenuLabel>
          </CommandMenuItem>
          <CommandMenuItem
            textValue="Agent support"
            href="/docs/workflows/agent-support"
            onAction={() => onOpenChange?.(false)}
          >
            <CommandMenuLabel>Agent support</CommandMenuLabel>
          </CommandMenuItem>
        </CommandMenuSection>
        <CommandMenuSection label="Docs">
          {menus.map((menu) => {
            return (
              <CommandMenuItem
                key={menu.href}
                textValue={menu.textValue}
                href={menu.href}
                onAction={() => onOpenChange?.(false)}
              >
                <HashtagIcon />
                {menu.label}
              </CommandMenuItem>
            )
          })}
        </CommandMenuSection>
      </CommandMenuList>
    </CommandMenu>
  )
}
