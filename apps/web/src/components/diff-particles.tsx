import styles from "./diff-particles.module.css"

const particles = [
  { label: "+1", kind: "addition", left: "9%", top: "82%", delay: "-2s", duration: "11s" },
  { label: "-2", kind: "deletion", left: "18%", top: "60%", delay: "-7s", duration: "13s" },
  { label: "+4", kind: "addition", left: "27%", top: "91%", delay: "-5s", duration: "12s" },
  { label: "-1", kind: "deletion", left: "13%", top: "35%", delay: "-10s", duration: "14s" },
  { label: "+8", kind: "addition", left: "31%", top: "24%", delay: "-1s", duration: "10s" },
  { label: "-3", kind: "deletion", left: "5%", top: "18%", delay: "-4s", duration: "12s" },
  { label: "+2", kind: "addition", left: "88%", top: "88%", delay: "-8s", duration: "14s" },
  { label: "-4", kind: "deletion", left: "78%", top: "68%", delay: "-3s", duration: "11s" },
  { label: "+1", kind: "addition", left: "94%", top: "52%", delay: "-11s", duration: "13s" },
  { label: "-2", kind: "deletion", left: "83%", top: "38%", delay: "-6s", duration: "12s" },
  { label: "+6", kind: "addition", left: "72%", top: "20%", delay: "-9s", duration: "14s" },
  { label: "-1", kind: "deletion", left: "92%", top: "13%", delay: "-1s", duration: "10s" },
] as const

export function DiffParticles() {
  return (
    <div aria-hidden className={styles.field}>
      {particles.map((particle) => (
        <span
          key={`${particle.left}-${particle.top}`}
          className={`${styles.particle} ${styles[particle.kind]}`}
          style={{
            left: particle.left,
            top: particle.top,
            animationDelay: particle.delay,
            animationDuration: particle.duration,
          }}
        >
          {particle.label}
        </span>
      ))}
    </div>
  )
}
