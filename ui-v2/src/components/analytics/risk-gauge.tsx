interface RiskGaugeProps {
  score: number // 0-100
}

function riskColor(score: number): string {
  if (score < 35) return 'bg-emerald-500'
  if (score < 60) return 'bg-yellow-500'
  if (score < 80) return 'bg-orange-500'
  return 'bg-red-500'
}

function riskLabel(score: number): string {
  if (score < 35) return 'Low'
  if (score < 60) return 'Moderate'
  if (score < 80) return 'High'
  return 'Critical'
}

function riskTextColor(score: number): string {
  if (score < 35) return 'text-emerald-600'
  if (score < 60) return 'text-yellow-600'
  if (score < 80) return 'text-orange-600'
  return 'text-red-600'
}

export function RiskGauge({ score }: RiskGaugeProps) {
  const clamped = Math.max(0, Math.min(100, score))
  const label = riskLabel(clamped)
  const textColor = riskTextColor(clamped)
  const color = riskColor(clamped)

  return (
    <div className="flex flex-col items-center gap-2 py-3">
      <div className="relative w-32 h-32 flex items-center justify-center">
        <svg className="absolute inset-0 w-full h-full -rotate-90" viewBox="0 0 120 120">
          <circle cx="60" cy="60" r="50" fill="none" stroke="currentColor" strokeWidth="10" className="text-secondary" />
          <circle
            cx="60" cy="60" r="50"
            fill="none"
            strokeWidth="10"
            strokeLinecap="round"
            strokeDasharray={`${2 * Math.PI * 50}`}
            strokeDashoffset={`${2 * Math.PI * 50 * (1 - clamped / 100)}`}
            className={`transition-all duration-700 ${
              clamped < 35 ? 'stroke-emerald-500'
              : clamped < 60 ? 'stroke-yellow-500'
              : clamped < 80 ? 'stroke-orange-500'
              : 'stroke-red-500'
            }`}
          />
        </svg>
        <div className="text-center z-10">
          <div className={`text-2xl font-bold ${textColor}`}>{clamped}</div>
          <div className="text-xs text-muted-foreground">/100</div>
        </div>
      </div>
      <div className={`text-sm font-semibold uppercase tracking-wide ${textColor}`}>{label} Risk</div>
      <div className="w-full h-2 bg-secondary rounded-full overflow-hidden">
        <div className={`h-full transition-all duration-700 rounded-full ${color}`} style={{ width: `${clamped}%` }} />
      </div>
    </div>
  )
}
