import { twMerge } from "tailwind-merge"

export function Container({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={twMerge("mx-auto max-w-(--breakpoint-2xl) px-4 sm:px-6 lg:px-8", className)}
      {...props}
    />
  )
}
