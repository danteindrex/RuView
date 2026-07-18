import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { PretextTitle } from "@/components/layout/pretext-title";

interface MetricCardProps {
  title: string;
  value: string;
  subtitle?: string;
  tone?: "default" | "success" | "warning" | "danger";
}

export function MetricCard({ title, value, subtitle, tone = "default" }: MetricCardProps) {
  return (
    <Card className="futuristic-outline">
      <CardHeader className="pb-3">
        <PretextTitle text={title} className="text-sm font-medium text-muted-foreground" lineHeight={20} maxLines={2} as="p" />
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-3xl font-semibold tracking-tight">{value}</div>
        {subtitle ? <Badge variant={tone}>{subtitle}</Badge> : null}
      </CardContent>
    </Card>
  );
}

