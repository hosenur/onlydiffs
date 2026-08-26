"use client"

import { CheckIcon } from "@heroicons/react/20/solid"
import { HandThumbDownIcon, HandThumbUpIcon } from "@heroicons/react/24/outline"
import { AnimatePresence, motion } from "motion/react"
import { forwardRef, useState } from "react"
import { twJoin } from "tailwind-merge"
import { Button, type ButtonProps } from "@onlydiffs/ui/button"
import { Text } from "@/components/ui/text"

function FeedbackButton(props: Omit<ButtonProps, "className" | "type">) {
  return <Button type="submit" intent="outline" size="xs" {...props} />
}

const FeedbackForm = forwardRef<
  HTMLFormElement,
  React.ComponentPropsWithoutRef<typeof motion.form>
>(function FeedbackForm({ onSubmit, className, ...props }, ref) {
  return (
    <motion.form
      {...props}
      ref={ref}
      onSubmit={onSubmit}
      exit={{ opacity: 0, pointerEvents: "none" }}
      transition={{ duration: 0.3 }}
      className={twJoin(className, "absolute inset-0 flex items-center justify-between gap-4")}
    >
      <Text>Was this page helpful?</Text>
      <div className="flex items-center gap-x-1">
        <FeedbackButton data-response="yes">
          <HandThumbUpIcon />
          Yes
        </FeedbackButton>
        <FeedbackButton data-response="no">
          <HandThumbDownIcon />
          No
        </FeedbackButton>
      </div>
    </motion.form>
  )
})

const FeedbackThanks = forwardRef<
  HTMLDivElement,
  React.ComponentPropsWithoutRef<typeof motion.div>
>(function FeedbackThanks({ className, ...props }, ref) {
  return (
    <motion.div
      {...props}
      ref={ref}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3, delay: 0.15 }}
      className={twJoin(className, "absolute inset-0 flex justify-center md:justify-start")}
    >
      <div className="inset-ring inset-ring-success/20 flex items-center gap-2 rounded-full bg-success-subtle py-1 pr-3 pl-1.5 text-sm text-success-subtle-fg">
        <CheckIcon className="size-4 flex-none fill-success" />
        Thanks for your feedback!
      </div>
    </motion.div>
  )
})

export function Feedback() {
  const [submitted, setSubmitted] = useState(false)

  function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setSubmitted(true)
  }

  return (
    <div className="not-typeset relative mt-6 h-8">
      <AnimatePresence initial={false}>
        {!submitted && <FeedbackForm key="form" onSubmit={onSubmit} />}
        {submitted && <FeedbackThanks key="thanks" />}
      </AnimatePresence>
    </div>
  )
}
