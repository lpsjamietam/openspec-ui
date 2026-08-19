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
});
