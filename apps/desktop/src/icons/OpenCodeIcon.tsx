import type { SVGProps } from 'react'

/** OpenCode's square mark, reduced to `currentColor` for the compact toolbar. */
export function OpenCodeIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 240 300"
      width={16}
      height={16}
      fill="currentColor"
      {...props}
    >
      <path d="M180 60H60v180h120V60ZM240 300H0V0h240v300Z" />
      <path d="M180 240H60V120h120v120Z" opacity="0.35" />
    </svg>
  )
}
