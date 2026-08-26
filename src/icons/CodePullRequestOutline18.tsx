import type { SVGProps } from "react";

export type CodePullRequestOutline18Props = SVGProps<SVGSVGElement> & {
  strokeWidth?: number | string;
};

export function CodePullRequestOutline18({
  strokeWidth = 1.5,
  ...props
}: CodePullRequestOutline18Props) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={18} height={18} viewBox="0 0 18 18" {...props}><path d="M14.25,12.25V5.75c0-1.105-.895-2-2-2h-3.5" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></path><polyline points="11 6 8.75 3.75 11 1.5" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></polyline><line x1="3.75" y1="5.75" x2="3.75" y2="12.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></line><circle cx="14.25" cy="14.25" r="2" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></circle><circle cx="3.75" cy="14.25" r="2" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></circle><circle cx="3.75" cy="3.75" r="2" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></circle></svg>
  );
}
