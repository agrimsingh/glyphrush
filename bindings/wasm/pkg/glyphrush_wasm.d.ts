/* tslint:disable */
/* eslint-disable */

/**
 * Parse PDF bytes through the shared lopdf extraction path and core artifact model.
 *
 * OCR adapters (sidecar, command, HTTP) are process and network seams that do not
 * apply to the wasm surface. Pages that need OCR keep their `requires_ocr` flags
 * and warnings exactly like a no-OCR CLI run.
 */
export function parse_pdf_bytes(bytes: Uint8Array, span_geometry: boolean): string;
