/**
 * OnboardingWizard — first-run guided setup (W3).
 *
 * Steps 0–4 (welcome, hub, nodes, place, calibrate) render inside a Radix
 * Dialog with a Stepper progress rail + Back/Next/Skip. Step 5 (tour) leaves the
 * Dialog entirely and renders the non-modal Tour layer so it can anchor tooltips
 * over the real dashboard / 3D view / medical page (which live behind the modal
 * and on other routes). Completion is persisted via the onboarding store.
 */

import { useCallback } from "react";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Stepper } from "./Stepper";
import { Tour, type TourStop } from "./Tour";
import { ONBOARDING_STEPS, persistOnboardingComplete, STEP_COUNT, useOnboardingStore } from "@/lib/onboarding-store";
import { WelcomeStep } from "./steps/WelcomeStep";
import { HubCheckStep } from "./steps/HubCheckStep";
import { AddNodeStep } from "./steps/AddNodeStep";
import { PlaceNodesStep } from "./steps/PlaceNodesStep";
import { CalibrationStep } from "./steps/CalibrationStep";
import type { StepProps } from "./step-shell";
import type { ServerStatusResponse } from "@/types";

const STEP_LABELS = ["Welcome", "Hub", "Add node", "Place", "Calibrate", "Tour"];

const TOUR_STOPS: TourStop[] = [
  {
    title: "Your live dashboard",
    body: "These cards summarize the system at a glance: how many nodes are registered and online, plus the medical and security engines.",
    page: "dashboard",
    anchor: '[data-tour="dashboard-metrics"]',
  },
  {
    title: "Nodes, presence & vitals",
    body: "Nodes are your sensors. Presence means someone is detected in the room; vitals are heart/breathing rates derived from WiFi CSI.",
    page: "dashboard",
    anchor: '[data-tour="dashboard-control-plane"]',
  },
  {
    title: "The 3D observatory",
    body: "This live view fuses every node into one room. Placed markers, detected people, and the signal field all render here in real time.",
    page: "pose3d",
    anchor: '[data-tour="observatory"]',
  },
  {
    title: "Medical monitoring",
    body: "The Medical Hub tracks vital signs and fall detection continuously, and can dispatch alerts when thresholds are crossed.",
    page: "medical",
    anchor: '[data-tour="medical-header"]',
  },
];

interface OnboardingWizardProps {
  accessToken: string | null;
  serverStatus: ServerStatusResponse | null;
  onRefreshServer?: () => Promise<void>;
}

export function OnboardingWizard({ accessToken, serverStatus, onRefreshServer }: OnboardingWizardProps) {
  const { open, step, next, back, goTo, close, setTourPage } = useOnboardingStore();

  const isTourStep = ONBOARDING_STEPS[step] === "tour";
  const isLastDialogStep = step === STEP_COUNT - 2; // calibrate → next enters the tour

  const finish = useCallback(async () => {
    await persistOnboardingComplete(accessToken);
    close();
  }, [accessToken, close]);

  const stepProps: StepProps = {
    accessToken,
    serverStatus,
    onNext: next,
    onRefreshServer,
  };

  if (!open) return null;

  // ─── Tour step: render outside the Dialog ──────────────────────────────────
  if (isTourStep) {
    return (
      <Tour stops={TOUR_STOPS} onFinish={finish} onSkip={finish} onNavigate={setTourPage} />
    );
  }

  const renderStep = () => {
    switch (ONBOARDING_STEPS[step]) {
      case "welcome":
        return <WelcomeStep {...stepProps} />;
      case "hub":
        return <HubCheckStep {...stepProps} />;
      case "nodes":
        return <AddNodeStep {...stepProps} />;
      case "place":
        return <PlaceNodesStep {...stepProps} />;
      case "calibrate":
        return <CalibrationStep {...stepProps} />;
      default:
        return null;
    }
  };

  return (
    <Dialog open={open} onOpenChange={(v) => (!v ? void finish() : undefined)}>
      <DialogContent className="flex h-[86vh] max-h-[860px] w-[92vw] max-w-4xl flex-col gap-4">
        <Stepper steps={STEP_LABELS.map((label) => ({ label }))} current={step} onSelect={goTo} />

        <div className="flex min-h-0 flex-1 flex-col">{renderStep()}</div>

        <div className="flex shrink-0 items-center justify-between border-t border-border/60 pt-3">
          <Button variant="ghost" size="sm" onClick={finish}>
            Skip setup
          </Button>
          <div className="flex gap-2">
            {step > 0 ? (
              <Button variant="outline" size="sm" onClick={back}>
                Back
              </Button>
            ) : null}
            <Button size="sm" onClick={next}>
              {isLastDialogStep ? "Start tour" : "Next"}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
