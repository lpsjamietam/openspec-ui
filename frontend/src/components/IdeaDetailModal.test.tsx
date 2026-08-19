import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Idea } from '../types';
import { IdeaDetailModal } from './IdeaDetailModal';

const idea: Idea = {
  id: 'reach/idea-1',
  sourceId: 'reach',
  projectId: null,
  title: 'Read-only idea',
  description: 'A stored idea',
  createdAt: '2026-08-19T00:00:00Z',
  updatedAt: '2026-08-19T00:00:00Z',
};

describe('IdeaDetailModal read-only mode', () => {
  it('hides editing controls by default', () => {
    render(<IdeaDetailModal idea={idea} onClose={vi.fn()} />);

    expect(screen.getByText('Read-only idea')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /delete/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /edit/i })).not.toBeInTheDocument();
  });

  it('shows editing controls only when writable mode is explicit', () => {
    render(<IdeaDetailModal idea={idea} onClose={vi.fn()} readOnly={false} />);

    expect(screen.getByRole('button', { name: /delete/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /edit/i })).toBeInTheDocument();
  });
});
