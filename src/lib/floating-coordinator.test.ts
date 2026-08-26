import { describe, expect, it } from 'vitest';

import {
  acceptPomodoroCommand,
  acceptPomodoroRead,
  beginPomodoroOperation,
  createPomodoroCoordinationState,
  pomodoroSnapshotAccepted,
  type PomodoroCoordinationState
} from './floating-coordinator';

function state(): PomodoroCoordinationState<string> {
  return createPomodoroCoordinationState<string>();
}

describe('floating Pomodoro coordination', () => {
  it('accepts only the newest started read or command', () => {
    let coordination = state();
    const command = beginPomodoroOperation(coordination);
    coordination = command.state;
    const read = beginPomodoroOperation(coordination);
    coordination = read.state;

    coordination = acceptPomodoroRead(coordination, read.token, 'read-newer', null);
    coordination = acceptPomodoroCommand(coordination, command.token, 'command-older', 'notify');

    expect(coordination.snapshot).toBe('read-newer');
    expect(coordination.notificationWarning).toBe('notify');
    expect(pomodoroSnapshotAccepted(coordination, command.token)).toBe(false);
    expect(pomodoroSnapshotAccepted(coordination, read.token)).toBe(true);
  });

  it('keeps notification warnings separate from successful read warnings', () => {
    let coordination = state();
    const command = beginPomodoroOperation(coordination);
    coordination = acceptPomodoroCommand(command.state, command.token, 'command', 'notification failed');
    const read = beginPomodoroOperation(coordination);
    coordination = acceptPomodoroRead(read.state, read.token, 'authority', null);

    expect(coordination.snapshot).toBe('authority');
    expect(coordination.readWarning).toBeNull();
    expect(coordination.notificationWarning).toBe('notification failed');
  });

  it('records a current read failure without changing snapshot or notification warning', () => {
    let coordination = state();
    const command = beginPomodoroOperation(coordination);
    coordination = acceptPomodoroCommand(command.state, command.token, 'command', 'notification failed');
    const read = beginPomodoroOperation(coordination);
    coordination = acceptPomodoroRead(read.state, read.token, null, 'read failed');

    expect(coordination.snapshot).toBe('command');
    expect(coordination.readWarning).toBe('read failed');
    expect(coordination.notificationWarning).toBe('notification failed');
  });
});
