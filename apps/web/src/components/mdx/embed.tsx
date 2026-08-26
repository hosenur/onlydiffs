"use client"

import {
  CodePen as MdxCodePen,
  CodeSandbox as MdxCodeSandbox,
  Figma as MdxFigma,
  Replit as MdxReplit,
  SoundCloud as MdxSoundCloud,
  Spotify as MdxSpotify,
  Vimeo as MdxVimeo,
  YouTube as MdxYouTube,
} from "mdx-embed"
import dynamic from "next/dynamic"
import type { ComponentProps } from "react"
import { twMerge } from "tailwind-merge"

interface TweetProps extends ComponentProps<typeof import("mdx-embed")["Tweet"]> {}

const MdxTweet = dynamic<TweetProps>(() => import("mdx-embed").then((mod) => mod.Tweet), {
  ssr: false,
})

interface YouTubeProps extends Omit<ComponentProps<typeof MdxYouTube>, "youTubeId"> {
  id?: string
  youTubeId?: string
}

interface VimeoProps extends Omit<ComponentProps<typeof MdxVimeo>, "vimeoId"> {
  id?: string
  vimeoId?: string
}

interface CodePenProps extends Omit<ComponentProps<typeof MdxCodePen>, "codePenId"> {
  id?: string
  codePenId?: string
}

interface CodeSandboxProps extends Omit<ComponentProps<typeof MdxCodeSandbox>, "codeSandboxId"> {
  id?: string
  codeSandboxId?: string
}

function normalizeTweetLink(tweetLink: string) {
  try {
    const url = new URL(tweetLink)
    return url.pathname.replace(/^\/|\/$/g, "")
  } catch {
    return tweetLink
      .replace(/^https?:\/\/(www\.)?(twitter|x)\.com\//, "")
      .replace(/^@/, "")
      .replace(/^\/|\/$/g, "")
  }
}

interface EmbedShellProps {
  className?: string
  children: React.ReactNode
}

function EmbedShell({ className, children }: EmbedShellProps) {
  return (
    <div
      className={twMerge(
        "not-typeset my-6 overflow-hidden rounded-lg border [&_.mdx-embed]:w-full [&_iframe]:block",
        className,
      )}
    >
      {children}
    </div>
  )
}

export function YouTube({ id, youTubeId, ...props }: YouTubeProps) {
  return (
    <EmbedShell>
      <MdxYouTube {...props} youTubeId={youTubeId ?? id} />
    </EmbedShell>
  )
}

export function Vimeo({ id, vimeoId, ...props }: VimeoProps) {
  return (
    <EmbedShell>
      <MdxVimeo {...props} vimeoId={vimeoId ?? id ?? ""} />
    </EmbedShell>
  )
}

export function CodePen({ id, codePenId, ...props }: CodePenProps) {
  return (
    <EmbedShell>
      <MdxCodePen {...props} codePenId={codePenId ?? id ?? ""} />
    </EmbedShell>
  )
}

export function CodeSandbox({ id, codeSandboxId, ...props }: CodeSandboxProps) {
  return (
    <EmbedShell>
      <MdxCodeSandbox {...props} codeSandboxId={codeSandboxId ?? id ?? ""} />
    </EmbedShell>
  )
}

export function Tweet({ tweetLink, theme, hideConversation = true, ...props }: TweetProps) {
  const normalizedTweetLink = normalizeTweetLink(tweetLink)

  return (
    <EmbedShell
      className={twMerge(
        "rounded-[16px] border-none bg-white [&_.twitter-tweet-mdx-embed]:overflow-hidden [&_.twitter-tweet-mdx-embed]:rounded-[16px] [&_.twitter-tweet-mdx-embed]:bg-white [&_.twitter-tweet-mdx-embed_iframe]:[clip-path:inset(1px_round_16px)]",
      )}
    >
      <MdxTweet
        key={normalizedTweetLink}
        {...props}
        align={props.align ?? "center"}
        hideConversation={hideConversation}
        tweetLink={normalizedTweetLink}
      />
    </EmbedShell>
  )
}

export function Figma(props: ComponentProps<typeof MdxFigma>) {
  return (
    <EmbedShell>
      <MdxFigma {...props} />
    </EmbedShell>
  )
}

export function Spotify(props: ComponentProps<typeof MdxSpotify>) {
  return (
    <EmbedShell>
      <MdxSpotify {...props} />
    </EmbedShell>
  )
}

export function SoundCloud(props: ComponentProps<typeof MdxSoundCloud>) {
  return (
    <EmbedShell>
      <MdxSoundCloud {...props} />
    </EmbedShell>
  )
}

export function Replit(props: ComponentProps<typeof MdxReplit>) {
  return (
    <EmbedShell>
      <MdxReplit {...props} />
    </EmbedShell>
  )
}
