# Word Media Clean-Room Port Notes

Subagent I scope: `apps/web/lib/word/editor/extensions/media/**`, this note, and isolated image layout helpers only. This note records behavior observed from the SuperDoc reference tree without copying source code into CTOX.

## SuperDoc Image / Drawing / Shape Inventory

- Image is an inline ProseMirror-style atom named `image`. It is selectable, draggable, and rendered as an `img` element with an internal media store mapping logical source keys to browser-safe display URLs.
- Image attributes cover user-facing metadata (`src`, `alt`, `title`, size) plus OOXML preservation state (`rId`, drawing child order, original attributes, passthrough siblings, VML watermark data, crop/clip metadata, transform data, anchor/wrap data, z-order, decorative flag, hyperlink, lock-aspect-ratio).
- Base64 images are accepted when enabled. Upload flow validates file size, constrains dimensions to the page content area, produces display data, creates or reuses a DOCX relationship, assigns a unique `docPr` id, and replaces an async placeholder once the upload finishes.
- URL import is deliberately defensive: remote images may fail because of CORS and need a controlled conversion/fetch path rather than direct trust of arbitrary URLs.
- Resize behavior is not embedded in the image node. A separate node-resizer extension watches node selection, decorates the selected image wrapper, renders four corner handles in an overlay, preserves aspect ratio while dragging, enforces a minimum width, and writes final dimensions back to the node.
- Floating images rely on wrap and anchor metadata. SuperDoc approximates Word wrapping in browser CSS with floats, absolute positioning, margins, `shape-outside`, top/bottom clearing, and z-index derived from OOXML relative height.
- Vector shapes are separate inline atom nodes with geometry, size, fill/stroke, rotation/flip, wrap/anchor, hidden/effect, and optional text metadata. They render through a NodeView rather than a plain DOM serializer.
- Shape groups are inline atom nodes that carry group transform, child shapes, size, padding, margin offset, wrap/anchor, and original drawing content. Rendering is NodeView-driven.
- Shape containers and textboxes are block isolating nodes with nested block content. They preserve raw/import attributes and expose only enough DOM data for editor interaction.
- Watermark/pict cases are special: image and shape import can preserve VML data, and resize interaction is suppressed for watermark images.

## CTOX Scaffold Added

- `apps/web/lib/word/editor/extensions/media/types.ts` defines the clean-room contract for inline image attrs, resize metadata, insert input, and safe source resolution.
- `safe-src.ts` provides a conservative `WordMediaSafeSrcAdapter`. It allows raster `data:` URLs from known content types, optional `blob:` URLs, same-origin HTTP(S), and explicitly trusted remote origins. Unknown schemes and SVG-by-default are rejected.
- `dimensions.ts` centralizes 96-DPI EMU/PX conversion, minimum resize sizing, and metadata updates for future overlays.
- `image-node.ts` defines a runtime `image` node scaffold with inline atom behavior, `mediaId`, alt text, title, decorative state, EMU/PX dimensions, lock-aspect-ratio, and resize metadata.
- `commands.ts` defines runtime commands for `insertInlineImage`, selected-image attr updates, alt text updates, and dimension updates.

The scaffold intentionally does not alter the existing shared schema, document converters, command surface, or OOXML writer. Integration can wire these extensions once the owning agents agree on the runtime extension composition point.

## Handoff Requests

- OOXML media relationships: writer/import owner should map image `mediaId` and future upload records to `/word/media/*`, `[Content_Types].xml` overrides/defaults, and document/header/footer relationship ids. The media extension expects to receive a resolved `mediaId`, optional safe `src`, content type, and dimensions; it does not create package relationships itself.
- OOXML drawing metadata: importer/exporter should preserve unsupported floating/wrap/anchor/crop/transform details either in model media metadata or a dedicated drawing metadata field before UI work starts. The current scaffold only models inline dimensions and accessibility metadata.
- UI overlay: component owner should build the resize overlay against the `setSelectedImageDimensions(widthPx, heightPx?)` command and keep overlay DOM outside the document flow. It should skip decorative/watermark-like locked images once that metadata is available.
- Safe source adapter integration: editor/viewer owner should resolve `WordMediaItem` bytes or stored object URLs through `WordMediaSafeSrcAdapter` before placing `src` on image attrs. Direct arbitrary remote `src` assignment should remain opt-in with trusted origins.
- Shape support: vector shapes, shape groups, shape containers, and shape textboxes should remain unsupported/locked or NodeView-only until a separate owner defines the model contract and OOXML roundtrip guarantees.
