import { invoke } from '@tauri-apps/api/core';

export type WindowLayout = 'adaptive';

export interface WindowBounds {
  x: number | null;
  y: number | null;
  width: number;
  height: number;
}

export interface WindowPreferences {
  layout: WindowLayout;
  maximized: boolean;
  normalBounds: WindowBounds;
  alwaysOnTop: boolean;
  lastImmersive: boolean;
}

export interface WindowState {
  maximized: boolean;
  fullscreen: boolean;
  normalBounds: WindowBounds;
}

export interface FloatingWindowPreferences {
  visible: boolean;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  alwaysOnTop: boolean;
}

export const getWindowPreferences = (): Promise<WindowPreferences> => invoke('get_window_preferences');
export const getWindowState = (): Promise<WindowState> => invoke('get_window_state');
export const enterImmersiveMode = (): Promise<WindowState> => invoke('enter_immersive_mode');
export const exitImmersiveMode = (): Promise<WindowState> => invoke('exit_immersive_mode');
export const setMainWindowMaximized = (maximized: boolean): Promise<WindowState> =>
  invoke('set_main_window_maximized', { maximized });
export const setAlwaysOnTop = (alwaysOnTop: boolean): Promise<void> =>
  invoke('set_always_on_top', { alwaysOnTop });
export const getFloatingWindowPreferences = (): Promise<FloatingWindowPreferences> =>
  invoke('get_floating_window_preferences');
export const showFloatingWindow = (): Promise<void> => invoke('show_floating_window');
export const hideFloatingWindow = (): Promise<void> => invoke('hide_floating_window');
export const toggleFloatingWindow = (): Promise<void> => invoke('toggle_floating_window');
