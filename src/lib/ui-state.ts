export type AppScene = 'tasks' | 'focus';
export type ShellDrawer = 'diagnostics' | null;
export type DrawerSize = 'normal' | 'wide';

export type ImmersiveDisplayState = 'off' | 'system' | 'visual-fallback';

export type ToastTone = 'info' | 'success' | 'warning' | 'danger';

export interface ToastMessage {
  id: string;
  tone: ToastTone;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
  onDismiss?: () => void;
}
