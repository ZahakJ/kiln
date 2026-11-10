import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}'],
    maxWorkers: 2,
    // The source scan parses every TSX file of the workspace.
    testTimeout: 60_000,
  },
});
