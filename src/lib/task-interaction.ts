export function shouldToggleTaskFromKeyboard(
  key: string,
  ctrlKey: boolean,
  metaKey: boolean,
  altKey: boolean,
  targetIsRow: boolean
): boolean {
  return (
    key === ' ' &&
    !ctrlKey &&
    !metaKey &&
    !altKey &&
    targetIsRow
  );
}
