import React from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { Child, Project, Resolution } from "@sprite-designer/shared/types";
import {
  editImage,
  exportImageToPath,
  generateImage,
  getProject,
  listProjects,
} from "./lib/api";
import { SpriteSheetPlayer } from "./components/SpriteSheetPlayer";
import {
  asErrorMessage,
  defaultProjectPlaceholder,
  latestChild,
  latestGenerateChild,
  safeResolution,
} from "./lib/format";
import { createSpriteGridDataUrl, readFileAsDataUrl } from "./lib/grid";
import type {
  AppTab,
  EditDraft,
  GenerateDraft,
  SelectionChildId,
} from "./lib/types";

const NEW_ITEM = "<new>" as const;
const RESOLUTIONS: Resolution[] = ["1K", "2K", "4K"];
type PendingScope = { kind: "new" } | { kind: "project"; projectId: string };

function createDefaultGenerateDraft(
  overrides?: Partial<GenerateDraft>,
): GenerateDraft {
  const base: GenerateDraft = {
    name: "",
    spriteMode: true,
    rows: 4,
    cols: 4,
    objectDescription: "",
    style: "",
    cameraAngle: "",
    promptText: "",
    resolution: "1K",
    imagePriorDataUrl: createSpriteGridDataUrl(4, 4, "1K"),
  };

  return {
    ...base,
    ...overrides,
  };
}

function toRenderableImage(path?: string): string | undefined {
  if (!path) {
    return undefined;
  }

  try {
    return convertFileSrc(path);
  } catch {
    return path;
  }
}

function toSafeExportName(value: string): string {
  const trimmed = value.trim();
  const normalized = trimmed
    .replace(/[^a-zA-Z0-9._-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");

  if (!normalized) {
    return "sprite-export";
  }

  return normalized;
}

function upsertProject(existing: Project[], incoming: Project): Project[] {
  const next = existing.filter((project) => project.id !== incoming.id);
  next.push(incoming);
  next.sort(
    (a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
  );
  return next;
}

function resolveGenerateDraft(
  project: Project,
  selectedChild?: Child,
): GenerateDraft {
  const generateSource =
    selectedChild?.type === "generate"
      ? selectedChild
      : latestGenerateChild(project.children);

  if (!generateSource) {
    return createDefaultGenerateDraft({ name: project.name });
  }

  const spriteMode = generateSource.mode === "sprite";
  const rows = Math.max(1, generateSource.inputs.rows ?? 4);
  const cols = Math.max(1, generateSource.inputs.cols ?? 4);
  const resolution = safeResolution(generateSource.inputs.resolution);

  let imagePriorDataUrl = generateSource.inputs.imagePriorDataUrl;
  if (spriteMode && !imagePriorDataUrl) {
    imagePriorDataUrl = createSpriteGridDataUrl(rows, cols, resolution);
  }

  return {
    name: project.name,
    spriteMode,
    rows,
    cols,
    objectDescription: generateSource.inputs.objectDescription ?? "",
    style: generateSource.inputs.style ?? "",
    cameraAngle: generateSource.inputs.cameraAngle ?? "",
    promptText: generateSource.inputs.promptText ?? "",
    resolution,
    imagePriorDataUrl,
  };
}

function validateGenerateDraft(draft: GenerateDraft): string | null {
  if (draft.spriteMode) {
    if (draft.rows < 1 || draft.cols < 1) {
      return "Rows and Cols must be positive integers.";
    }
    if (!draft.objectDescription.trim()) {
      return "Description is required in sprite mode.";
    }
    if (!draft.style.trim()) {
      return "Style is required in sprite mode.";
    }
    if (!draft.cameraAngle.trim()) {
      return "Camera angle is required in sprite mode.";
    }
    return null;
  }

  if (!draft.promptText.trim()) {
    return "Prompt is required in normal mode.";
  }

  return null;
}

const MAX_REASONING_FALLBACK_CHARS = 6000;

function collectReasoningDetailText(value: unknown, output: string[]): void {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectReasoningDetailText(item, output);
    }
    return;
  }

  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const text = record.text;
    if (typeof text === "string" && text.trim()) {
      output.push(text.trim());
    }

    for (const [key, nested] of Object.entries(record)) {
      if (key === "text") {
        continue;
      }
      if (Array.isArray(nested) || (nested && typeof nested === "object")) {
        collectReasoningDetailText(nested, output);
      }
    }
  }
}

function reasoningDetailsText(reasoningDetails?: string): string | undefined {
  if (!reasoningDetails) {
    return undefined;
  }

  const trimmed = reasoningDetails.trim();
  if (!trimmed) {
    return undefined;
  }

  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
    return trimmed;
  }

  try {
    const parsed = JSON.parse(trimmed);
    const snippets: string[] = [];
    collectReasoningDetailText(parsed, snippets);
    if (snippets.length > 0) {
      return snippets.join("\n\n");
    }
  } catch {
    // Keep a bounded fallback for legacy payloads that are not valid JSON.
  }

  if (trimmed.length <= MAX_REASONING_FALLBACK_CHARS) {
    return trimmed;
  }

  return `${trimmed.slice(0, MAX_REASONING_FALLBACK_CHARS)}\n\n[truncated large reasoning details]`;
}

function App() {
  const [projects, setProjects] = React.useState<Project[]>([]);
  const [selectedProjectId, setSelectedProjectId] = React.useState<
    string | null
  >(null);
  const [selectedChildId, setSelectedChildId] =
    React.useState<SelectionChildId>(NEW_ITEM);
  const [activeTab, setActiveTab] = React.useState<AppTab>("generate");

  const [draftGenerateForm, setDraftGenerateForm] =
    React.useState<GenerateDraft>(() => createDefaultGenerateDraft());
  const [draftEditForm, setDraftEditForm] = React.useState<EditDraft>({
    editPrompt: "",
  });

  const [isLoadingProjects, setIsLoadingProjects] = React.useState(false);
  const [pendingAction, setPendingAction] = React.useState<
    "generate" | "edit" | null
  >(null);
  const [pendingScope, setPendingScope] = React.useState<PendingScope | null>(
    null,
  );
  const [generateError, setGenerateError] = React.useState<string | null>(null);
  const [editError, setEditError] = React.useState<string | null>(null);
  const [isExporting, setIsExporting] = React.useState(false);
  const [exportError, setExportError] = React.useState<string | null>(null);
  const [exportResult, setExportResult] = React.useState<string | null>(null);
  const [isSeedImageExpanded, setIsSeedImageExpanded] = React.useState(false);
  const [previewMode, setPreviewMode] = React.useState<"image" | "animation">(
    "image",
  );
  const [frameDelayInput, setFrameDelayInput] = React.useState("120");
  const imagePriorInputRef = React.useRef<HTMLInputElement | null>(null);

  const selectedProject = React.useMemo(
    () => projects.find((project) => project.id === selectedProjectId),
    [projects, selectedProjectId],
  );

  const selectedChild = React.useMemo(() => {
    if (
      !selectedProject ||
      selectedChildId === NEW_ITEM ||
      selectedChildId === null
    ) {
      return undefined;
    }

    return selectedProject.children.find(
      (child) => child.id === selectedChildId,
    );
  }, [selectedProject, selectedChildId]);

  const generatePreviewChild = React.useMemo(() => {
    if (selectedChild?.type === "generate") {
      return selectedChild;
    }

    if (selectedProject && selectedChildId === null) {
      return latestGenerateChild(selectedProject.children);
    }

    return undefined;
  }, [selectedChild, selectedProject, selectedChildId]);

  const baseChildForEdit = React.useMemo(() => {
    if (!selectedProject) {
      return undefined;
    }

    const resolveBaseChild = (candidate?: Child): Child | undefined => {
      if (!candidate) {
        return undefined;
      }

      if (candidate.type !== "edit") {
        return candidate;
      }

      const baseChildId = candidate.inputs.baseChildId;
      if (!baseChildId) {
        return candidate;
      }

      return (
        selectedProject.children.find((child) => child.id === baseChildId) ??
        candidate
      );
    };

    if (selectedChild) {
      return resolveBaseChild(selectedChild);
    }

    if (selectedChildId === null) {
      return resolveBaseChild(latestChild(selectedProject.children));
    }

    return undefined;
  }, [selectedProject, selectedChild, selectedChildId]);

  const editedPreviewChild =
    selectedChild?.type === "edit" ? selectedChild : undefined;
  const previewChild = React.useMemo(() => {
    if (!selectedProject) {
      return undefined;
    }

    if (selectedChild) {
      return selectedChild;
    }

    if (selectedChildId === null) {
      return latestChild(selectedProject.children);
    }

    return undefined;
  }, [selectedProject, selectedChild, selectedChildId]);

  const loadProjects = React.useCallback(async () => {
    setIsLoadingProjects(true);
    try {
      const summaries = await listProjects();
      const fullProjects = await Promise.all(
        summaries.map((summary) => getProject(summary.id)),
      );
      fullProjects.sort(
        (a, b) =>
          new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
      );
      setProjects(fullProjects);
    } catch (error) {
      setGenerateError(`Failed to load projects: ${asErrorMessage(error)}`);
    } finally {
      setIsLoadingProjects(false);
    }
  }, []);

  React.useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  React.useEffect(() => {
    if (selectedChildId === NEW_ITEM) {
      setDraftGenerateForm(createDefaultGenerateDraft());
      setDraftEditForm({ editPrompt: "" });
      return;
    }

    if (!selectedProject) {
      return;
    }

    setDraftGenerateForm(resolveGenerateDraft(selectedProject, selectedChild));

    if (selectedChild?.type === "edit") {
      setDraftEditForm({ editPrompt: selectedChild.inputs.editPrompt ?? "" });
    } else {
      setDraftEditForm({ editPrompt: "" });
    }
  }, [selectedProject, selectedChild, selectedChildId]);

  React.useEffect(() => {
    if (
      selectedProjectId &&
      !projects.some((project) => project.id === selectedProjectId)
    ) {
      setSelectedProjectId(null);
      setSelectedChildId(NEW_ITEM);
    }
  }, [projects, selectedProjectId]);

  const refreshProjectAndSelectChild = React.useCallback(
    async (projectId: string, childId: string) => {
      const freshProject = await getProject(projectId);
      setProjects((previous) => upsertProject(previous, freshProject));
      setSelectedProjectId(projectId);
      setSelectedChildId(childId);
    },
    [],
  );

  const handleSelectNew = () => {
    setSelectedProjectId(null);
    setSelectedChildId(NEW_ITEM);
    setGenerateError(null);
    setEditError(null);
    setExportError(null);
    setExportResult(null);
  };

  const handleSelectProject = (projectId: string) => {
    setSelectedProjectId(projectId);
    setSelectedChildId(null);
    setGenerateError(null);
    setEditError(null);
    setExportError(null);
    setExportResult(null);
  };

  const handleSelectChild = (projectId: string, childId: string) => {
    setSelectedProjectId(projectId);
    setSelectedChildId(childId);
    setGenerateError(null);
    setEditError(null);
    setExportError(null);
    setExportResult(null);
  };

  const patchGenerateDraft = (patch: Partial<GenerateDraft>) => {
    setDraftGenerateForm((previous) => ({ ...previous, ...patch }));
  };

  const updateSpriteGrid = (patch: Partial<GenerateDraft>) => {
    setDraftGenerateForm((previous) => {
      const next = { ...previous, ...patch };
      if (!next.spriteMode) {
        return next;
      }

      return {
        ...next,
        imagePriorDataUrl: createSpriteGridDataUrl(
          next.rows,
          next.cols,
          next.resolution,
        ),
      };
    });
  };

  const handleToggleSpriteMode = (enabled: boolean) => {
    if (enabled) {
      updateSpriteGrid({ spriteMode: true });
      return;
    }

    setDraftGenerateForm((previous) => ({
      ...previous,
      spriteMode: false,
      imagePriorDataUrl: undefined,
    }));
  };

  const handleGeneratePriorUpload = async (
    event: React.ChangeEvent<HTMLInputElement>,
  ) => {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) {
      return;
    }

    if (!/^image\/(png|jpeg|webp)$/.test(file.type)) {
      setGenerateError(
        "Only PNG, JPEG, and WEBP imagePrior files are supported.",
      );
      return;
    }

    try {
      const dataUrl = await readFileAsDataUrl(file);
      setDraftGenerateForm((previous) => ({
        ...previous,
        imagePriorDataUrl: dataUrl,
      }));
      setGenerateError(null);
    } catch (error) {
      setGenerateError(`Failed to read image prior: ${asErrorMessage(error)}`);
    }
  };

  const handleGenerate = async () => {
    const validationError = validateGenerateDraft(draftGenerateForm);
    if (validationError) {
      setGenerateError(validationError);
      return;
    }

    setPendingAction("generate");
    setPendingScope(
      selectedProjectId
        ? {
            kind: "project",
            projectId: selectedProjectId,
          }
        : {
            kind: "new",
          },
    );
    setGenerateError(null);

    try {
      const trimmedName = draftGenerateForm.name.trim();
      const requestBase = {
        projectId:
          selectedChildId === NEW_ITEM
            ? undefined
            : (selectedProjectId ?? undefined),
        name: trimmedName || undefined,
        spriteMode: draftGenerateForm.spriteMode,
        resolution: draftGenerateForm.resolution,
        imagePriorDataUrl: draftGenerateForm.imagePriorDataUrl,
      };

      const response = draftGenerateForm.spriteMode
        ? await generateImage({
            ...requestBase,
            rows: draftGenerateForm.rows,
            cols: draftGenerateForm.cols,
            objectDescription: draftGenerateForm.objectDescription,
            style: draftGenerateForm.style,
            cameraAngle: draftGenerateForm.cameraAngle,
          })
        : await generateImage({
            ...requestBase,
            promptText: draftGenerateForm.promptText,
          });

      await refreshProjectAndSelectChild(
        response.project.id,
        response.child.id,
      );
      setActiveTab("generate");
    } catch (error) {
      setGenerateError(asErrorMessage(error));
    } finally {
      setPendingAction(null);
      setPendingScope(null);
    }
  };

  const handleEdit = async () => {
    if (!selectedProject || !baseChildForEdit) {
      setEditError("Select a project and child before running edit.");
      return;
    }

    if (!draftEditForm.editPrompt.trim()) {
      setEditError("Edit prompt is required.");
      return;
    }

    const baseImagePath = baseChildForEdit.outputs.primaryImagePath;
    if (!baseImagePath) {
      setEditError("No base image available to edit.");
      return;
    }

    setPendingAction("edit");
    setPendingScope({
      kind: "project",
      projectId: selectedProject.id,
    });
    setEditError(null);

    try {
      const response = await editImage({
        projectId: selectedProject.id,
        baseChildId: baseChildForEdit.id,
        name: draftGenerateForm.name.trim() || selectedProject.name,
        editPrompt: draftEditForm.editPrompt,
        resolution: draftGenerateForm.resolution,
        baseImagePath,
      });

      await refreshProjectAndSelectChild(
        response.project.id,
        response.child.id,
      );
      setActiveTab("edit");
    } catch (error) {
      setEditError(asErrorMessage(error));
    } finally {
      setPendingAction(null);
      setPendingScope(null);
    }
  };

  const projectNamePlaceholder = defaultProjectPlaceholder(
    draftGenerateForm.rows,
    draftGenerateForm.cols,
    projects.length,
  );

  const baseImageSrc = toRenderableImage(
    baseChildForEdit?.outputs.primaryImagePath,
  );
  const editedImageSrc = toRenderableImage(
    editedPreviewChild?.outputs.primaryImagePath,
  );
  const retainedEditBaseImageSrc = React.useMemo(() => {
    if (selectedChild?.type !== "edit") {
      return undefined;
    }

    const retainedBasePath =
      selectedChild.inputs.baseImagePath ??
      baseChildForEdit?.outputs.primaryImagePath;
    return toRenderableImage(retainedBasePath);
  }, [selectedChild, baseChildForEdit]);
  const parsedReasoningDetails = React.useMemo(
    () =>
      reasoningDetailsText(
        generatePreviewChild?.outputs.completion?.reasoningDetails,
      ),
    [generatePreviewChild?.outputs.completion?.reasoningDetails],
  );
  const previewImagePath =
    previewChild?.outputs.primaryImagePath ??
    previewChild?.outputs.imagePaths[0];
  const previewImageSrc = toRenderableImage(previewImagePath);
  const exportFileName = React.useMemo(() => {
    const baseName = previewChild?.name
      ? toSafeExportName(previewChild.name)
      : "sprite-export";
    return `${baseName}.png`;
  }, [previewChild?.name]);
  const previewRows = Math.max(1, previewChild?.inputs.rows ?? 1);
  const previewCols = Math.max(1, previewChild?.inputs.cols ?? 1);
  const previewIsSpriteSheet =
    previewChild?.mode === "sprite" && previewRows * previewCols > 1;
  const frameDelayMs = React.useMemo(() => {
    const parsed = Number.parseInt(frameDelayInput, 10);
    if (Number.isNaN(parsed)) {
      return 16;
    }

    return Math.max(16, parsed);
  }, [frameDelayInput]);
  const isPendingInCurrentContext = React.useMemo(() => {
    if (!pendingAction || !pendingScope) {
      return false;
    }

    if (pendingScope.kind === "new") {
      return selectedChildId === NEW_ITEM;
    }

    return selectedProjectId === pendingScope.projectId;
  }, [pendingAction, pendingScope, selectedChildId, selectedProjectId]);

  React.useEffect(() => {
    if (!previewIsSpriteSheet && previewMode !== "image") {
      setPreviewMode("image");
    }
  }, [previewIsSpriteSheet, previewMode]);

  React.useEffect(() => {
    setExportError(null);
    setExportResult(null);
  }, [previewImagePath]);

  const handleSaveAsExport = React.useCallback(async () => {
    if (!previewImagePath) {
      setExportError("No output image available to export.");
      setExportResult(null);
      return;
    }

    setExportError(null);
    setExportResult(null);

    const destinationPath = await save({
      defaultPath: exportFileName,
      filters: [{ name: "PNG Image", extensions: ["png"] }],
    });

    if (!destinationPath) {
      return;
    }

    setIsExporting(true);
    try {
      const finalPath = await exportImageToPath(previewImagePath, destinationPath);
      setExportResult(`Saved to ${finalPath}`);
    } catch (error) {
      setExportError(asErrorMessage(error));
    } finally {
      setIsExporting(false);
    }
  }, [previewImagePath, exportFileName]);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <button
          className={`list-item list-item-new ${selectedChildId === NEW_ITEM ? "is-selected" : ""}`}
          onClick={handleSelectNew}
          type="button"
        >
          NEW
        </button>

        {isLoadingProjects && <p className="muted">Loading projects...</p>}

        {projects.map((project) => {
          const isProjectSelected =
            selectedProjectId === project.id && selectedChildId === null;
          const isProjectPending =
            pendingAction !== null &&
            pendingScope?.kind === "project" &&
            pendingScope.projectId === project.id;

          return (
            <section className="project-group" key={project.id}>
              <button
                className={`list-item project-item ${isProjectSelected ? "is-selected" : ""}`}
                onClick={() => handleSelectProject(project.id)}
                type="button"
              >
                <span className="project-name">{project.name}</span>
                <span className="project-meta-wrap">
                  <span className="project-meta">
                    {project.children.length}
                  </span>
                  {isProjectPending && (
                    <span aria-hidden="true" className="project-spinner" />
                  )}
                </span>
              </button>
              <div className="child-list">
                {project.children.map((child) => {
                  const isSelectedChild =
                    selectedProjectId === project.id &&
                    selectedChildId === child.id;

                  return (
                    <button
                      className={`list-item child-item ${isSelectedChild ? "is-selected" : ""}`}
                      key={child.id}
                      onClick={() => handleSelectChild(project.id, child.id)}
                      type="button"
                    >
                      <span className="child-item-name">{child.name}</span>
                      <span className="child-item-type">{child.type}</span>
                    </button>
                  );
                })}
              </div>
            </section>
          );
        })}
      </aside>

      <main className="main-panel">
        <header className="tabs">
          {(["generate", "edit", "preview", "export"] as AppTab[]).map((tab) => (
            <button
              className={`tab ${activeTab === tab ? "tab-active" : ""}`}
              key={tab}
              onClick={() => setActiveTab(tab)}
              type="button"
            >
              {tab[0].toUpperCase() + tab.slice(1)}
            </button>
          ))}
        </header>

        {pendingAction !== null && isPendingInCurrentContext && (
          <section
            aria-live="polite"
            className="operation-banner"
            role="status"
          >
            <span aria-hidden="true" className="operation-banner-spinner" />
            <div className="operation-banner-copy">
              <p className="operation-banner-title">Generation in progress</p>
              <p className="operation-banner-detail">
                {pendingAction === "edit"
                  ? "Applying edit prompt and generating output..."
                  : "Generating image output..."}
              </p>
            </div>
          </section>
        )}

        {activeTab === "generate" && (
          <section className="panel-content">
            <div className="form-row header-row">
              <label className="field field-inline grow">
                <span>Name</span>
                <input
                  onChange={(event) =>
                    patchGenerateDraft({ name: event.target.value })
                  }
                  placeholder={projectNamePlaceholder}
                  type="text"
                  value={draftGenerateForm.name}
                />
              </label>

              <label className="toggle-field">
                <span>Sprite sheet</span>
                <input
                  className="toggle-switch"
                  checked={draftGenerateForm.spriteMode}
                  onChange={(event) =>
                    handleToggleSpriteMode(event.target.checked)
                  }
                  type="checkbox"
                />
              </label>
            </div>

            {draftGenerateForm.spriteMode ? (
              <>
                <div className="form-row grid-fields compact-grid-fields">
                  <label className="field field-inline compact-field number-field">
                    <span>Rows</span>
                    <input
                      className="number-input"
                      min={1}
                      onChange={(event) =>
                        updateSpriteGrid({
                          rows: Math.max(
                            1,
                            Math.floor(Number(event.target.value) || 1),
                          ),
                        })
                      }
                      type="number"
                      value={draftGenerateForm.rows}
                    />
                  </label>
                  <label className="field field-inline compact-field number-field">
                    <span>Cols</span>
                    <input
                      className="number-input"
                      min={1}
                      onChange={(event) =>
                        updateSpriteGrid({
                          cols: Math.max(
                            1,
                            Math.floor(Number(event.target.value) || 1),
                          ),
                        })
                      }
                      type="number"
                      value={draftGenerateForm.cols}
                    />
                  </label>
                  <label className="field field-inline">
                    <span>Camera angle</span>
                    <input
                      autoCapitalize="off"
                      autoComplete="off"
                      autoCorrect="off"
                      onChange={(event) =>
                        patchGenerateDraft({ cameraAngle: event.target.value })
                      }
                      spellCheck={false}
                      type="text"
                      value={draftGenerateForm.cameraAngle}
                    />
                  </label>
                  <label className="field field-inline compact-field">
                    <span>Resolution</span>
                    <select
                      onChange={(event) =>
                        updateSpriteGrid({
                          resolution: event.target.value as Resolution,
                        })
                      }
                      value={draftGenerateForm.resolution}
                    >
                      {RESOLUTIONS.map((resolution) => (
                        <option key={resolution} value={resolution}>
                          {resolution}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <label className="field field-inline grow">
                  <span>Style</span>
                  <input
                    autoCapitalize="off"
                    autoComplete="off"
                    autoCorrect="off"
                    onChange={(event) =>
                      patchGenerateDraft({ style: event.target.value })
                    }
                    spellCheck={false}
                    type="text"
                    value={draftGenerateForm.style}
                  />
                </label>
                <label className="field field-inline">
                  <span>Description</span>
                  <textarea
                    autoCapitalize="off"
                    autoComplete="off"
                    autoCorrect="off"
                    onChange={(event) =>
                      patchGenerateDraft({
                        objectDescription: event.target.value,
                      })
                    }
                    rows={2}
                    spellCheck={false}
                    value={draftGenerateForm.objectDescription}
                  />
                </label>
              </>
            ) : (
              <>
                <label className="field">
                  <span>Prompt</span>
                  <textarea
                    onChange={(event) =>
                      patchGenerateDraft({ promptText: event.target.value })
                    }
                    rows={5}
                    value={draftGenerateForm.promptText}
                  />
                </label>
                <label className="field compact-field">
                  <span>Resolution</span>
                  <select
                    onChange={(event) =>
                      patchGenerateDraft({
                        resolution: event.target.value as Resolution,
                      })
                    }
                    value={draftGenerateForm.resolution}
                  >
                    {RESOLUTIONS.map((resolution) => (
                      <option key={resolution} value={resolution}>
                        {resolution}
                      </option>
                    ))}
                  </select>
                </label>
              </>
            )}

            <section
              className={`image-prior-panel ${isSeedImageExpanded ? "is-expanded" : "is-collapsed"}`}
            >
              <button
                aria-expanded={isSeedImageExpanded}
                className="seed-panel-toggle"
                onClick={() => setIsSeedImageExpanded((previous) => !previous)}
                type="button"
              >
                <span className="seed-panel-title">Seed image</span>
                <span className="seed-panel-summary">
                  {draftGenerateForm.imagePriorDataUrl ? "ready" : "empty"}
                </span>
                <span
                  className={`seed-panel-chevron ${isSeedImageExpanded ? "is-open" : ""}`}
                  aria-hidden="true"
                >
                  ▾
                </span>
              </button>

              {isSeedImageExpanded && (
                <button
                  className="image-dropzone"
                  onClick={() => imagePriorInputRef.current?.click()}
                  type="button"
                >
                  {draftGenerateForm.imagePriorDataUrl ? (
                    <img
                      alt="Image prior"
                      src={draftGenerateForm.imagePriorDataUrl}
                    />
                  ) : (
                    <span className="muted">
                      Click to choose image prior (optional)
                    </span>
                  )}
                </button>
              )}

              <input
                accept="image/png,image/jpeg,image/webp"
                className="hidden-file-input"
                onChange={handleGeneratePriorUpload}
                ref={imagePriorInputRef}
                type="file"
              />
            </section>

            <div className="action-row">
              <button
                className="primary-button"
                disabled={pendingAction !== null}
                onClick={handleGenerate}
                type="button"
              >
                {pendingAction === "generate"
                  ? "Generating..."
                  : selectedChild?.type === "generate"
                    ? "Re-generate"
                    : "Generate"}
              </button>
            </div>

            {generateError && <p className="error-banner">{generateError}</p>}

            {(generatePreviewChild || retainedEditBaseImageSrc) && (
              <section className="output-panel">
                {generatePreviewChild ? (
                  <>
                    {generatePreviewChild.outputs.imagePaths.length > 0 ? (
                      <div className="image-grid">
                        {generatePreviewChild.outputs.imagePaths.map((path) => {
                          const src = toRenderableImage(path);
                          if (!src) {
                            return null;
                          }

                          return (
                            <img
                              alt={generatePreviewChild.name}
                              key={path}
                              src={src}
                            />
                          );
                        })}
                      </div>
                    ) : (
                      <p className="muted">
                        No output image saved for this child.
                      </p>
                    )}
                    <div className="result-metadata">
                      {generatePreviewChild.outputs.completion
                        ?.finishReason && (
                        <p className="meta-line">
                          <span className="meta-key">finish_reason</span>
                          <span className="meta-value">
                            {
                              generatePreviewChild.outputs.completion
                                .finishReason
                            }
                          </span>
                        </p>
                      )}

                      {generatePreviewChild.outputs.text && (
                        <div className="meta-block">
                          <p className="meta-key">message.content</p>
                          <pre className="output-text">
                            {generatePreviewChild.outputs.text}
                          </pre>
                        </div>
                      )}

                      {generatePreviewChild.outputs.completion?.refusal && (
                        <div className="meta-block">
                          <p className="meta-key">message.refusal</p>
                          <pre className="output-text">
                            {generatePreviewChild.outputs.completion.refusal}
                          </pre>
                        </div>
                      )}

                      {generatePreviewChild.outputs.completion?.reasoning && (
                        <div className="meta-block">
                          <p className="meta-key">message.reasoning</p>
                          <pre className="output-text">
                            {generatePreviewChild.outputs.completion.reasoning}
                          </pre>
                        </div>
                      )}

                      {parsedReasoningDetails && (
                        <div className="meta-block">
                          <p className="meta-key">
                            message.reasoning_details.text
                          </p>
                          <pre className="output-text">
                            {parsedReasoningDetails}
                          </pre>
                        </div>
                      )}
                    </div>
                  </>
                ) : retainedEditBaseImageSrc ? (
                  <>
                    <div className="image-grid">
                      <img
                        alt="Retained base image"
                        src={retainedEditBaseImageSrc}
                      />
                    </div>
                    <div className="result-metadata">
                      <p className="meta-line">
                        <span className="meta-key">retained_base_image</span>
                        <span className="meta-value">
                          {baseChildForEdit?.name
                            ? `${baseChildForEdit.name} (${baseChildForEdit.type})`
                            : "edit base"}
                        </span>
                      </p>
                    </div>
                  </>
                ) : (
                  <p className="muted">
                    No retained base image available for this edit child.
                  </p>
                )}
              </section>
            )}
          </section>
        )}

        {activeTab === "edit" && (
          <section className="panel-content">
            {!selectedProject && (
              <p className="muted">
                Select a project child to set the base image for edits.
              </p>
            )}

            {selectedProject && (
              <>
                <div className="edit-preview-grid">
                  <section className="preview-card">
                    <h2>Base image</h2>
                    {baseImageSrc ? (
                      <img alt="Base" src={baseImageSrc} />
                    ) : (
                      <div className="placeholder">
                        No base image available.
                      </div>
                    )}
                  </section>

                  <section className="preview-card">
                    <h2>Edited output</h2>
                    {editedImageSrc ? (
                      <img alt="Edited" src={editedImageSrc} />
                    ) : (
                      <div className="placeholder">No edited output yet.</div>
                    )}
                  </section>
                </div>

                <label className="field">
                  <span>Edit prompt</span>
                  <textarea
                    autoCapitalize="off"
                    autoComplete="off"
                    autoCorrect="off"
                    onChange={(event) =>
                      setDraftEditForm((previous) => ({
                        ...previous,
                        editPrompt: event.target.value,
                      }))
                    }
                    rows={4}
                    spellCheck={false}
                    value={draftEditForm.editPrompt}
                  />
                </label>

                <div className="action-row">
                  <button
                    className="primary-button"
                    disabled={pendingAction !== null}
                    onClick={handleEdit}
                    type="button"
                  >
                    {pendingAction === "edit"
                      ? "Generating..."
                      : selectedChild?.type === "edit"
                        ? "Re-generate"
                        : "Generate"}
                  </button>
                </div>

                {editError && <p className="error-banner">{editError}</p>}
              </>
            )}
          </section>
        )}

        {activeTab === "preview" && (
          <section className="panel-content preview-panel">
            {!previewChild && (
              <p className="muted">Select a project child to preview output.</p>
            )}

            {previewChild && (
              <>
                <div className="preview-toolbar">
                  <p className="muted">
                    {previewChild.name} ({previewChild.type})
                  </p>

                  <div className="preview-controls">
                    {previewIsSpriteSheet && (
                      <div className="mode-toggle">
                        <button
                          className={`mode-toggle-button ${previewMode === "image" ? "is-active" : ""}`}
                          onClick={() => setPreviewMode("image")}
                          type="button"
                        >
                          Full image
                        </button>
                        <button
                          className={`mode-toggle-button ${
                            previewMode === "animation" ? "is-active" : ""
                          }`}
                          onClick={() => setPreviewMode("animation")}
                          type="button"
                        >
                          Animation
                        </button>
                      </div>
                    )}

                    {previewIsSpriteSheet && (
                      <label className="field field-inline preview-delay-field">
                        <span>Frame delay (ms)</span>
                        <input
                          className="number-input"
                          inputMode="numeric"
                          onBlur={() => {
                            const parsed = Number.parseInt(frameDelayInput, 10);
                            setFrameDelayInput(
                              String(
                                Number.isNaN(parsed)
                                  ? 120
                                  : Math.max(16, parsed),
                              ),
                            );
                          }}
                          onChange={(event) => {
                            const next = event.target.value;
                            if (/^\d*$/.test(next)) {
                              setFrameDelayInput(next);
                            }
                          }}
                          pattern="[0-9]*"
                          type="text"
                          value={frameDelayInput}
                        />
                      </label>
                    )}
                  </div>
                </div>

                <section className="preview-card preview-stage">
                  {previewImageSrc ? (
                    previewIsSpriteSheet && previewMode === "animation" ? (
                      <SpriteSheetPlayer
                        alt={`${previewChild.name} animation`}
                        cols={previewCols}
                        frameDelayMs={frameDelayMs}
                        rows={previewRows}
                        src={previewImageSrc}
                      />
                    ) : (
                      <img
                        alt={previewChild.name}
                        className="preview-large-image"
                        src={previewImageSrc}
                      />
                    )
                  ) : (
                    <div className="placeholder">
                      No output image saved for this child.
                    </div>
                  )}
                </section>

                {previewIsSpriteSheet && (
                  <p className="muted preview-meta">
                    {previewRows} rows × {previewCols} cols ·{" "}
                    {previewRows * previewCols} frames
                  </p>
                )}
              </>
            )}
          </section>
        )}

        {activeTab === "export" && (
          <section className="panel-content preview-panel">
            {!previewChild && (
              <p className="muted">Select a project child to export output.</p>
            )}

            {previewChild && (
              <>
                <div className="preview-toolbar">
                  <p className="muted">
                    {previewChild.name} ({previewChild.type})
                  </p>
                </div>

                <section className="preview-card preview-stage">
                  {previewImageSrc ? (
                    <img
                      alt={`${previewChild.name} export preview`}
                      className="preview-large-image"
                      src={previewImageSrc}
                    />
                  ) : (
                    <div className="placeholder">
                      No output image saved for this child.
                    </div>
                  )}
                </section>

                <div className="action-row">
                  <button
                    className="primary-button"
                    disabled={!previewImagePath || isExporting}
                    onClick={handleSaveAsExport}
                    type="button"
                  >
                    {isExporting ? "Saving..." : "Save As"}
                  </button>
                </div>

                {exportError && <p className="error-banner">{exportError}</p>}
                {exportResult && <p className="muted">{exportResult}</p>}
              </>
            )}
          </section>
        )}
      </main>
    </div>
  );
}

export default App;
