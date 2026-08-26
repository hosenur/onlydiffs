import type { SVGProps } from "react";

export type PlugOffOutline18Props = SVGProps<SVGSVGElement> & {
  strokeWidth?: number | string;
};

export function PlugOffOutline18({
  strokeWidth = 1.5,
  ...props
}: PlugOffOutline18Props) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={18} height={18} viewBox="0 0 18 18" {...props}><line x1="6.25" y1="4.75" x2="6.25" y2="1.75" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></line><line x1="11.75" y1="4.75" x2="11.75" y2="1.75" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></line><line x1="12" y1="12.25" x2="16" y2="16.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></line><path d="m9,16.25v-2.5l-.3401-.0171c-3.2902-.1782-5.9099-2.8987-5.9099-6.2329v-1.75c0-.552.448-1,1-1h10.5c.552,0,1,.448,1,1v1.75c0,.6189-.0934,1.2156-.2614,1.7803" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></path><line x1="16" y1="12.25" x2="12" y2="16.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></line></svg>
  );
}
