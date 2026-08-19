import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Change } from '../types';
import { ChangeCard } from './ChangeCard';

const worktreeChange: Change = {
  id: 'reach-demo/example-change',
  name: 'example-change',
  sourceId: 'reach-demo',
  status: 'todo',
  taskStats: { done: 0, total: 2 },
  hasProposal: true,
  hasSpecs: false,
  hasTasks: true,
  hasDesign: false,
  schema: 'spec-driven',
  artifacts: [
    { id: 'proposal', state: 'complete', missingDeps: [] },
    { id: 'design', state: 'skipped', missingDeps: [] },
    { id: 'specs', state: 'ready', missingDeps: [] },
    { id: 'tasks', state: 'blocked', missingDeps: ['specs'] },
  ],
  statusSource: 'filesystem_fallback',
  track: 'demo',
  targetBranch: 'demo/main',
  git: {
    worktreeRoot: '/worktrees/reach-demo',
    branch: 'demo/example-change',
    commit: 'abc123def456',
    detached: false,
  },
  duplicateCount: 2,
  duplicateSources: ['reach-demo', 'reach-copy'],
};

describe('ChangeCard worktree metadata', () => {
  it('shows branch, track, target, grouped copies, and fallback provenance', () => {
    render(<ChangeCard change={worktreeChange} onClick={vi.fn()} />);

    expect(screen.getByText('demo/example-change')).toBeInTheDocument();
    expect(screen.getByText('demo')).toBeInTheDocument();
    expect(screen.getByText('→ demo/main')).toBeInTheDocument();
    expect(screen.getByText('2 worktree copies grouped')).toBeInTheDocument();
    expect(screen.getByText('filesystem fallback')).toBeInTheDocument();
    expect(screen.getByTitle('Skipped by the OpenSpec workflow')).toBeInTheDocument();
  });

  it('shows GitHub PR provenance and a merged-but-unarchived warning', () => {
    render(
      <ChangeCard
        change={{
          ...worktreeChange,
          id: 'github-pr-42/add-server-sync',
          name: 'add-server-sync',
          sourceId: 'github-pr-42',
          git: null,
          track: 'github',
          github: {
            repository: 'ToruAI/openspec-ui',
            refName: 'feature/server-sync',
            commit: '0123456789abcdef',
            htmlUrl: 'https://github.com/ToruAI/openspec-ui/pull/42',
            pullRequest: {
              number: 42,
              headRef: 'feature/server-sync',
              baseRef: 'demo/main',
              htmlUrl: 'https://github.com/ToruAI/openspec-ui/pull/42',
            },
          },
          archiveWarning: {
            pullRequestNumber: 41,
            mergedAt: '2026-07-01T00:00:00Z',
            htmlUrl: 'https://github.com/ToruAI/openspec-ui/pull/41',
          },
        }}
        onClick={vi.fn()}
      />,
    );

    expect(screen.getByText('PR #42')).toHaveAttribute(
      'href',
      'https://github.com/ToruAI/openspec-ui/pull/42',
    );
    expect(screen.getByText(/Merged .* but still active/)).toBeInTheDocument();
    expect(screen.getByText('PR #41')).toBeInTheDocument();
  });

  it('does not invent an archive warning when merge association is absent', () => {
    render(<ChangeCard change={{ ...worktreeChange, archiveWarning: null }} onClick={vi.fn()} />);
    expect(screen.queryByText(/but still active/)).not.toBeInTheDocument();
  });
});
