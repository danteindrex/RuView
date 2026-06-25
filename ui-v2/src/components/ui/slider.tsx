import * as React from "react";
import { cn } from "@/lib/utils";

interface SliderProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> {
  value?: number[];
  onValueChange?: (value: number[]) => void;
}

const Slider = React.forwardRef<HTMLInputElement, SliderProps>(
  ({ className, value, onValueChange, min = 0, max = 100, step = 1, ...props }, ref) => {
    const currentValue = value?.[0] ?? (typeof min === "number" ? min : 0);

    return (
      <input
        type="range"
        ref={ref}
        min={min}
        max={max}
        step={step}
        value={currentValue}
        onChange={(e) => onValueChange?.([Number(e.target.value)])}
        className={cn(
          "w-full h-2 rounded-full appearance-none cursor-pointer",
          "bg-secondary accent-primary",
          "[&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:h-4 [&::-webkit-slider-thumb]:w-4",
          "[&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-primary",
          "[&::-webkit-slider-thumb]:border-2 [&::-webkit-slider-thumb]:border-background",
          "[&::-webkit-slider-thumb]:shadow-md [&::-webkit-slider-thumb]:cursor-pointer",
          "focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
          className
        )}
        {...props}
      />
    );
  }
);

Slider.displayName = "Slider";

export { Slider };
