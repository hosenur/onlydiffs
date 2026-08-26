"use client"

import { ArrowsPointingOutIcon, XMarkIcon } from "@heroicons/react/24/outline"
import { AnimatePresence, LayoutGroup, motion, useReducedMotion } from "motion/react"
import { useId, useState } from "react"
import { Button } from "react-aria-components/Button"
import { Dialog } from "react-aria-components/Dialog"
import { Modal as AriaModal, ModalOverlay } from "react-aria-components/Modal"
import { twMerge } from "tailwind-merge"

const LIGHTBOX = {
  spring: { type: "spring" as const, stiffness: 380, damping: 34, mass: 0.8 },
  fade: { duration: 0.18, ease: "easeOut" as const },
  contentDelay: 0.08,
  contentOffset: 8,
}

const MotionModalOverlay = motion.create(ModalOverlay)
const MotionModal = motion.create(AriaModal)

interface DocsImageProps {
  src: string
  alt: string
  title?: string
  className?: string
}

export function DocsImage({ src, alt, title, className }: DocsImageProps) {
  const [isOpen, setIsOpen] = useState(false)
  const shouldReduceMotion = useReducedMotion()
  const imageId = useId()
  const caption = title || alt
  const layoutId = `docs-image-${imageId}`

  const layoutTransition = shouldReduceMotion ? { duration: 0 } : LIGHTBOX.spring
  const fadeTransition = shouldReduceMotion ? { duration: 0 } : LIGHTBOX.fade

  return (
    <LayoutGroup id={imageId}>
      <span className="not-typeset my-8 block">
        <Button
          aria-label={`Enlarge image: ${alt}`}
          onPress={() => setIsOpen(true)}
          className="group relative block w-full overflow-hidden rounded-xl text-left outline-hidden ring-1 ring-border transition-shadow hover:ring-muted-fg/35 focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-3 focus-visible:ring-offset-bg"
        >
          <motion.span
            layoutId={layoutId}
            transition={layoutTransition}
            className="block overflow-hidden rounded-xl bg-secondary"
          >
            <img
              src={src}
              alt={alt}
              className={twMerge(
                "h-auto w-full transition-transform duration-200 ease-out group-hover:scale-[1.008] motion-reduce:transition-none motion-reduce:group-hover:scale-100",
                className,
              )}
            />
          </motion.span>
          <span className="absolute right-3 bottom-3 inline-flex size-9 translate-y-1 items-center justify-center rounded-full bg-black/70 text-white opacity-0 shadow-sm backdrop-blur-sm transition duration-150 group-hover:translate-y-0 group-hover:opacity-100 group-focus-visible:translate-y-0 group-focus-visible:opacity-100 motion-reduce:transition-none">
            <ArrowsPointingOutIcon className="size-4" />
          </span>
        </Button>

        {caption ? (
          <span className="mt-3 block text-center text-muted-fg text-sm/6">{caption}</span>
        ) : null}

        <AnimatePresence>
          {isOpen ? (
            <MotionModalOverlay
              isOpen
              isDismissable
              onOpenChange={setIsOpen}
              initial={shouldReduceMotion ? false : { opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fadeTransition}
              className="fixed inset-0 z-50 flex h-(--visual-viewport-height,100dvh) w-screen items-center justify-center bg-black/50 p-3 backdrop-blur-sm sm:p-6"
            >
              <MotionModal
                initial={shouldReduceMotion ? false : { opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={fadeTransition}
                className="relative flex max-h-full w-full max-w-6xl items-center justify-center outline-hidden"
              >
                <Dialog
                  aria-label={`Image preview: ${alt}`}
                  className="relative flex max-h-full w-full flex-col items-center outline-hidden"
                >
                  {({ close }) => (
                    <>
                      <motion.div
                        layoutId={layoutId}
                        transition={layoutTransition}
                        className="relative flex max-h-[calc(100dvh-7rem)] max-w-full items-center justify-center overflow-hidden rounded-xl bg-bg shadow-2xl ring-1 ring-white/15"
                      >
                        <img
                          src={src}
                          alt={alt}
                          className="h-auto max-h-[calc(100dvh-7rem)] w-auto max-w-full object-contain"
                        />
                      </motion.div>

                      <motion.div
                        initial={
                          shouldReduceMotion ? false : { opacity: 0, y: LIGHTBOX.contentOffset }
                        }
                        animate={{ opacity: 1, y: 0 }}
                        exit={
                          shouldReduceMotion
                            ? { opacity: 0 }
                            : { opacity: 0, y: LIGHTBOX.contentOffset / 2 }
                        }
                        transition={{
                          ...fadeTransition,
                          delay: shouldReduceMotion ? 0 : LIGHTBOX.contentDelay,
                        }}
                        className="mt-3 flex min-h-10 max-w-3xl items-start justify-center px-12 text-center text-sm/6 text-white/75"
                      >
                        {caption}
                      </motion.div>

                      <Button
                        aria-label="Close image preview"
                        onPress={close}
                        className="sm:-top-2 sm:-right-2 fixed top-[max(0.75rem,env(safe-area-inset-top))] right-3 z-10 flex size-10 items-center justify-center rounded-full bg-black/65 text-white outline-hidden ring-1 ring-white/15 backdrop-blur-md transition-colors hover:bg-black/80 focus-visible:ring-2 focus-visible:ring-white sm:absolute"
                      >
                        <XMarkIcon className="size-5" />
                      </Button>
                    </>
                  )}
                </Dialog>
              </MotionModal>
            </MotionModalOverlay>
          ) : null}
        </AnimatePresence>
      </span>
    </LayoutGroup>
  )
}
