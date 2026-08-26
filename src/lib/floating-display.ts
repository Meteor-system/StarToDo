export type FloatingDisplayMode =
  | 'expanded'
  | 'capsule'
  | 'interaction-expanded';

export type FloatingSizeMode = 'expanded' | 'capsule';

export interface FloatingDisplayState {
  mode: FloatingDisplayMode;
  focusActive: boolean;
  alwaysExpanded: boolean;
  pointerInside: boolean;
  focusInside: boolean;
  lastInteractionAt: number | null;
  collapseAt: number | null;
}

export type FloatingDisplayEvent =
  | { type: 'snapshot'; focusActive: boolean; at: number }
  | { type: 'always-expanded'; value: boolean; at: number }
  | {
      type: 'pointer-enter' | 'pointer-leave' | 'focus-in' | 'focus-out' | 'timeout';
      at: number;
    };

export const FLOATING_INTERACTION_MS = 5_000;
export const FLOATING_LEAVE_BUFFER_MS = 700;

export function createFloatingDisplayState(
  focusActive: boolean,
  alwaysExpanded: boolean
): FloatingDisplayState {
  return {
    mode: alwaysExpanded || !focusActive ? 'expanded' : 'capsule',
    focusActive,
    alwaysExpanded,
    pointerInside: false,
    focusInside: false,
    lastInteractionAt: null,
    collapseAt: null
  };
}

function armCollapse(state: FloatingDisplayState, at: number): FloatingDisplayState {
  if (state.alwaysExpanded || !state.focusActive) {
    return { ...state, mode: 'expanded', collapseAt: null };
  }

  if (state.pointerInside || state.focusInside) {
    return { ...state, mode: 'interaction-expanded', collapseAt: null };
  }

  if (state.mode !== 'interaction-expanded') {
    return { ...state, mode: 'capsule', collapseAt: null };
  }

  const interactionDeadline =
    (state.lastInteractionAt ?? at) + FLOATING_INTERACTION_MS;

  return {
    ...state,
    collapseAt: Math.max(interactionDeadline, at + FLOATING_LEAVE_BUFFER_MS)
  };
}

export function reduceFloatingDisplay(
  state: FloatingDisplayState,
  event: FloatingDisplayEvent
): FloatingDisplayState {
  if (event.type === 'snapshot') {
    const next = { ...state, focusActive: event.focusActive, collapseAt: null };
    if (next.alwaysExpanded || !next.focusActive) return { ...next, mode: 'expanded' };
    return {
      ...next,
      mode: next.pointerInside || next.focusInside ? 'interaction-expanded' : 'capsule'
    };
  }

  if (event.type === 'always-expanded') {
    const next = { ...state, alwaysExpanded: event.value, collapseAt: null };
    if (event.value || !next.focusActive) return { ...next, mode: 'expanded' };
    return {
      ...next,
      mode: next.pointerInside || next.focusInside ? 'interaction-expanded' : 'capsule'
    };
  }

  if (event.type === 'pointer-enter' || event.type === 'focus-in') {
    const next = {
      ...state,
      pointerInside: event.type === 'pointer-enter' ? true : state.pointerInside,
      focusInside: event.type === 'focus-in' ? true : state.focusInside,
      lastInteractionAt: event.at,
      collapseAt: null
    };
    return {
      ...next,
      mode: next.alwaysExpanded || !next.focusActive ? 'expanded' : 'interaction-expanded'
    };
  }

  if (event.type === 'pointer-leave' || event.type === 'focus-out') {
    return armCollapse(
      {
        ...state,
        pointerInside: event.type === 'pointer-leave' ? false : state.pointerInside,
        focusInside: event.type === 'focus-out' ? false : state.focusInside
      },
      event.at
    );
  }

  if (
    state.collapseAt === null ||
    event.at < state.collapseAt ||
    state.pointerInside ||
    state.focusInside
  ) {
    return state;
  }

  return {
    ...state,
    mode: state.alwaysExpanded || !state.focusActive ? 'expanded' : 'capsule',
    collapseAt: null
  };
}

export function floatingSizeMode(mode: FloatingDisplayMode): FloatingSizeMode {
  return mode === 'capsule' ? 'capsule' : 'expanded';
}
