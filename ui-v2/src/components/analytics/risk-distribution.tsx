/**
 * CSS bar chart — risk distribution across sessions.
 */
interface RiskBucket {
  label: 'Low' | 'Moderate' | 'High' | 'Critical'
  count: number
}

interface RiskDistributionProps {
  buckets: RiskBucket[]
}

const COLORS: Record<string, string> = {
  Low: 'bg-emerald-500',
  Moderate: 'bg-yellow-500',
  High: 'bg-orange-500',
  Critical: 'bg-red-500',
}

export function RiskDistribution({ buckets }: RiskDistributionProps) {
  const maxCount = Math.max(...buckets.map(b => b.count), 1)
  if (buckets.every(b => b.count === 0)) {
    return <div className="flex items-center justify-center h-20 text-sm text-muted-foreground">No session data yet</div>
  }
  return (
    <div className="space-y-2">
      {buckets.map(bucket => (
        <div key={bucket.label} className="flex items-center gap-3">
          <span className="text-xs w-16 text-muted-foreground shrink-0">{bucket.label}</span>
          <div className="flex-1 h-4 bg-secondary rounded-full overflow-hidden">
            <div
              className={`h-full rounded-full transition-all duration-500 ${COLORS[bucket.label]}`}
              style={{ width: `${(bucket.count / maxCount) * 100}%` }}
            />
          </div>
          <span className="text-xs font-mono w-6 text-right">{bucket.count}</span>
        </div>
      ))}
    </div>
  )
}
