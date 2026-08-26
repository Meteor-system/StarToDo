import type { FloatingSizeMode } from './floating-display';

export interface FloatingSizeRequest {
  mode: FloatingSizeMode;
  generation: number;
}

export interface FloatingResetRequest {
  generation: number;
}

export interface FloatingSizeSyncState<T> {
  ready: boolean;
  generation: number;
  desired: FloatingSizeRequest | null;
  inFlight: FloatingSizeRequest | null;
  failed: FloatingSizeRequest | null;
  resetDesired: FloatingResetRequest | null;
  resetInFlight: FloatingResetRequest | null;
  appliedGeneration: number;
  preferences: T | null;
}

export function createFloatingSizeSyncState<T>(): FloatingSizeSyncState<T> {
  return {
    ready: false,
    generation: 0,
    desired: null,
    inFlight: null,
    failed: null,
    resetDesired: null,
    resetInFlight: null,
    appliedGeneration: 0,
    preferences: null
  };
}

export function requestFloatingSize<T>(
  state: FloatingSizeSyncState<T>,
  mode: FloatingSizeMode
): FloatingSizeSyncState<T> {
  const generation = state.generation + 1;
  return {
    ...state,
    generation,
    desired: { mode, generation },
    failed: null,
    resetDesired: null
  };
}

export function requestFloatingReset<T>(
  state: FloatingSizeSyncState<T>
): FloatingSizeSyncState<T> {
  const generation = state.generation + 1;
  return {
    ...state,
    generation,
    resetDesired: { generation },
    failed: null
  };
}

export function nextFloatingResetRequest<T>(
  state: FloatingSizeSyncState<T>
): FloatingResetRequest | null {
  if (!state.ready || state.inFlight !== null || state.resetInFlight !== null) return null;
  return state.resetDesired;
}

export function beginFloatingResetRequest<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingResetRequest
): FloatingSizeSyncState<T> {
  return { ...state, resetInFlight: request };
}

export function acceptFloatingResetSuccess<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingResetRequest,
  preferences: T
): FloatingSizeSyncState<T> {
  const current = state.generation === request.generation && state.resetDesired?.generation === request.generation;
  return {
    ...state,
    resetInFlight: state.resetInFlight?.generation === request.generation ? null : state.resetInFlight,
    resetDesired: current ? null : state.resetDesired,
    preferences: current ? preferences : state.preferences,
    appliedGeneration: current ? request.generation : state.appliedGeneration
  };
}

export function acceptFloatingResetFailure<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingResetRequest
): FloatingSizeSyncState<T> {
  return {
    ...state,
    resetInFlight: state.resetInFlight?.generation === request.generation ? null : state.resetInFlight,
    resetDesired: state.resetDesired?.generation === request.generation ? null : state.resetDesired
  };
}

export function acceptFloatingPreferencesRead<T extends { displayMode: FloatingSizeMode }>(
  state: FloatingSizeSyncState<T>,
  preferences: T
): FloatingSizeSyncState<T> {
  const initialRead = !state.ready;
  const matchesDesired = state.desired?.mode === preferences.displayMode;
  return {
    ...state,
    ready: true,
    preferences,
    appliedGeneration:
      initialRead && matchesDesired ? state.desired!.generation : state.appliedGeneration
  };
}

export function acceptFloatingReconciliation<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingSizeRequest,
  preferences: T
): FloatingSizeSyncState<T> {
  if (state.desired?.generation !== request.generation) return state;
  return { ...state, ready: true, preferences };
}

export function failFloatingPreferencesRead<T>(
  state: FloatingSizeSyncState<T>
): FloatingSizeSyncState<T> {
  return { ...state, ready: true };
}

export function nextFloatingSizeRequest<T>(
  state: FloatingSizeSyncState<T>
): FloatingSizeRequest | null {
  if (
    !state.ready ||
    state.inFlight !== null ||
    state.resetDesired !== null ||
    state.resetInFlight !== null ||
    state.desired === null
  ) return null;
  if (state.failed?.generation === state.desired.generation) return null;
  if (state.appliedGeneration >= state.desired.generation) return null;
  return state.desired;
}

export function beginFloatingSizeRequest<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingSizeRequest
): FloatingSizeSyncState<T> {
  return { ...state, inFlight: request };
}

export function acceptFloatingSizeSuccess<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingSizeRequest,
  preferences: T
): FloatingSizeSyncState<T> {
  const current = state.generation === request.generation && state.desired?.generation === request.generation;
  return {
    ...state,
    inFlight: state.inFlight?.generation === request.generation ? null : state.inFlight,
    preferences: current ? preferences : state.preferences,
    appliedGeneration: current ? request.generation : state.appliedGeneration,
    failed: current ? null : state.failed
  };
}

export function acceptFloatingSizeFailure<T>(
  state: FloatingSizeSyncState<T>,
  request: FloatingSizeRequest
): FloatingSizeSyncState<T> {
  const current = state.generation === request.generation && state.desired?.generation === request.generation;
  return {
    ...state,
    inFlight: state.inFlight?.generation === request.generation ? null : state.inFlight,
    failed: current ? request : state.failed
  };
}

export function retryFloatingSize<T>(
  state: FloatingSizeSyncState<T>
): FloatingSizeSyncState<T> {
  if (state.desired === null || state.failed?.generation !== state.desired.generation) return state;
  const generation = state.generation + 1;
  return {
    ...state,
    generation,
    desired: { mode: state.desired.mode, generation },
    failed: null
  };
}
