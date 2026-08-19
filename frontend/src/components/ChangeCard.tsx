import type { Change, Artifact } from '../types';
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { FileText, Layers, CheckSquare, Palette, GitBranch, Copy, TriangleAlert } from 'lucide-react';
import { cn } from "@/lib/utils";

interface ChangeCardProps {
  change: Change;
  onClick: () => void;
}

const ARTIFACTS: { id: string; label: string; Icon: typeof FileText; tone: string }[] = [
  { id: 'proposal', label: 'Proposal', Icon: FileText, tone: 'bg-blue-500/10 text-blue-500 dark:text-blue-400' },
  { id: 'design', label: 'Design', Icon: Palette, tone: 'bg-purple-500/10 text-purple-500 dark:text-purple-400' },
  { id: 'specs', label: 'Specs', Icon: Layers, tone: 'bg-emerald-500/10 text-emerald-500 dark:text-emerald-400' },
  { id: 'tasks', label: 'Tasks', Icon: CheckSquare, tone: 'bg-amber-500/10 text-amber-500 dark:text-amber-400' },
];

/** Fall back to the boolean flags when a source is served by an older backend. */
function artifactStates(change: Change): Record<string, Artifact> {
  const states: Record<string, Artifact> = {};

  if (change.artifacts?.length) {
    for (const artifact of change.artifacts) states[artifact.id] = artifact;
    return states;
  }

  const present: Record<string, boolean> = {
    proposal: change.hasProposal,
    design: change.hasDesign,
    specs: change.hasSpecs,
    tasks: change.hasTasks,
  };
  for (const { id } of ARTIFACTS) {
    states[id] = { id, state: present[id] ? 'complete' : 'blocked', missingDeps: [] };
  }
  return states;
}

function artifactTitle(artifact: Artifact): string {
  if (artifact.state === 'complete') return 'Written';
  if (artifact.state === 'skipped') return 'Skipped by the OpenSpec workflow';
  if (artifact.state === 'ready') return 'Ready to write — nothing blocking it';
  return artifact.missingDeps.length > 0
    ? `Waiting for ${artifact.missingDeps.join(' and ')}`
    : 'Not written yet';
}

export function ChangeCard({ change, onClick }: ChangeCardProps) {
  const taskStats = change.taskStats;
  const progress = taskStats && taskStats.total > 0 ? (taskStats.done / taskStats.total) * 100 : 0;
  const isComplete = progress === 100;
  const states = artifactStates(change);
  const branchLabel = change.git?.detached
    ? `detached @ ${change.git.commit}`
    : change.git?.branch;

  return (
    <Card
      className={cn(
        "cursor-pointer card-hover border-border/50 bg-card/80 backdrop-blur-sm",
        "hover:border-primary/30 transition-all duration-200",
        isComplete && "border-[var(--accent-emerald)]/30 bg-[var(--accent-emerald)]/5"
      )}
      onClick={onClick}
    >
      <CardContent className="p-4">
        {/* Header: Title + Source */}
        <div className="flex items-start justify-between gap-3 mb-3">
          <h3 className="font-semibold text-sm flex-1 line-clamp-2 leading-snug">
            {change.name}
          </h3>
          <Badge 
            variant="secondary" 
            className="text-[10px] font-medium px-2 py-0.5 shrink-0 bg-muted/80"
          >
            {change.sourceId}
          </Badge>
        </div>

        {(branchLabel || change.track || change.targetBranch) && (
          <div className="mb-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-[10px] text-muted-foreground">
            {branchLabel && (
              <span className="flex min-w-0 items-center gap-1 font-mono" title={change.git?.worktreeRoot}>
                <GitBranch className="h-3 w-3 shrink-0" />
                <span className="truncate">{branchLabel}</span>
              </span>
            )}
            {change.track && <span className="rounded bg-muted px-1.5 py-0.5 font-medium">{change.track}</span>}
            {change.targetBranch && <span className="font-mono">→ {change.targetBranch}</span>}
          </div>
        )}

        {((change.duplicateCount ?? 1) > 1 || change.statusSource?.startsWith('filesystem')) && (
          <div className="mb-3 flex flex-wrap gap-2 text-[10px]">
            {(change.duplicateCount ?? 1) > 1 && (
              <span className="flex items-center gap-1 text-muted-foreground" title={change.duplicateSources?.join(', ')}>
                <Copy className="h-3 w-3" />
                {change.duplicateCount} worktree copies grouped
              </span>
            )}
            {change.statusSource?.startsWith('filesystem') && (
              <span
                className={cn(
                  "flex items-center gap-1",
                  change.statusSource === 'filesystem_fallback'
                    ? "text-amber-600 dark:text-amber-400"
                    : "text-muted-foreground"
                )}
                title={change.statusSource === 'filesystem_fallback'
                  ? 'OpenSpec CLI status was unavailable; artifact state was inferred from files'
                  : 'Artifact state was read from files'}
              >
                <TriangleAlert className="h-3 w-3" />
                {change.statusSource === 'filesystem_fallback' ? 'filesystem fallback' : 'filesystem status'}
              </span>
            )}
          </div>
        )}

        {/* Progress bar */}
        {taskStats && taskStats.total > 0 && (
          <div className="space-y-2 mb-3">
            <div className="flex justify-between items-center text-xs">
              <span className="text-muted-foreground font-medium">
                {taskStats.done} of {taskStats.total} tasks
              </span>
              <span className={cn(
                "font-semibold tabular-nums",
                isComplete ? "text-[var(--accent-emerald)]" : "text-foreground"
              )}>
                {Math.round(progress)}%
              </span>
            </div>
            <div className="h-1.5 bg-muted rounded-full overflow-hidden">
              <div
                className={cn(
                  "h-full rounded-full transition-all duration-500 ease-out",
                  isComplete 
                    ? "bg-[var(--accent-emerald)]" 
                    : "progress-gradient"
                )}
                style={{ width: `${progress}%` }}
              />
            </div>
          </div>
        )}

        {/* Artifact chain: written, ready to write, or waiting on another artifact */}
        <div className="flex gap-2 flex-wrap items-center">
          {ARTIFACTS.map(({ id, label, Icon, tone }) => {
            const artifact = states[id];
            if (!artifact) return null;
            const isWritten = artifact.state === 'complete';
            const isReady = artifact.state === 'ready';
            const isSkipped = artifact.state === 'skipped';

            return (
              <div
                key={id}
                title={artifactTitle(artifact)}
                className={cn(
                  "flex items-center gap-1.5 px-2 py-1 rounded-md transition-opacity",
                  isWritten
                    ? tone
                    : isSkipped
                      ? "border border-dashed border-border/70 bg-muted/30 text-muted-foreground/60"
                    : isReady
                      ? "bg-foreground/5 text-foreground/70 ring-1 ring-inset ring-foreground/15"
                      : "bg-muted/50 text-muted-foreground/50"
                )}
              >
                <Icon className="h-3 w-3" />
                <span className="text-[10px] font-medium">{label}</span>
                {isReady && <span className="text-[9px] font-normal opacity-70">next</span>}
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
