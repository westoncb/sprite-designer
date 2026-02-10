import React from "react";

interface SpriteSheetPlayerProps {
  src: string;
  rows: number;
  cols: number;
  frameDelayMs: number;
  alt?: string;
}

function toPositiveInt(value: number, fallback: number): number {
  if (!Number.isFinite(value)) {
    return fallback;
  }

  return Math.max(1, Math.floor(value));
}

export function SpriteSheetPlayer({
  src,
  rows,
  cols,
  frameDelayMs,
  alt = "Sprite animation preview"
}: SpriteSheetPlayerProps) {
  const canvasRef = React.useRef<HTMLCanvasElement | null>(null);
  const [image, setImage] = React.useState<HTMLImageElement | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [frameIndex, setFrameIndex] = React.useState(0);

  const safeRows = toPositiveInt(rows, 1);
  const safeCols = toPositiveInt(cols, 1);
  const totalFrames = safeRows * safeCols;
  const effectiveDelay = Math.max(16, Math.floor(frameDelayMs || 120));

  React.useEffect(() => {
    setFrameIndex(0);
  }, [src, safeRows, safeCols]);

  React.useEffect(() => {
    let disposed = false;
    setLoadError(null);
    setImage(null);

    const nextImage = new Image();
    nextImage.decoding = "async";
    nextImage.onload = () => {
      if (disposed) {
        return;
      }

      setImage(nextImage);
    };
    nextImage.onerror = () => {
      if (disposed) {
        return;
      }

      setLoadError("Failed to load sprite sheet image.");
    };
    nextImage.src = src;

    return () => {
      disposed = true;
    };
  }, [src]);

  React.useEffect(() => {
    if (!image || totalFrames <= 1) {
      return;
    }

    const intervalId = window.setInterval(() => {
      setFrameIndex((previous) => (previous + 1) % totalFrames);
    }, effectiveDelay);

    return () => window.clearInterval(intervalId);
  }, [image, effectiveDelay, totalFrames]);

  React.useEffect(() => {
    if (!image || !canvasRef.current) {
      return;
    }

    const frameWidth = Math.max(1, Math.floor(image.naturalWidth / safeCols));
    const frameHeight = Math.max(1, Math.floor(image.naturalHeight / safeRows));
    const canvas = canvasRef.current;
    const context = canvas.getContext("2d");

    if (!context) {
      return;
    }

    const frame = frameIndex % totalFrames;
    const frameX = (frame % safeCols) * frameWidth;
    const frameY = Math.floor(frame / safeCols) * frameHeight;

    canvas.width = frameWidth;
    canvas.height = frameHeight;
    context.imageSmoothingEnabled = false;
    context.clearRect(0, 0, frameWidth, frameHeight);
    context.drawImage(
      image,
      frameX,
      frameY,
      frameWidth,
      frameHeight,
      0,
      0,
      frameWidth,
      frameHeight
    );
  }, [image, frameIndex, safeCols, safeRows, totalFrames]);

  if (loadError) {
    return <div className="placeholder">{loadError}</div>;
  }

  if (!image) {
    return <div className="placeholder">Loading animation preview...</div>;
  }

  return (
    <figure className="sprite-sheet-player">
      <canvas aria-label={alt} className="sprite-sheet-canvas" ref={canvasRef} role="img" />
      <figcaption className="sprite-sheet-meta">
        Frame {Math.min(frameIndex + 1, totalFrames)} / {totalFrames}
      </figcaption>
    </figure>
  );
}
