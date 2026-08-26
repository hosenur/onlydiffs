"use client"

import { ChevronDownIcon } from "@heroicons/react/20/solid"
import { CheckIcon, LinkIcon } from "@heroicons/react/24/outline"
import { BskyIcon } from "@/components/icons/bsky-icon"
import { LinkedinIcon } from "@/components/icons/linkedin-icon"
import { TwitterIcon } from "@/components/icons/twitter-icon"
import { WhatsappIcon } from "@/components/icons/whatsapp-icon"
import { Button } from "@onlydiffs/ui/button"
import { ButtonGroup } from "@onlydiffs/ui/button-group"
import { Menu, MenuContent, MenuItem, MenuLabel } from "@/components/ui/menu"
import { useClipboard } from "@/hooks/use-clipboard"

interface BlogShareActionsProps {
  title: string
  url: string
}

export function BlogShareActions({ title, url }: BlogShareActionsProps) {
  const { copy, copied } = useClipboard()
  const encodedTitle = encodeURIComponent(title)
  const encodedUrl = encodeURIComponent(url)

  return (
    <ButtonGroup>
      <Button intent="secondary" onPress={() => copy(url)}>
        {copied ? <CheckIcon /> : <LinkIcon />}
        {copied ? "Copied" : "Copy link"}
      </Button>
      <Menu>
        <Button intent="secondary" aria-label="Share article" size="sq-md">
          <ChevronDownIcon />
        </Button>
        <MenuContent placement="bottom end">
          <MenuItem
            href={`https://x.com/intent/post?text=${encodedTitle}&url=${encodedUrl}`}
            target="_blank"
            rel="noopener noreferrer"
          >
            <TwitterIcon />
            <MenuLabel>Post on X</MenuLabel>
          </MenuItem>
          <MenuItem
            href={`https://bsky.app/intent/compose?text=${encodeURIComponent(`${title} ${url}`)}`}
            target="_blank"
            rel="noopener noreferrer"
          >
            <BskyIcon />
            <MenuLabel>Share on Bluesky</MenuLabel>
          </MenuItem>
          <MenuItem
            href={`https://www.linkedin.com/sharing/share-offsite/?url=${encodedUrl}`}
            target="_blank"
            rel="noopener noreferrer"
          >
            <LinkedinIcon />
            <MenuLabel>Share on LinkedIn</MenuLabel>
          </MenuItem>
          <MenuItem
            href={`https://wa.me/?text=${encodeURIComponent(`${title} ${url}`)}`}
            target="_blank"
            rel="noopener noreferrer"
          >
            <WhatsappIcon />
            <MenuLabel>Share on WhatsApp</MenuLabel>
          </MenuItem>
        </MenuContent>
      </Menu>
    </ButtonGroup>
  )
}
