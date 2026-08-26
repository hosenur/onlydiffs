"use client"

import { twMerge } from "tailwind-merge"
import { Logo } from "@/components/logo"
import { ThemeSwitcherFooter } from "@/components/theme-switcher-footer"
import { Container } from "@onlydiffs/ui/container"
import { Link as PrimitiveLink } from "@/components/ui/link"
import { Text } from "@/components/ui/text"
import { app } from "@/config/app"
import { cx } from "@/lib/primitive"

const navigation = {
  product: [
    { name: "Documentation", href: "/docs" },
    { name: "Components", href: "/docs/components/api-reference" },
    { name: "Changelog", href: "/docs/components/changelog" },
    { name: "Roadmap", href: "#" },
  ],
  resources: [
    { name: "Blocks", href: "https://design.intentui.com/blocks" },
    { name: "Other templates", href: "https://design.intentui.com/templates" },
    { name: "Patterns", href: "https://design.intentui.com/patterns" },
    { name: "Starter kits", href: "https://design.intentui.com/starter-kits" },
    {
      name: "Themes",
      href: "https://design.intentui.com/themes?f=inter&mf=geist-mono&g=zinc&p=blue&a=zinc&ss=emerald&i=sky&w=amber&d=red&r=0.5",
    },
  ],
  company: [
    { name: "About", href: "#" },
    { name: "Careers", href: "#" },
    { name: "Blog", href: "/blog" },
    { name: "Press", href: "#" },
  ],
  legal: [
    { name: "Terms", href: "#" },
    { name: "Privacy", href: "#" },
    { name: "Security", href: "#" },
    { name: "Cookies", href: "#" },
  ],
  social: [
    {
      name: "X",
      href: "#",
      icon: (props) => (
        <svg fill="currentColor" viewBox="0 0 24 24" {...props}>
          <path d="M13.6823 10.6218L20.2391 3H18.6854L12.9921 9.61788L8.44486 3H3.2002L10.0765 13.0074L3.2002 21H4.75404L10.7663 14.0113L15.5685 21H20.8131L13.6819 10.6218H13.6823ZM11.5541 13.0956L10.8574 12.0991L5.31391 4.16971H7.70053L12.1742 10.5689L12.8709 11.5655L18.6861 19.8835H16.2995L11.5541 13.096V13.0956Z" />
        </svg>
      ),
    },
    {
      name: "Bluesky",
      href: "#",
      icon: (props) => (
        <svg
          {...props}
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
          data-slot="icon"
          aria-hidden="true"
        >
          <path
            fill="currentColor"
            d="M12 11.454c-.922-1.915-3.436-5.483-5.772-7.242-1.685-1.27-4.417-2.252-4.417.874 0 .624.357 5.244.566 5.994.728 2.608 3.378 3.273 5.736 2.87-4.121.704-5.17 3.035-2.905 5.366 4.3 4.426 6.18-1.11 6.661-2.53.09-.262.131-.383.131-.276 0-.107.041.014.13.276.482 1.42 2.362 6.956 6.662 2.53 2.264-2.331 1.216-4.662-2.905-5.365 2.358.402 5.008-.263 5.736-2.87.21-.75.566-5.371.566-5.995 0-3.126-2.732-2.144-4.417-.874-2.336 1.76-4.85 5.327-5.772 7.242"
          />
        </svg>
      ),
    },
    {
      name: "Youtube",
      href: "#",
      icon: (props) => (
        <svg fill="currentColor" viewBox="0 0 24 24" {...props}>
          <path
            fillRule="evenodd"
            d="M19.812 5.418c.861.23 1.538.907 1.768 1.768C21.998 8.746 22 12 22 12s0 3.255-.418 4.814a2.504 2.504 0 0 1-1.768 1.768c-1.56.419-7.814.419-7.814.419s-6.255 0-7.814-.419a2.505 2.505 0 0 1-1.768-1.768C2 15.255 2 12 2 12s0-3.255.417-4.814a2.507 2.507 0 0 1 1.768-1.768C5.744 5 11.998 5 11.998 5s6.255 0 7.814.418ZM15.194 12 10 15V9l5.194 3Z"
            clipRule="evenodd"
          />
        </svg>
      ),
    },
    {
      name: "Instagram",
      href: "#",
      icon: (props) => (
        <svg fill="currentColor" viewBox="0 0 24 24" {...props}>
          <path
            fillRule="evenodd"
            d="M12.315 2c2.43 0 2.784.013 3.808.06 1.064.049 1.791.218 2.427.465a4.902 4.902 0 011.772 1.153 4.902 4.902 0 011.153 1.772c.247.636.416 1.363.465 2.427.048 1.067.06 1.407.06 4.123v.08c0 2.643-.012 2.987-.06 4.043-.049 1.064-.218 1.791-.465 2.427a4.902 4.902 0 01-1.153 1.772 4.902 4.902 0 01-1.772 1.153c-.636.247-1.363.416-2.427.465-1.067.048-1.407.06-4.123.06h-.08c-2.643 0-2.987-.012-4.043-.06-1.064-.049-1.791-.218-2.427-.465a4.902 4.902 0 01-1.772-1.153 4.902 4.902 0 01-1.153-1.772c-.247-.636-.416-1.363-.465-2.427-.047-1.024-.06-1.379-.06-3.808v-.63c0-2.43.013-2.784.06-3.808.049-1.064.218-1.791.465-2.427a4.902 4.902 0 011.153-1.772A4.902 4.902 0 015.45 2.525c.636-.247 1.363-.416 2.427-.465C8.901 2.013 9.256 2 11.685 2h.63zm-.081 1.802h-.468c-2.456 0-2.784.011-3.807.058-.975.045-1.504.207-1.857.344-.467.182-.8.398-1.15.748-.35.35-.566.683-.748 1.15-.137.353-.3.882-.344 1.857-.047 1.023-.058 1.351-.058 3.807v.468c0 2.456.011 2.784.058 3.807.045.975.207 1.504.344 1.857.182.466.399.8.748 1.15.35.35.683.566 1.15.748.353.137.882.3 1.857.344 1.054.048 1.37.058 4.041.058h.08c2.597 0 2.917-.01 3.96-.058.976-.045 1.505-.207 1.858-.344.466-.182.8-.398 1.15-.748.35-.35.566-.683.748-1.15.137-.353.3-.882.344-1.857.048-1.055.058-1.37.058-4.041v-.08c0-2.597-.01-2.917-.058-3.96-.045-.976-.207-1.505-.344-1.858a3.097 3.097 0 00-.748-1.15 3.098 3.098 0 00-1.15-.748c-.353-.137-.882-.3-1.857-.344-1.023-.047-1.351-.058-3.807-.058zM12 6.865a5.135 5.135 0 110 10.27 5.135 5.135 0 010-10.27zm0 1.802a3.333 3.333 0 100 6.666 3.333 3.333 0 000-6.666zm5.338-3.205a1.2 1.2 0 110 2.4 1.2 1.2 0 010-2.4z"
            clipRule="evenodd"
          />
        </svg>
      ),
    },
  ],
} satisfies Record<
  string,
  {
    name: string
    href: string
    icon?: (props: React.SVGProps<SVGSVGElement>) => React.ReactNode
  }[]
>

export function Footer({ className }: { className?: string }) {
  return (
    <footer className={twMerge("mt-6 bg-bg py-8 sm:pt-16 lg:pl-28", className)}>
      <div className="lg:grid lg:grid-cols-3 lg:gap-8">
        <div className="space-y-8">
          <Logo className="size-6" />
          <Text className="mt-2 text-balance">{app.description}</Text>
          <div className="flex gap-x-6">
            {navigation.social.map((item) => (
              <a key={item.name} href={item.href} className="text-fg/80 hover:text-fg">
                <span className="sr-only">{item.name}</span>
                <item.icon aria-hidden="true" className="size-6" />
              </a>
            ))}
          </div>
        </div>
        <div className="mt-16 grid grid-cols-2 gap-8 lg:col-span-2 lg:mt-0 lg:grid-cols-4">
          <div className="contents">
            <div>
              <h3 className="font-semibold text-fg text-sm/6">Product</h3>
              <ul role="list" className="mt-6 space-y-3">
                {navigation.product.map((item) => (
                  <li key={item.name}>
                    <Link className="font-normal" href={item.href}>
                      {item.name}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <div className="font-semibold text-fg text-sm/6">Company</div>
              <ul role="list" className="mt-6 space-y-3">
                {navigation.company.map((item) => (
                  <li key={item.name}>
                    <Link className="font-normal" href={item.href}>
                      {item.name}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          </div>
          <div className="contents">
            <div>
              <div className="font-semibold text-fg text-sm/6">Resources</div>
              <ul role="list" className="mt-6 space-y-3">
                {navigation.resources.map((item) => (
                  <li key={item.name}>
                    <Link className="font-normal" href={item.href}>
                      {item.name}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <div className="font-semibold text-fg text-sm/6">Legal</div>
              <ul role="list" className="mt-6 space-y-3">
                {navigation.legal.map((item) => (
                  <li key={item.name}>
                    <Link className="font-normal" href={item.href}>
                      {item.name}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </div>
      </div>
      <Container>
        <div className="mt-16 flex flex-col justify-between gap-6 border-t pt-8 sm:mt-20 lg:mt-24 lg:flex-row">
          <p className="text-muted-fg text-sm/6">
            &copy; {new Date().getFullYear()} {app.name}, Inc. All rights reserved.
          </p>
          <ThemeSwitcherFooter />
        </div>
      </Container>
    </footer>
  )
}

export function Link({ className, ...props }: React.ComponentProps<typeof PrimitiveLink>) {
  return (
    <PrimitiveLink
      className={cx("text-base/6 text-fg/80 hover:text-fg sm:text-sm/6", className)}
      {...props}
    />
  )
}
