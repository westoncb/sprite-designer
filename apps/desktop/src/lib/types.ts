import type { Child, Project, ProjectSummary, Resolution } from "@sprite-designer/shared/types";

export type AppTab = "generate" | "edit" | "preview";
export type SelectionChildId = string | null | "<new>";

export interface GenerateDraft {
  name: string;
  spriteMode: boolean;
  rows: number;
  cols: number;
  objectDescription: string;
  style: string;
  cameraAngle: string;
  promptText: string;
  resolution: Resolution;
  imagePriorDataUrl?: string;
}

export interface EditDraft {
  editPrompt: string;
}

export interface ChildResult {
  project: ProjectSummary;
  child: Child;
}

export interface GenerateRequest {
  projectId?: string;
  name?: string;
  spriteMode: boolean;
  rows?: number;
  cols?: number;
  objectDescription?: string;
  style?: string;
  cameraAngle?: string;
  promptText?: string;
  resolution: Resolution;
  imagePriorDataUrl?: string;
}

export interface EditRequest {
  projectId: string;
  baseChildId: string;
  name?: string;
  editPrompt: string;
  resolution?: Resolution;
  baseImageDataUrl?: string;
  baseImagePath?: string;
}

export interface ProjectRecord extends Project {}
