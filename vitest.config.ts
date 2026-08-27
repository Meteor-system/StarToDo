import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    exclude: [
      'e2e/**',
      'playwright-report/**',
      'test-results/**',
      'blob-report/**',
      '.playwright/**',
      'coverage/**',
      'build/**',
      'dist/**',
      '.svelte-kit/**',
      'node_modules/**'
    ],
    clearMocks: true,
    mockReset: true,
    restoreMocks: true,
    unstubGlobals: true,
    unstubEnvs: true
  }
});