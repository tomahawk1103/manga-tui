const EOCD_SIGNATURE = 0x06054b50;
const CENTRAL_DIR_SIGNATURE = 0x02014b50;
const LOCAL_HEADER_SIGNATURE = 0x04034b50;

const IMAGE_MIME_BY_EXT: Record<string, string> = {
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  webp: "image/webp",
  avif: "image/avif",
  gif: "image/gif",
};

interface ZipEntryMeta {
  name: string;
  compressionMethod: number;
  compressedSize: number;
  uncompressedSize: number;
  localHeaderOffset: number;
  mime: string;
}

function getImageMimeForFilename(name: string): string | null {
  const lower = name.toLowerCase();
  if (
    lower.endsWith("/") ||
    lower.startsWith("__macosx/") ||
    lower.includes("/__macosx/")
  ) {
    return null;
  }
  const dot = lower.lastIndexOf(".");
  if (dot < 0) return null;
  const ext = lower.slice(dot + 1);
  return IMAGE_MIME_BY_EXT[ext] ?? null;
}

function findEocdOffset(view: DataView, byteLength: number): number {
  const minOffset = Math.max(0, byteLength - 65557);
  for (let offset = byteLength - 22; offset >= minOffset; offset -= 1) {
    if (view.getUint32(offset, true) === EOCD_SIGNATURE) {
      const commentLen = view.getUint16(offset + 20, true);
      if (offset + 22 + commentLen <= byteLength) {
        return offset;
      }
    }
  }
  return -1;
}

async function inflateRawBytes(compressed: Uint8Array): Promise<Uint8Array> {
  if (typeof DecompressionStream === "undefined") {
    throw new Error(
      "Tu navegador no soporta DecompressionStream ('deflate-raw') para leer archivos comprimidos.",
    );
  }
  const stream = new Blob([compressed as unknown as BlobPart])
    .stream()
    .pipeThrough(new DecompressionStream("deflate-raw"));
  const outBuffer = await new Response(stream).arrayBuffer();
  return new Uint8Array(outBuffer);
}

/**
 * Extracts and naturally sorts all image pages inside a ZIP-based archive (`.cbz`, `.zip`, `.epub`).
 */
export async function extractImagesFromZipArchive(buffer: ArrayBuffer): Promise<Blob[]> {
  const bytes = new Uint8Array(buffer);
  const view = new DataView(buffer);
  if (bytes.byteLength < 22) {
    throw new Error("El archivo está vacío o no es un CBZ/EPUB válido.");
  }

  const eocdOffset = findEocdOffset(view, bytes.byteLength);
  if (eocdOffset < 0) {
    throw new Error("No se encontró el índice ZIP del capítulo (.cbz/.epub).");
  }

  const totalEntries = view.getUint16(eocdOffset + 10, true);
  const centralDirOffset = view.getUint32(eocdOffset + 16, true);
  const centralDirSize = view.getUint32(eocdOffset + 12, true);
  const utf8Decoder = new TextDecoder("utf-8", { fatal: false });

  const entries: ZipEntryMeta[] = [];
  let cursor = centralDirOffset;
  const centralDirLimit = Math.min(bytes.byteLength, centralDirOffset + centralDirSize);

  for (let i = 0; i < totalEntries && cursor + 46 <= centralDirLimit; i += 1) {
    if (view.getUint32(cursor, true) !== CENTRAL_DIR_SIGNATURE) {
      break;
    }
    const compressionMethod = view.getUint16(cursor + 10, true);
    const compressedSize = view.getUint32(cursor + 20, true);
    const uncompressedSize = view.getUint32(cursor + 24, true);
    const nameLen = view.getUint16(cursor + 28, true);
    const extraLen = view.getUint16(cursor + 30, true);
    const commentLen = view.getUint16(cursor + 32, true);
    const localHeaderOffset = view.getUint32(cursor + 42, true);

    const nameStart = cursor + 46;
    const nameEnd = nameStart + nameLen;
    if (nameEnd > bytes.byteLength) break;

    const name = utf8Decoder.decode(bytes.subarray(nameStart, nameEnd));
    const mime = getImageMimeForFilename(name);
    if (mime && (compressionMethod === 0 || compressionMethod === 8)) {
      entries.push({
        name,
        compressionMethod,
        compressedSize,
        uncompressedSize,
        localHeaderOffset,
        mime,
      });
    }

    cursor = nameEnd + extraLen + commentLen;
  }

  entries.sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }),
  );

  const pages: Blob[] = [];
  for (const entry of entries) {
    const lh = entry.localHeaderOffset;
    if (lh + 30 > bytes.byteLength || view.getUint32(lh, true) !== LOCAL_HEADER_SIGNATURE) {
      continue;
    }
    const localNameLen = view.getUint16(lh + 26, true);
    const localExtraLen = view.getUint16(lh + 28, true);
    const dataStart = lh + 30 + localNameLen + localExtraLen;
    const dataEnd = dataStart + entry.compressedSize;
    if (dataStart < 0 || dataEnd > bytes.byteLength) {
      continue;
    }

    const slice = bytes.subarray(dataStart, dataEnd);
    const pageBytes =
      entry.compressionMethod === 0 ? slice : await inflateRawBytes(slice);
    if (pageBytes.byteLength > 0) {
      pages.push(new Blob([pageBytes as unknown as BlobPart], { type: entry.mime }));
    }
  }

  if (pages.length === 0) {
    throw new Error("El archivo del capítulo no contiene páginas de imagen legibles.");
  }
  return pages;
}

export function parseCloudChapterFilename(fileName: string): {
  chapterNumber: string;
  title: string;
  format: "CBZ" | "EPUB" | "PDF" | "ARCHIVE";
} {
  const trimmed = fileName.trim();
  const dot = trimmed.lastIndexOf(".");
  const ext = dot >= 0 ? trimmed.slice(dot + 1).toLowerCase() : "";
  const stem = (dot >= 0 ? trimmed.slice(0, dot) : trimmed).trim();

  const format: "CBZ" | "EPUB" | "PDF" | "ARCHIVE" =
    ext === "cbz" || ext === "zip"
      ? "CBZ"
      : ext === "epub"
        ? "EPUB"
        : ext === "pdf"
          ? "PDF"
          : "ARCHIVE";

  const chMatch =
    stem.match(/(?:^|\b)(?:ch|cap|chapter|cap[ií]tulo)\.?\s*(\d+(?:\.\d+)?)/i) ??
    stem.match(/\b(\d+(?:\.\d+)?)\b/);
  const chapterNumber = chMatch?.[1] ?? "?";

  const cleanedTitle = stem
    .replace(/\s+(?:Scan|InManga|MangaDex|MangaOni|LeerCapitulo|WeebCentral|MangaPill|Comizy)\b.*$/i, "")
    .replace(/\s+(?=[a-z0-9-]*\d)[a-z0-9-]{6,}$/i, "")
    .trim();

  return {
    chapterNumber,
    title: cleanedTitle || stem || trimmed,
    format,
  };
}

export function formatByteSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 KB";
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
