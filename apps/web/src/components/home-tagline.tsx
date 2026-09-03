"use client"

import { DrawablyButton, DrawablyHighlight, DrawablyUnderline } from "drawably/react"
import "drawably/style.css"
import styles from "./home-tagline.module.css"

export function HomeTagline() {
  return (
    <p className="mt-3 max-w-lg text-base/7 text-white/70">
      <DrawablyUnderline
        seed={17}
        boil={0.2}
        stroke="#f87171"
        className={`${styles.decoration} ${styles.strike}`}
      >
        Not another code editor.
      </DrawablyUnderline>
      <br />A desktop app for reviewing{" "}
      <DrawablyHighlight
        seed={29}
        boil={0.2}
        fill="#facc15"
        className={`${styles.decoration} ${styles.highlight}`}
      >
        every change
      </DrawablyHighlight>{" "}
      your coding agent makes.
      <br />
      Inspect each diff and send{" "}
      <DrawablyHighlight
        seed={37}
        boil={0.2}
        fill="#facc15"
        className={`${styles.decoration} ${styles.highlight}`}
      >
        line-level feedback
      </DrawablyHighlight>{" "}
      straight to Claude.
    </p>
  )
}

export function HomeDocsButton() {
  return (
    <DrawablyButton seed={41} boil={0.2} stroke="#d4d4d4" width={1.5} type="submit">
      Docs
    </DrawablyButton>
  )
}
