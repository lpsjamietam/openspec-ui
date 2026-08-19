import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Header } from './Header';

vi.mock('../hooks/useTheme', () => ({
  useTheme: () => ({ theme: 'light', toggle: vi.fn() }),
}));

const props = {
  currentView: 'kanban' as const,
  onViewChange: vi.fn(),
  sources: [],
  selectedSourceId: null,
  onSourceChange: vi.fn(),
  sourceMode: 'github' as const,
  githubConfig: {
    repository: 'ToruAI/openspec-ui',
    specsRef: 'demo/main',
    changesBaseRef: 'demo/main',
    pullRequestTargets: ['demo/main'],
    cachePath: '/data/openspec-ui',
    reconciliationIntervalSeconds: 900,
    maxPullRequests: 50,
    apiBaseUrl: 'https://api.github.com',
    maxFileBytes: 1024,
    maxSnapshotBytes: 4096,
  },
};

describe('Header GitHub synchronization status', () => {
  it('shows an explicit initializing state before the first snapshot', () => {
    render(<Header {...props} syncHealth={null} />);
    expect(screen.getByText('Synchronizing')).toBeInTheDocument();
    expect(screen.getByText(/not synchronized yet/)).toBeInTheDocument();
  });

  it('shows canonical ref and last successful synchronization', () => {
    render(
      <Header
        {...props}
        syncHealth={{
          state: 'healthy',
          activeRevision: 'revision',
          contributingRefs: [],
          lastSuccessAt: '2026-08-19T12:00:00Z',
          servingLastKnownGood: false,
        }}
      />,
    );
    expect(screen.getByText('ToruAI/openspec-ui@demo/main')).toBeInTheDocument();
    expect(screen.getByText('Current')).toBeInTheDocument();
  });

  it('makes degraded last-known-good state visible', () => {
    render(
      <Header
        {...props}
        syncHealth={{
          state: 'degraded',
          activeRevision: 'revision',
          contributingRefs: [],
          lastSuccessAt: '2026-08-18T12:00:00Z',
          lastFailure: {
            category: 'github',
            summary: 'GitHub returned HTTP 503',
            occurredAt: '2026-08-19T12:00:00Z',
          },
          servingLastKnownGood: true,
        }}
      />,
    );
    expect(screen.getByText('Degraded')).toBeInTheDocument();
    expect(screen.getByText(/Showing last-known-good data/)).toBeInTheDocument();
  });
});
