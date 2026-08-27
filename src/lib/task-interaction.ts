export function ownsTaskDetailsOperation(
  currentTaskId: number | null,
  currentDraftTaskId: number | null,
  currentDraftGeneration: number,
  currentOperationId: number,
  operationTaskId: number,
  operationDraftGeneration: number,
  operationId: number
): boolean {
  return (
    currentTaskId === operationTaskId &&
    currentDraftTaskId === operationTaskId &&
    currentDraftGeneration === operationDraftGeneration &&
    currentOperationId === operationId
  );
}

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
