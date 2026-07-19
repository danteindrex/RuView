import { Progress } from '@/components/ui/progress'
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from '@/components/ui/accordion'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

interface InsightResult {
  vitals_analysis?: {
    hr_classification?: string
    br_classification?: string
    hrv_assessment?: string
    observations?: string[]
  }
  anomaly_analysis?: {
    detected?: boolean
    fall_risk_score?: number
    mobility_score?: number
  }
  clinical_interpretation?: {
    primary_findings?: string
    differential?: string
    recommended_actions?: string[]
    urgency?: 'routine' | 'urgent' | 'emergency'
  }
  trend_analysis?: {
    direction?: 'improving' | 'stable' | 'deteriorating'
    significant_changes?: string[]
  }
  synthesis?: { summary?: string; action_items?: string[] }
}

function UrgencyBadge({ urgency }: { urgency?: string }) {
  if (urgency === 'emergency') return <Badge variant="danger">Emergency</Badge>
  if (urgency === 'urgent') return <Badge variant="outline" className="text-orange-600 border-orange-400">Urgent</Badge>
  return <Badge variant="outline" className="text-emerald-600 border-emerald-400">Routine</Badge>
}

function dirIcon(dir?: string) {
  if (dir === 'improving') return 'Improving'
  if (dir === 'deteriorating') return 'Deteriorating'
  return 'Stable'
}

export function AgentAccordion({ result }: { result: InsightResult }) {
  return (
    <div className="space-y-4">
      <Accordion type="multiple" className="border rounded-lg px-3">
        <AccordionItem value="vitals">
          <AccordionTrigger>Vitals Analysis</AccordionTrigger>
          <AccordionContent>
            {result.vitals_analysis ? (
              <div className="space-y-1 text-sm">
                <p><span className="text-muted-foreground">HR:</span> {result.vitals_analysis.hr_classification ?? '--'}</p>
                <p><span className="text-muted-foreground">BR:</span> {result.vitals_analysis.br_classification ?? '--'}</p>
                <p><span className="text-muted-foreground">HRV:</span> {result.vitals_analysis.hrv_assessment ?? '--'}</p>
                {result.vitals_analysis.observations?.map((obs, i) => (
                  <p key={i} className="text-muted-foreground text-xs">{obs}</p>
                ))}
              </div>
            ) : <p className="text-sm text-muted-foreground">No vitals data.</p>}
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="anomaly">
          <AccordionTrigger>Anomaly Analysis</AccordionTrigger>
          <AccordionContent>
            {result.anomaly_analysis ? (
              <div className="space-y-2 text-sm">
                <p><span className="text-muted-foreground">Detected:</span> {result.anomaly_analysis.detected ? 'Yes' : 'No'}</p>
                <div>
                  <div className="flex justify-between mb-1"><span className="text-muted-foreground">Fall Risk</span><span>{result.anomaly_analysis.fall_risk_score ?? 0}%</span></div>
                  <Progress value={result.anomaly_analysis.fall_risk_score ?? 0} />
                </div>
                <div>
                  <div className="flex justify-between mb-1"><span className="text-muted-foreground">Mobility</span><span>{result.anomaly_analysis.mobility_score ?? 0}%</span></div>
                  <Progress value={result.anomaly_analysis.mobility_score ?? 0} />
                </div>
              </div>
            ) : <p className="text-sm text-muted-foreground">No anomaly data.</p>}
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="clinical">
          <AccordionTrigger>Clinical Interpretation</AccordionTrigger>
          <AccordionContent>
            {result.clinical_interpretation ? (
              <div className="space-y-2 text-sm">
                <div className="flex items-center gap-2">
                  <span className="text-muted-foreground">Urgency:</span>
                  <UrgencyBadge urgency={result.clinical_interpretation.urgency} />
                </div>
                <p><span className="text-muted-foreground">Findings:</span> {result.clinical_interpretation.primary_findings ?? '--'}</p>
                <p><span className="text-muted-foreground">Differential:</span> {result.clinical_interpretation.differential ?? '--'}</p>
              </div>
            ) : <p className="text-sm text-muted-foreground">No clinical data.</p>}
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="trend">
          <AccordionTrigger>Trend Analysis</AccordionTrigger>
          <AccordionContent>
            {result.trend_analysis ? (
              <div className="space-y-2 text-sm">
                <p className="font-medium">{dirIcon(result.trend_analysis.direction)}</p>
                {result.trend_analysis.significant_changes?.map((c, i) => (
                  <p key={i} className="text-muted-foreground text-xs">{c}</p>
                ))}
              </div>
            ) : <p className="text-sm text-muted-foreground">No trend data.</p>}
          </AccordionContent>
        </AccordionItem>
      </Accordion>

      {result.synthesis && (
        <Card className="bg-secondary/20">
          <CardHeader className="pb-2"><CardTitle className="text-sm">Synthesis Summary</CardTitle></CardHeader>
          <CardContent className="text-sm space-y-2">
            <p>{result.synthesis.summary}</p>
            {result.synthesis.action_items && result.synthesis.action_items.length > 0 && (
              <div>
                <p className="font-medium text-xs uppercase tracking-wide text-muted-foreground mt-2 mb-1">Recommended Actions</p>
                <ol className="list-decimal list-inside space-y-1">
                  {result.synthesis.action_items.map((item, i) => <li key={i} className="text-xs">{item}</li>)}
                </ol>
              </div>
            )}
            {result.clinical_interpretation?.recommended_actions && (
              <div>
                <p className="font-medium text-xs uppercase tracking-wide text-muted-foreground mt-2 mb-1">Clinical Actions</p>
                <ol className="list-decimal list-inside space-y-1">
                  {result.clinical_interpretation.recommended_actions.map((a, i) => <li key={i} className="text-xs">{a}</li>)}
                </ol>
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  )
}
