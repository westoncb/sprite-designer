export type ChildType = "generate" | "edit";
export type ChildMode = "sprite" | "normal" | "edit";
export type Resolution = "1K" | "2K" | "4K";

export interface ProjectSummary {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  childCount: number;
}

export interface ChildInputs {
  rows?: number;
  cols?: number;
  objectDescription?: string;
  style?: string;
  cameraAngle?: string;
  promptText?: string;
  editPrompt?: string;
  baseChildId?: string;
  resolution?: Resolution;
  imagePriorDataUrl?: string;
  baseImagePath?: string;
}

export interface OpenRouterSnapshot {
  model: string;
  payload: Record<string, unknown>;
}

export interface ChildOutputs {
  text?: string;
  imagePaths: string[];
  primaryImagePath?: string;
}

export interface Child {
  id: string;
  projectId: string;
  type: ChildType;
  name: string;
  createdAt: string;
  mode: ChildMode;
  inputs: ChildInputs;
  openrouter: OpenRouterSnapshot;
  outputs: ChildOutputs;
}

export interface Project {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  children: Child[];
}
