export const MIN_ALLOWED_FONT_SIZE: 8;
export const MAX_ALLOWED_FONT_SIZE: 72;
export const DEFAULT_FONT_SIZE: 15;

export const UpdateFontSizeType: {
  readonly increment: 1;
  readonly decrement: 2;
};

export type UpdateFontSizeType = typeof UpdateFontSizeType[keyof typeof UpdateFontSizeType];

export function calculateNextFontSize(currentFontSize: number, updateType: UpdateFontSizeType | null): number;
export function updateFontSizeInSelection(editor: unknown, newFontSize: string | null, updateType: UpdateFontSizeType | null, skipRefocus?: boolean): void;
export function formatParagraph(editor: unknown): void;
export function formatHeading(editor: unknown, headingSize: "h1" | "h2" | "h3" | "h4" | "h5" | "h6"): void;
export function formatBulletList(editor: unknown): void;
export function formatNumberedList(editor: unknown): void;
export function formatQuote(editor: unknown): void;
export function formatCode(editor: unknown): void;
