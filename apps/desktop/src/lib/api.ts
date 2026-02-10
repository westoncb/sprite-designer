import { invoke } from "@tauri-apps/api/core";
import type { Project, ProjectSummary } from "@sprite-designer/shared/types";
import type { ChildResult, EditRequest, GenerateRequest } from "./types";

export async function listProjects(): Promise<ProjectSummary[]> {
  return invoke<ProjectSummary[]>("list_projects");
}

export async function getProject(projectId: string): Promise<Project> {
  return invoke<Project>("get_project", { projectId });
}

export async function generateImage(req: GenerateRequest): Promise<ChildResult> {
  return invoke<ChildResult>("generate_image", { req });
}

export async function editImage(req: EditRequest): Promise<ChildResult> {
  return invoke<ChildResult>("edit_image", { req });
}

export async function exportImageToPath(
  sourceImagePath: string,
  destinationPath: string,
  removeChromakeyBackground: boolean,
): Promise<string> {
  return invoke<string>("export_image_to_path", {
    sourceImagePath,
    destinationPath,
    removeChromakeyBackground,
  });
}
