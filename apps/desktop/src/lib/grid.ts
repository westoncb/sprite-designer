import type { Resolution } from "@sprite-designer/shared/types";

const SIZE_BY_RESOLUTION: Record<Resolution, number> = {
  "1K": 1024,
  "2K": 2048,
  "4K": 4096
};

export function createSpriteGridDataUrl(rows: number, cols: number, resolution: Resolution): string {
  const safeRows = Math.max(1, Math.floor(rows || 1));
  const safeCols = Math.max(1, Math.floor(cols || 1));

  const ratio = safeCols / safeRows;
  const longEdge = SIZE_BY_RESOLUTION[resolution];
  const width = ratio >= 1 ? longEdge : Math.max(256, Math.round(longEdge * ratio));
  const height = ratio >= 1 ? Math.max(256, Math.round(longEdge / ratio)) : longEdge;

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;

  const context = canvas.getContext("2d");
  if (!context) {
    return "";
  }

  context.clearRect(0, 0, width, height);
  context.strokeStyle = "rgba(0,0,0,0.9)";
  context.lineWidth = Math.max(1, Math.round(Math.min(width, height) / 512));

  context.beginPath();
  context.rect(0.5, 0.5, width - 1, height - 1);

  for (let col = 1; col < safeCols; col += 1) {
    const x = Math.round((col * width) / safeCols) + 0.5;
    context.moveTo(x, 0);
    context.lineTo(x, height);
  }

  for (let row = 1; row < safeRows; row += 1) {
    const y = Math.round((row * height) / safeRows) + 0.5;
    context.moveTo(0, y);
    context.lineTo(width, y);
  }

  context.stroke();

  return canvas.toDataURL("image/png");
}

export function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string") {
        reject(new Error("Failed to read file"));
        return;
      }
      resolve(result);
    };

    reader.onerror = () => reject(reader.error ?? new Error("Failed to read file"));
    reader.readAsDataURL(file);
  });
}
