import type { FloatingSizeMode } from './floating-display';

export interface FloatingSizeRequest {
  mode: FloatingSizeMode;
  generation: number;
}

export interface FloatingSizeSyncState<T> {
  ready: boolean;
  generation: number;
  desired: FloatingSizeRequest | null;
  inFlight: FloatingSizeRequest | null;
  failed: FloatingSizeRequest | null;
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
    failed: null
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

export function failFloatingPreferencesRead<T>(
  state: FloatingSizeSyncState<T>
): FloatingSizeSyncState<T> {
  return { ...state, ready: true };
}

export function nextFloatingSizeRequest<T>(
  state: FloatingSizeSyncState<T>
): FloatingSizeRequest | null {
  if (!state.ready || state.inFlight !== null || state.desired === null) return null;
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
  const current = state.desired?.generation === request.generation;
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
  const current = state.desired?.generation === request.generation;
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
  return { ...state, failed: null };
}
