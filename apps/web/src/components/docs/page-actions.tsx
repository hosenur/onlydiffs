"use client"

import { ChevronDownIcon } from "@heroicons/react/20/solid"
import { CheckIcon, Square2StackIcon } from "@heroicons/react/24/outline"
import { useEffect, useMemo, useState } from "react"
import { ClaudeIcon } from "@/components/icons/claude-icon"
import { MarkdownIcon } from "@/components/icons/markdown-icon"
import { OpenaiIcon } from "@/components/icons/openai-icon"
import { PerflexityIcon } from "@/components/icons/perflexity-icon"
import { Button } from "@onlydiffs/ui/button"
import { ButtonGroup } from "@onlydiffs/ui/button-group"
import {
  Menu,
  MenuContent,
  MenuDescription,
  MenuItem,
  MenuLabel,
  MenuSeparator,
} from "@/components/ui/menu"
import { app } from "@/config/app"
import { useClipboard } from "@/hooks/use-clipboard"

interface DocsPageActionsProps {
  content: string
  url: string
}

function getPrompt(url: string) {
  return (
    `I'm currently reading the ${app.name} documentation at: ${url}\n\n` +
    "Please help me understand it thoroughly. " +
    "Explain the key concepts, show practical examples, and be ready to help me debug or implement features based on this documentation."
  )
}

function getPromptUrl(baseUrl: string, url: string) {
  return `${baseUrl}?q=${encodeURIComponent(getPrompt(url))}`
}

export function DocsPageActions({ content, url }: DocsPageActionsProps) {
  const { copy, copied } = useClipboard()
  const [origin, setOrigin] = useState("")
  const fullUrl = useMemo(() => (origin ? new URL(url, origin).toString() : ""), [origin, url])

  useEffect(() => {
    setOrigin(window.location.origin)
  }, [])

  return (
    <ButtonGroup>
      <Button
        intent="secondary"
        size="sm"
        onPress={() => {
          copy(content)
        }}
      >
        {copied ? <CheckIcon /> : <Square2StackIcon />}
        {copied ? "Copied" : "Copy page"}
      </Button>
      <Menu>
        <Button aria-label="Open with" size="sq-sm" intent="secondary">
          <ChevronDownIcon />
        </Button>
        <MenuContent className="min-w-64" placement="bottom end">
          <MenuItem href={`${url}.md`} target="_blank" rel="noopener noreferrer">
            <MarkdownIcon className="w-5" />
            <MenuLabel>View as Markdown</MenuLabel>
            <MenuDescription>Open the raw markdown version of this page.</MenuDescription>
          </MenuItem>
          <MenuSeparator />
          <MenuItem
            href={fullUrl ? getPromptUrl("https://chatgpt.com", fullUrl) : undefined}
            isDisabled={!fullUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            <OpenaiIcon className="w-5" />
            <MenuLabel>Open in ChatGPT</MenuLabel>
            <MenuDescription>Ask about this page.</MenuDescription>
          </MenuItem>
          <MenuItem
            href={fullUrl ? getPromptUrl("https://claude.ai/new", fullUrl) : undefined}
            isDisabled={!fullUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            <ClaudeIcon className="w-5" />
            <MenuLabel>Open in Claude</MenuLabel>
            <MenuDescription>Start a prompt grounded in this docs page.</MenuDescription>
          </MenuItem>
          <MenuItem
            href={
              fullUrl
                ? `https://www.perplexity.ai/search/new?q=${encodeURIComponent(getPrompt(fullUrl))}`
                : undefined
            }
            isDisabled={!fullUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            <PerflexityIcon className="w-5" />
            <MenuLabel>Open in Perplexity</MenuLabel>
            <MenuDescription>Research the page with its summary as context.</MenuDescription>
          </MenuItem>
        </MenuContent>
      </Menu>
    </ButtonGroup>
  )
}
