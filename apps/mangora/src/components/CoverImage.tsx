import { useEffect, useState } from "react";
import { resolveCoverImageUrl } from "../lib/tauri";

interface CoverImageProps {
  src?: string | null;
  provider?: string;
  title: string;
  alt?: string;
  className?: string;
  imgClassName?: string;
  fallbackTextClassName?: string;
}

export function CoverImage({
  src,
  provider,
  title,
  alt = "",
  className = "aspect-[2/3] w-full",
  imgClassName = "h-full w-full object-cover",
  fallbackTextClassName = "px-2 py-1 font-display text-lg",
}: CoverImageProps) {
  const resolvedSrc = resolveCoverImageUrl(src, provider);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setFailed(false);
  }, [resolvedSrc]);

  const monogram = title.trim().slice(0, 2).toUpperCase() || "M!";

  return (
    <div className={`relative overflow-hidden bg-[var(--surface)] ${className}`}>
      {resolvedSrc && !failed ? (
        <img
          src={resolvedSrc}
          alt={alt}
          loading="lazy"
          referrerPolicy="no-referrer"
          className={imgClassName}
          onError={() => setFailed(true)}
        />
      ) : (
        <div className="screentone-bg flex h-full w-full items-center justify-center p-2 text-center">
          <span
            className={`border-2 border-[var(--ink)] bg-[var(--paper)] text-[var(--ink)] ${fallbackTextClassName}`}
          >
            {monogram}
          </span>
        </div>
      )}
    </div>
  );
}
