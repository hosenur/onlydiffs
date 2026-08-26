import { Link as RouterLink } from '@tanstack/react-router'
import {
  Link as LinkPrimitive,
  type LinkProps as LinkPrimitiveProps,
} from 'react-aria-components/Link'
import { cx } from '@/lib/primitive'

export interface LinkProps extends LinkPrimitiveProps {
  ref?: React.RefObject<HTMLAnchorElement>
}

/*
 * TanStack types `to` as a union of the registered route paths. Here the value
 * is an already-resolved URL string handed to us by React Aria, so widen at
 * this single boundary rather than casting at every call site.
 */
const NavigateLink = RouterLink as unknown as React.FC<
  React.AnchorHTMLAttributes<HTMLAnchorElement> & { to: string }
>

/**
 * React Aria renders `href` links as a plain `<a>`, which would reload the
 * webview. The `render` prop swaps in TanStack's Link so navigation stays
 * client-side — see intentui.com/docs/getting-started/client-side-routing.
 * Keeping the `href` API (rather than `createLink`, which makes `to` required)
 * means Intent's own components — SidebarItem above all — need no changes.
 */
export function Link({ className, ref, ...props }: LinkProps) {
  return (
    <LinkPrimitive
      ref={ref}
      className={cx(
        'font-medium text-(--text)',
        'outline-0 outline-offset-2 focus-visible:outline-2 focus-visible:outline-ring forced-colors:outline-[Highlight]',
        'disabled:cursor-default disabled:opacity-50 forced-colors:disabled:text-[GrayText]',
        'href' in props && 'cursor-pointer',
        className
      )}
      render={(domProps) =>
        'href' in domProps ? (
          <NavigateLink {...domProps} to={domProps.href} />
        ) : (
          <span {...domProps} />
        )
      }
      {...props}
    />
  )
}
