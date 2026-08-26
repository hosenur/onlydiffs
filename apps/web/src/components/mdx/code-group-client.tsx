"use client"

import { ChevronDownIcon } from "@heroicons/react/20/solid"
import { SparklesIcon } from "@heroicons/react/24/outline"
import { useState } from "react"
import { Button } from "@onlydiffs/ui/button"
import { CopyButton } from "@/components/ui/copy-button"
import { Menu, MenuContent, MenuItem, MenuLabel } from "@/components/ui/menu"
import { Tab, TabList, TabPanel, TabPanels, Tabs } from "@onlydiffs/ui/tabs"

interface CodeGroupTab {
  title: string
  lang: string
  code: string
}

interface CodeGroupClientProps {
  asMenu?: boolean
  tabs: CodeGroupTab[]
  children: React.ReactNode
}

export function CodeGroupClient({ asMenu = false, tabs, children }: CodeGroupClientProps) {
  const firstTab = tabs[0]
  const [selectedKey, setSelectedKey] = useState(firstTab?.title)

  if (!firstTab) {
    return null
  }

  const selectedTab = tabs.find((tab) => tab.title === selectedKey) ?? firstTab

  return (
    <Tabs
      className="not-typeset my-6 gap-0 overflow-hidden rounded-lg border bg-shiki"
      selectedKey={selectedKey}
      onSelectionChange={(key) => {
        setSelectedKey(String(key))
      }}
    >
      <div className="flex items-center justify-between border-b">
        {asMenu ? (
          <>
            <TabList aria-label="Code examples" className="sr-only" items={tabs}>
              {(tab) => (
                <Tab id={tab.title} key={tab.title}>
                  {tab.title}
                </Tab>
              )}
            </TabList>
            <div className="min-w-0 flex-1 px-4 py-2 font-medium text-sm/6">
              <span className="block truncate">{selectedTab.title}</span>
            </div>
          </>
        ) : (
          <TabList aria-label="Code examples" className="border-b-0 px-4" items={tabs}>
            {(tab) => (
              <Tab id={tab.title} key={tab.title}>
                {tab.title}
              </Tab>
            )}
          </TabList>
        )}
        {asMenu ? (
          <div className="mr-1 flex items-center gap-x-1">
            <Menu>
              <Button size="sm" intent="plain">
                {selectedTab.lang}
                <ChevronDownIcon />
              </Button>
              <MenuContent
                aria-label="Code examples"
                selectionMode="single"
                selectedKeys={selectedKey ? [selectedKey] : []}
                onAction={(key) => {
                  setSelectedKey(String(key))
                }}
              >
                {tabs.map((tab) => (
                  <MenuItem id={tab.title} key={tab.title}>
                    <MenuLabel>{tab.lang}</MenuLabel>
                  </MenuItem>
                ))}
              </MenuContent>
            </Menu>
            <CopyButton text={selectedTab.code} />
            <Button size="sq-sm" intent="plain">
              <SparklesIcon />
            </Button>
          </div>
        ) : (
          <CopyButton className="mr-1 shrink-0" text={selectedTab.code} />
        )}
      </div>
      <TabPanels>{children}</TabPanels>
    </Tabs>
  )
}

interface CodeGroupPanelProps {
  children: React.ReactNode
  code?: string
  id: string
}

export function CodeGroupPanel({ children, id }: CodeGroupPanelProps) {
  return (
    <TabPanel className="relative mt-0 overflow-auto [&_pre]:p-4 [&_pre]:text-sm/0" id={id}>
      {children}
    </TabPanel>
  )
}
