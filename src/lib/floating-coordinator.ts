export interface PomodoroCoordinationState<T> {
  epoch: number;
  snapshot: T | null;
  readWarning: string | null;
  notificationWarning: string | null;
}

export interface PomodoroOperation<T> {
  state: PomodoroCoordinationState<T>;
  token: number;
}

export function createPomodoroCoordinationState<T>(): PomodoroCoordinationState<T> {
  return {
    epoch: 0,
    snapshot: null,
    readWarning: null,
    notificationWarning: null
  };
}

export function beginPomodoroOperation<T>(
  state: PomodoroCoordinationState<T>
): PomodoroOperation<T> {
  const token = state.epoch + 1;
  return { state: { ...state, epoch: token }, token };
}

export function pomodoroSnapshotAccepted<T>(
  state: PomodoroCoordinationState<T>,
  token: number
): boolean {
  return token === state.epoch;
}

export function acceptPomodoroRead<T>(
  state: PomodoroCoordinationState<T>,
  token: number,
  snapshot: T | null,
  warning: string | null
): PomodoroCoordinationState<T> {
  if (token !== state.epoch) return state;
  return {
    ...state,
    snapshot: snapshot ?? state.snapshot,
    readWarning: warning
  };
}

export function acceptPomodoroCommand<T>(
  state: PomodoroCoordinationState<T>,
  token: number,
  snapshot: T,
  notificationWarning: string | null
): PomodoroCoordinationState<T> {
  if (token !== state.epoch) return { ...state, notificationWarning };
  return {
    ...state,
    snapshot,
    notificationWarning
  };
}
