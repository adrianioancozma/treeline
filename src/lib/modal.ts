// Keyboard rules shared by every dialog, kept free of React and the DOM so they can be unit tested.

const stack: string[] = [];

/** Registers an open dialog; the most recently opened one receives Escape and Tab. */
export function openModal(id: string): void {
  stack.push(id);
}

/** Removes a dialog wherever it is in the stack (dialogs can close out of order). */
export function closeModal(id: string): void {
  const i = stack.lastIndexOf(id);
  if (i >= 0) stack.splice(i, 1);
}

export function isTopModal(id: string): boolean {
  return stack.length > 0 && stack[stack.length - 1] === id;
}

/** Test helper. */
export function resetModals(): void {
  stack.length = 0;
}

/**
 * What a key press means for a dialog: Escape closes the top dialog unless an operation is running
 * (closing then could leave git half-way); Tab is kept inside the top dialog.
 */
export function modalKeyAction(key: string, s: { top: boolean; busy: boolean }): "close" | "trap" | null {
  if (!s.top) return null;
  if (key === "Escape") return s.busy ? null : "close";
  if (key === "Tab") return "trap";
  return null;
}

/** Index of the element Tab (or Shift+Tab) should focus next among `count` focusable elements; -1 if none. */
export function nextFocusIndex(count: number, current: number, backwards: boolean): number {
  if (count <= 0) return -1;
  if (current < 0 || current >= count) return backwards ? count - 1 : 0;
  return backwards ? (current - 1 + count) % count : (current + 1) % count;
}
