import type { SVGProps } from "react";

export type CodeMergeOutline18Props = SVGProps<SVGSVGElement> & {
  strokeWidth?: number | string;
};

export function CodeMergeOutline18({
  strokeWidth = 1.5,
  ...props
}: CodeMergeOutline18Props) {
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={18} height={18} viewBox="0 0 18 18" {...props}><line x1="4.75" y1="6.25" x2="4.75" y2="16.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></line><path d="M11,12.5c-3.452,0-6.25-2.798-6.25-6.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth} data-color="color-2"></path><circle cx="4.75" cy="4" r="2.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></circle><circle cx="13.25" cy="12.5" r="2.25" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth={strokeWidth}></circle></svg>
  );
}
