interface UpgradePromptProps {
  feature: string
  requiredPlan: 'cloud' | 'enterprise'
  className?: string
}

export function UpgradePrompt({ feature, requiredPlan, className = '' }: UpgradePromptProps) {
  const planLabel = requiredPlan === 'cloud' ? 'Cloud' : 'Enterprise'
  return (
    <div className={`flex flex-col items-center justify-center p-8 text-center ${className}`}>
      <div className="text-4xl mb-3">🔒</div>
      <h3 className="text-lg font-semibold mb-2">{feature} — {planLabel} Plan Required</h3>
      <p className="text-sm text-gray-500 mb-4 max-w-xs">
        Upgrade to the {planLabel} plan to access {feature} and other advanced features.
      </p>
      <a
        href="mailto:sales@ruview.io"
        className="bg-blue-600 text-white px-5 py-2 rounded-md text-sm font-medium hover:bg-blue-700"
      >
        Contact Sales — {planLabel} Plan
      </a>
    </div>
  )
}
