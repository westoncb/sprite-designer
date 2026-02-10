import type { Child, Resolution } from "@sprite-designer/shared/types";

const DEFAULT_RESOLUTION: Resolution = "1K";

export function todayStamp(): string {
  const date = new Date();
  const mm = String(date.getMonth() + 1).padStart(2, "0");
  const dd = String(date.getDate()).padStart(2, "0");
  const yyyy = String(date.getFullYear());
  return `${mm}-${dd}-${yyyy}`;
}

export function defaultProjectPlaceholder(rows: number, cols: number, projectCount: number): string {
  return `sprite-${rows}x${cols}-${todayStamp()}-${projectCount + 1}`;
}

export function latestChild(children: Child[]): Child | undefined {
  if (children.length === 0) {
    return undefined;
  }
  return children[children.length - 1];
}

export function latestGenerateChild(children: Child[]): Child | undefined {
  for (let index = children.length - 1; index >= 0; index -= 1) {
    if (children[index].type === "generate") {
      return children[index];
    }
  }
  return undefined;
}

export function safeResolution(value?: Resolution): Resolution {
  return value ?? DEFAULT_RESOLUTION;
}

export function asErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return String(error);
}
