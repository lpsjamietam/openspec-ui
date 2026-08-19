export interface Source {
  id: string;
  name: string;
  path: string;
  valid: boolean;
  track?: string | null;
  targetBranch?: string | null;
  git?: GitContext | null;
  github?: GitHubProvenance | null;
  canonicalSpecs?: boolean;
}

export interface GitContext {
  worktreeRoot: string;
  branch: string | null;
  commit: string;
  detached: boolean;
}

export interface PullRequestProvenance {
  number: number;
  headRef: string;
  baseRef: string;
  htmlUrl: string;
}

export interface GitHubProvenance {
  repository: string;
  refName: string;
  commit: string;
  htmlUrl: string;
  pullRequest?: PullRequestProvenance | null;
}

export interface ArchiveWarning {
  pullRequestNumber: number;
  mergedAt: string;
  htmlUrl: string;
}

export type SyncState = 'disabled' | 'initializing' | 'healthy' | 'degraded';

export interface ContributingRef {
  sourceId: string;
  refName: string;
  commit: string;
  pullRequestNumber?: number | null;
}

export interface SyncFailure {
  category: string;
  summary: string;
  occurredAt: string;
}

export interface SyncHealth {
  state: SyncState;
  activeRevision?: string | null;
  contributingRefs: ContributingRef[];
  lastAttemptAt?: string | null;
  lastSuccessAt?: string | null;
  lastFailure?: SyncFailure | null;
  servingLastKnownGood: boolean;
}

export interface TaskStats {
  total: number;
  done: number;
}

export type ChangeStatus = 'draft' | 'todo' | 'in_progress' | 'done' | 'archived';

/** Same vocabulary as `openspec status`. */
export type ArtifactState = 'complete' | 'ready' | 'blocked' | 'skipped';

export interface Artifact {
  id: string;
  state: ArtifactState;
  /** Artifacts this one waits for, when blocked */
  missingDeps: string[];
}

export interface Change {
  id: string;
  name: string;
  sourceId: string;
  status: ChangeStatus;
  hasProposal: boolean;
  hasSpecs: boolean;
  hasTasks: boolean;
  hasDesign: boolean;
  taskStats: TaskStats | null;
  /** Workflow schema from .openspec.yaml (OpenSpec >= 1.0) */
  schema: string | null;
  artifacts: Artifact[];
  statusSource?: 'cli' | 'filesystem' | 'filesystem_fallback';
  git?: GitContext | null;
  github?: GitHubProvenance | null;
  archiveWarning?: ArchiveWarning | null;
  track?: string | null;
  targetBranch?: string | null;
  duplicateCount?: number;
  duplicateSources?: string[];
}

export interface SpecContent {
  path: string;
  content: string;
}

export interface TasksContent {
  raw: string;
  stats: TaskStats;
}

export interface ChangeDetail {
  id: string;
  name: string;
  sourceId: string;
  status: ChangeStatus;
  proposal: string | null;
  design: string | null;
  specs: SpecContent[];
  tasks: TasksContent | null;
  schema: string | null;
  artifacts: Artifact[];
  github?: GitHubProvenance | null;
  archiveWarning?: ArchiveWarning | null;
}

export interface Spec {
  id: string;
  sourceId: string;
  path: string;
  github?: GitHubProvenance | null;
}

export interface SpecDetail {
  id: string;
  sourceId: string;
  path: string;
  content: string;
  github?: GitHubProvenance | null;
}

export interface Idea {
  id: string;
  sourceId: string;
  projectId: string | null;
  title: string;
  description: string;
  createdAt: string;
  updatedAt: string;
}
