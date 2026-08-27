const PAPER = "var(--color-neutral-900)"
const INK = "color-mix(in srgb, var(--color-neutral-700) 75%, transparent)"
const FAINT = "color-mix(in srgb, var(--color-neutral-700) 35%, transparent)"
const UNIT = 20
const ORIGIN_X = 50
const ORIGIN_Y = 750
const X_MAX = 50
const Y_MAX = 35
const ANGLES = [15, 30, 45, 60] as const
const RADII = [200, 400, 600] as const

function plotX(units: number) {
  return ORIGIN_X + units * UNIT
}

function plotY(units: number) {
  return ORIGIN_Y - units * UNIT
}

function range(end: number) {
  return Array.from({ length: end + 1 }, (_, index) => index)
}

export function HeroGrid() {
  return (
    <svg
      aria-hidden
      viewBox="0 0 1100 800"
      className="pointer-events-none absolute inset-0 size-full [font-family:inherit]"
      preserveAspectRatio="xMidYMid meet"
    >
      <rect width="1100" height="800" fill={PAPER} />
      <defs>
        <pattern
          id="hero-grid"
          width="100"
          height="100"
          patternUnits="userSpaceOnUse"
          x="50"
          y="50"
        >
          <path
            d="M 20 0 V 100 M 40 0 V 100 M 60 0 V 100 M 80 0 V 100 M 0 20 H 100 M 0 40 H 100 M 0 60 H 100 M 0 80 H 100"
            fill="none"
            stroke={FAINT}
            strokeWidth="0.5"
          />
          <path d="M 100 0 H 0 V 100" fill="none" stroke={INK} strokeWidth="1" />
        </pattern>
        <clipPath id="hero-plot">
          <rect x="50" y="50" width="1000" height="700" />
        </clipPath>
      </defs>
      <rect x="50" y="50" width="1000" height="700" fill="url(#hero-grid)" />

      <g fill="none" stroke={FAINT} strokeWidth="1" clipPath="url(#hero-plot)">
        {RADII.map((radius) => (
          <g key={radius}>
            <path
              d={`M ${ORIGIN_X} ${ORIGIN_Y - radius} A ${radius} ${radius} 0 0 1 ${ORIGIN_X + radius} ${ORIGIN_Y}`}
            />
            {range(18).map((step) => {
              const rad = (step * 5 * Math.PI) / 180
              const cx = ORIGIN_X + radius * Math.cos(rad)
              const cy = ORIGIN_Y - radius * Math.sin(rad)
              const dx = 4 * Math.cos(rad)
              const dy = 4 * Math.sin(rad)
              return (
                <line
                  key={step}
                  x1={cx - dx}
                  y1={cy + dy}
                  x2={cx + dx}
                  y2={cy - dy}
                  strokeWidth="0.5"
                />
              )
            })}
          </g>
        ))}
      </g>

      <rect x="50" y="50" width="1000" height="700" fill="none" stroke={INK} strokeWidth="1" />

      {range(X_MAX).map((units) => {
        const px = plotX(units)
        const major = units % 5 === 0
        return (
          <g key={`x-${units}`} stroke={INK} strokeWidth="0.5">
            <line x1={px} y1="30" x2={px} y2={major ? 50 : 40} />
            <line x1={px} y1={major ? 750 : 760} x2={px} y2="770" />
          </g>
        )
      })}
      {range(Y_MAX).map((units) => {
        const py = plotY(units)
        const major = units % 5 === 0
        return (
          <g key={`y-${units}`} stroke={INK} strokeWidth="0.5">
            <line x1="30" y1={py} x2={major ? 50 : 40} y2={py} />
            <line x1={major ? 1050 : 1060} y1={py} x2="1070" y2={py} />
          </g>
        )
      })}

      <g
        fill="none"
        stroke={FAINT}
        strokeWidth="1"
        strokeDasharray="3 2"
        clipPath="url(#hero-plot)"
      >
        {ANGLES.map((deg) => {
          const rad = (deg * Math.PI) / 180
          const length = Math.min(X_MAX / Math.cos(rad), Y_MAX / Math.sin(rad))
          return (
            <line
              key={deg}
              x1={ORIGIN_X}
              y1={ORIGIN_Y}
              x2={plotX(length * Math.cos(rad))}
              y2={plotY(length * Math.sin(rad))}
            />
          )
        })}
      </g>

      {range(X_MAX / 5).map((step) => {
        const units = step * 5
        const px = plotX(units)
        return (
          <g key={`x-label-${units}`} fill={INK} fontSize="10" textAnchor="middle">
            <rect x={px - 12} y="15" width="24" height="12" fill={PAPER} />
            <text x={px} y="25">
              {units}
            </text>
            <rect x={px - 12} y="773" width="24" height="12" fill={PAPER} />
            <text x={px} y="783">
              {units}
            </text>
          </g>
        )
      })}
      {range(Y_MAX / 5).map((step) => {
        const units = step * 5
        const py = plotY(units)
        return (
          <g key={`y-label-${units}`} fill={INK} fontSize="10" textAnchor="middle">
            <rect x="11" y={py - 8} width="24" height="12" fill={PAPER} />
            <text x="23" y={py + 3}>
              {units}
            </text>
            <rect x="1065" y={py - 8} width="24" height="12" fill={PAPER} />
            <text x="1077" y={py + 3}>
              {units}
            </text>
          </g>
        )
      })}

      {ANGLES.map((deg) => {
        const rad = (deg * Math.PI) / 180
        const lx = plotX(10.5 * Math.cos(rad))
        const ly = plotY(10.5 * Math.sin(rad))
        return (
          <g key={`angle-${deg}`} fill={INK} fontSize="10" textAnchor="middle">
            <rect x={lx - 12} y={ly - 7} width="24" height="14" rx="7" fill={PAPER} />
            <text x={lx} y={ly + 4}>
              {deg}°
            </text>
          </g>
        )
      })}
    </svg>
  )
}
