"use client"

import { AnimatePresence, motion, useReducedMotion } from "motion/react"
import { CheckIcon } from "@/components/icons/check-icon"
import { DuplicateIcon } from "@/components/icons/duplicate-icon"
import { Button, type ButtonProps } from "@onlydiffs/ui/button"
import { useClipboard } from "@/hooks/use-clipboard"
import { cx } from "@/lib/primitive"

const variants = {
  hidden: { opacity: 0, scale: 0.5 },
  visible: { opacity: 1, scale: 1 },
}

const iconTransition = {
  duration: 0.15,
  ease: "easeOut" as const,
}

interface CopyButtonProps extends Omit<ButtonProps, "children"> {
  isCopied?: boolean
  text?: string
}

export function CopyButton({
  text,
  isCopied: isCopiedProp,
  intent = "plain",
  size = "sq-sm",
  className,
  onPress,
  "aria-label": ariaLabel,
  ...props
}: CopyButtonProps) {
  const { copy, copied } = useClipboard()
  const shouldReduceMotion = useReducedMotion()
  const isCopied = isCopiedProp ?? copied

  const handlePress: ButtonProps["onPress"] = async (event) => {
    if (onPress) {
      onPress(event)
      return
    }

    if (text) {
      await copy(text)
    }
  }

  return (
    <Button
      aria-label={ariaLabel ?? (isCopied ? "Copied" : "Copy to clipboard")}
      intent={intent}
      className={cx("relative rounded-[calc(var(--radius-lg)-(--spacing(1)))]", className)}
      onPress={handlePress}
      size={size}
      {...props}
    >
      <AnimatePresence initial={false}>
        <motion.span
          animate="visible"
          className="absolute inset-0 grid place-items-center [&_svg]:size-4.5 [&_svg]:text-muted-fg sm:[&_svg]:size-4"
          exit={shouldReduceMotion ? "visible" : "hidden"}
          initial={shouldReduceMotion ? false : "hidden"}
          key={isCopied ? "copied" : "copy"}
          transition={shouldReduceMotion ? { duration: 0 } : iconTransition}
          variants={variants}
        >
          {isCopied ? <CheckIcon /> : <DuplicateIcon />}
        </motion.span>
      </AnimatePresence>
    </Button>
  )
}
