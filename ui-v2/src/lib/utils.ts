import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function prettyJson(value: unknown) {
  return JSON.stringify(value, null, 2);
}

/** True when a "host" or "host:port" string targets a loopback address. */
export function isLoopbackHostPort(value: string): boolean {
  const host = value.split(":")[0]?.trim().toLowerCase() ?? "";
  return host === "localhost" || host === "::1" || host === "127.0.0.1" || host.startsWith("127.");
}

