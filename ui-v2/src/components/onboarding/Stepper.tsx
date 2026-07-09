/**
 * Stepper — horizontal progress rail for the onboarding wizard (W3).
 *
 * Each step is a clickable dot with a label; completed steps show a check.
 * Clicking a dot jumps to that step (resume-to-any-step).
 */

import { Check } from "lucide-react";
import { cn } from "@/lib/utils";

export interface StepperItem {
  label: string;
}

interface StepperProps {
  steps: StepperItem[];
  current: number;
  onSelect: (index: number) => void;
}

export function Stepper({ steps, current, onSelect }: StepperProps) {
  return (
    <nav aria-label="Onboarding progress" className="flex items-center gap-1 overflow-x-auto py-1">
      {steps.map((step, index) => {
        const isDone = index < current;
        const isActive = index === current;
        return (
          <div key={step.label} className="flex items-center">
            <button
              type="button"
              onClick={() => onSelect(index)}
              aria-current={isActive ? "step" : undefined}
              className={cn(
                "flex items-center gap-2 rounded-full px-3 py-1.5 text-xs font-medium transition-colors",
                isActive
                  ? "bg-primary text-primary-foreground"
                  : isDone
                    ? "text-primary hover:bg-primary/10"
                    : "text-muted-foreground hover:bg-secondary/60",
              )}
            >
              <span
                className={cn(
                  "flex h-5 w-5 items-center justify-center rounded-full border text-[10px]",
                  isActive
                    ? "border-primary-foreground/60"
                    : isDone
                      ? "border-primary bg-primary/10"
                      : "border-border",
                )}
              >
                {isDone ? <Check className="h-3 w-3" /> : index + 1}
              </span>
              <span className="hidden whitespace-nowrap sm:inline">{step.label}</span>
            </button>
            {index < steps.length - 1 ? (
              <div className={cn("mx-1 h-px w-4 shrink-0", isDone ? "bg-primary/60" : "bg-border")} />
            ) : null}
          </div>
        );
      })}
    </nav>
  );
}
