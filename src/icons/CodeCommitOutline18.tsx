import type { SVGProps } from "react";

export type CodeCommitOutline18Props = SVGProps<SVGSVGElement> & {
  strokeWidth?: number | string;
};

export function CodeCommitOutline18({
  strokeWidth = 1.5,
  ...props
}: CodeCommitOutline18Props) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={18} height={18} viewBox="0 0 18 18" {...props}><line x1="1" y1="9" x2="5.75" y2="9" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></line><line x1="17" y1="9" x2="12.25" y2="9" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></line><circle cx="9" cy="9" r="3.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></circle></svg>
  );
}
