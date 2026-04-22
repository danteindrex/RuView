import type React from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { PretextTitle } from "@/components/layout/pretext-title";
import { cn } from "@/lib/utils";

interface PageSectionProps {
  title: string;
  description: string;
  className?: string;
  children: React.ReactNode;
}

export function PageSection({ title, description, children, className }: PageSectionProps) {
  return (
    <Card className={cn("futuristic-outline", className)}>
      <CardHeader>
        <CardTitle>
          <PretextTitle text={title} as="h3" className="text-base font-semibold" maxLines={2} />
        </CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}
