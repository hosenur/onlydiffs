"use client"

import { MagnifyingGlassIcon } from "@heroicons/react/20/solid"
import NextLink from "next/link"
import { usePathname } from "next/navigation"
import { Autocomplete, useFilter } from "react-aria-components/Autocomplete"
import { Menu, MenuTrigger, Popover, MenuItem as PrimitiveMenu } from "react-aria-components/Menu"
import { Input, SearchField } from "react-aria-components/SearchField"
import { twJoin } from "tailwind-merge"
import { Logo } from "@/components/logo"
import { Button } from "@onlydiffs/ui/button"
import { flattenSidebar, normalizePath } from "@/lib/docs"
import type { PageTreeRoot } from "@/types/content"

export function ResponsiveNavigation({ tree }: { tree?: PageTreeRoot }) {
  const pathname = normalizePath(usePathname())
  const menus = tree ? flattenSidebar(tree.children) : []
  const { contains } = useFilter({ sensitivity: "base" })
  return (
    <MenuTrigger>
      <Button
        size="sq-sm"
        intent="plain"
        className="group -ml-2 pressed:bg-transparent outline-hidden hover:bg-transparent sm:hidden"
      >
        <div className="relative flex h-8 w-(--width) items-center justify-center [--width:--spacing(4.5)]">
          <div className="relative size-(--width) [--bar:var(--color-fg)]">
            <span
              className={twJoin(
                "absolute start-0 block h-0.5 w-(--width) bg-(--bar) transition-all duration-150 ease-out motion-reduce:transition-none",
                "group-pressed:-rotate-45 top-1 group-pressed:top-[0.4rem]",
              )}
            />
            <span
              className={twJoin(
                "absolute start-0 block h-0.5 w-(--width) bg-(--bar) transition-all duration-150 ease-out motion-reduce:transition-none",
                "top-[--spacing(2.6)] group-pressed:top-[0.4rem] group-pressed:rotate-45",
              )}
            />
          </div>
          <span className="sr-only">Toggle Menu</span>
        </div>
      </Button>
      <Popover
        placement="bottom"
        offset={10}
        className={twJoin([
          "placement-bottom:entering:slide-in-from-top-1 -mt-px scroll-fade-b fixed min-h-screen overflow-y-auto bg-bg outline-hidden",
          "entering:fade-in entering:animate-in entering:duration-200 entering:ease-out",
          "exiting:fade-out exiting:animate-out exiting:ease-out",
          "placement-bottom:entering:slide-in-from-top-1",
          "placement-bottom:exiting:slide-out-to-top-1",
          "relative isolate z-50 w-full pb-24",
        ])}
        containerPadding={0}
      >
        <Logo className="-rotate-6 -right-10 fixed bottom-4 size-56 opacity-5" />
        <Autocomplete filter={contains}>
          {tree && (
            <div className="-mt-2.5 sticky top-0 h-16 shrink-0 bg-linear-to-b from-bg via-bg px-4 pt-3">
              <SearchField aria-label="Search" className="relative">
                <MagnifyingGlassIcon className="absolute top-3 left-3 size-4.5 text-muted-fg" />
                <Input
                  placeholder="Search&hellip;"
                  className="w-full rounded-lg border py-2 pr-4 pl-10 text-base/6 placeholder-muted-fg outline-hidden dark:bg-secondary [&::-ms-reveal]:hidden [&::-webkit-search-cancel-button]:hidden"
                />
              </SearchField>
            </div>
          )}
          <Menu className="flex flex-col p-2.5 outline-hidden" aria-label="Menu">
            <MenuItem textValue="docs" href="/">
              Home
            </MenuItem>
            <MenuItem textValue="docs" href="/docs">
              Docs
            </MenuItem>
            <MenuItem textValue="blog" href="/blog">
              Blog
            </MenuItem>
            <MenuItem href="#">Guides</MenuItem>
            <MenuItem href="#">Changelog</MenuItem>
            {menus.map((menu) => {
              const active = normalizePath(menu.href) === pathname

              return (
                <MenuItem
                  key={menu.href}
                  textValue={menu.textValue}
                  href={menu.href}
                  active={active}
                >
                  {menu.label}
                </MenuItem>
              )
            })}
          </Menu>
        </Autocomplete>
      </Popover>
    </MenuTrigger>
  )
}

interface MenuItemProps extends React.ComponentProps<typeof PrimitiveMenu> {
  active?: boolean
}
function MenuItem({ active, ...props }: MenuItemProps) {
  return (
    <PrimitiveMenu
      {...props}
      render={(domProps) =>
        "href" in domProps ? <NextLink {...domProps} /> : <div {...domProps} />
      }
      className={twJoin(
        "block w-full px-3 py-2 font-medium text-xl outline-hidden",
        active ? "text-primary-subtle-fg" : "text-fg hover:text-primary-subtle-fg",
      )}
    />
  )
}
