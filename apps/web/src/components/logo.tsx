import { twMerge } from "tailwind-merge"

export const Logo = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => {
  return (
    <svg
      className={twMerge("shrink-0", className)}
      aria-hidden
      xmlns="http://www.w3.org/2000/svg"
      width="24"
      height="24"
      fill="none"
      viewBox="0 0 24 24"
      {...props}
    >
      <path
        fill="currentColor"
        d="M13.12 22.07C6.354 22.07 3 18.2 3 11.522 3 5.185 7.1 1 13.292 1c2.38 0 4.5.401 6.421 1.204l-.63 1.405c-1.835-.774-3.784-1.147-5.849-1.147-1.834 0-3.411.373-4.73 1.09v16.11c1.262.631 2.781.947 4.587.947.487 0 1.147-.029 1.95-.144v-7.97h-2.982v-1.519H20v10.005c-1.978.717-4.271 1.09-6.88 1.09m-6.135-3.497V4.698c-1.577 1.577-2.38 3.899-2.38 6.91 0 3.095.803 5.417 2.38 6.965m9.575 1.663a14 14 0 0 0 1.92-.43v-7.31h-1.92z"
      />
    </svg>
  )
}
