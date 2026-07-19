/**
 * CSS-based sparkline trend chart.
 * Shows HR and BR moving averages over the last N data points.
 */
interface TrendPoint {
  label: string
  hr: number
  br: number
}

interface TrendChartProps {
  points: TrendPoint[]
}

function normalize(values: number[]): number[] {
  const max = Math.max(...values, 1)
  const min = Math.min(...values, 0)
  const range = max - min || 1
  return values.map(v => ((v - min) / range) * 100)
}

function Polyline({ values, color }: { values: number[]; color: string }) {
  if (values.length < 2) return null
  const pts = values.map((v, i) => `${(i / (values.length - 1)) * 100},${100 - v}`).join(' ')
  return <polyline fill="none" stroke={color} strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" points={pts} />
}

export function TrendChart({ points }: TrendChartProps) {
  if (points.length === 0) {
    return <div className="flex items-center justify-center h-24 text-sm text-muted-foreground">No trend data yet</div>
  }

  const hrs = normalize(points.map(p => p.hr))
  const brs = normalize(points.map(p => p.br))

  return (
    <div className="space-y-1">
      <svg viewBox="0 0 100 100" className="w-full h-20 overflow-visible">
        <Polyline values={hrs} color="#f43f5e" />
        <Polyline values={brs} color="#3b82f6" />
      </svg>
      <div className="flex gap-4 text-xs text-muted-foreground">
        <span className="flex items-center gap-1">
          <span className="inline-block w-3 h-0.5 bg-rose-500 rounded" />
          Heart Rate
        </span>
        <span className="flex items-center gap-1">
          <span className="inline-block w-3 h-0.5 bg-blue-500 rounded" />
          Breathing Rate
        </span>
      </div>
    </div>
  )
}
