import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SpecsView } from './SpecsView';

const github = {
  repository: 'ToruAI/openspec-ui',
  refName: 'demo/main',
  commit: '0123456789abcdef',
  htmlUrl: 'https://github.com/ToruAI/openspec-ui/tree/demo/main/openspec/specs/sample/spec.md',
};

vi.mock('../hooks/useMediaQuery', () => ({
  useIsMobile: () => false,
}));

vi.mock('../hooks/useApi', () => ({
  useSpecs: () => ({
    specs: [{
      id: 'github-base/sample/spec.md',
      path: 'sample/spec.md',
      sourceId: 'github-base',
      github,
    }],
    loading: false,
    error: null,
  }),
  useSpec: () => ({
    spec: {
      id: 'github-base/sample/spec.md',
      path: 'sample/spec.md',
      sourceId: 'github-base',
      content: '# Canonical requirement',
      github,
    },
    loading: false,
    error: null,
  }),
}));

describe('SpecsView GitHub provenance', () => {
  it('identifies accepted Specs with the canonical repository, ref, and commit', () => {
    render(<SpecsView selectedSourceId={null} />);
    expect(screen.getByText('ToruAI/openspec-ui@demo/main')).toHaveAttribute(
      'href',
      github.htmlUrl,
    );

    fireEvent.click(screen.getByRole('button', { name: /sample\/spec.md/ }));
    expect(screen.getByText(/Accepted from ToruAI\/openspec-ui@demo\/main/)).toHaveTextContent(
      '01234567',
    );
  });
});
