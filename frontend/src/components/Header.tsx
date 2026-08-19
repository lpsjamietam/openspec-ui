import { useState } from 'react';
import { useTheme } from '../hooks/useTheme';
import { Menu, Settings, Sun, Moon, Sparkles, Lightbulb, LockKeyhole, Cloud, CheckCircle2, TriangleAlert, Loader2, ExternalLink } from 'lucide-react';
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Sheet,
  SheetContent,
  SheetTrigger,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import type { Source, SyncHealth } from '../types';
import type { ConfigResponse } from '../hooks/useApi';

type View = 'kanban' | 'specs';

interface HeaderProps {
  currentView: View;
  onViewChange: (view: View) => void;
  sources: Source[];
  selectedSourceId: string | null;
  onSourceChange: (id: string | null) => void;
  showViewToggle?: boolean;
  showArchived?: boolean;
  onShowArchivedChange?: (show: boolean) => void;
  onOpenSettings?: () => void;
  onOpenNewIdea?: () => void;
  readOnly?: boolean;
  sourceMode?: 'filesystem' | 'github';
  githubConfig?: ConfigResponse['github'];
  syncHealth?: SyncHealth | null;
}

function GithubSyncStatus({
  sourceMode,
  githubConfig,
  syncHealth,
}: Pick<HeaderProps, 'sourceMode' | 'githubConfig' | 'syncHealth'>) {
  if (sourceMode !== 'github' || !githubConfig) return null;
  const state = syncHealth?.state ?? 'initializing';
  const lastSuccess = syncHealth?.lastSuccessAt
    ? new Date(syncHealth.lastSuccessAt).toLocaleString()
    : 'not synchronized yet';
  const status = state === 'healthy'
    ? { label: 'Current', Icon: CheckCircle2, tone: 'text-emerald-600 dark:text-emerald-400' }
    : state === 'degraded'
      ? { label: 'Degraded', Icon: TriangleAlert, tone: 'text-amber-600 dark:text-amber-400' }
      : { label: 'Synchronizing', Icon: Loader2, tone: 'text-blue-600 dark:text-blue-400' };
  const StatusIcon = status.Icon;

  return (
    <div
      className="flex min-h-9 flex-wrap items-center gap-x-3 gap-y-1 border-t border-border/40 py-2 text-[11px] text-muted-foreground"
      role="status"
    >
      <span className="flex items-center gap-1.5 font-medium text-foreground">
        <Cloud className="h-3.5 w-3.5" />
        GitHub
      </span>
      <a
        href={`https://github.com/${githubConfig.repository}/tree/${encodeURIComponent(githubConfig.specsRef)}`}
        target="_blank"
        rel="noreferrer"
        className="flex items-center gap-1 font-mono hover:text-foreground"
      >
        {githubConfig.repository}@{githubConfig.specsRef}
        <ExternalLink className="h-3 w-3" />
      </a>
      <span className={cn("flex items-center gap-1 font-medium", status.tone)}>
        <StatusIcon className={cn("h-3.5 w-3.5", state === 'initializing' && 'animate-spin')} />
        {status.label}
      </span>
      <span>Last successful sync: {lastSuccess}</span>
      {state === 'degraded' && syncHealth?.lastFailure && (
        <span className="font-medium text-amber-600 dark:text-amber-400">
          Showing last-known-good data · {syncHealth.lastFailure.summary}
        </span>
      )}
    </div>
  );
}

export function Header({
  currentView,
  onViewChange,
  sources,
  selectedSourceId,
  onSourceChange,
  showViewToggle = true,
  showArchived = false,
  onShowArchivedChange,
  onOpenSettings,
  onOpenNewIdea,
  readOnly = true,
  sourceMode = 'filesystem',
  githubConfig = null,
  syncHealth = null,
}: HeaderProps) {
  const { theme, toggle } = useTheme();
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  const handleOpenSettings = () => {
    setMobileMenuOpen(false);
    onOpenSettings?.();
  };

  const logo = (
    <div className="flex items-center gap-2.5 group cursor-default select-none shrink-0">
      <div className="relative">
        <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-[var(--accent-violet)] to-[var(--accent-cyan)] flex items-center justify-center shadow-sm group-hover:shadow-md transition-shadow duration-300">
          <Sparkles className="h-4 w-4 text-white" />
        </div>
        <div className="absolute inset-0 rounded-lg bg-gradient-to-br from-[var(--accent-violet)] to-[var(--accent-cyan)] opacity-0 group-hover:opacity-40 blur-md transition-opacity duration-300" />
      </div>
      <span className="text-lg font-bold tracking-tight whitespace-nowrap">
        <span className="text-gradient">Open</span>
        <span className="text-foreground">Spec</span>
      </span>
    </div>
  );

  const renderViewToggle = (mobile = false) => {
    const handleViewChange = (view: View) => {
      onViewChange(view);
      if (mobile) {
        setMobileMenuOpen(false);
      }
    };

    if (mobile) {
      return (
        <div className="space-y-1">
          <button
            onClick={() => handleViewChange('kanban')}
            className={cn(
              "flex items-center w-full px-3 py-2.5 text-sm rounded-lg transition-all duration-200",
              currentView === 'kanban'
                ? "bg-primary text-primary-foreground font-medium shadow-sm"
                : "text-foreground hover:bg-muted"
            )}
          >
            <span className="mr-3">📋</span>
            Kanban Board
          </button>
          <button
            onClick={() => handleViewChange('specs')}
            className={cn(
              "flex items-center w-full px-3 py-2.5 text-sm rounded-lg transition-all duration-200",
              currentView === 'specs'
                ? "bg-primary text-primary-foreground font-medium shadow-sm"
                : "text-foreground hover:bg-muted"
            )}
          >
            <span className="mr-3">📄</span>
            Specifications
          </button>
        </div>
      );
    }

    return (
      <nav className="flex p-1 bg-muted/50 rounded-lg gap-1">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => handleViewChange('kanban')}
          className={cn(
            "h-8 px-3 rounded-md transition-all duration-200",
            currentView === 'kanban'
              ? "bg-background text-foreground shadow-sm font-medium"
              : "text-muted-foreground hover:text-foreground hover:bg-transparent"
          )}
        >
          Kanban
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => handleViewChange('specs')}
          className={cn(
            "h-8 px-3 rounded-md transition-all duration-200",
            currentView === 'specs'
              ? "bg-background text-foreground shadow-sm font-medium"
              : "text-muted-foreground hover:text-foreground hover:bg-transparent"
          )}
        >
          Specs
        </Button>
      </nav>
    );
  };

  const sourceSelect = (
    <Select
      value={selectedSourceId || "all"}
      onValueChange={(val) => onSourceChange(val === "all" ? null : val)}
    >
      <SelectTrigger className="w-[160px] md:w-[180px] h-9 text-sm bg-background/50 border-border/50 hover:border-border transition-colors shrink-0">
        <SelectValue placeholder="All Projects" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="all">
          <span className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-gradient-to-r from-[var(--accent-violet)] to-[var(--accent-cyan)] shrink-0" />
            <span className="truncate">All Projects</span>
          </span>
        </SelectItem>
        {sources.map(s => (
          <SelectItem key={s.id} value={s.id}>
            <span className="flex items-center gap-2 min-w-0">
              <span className="w-2 h-2 rounded-full bg-[var(--accent-emerald)] shrink-0" />
              <span className="truncate">{s.name}</span>
            </span>
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );

  const renderThemeToggle = (mobile = false) => {
    if (mobile) {
      return (
        <button
          onClick={toggle}
          className="flex items-center gap-3 w-full px-3 py-2.5 text-sm rounded-lg text-foreground hover:bg-muted transition-all duration-200"
          aria-label="Toggle theme"
        >
          {theme === 'dark' ? (
            <>
              <Sun className="h-4 w-4 text-amber-500" />
              Light Mode
            </>
          ) : (
            <>
              <Moon className="h-4 w-4 text-indigo-500" />
              Dark Mode
            </>
          )}
        </button>
      );
    }

    return (
      <Button 
        variant="ghost" 
        size="icon" 
        onClick={toggle} 
        aria-label="Toggle theme"
        className="h-9 w-9 rounded-lg hover:bg-muted transition-colors"
      >
        {theme === 'dark' ? (
          <Sun className="h-4 w-4 text-amber-500 transition-transform hover:rotate-45" />
        ) : (
          <Moon className="h-4 w-4 text-indigo-500 transition-transform hover:-rotate-12" />
        )}
      </Button>
    );
  };

  const renderSettingsButton = (mobile = false) => {
    if (mobile) {
      return (
        <button
          onClick={handleOpenSettings}
          className="flex items-center gap-3 w-full px-3 py-2.5 text-sm rounded-lg text-foreground hover:bg-muted transition-all duration-200"
          aria-label="Open settings"
        >
          <Settings className="h-4 w-4" />
          Settings
        </button>
      );
    }

    return (
      <Button 
        variant="ghost" 
        size="icon" 
        onClick={handleOpenSettings} 
        aria-label="Open settings"
        className="h-9 w-9 rounded-lg hover:bg-muted transition-colors"
      >
        <Settings className="h-4 w-4 transition-transform hover:rotate-90 duration-300" />
      </Button>
    );
  };

  const renderNewIdeaButton = (mobile = false) => {
    if (readOnly) return null;
    if (mobile) {
      return (
        <button
          onClick={() => {
            onOpenNewIdea?.();
            setMobileMenuOpen(false);
          }}
          className="flex items-center gap-3 w-full px-3 py-2.5 text-sm rounded-lg text-foreground hover:bg-muted transition-all duration-200"
          aria-label="Create new idea"
        >
          <Lightbulb className="h-4 w-4" />
          New Idea
        </button>
      );
    }

    return (
      <Button
        variant="default"
        size="sm"
        onClick={onOpenNewIdea}
        className="h-9"
      >
        <Lightbulb className="h-4 w-4 mr-2" />
        New Idea
      </Button>
    );
  };

  const renderShowArchivedToggle = (mobile = false) => {
    if (currentView !== 'kanban' || !onShowArchivedChange) {
      return null;
    }

    const handleToggle = () => {
      onShowArchivedChange(!showArchived);
      if (mobile) {
        setMobileMenuOpen(false);
      }
    };

    if (mobile) {
      return (
        <button
          onClick={handleToggle}
          className={cn(
            "flex items-center w-full px-3 py-2.5 text-sm rounded-lg transition-all duration-200",
            showArchived
              ? "bg-primary text-primary-foreground font-medium shadow-sm"
              : "text-foreground hover:bg-muted"
          )}
          aria-label="Toggle archived changes"
        >
          <span className="mr-3">📦</span>
          {showArchived ? 'Hide Archived' : 'Show Archived'}
        </button>
      );
    }

    return (
      <Button
        variant={showArchived ? 'default' : 'outline'}
        size="sm"
        onClick={handleToggle}
        aria-label="Toggle archived changes"
        className={cn(
          "h-9 transition-all duration-200",
          !showArchived && "bg-background/50 border-border/50 hover:border-border"
        )}
      >
        {showArchived ? 'Hide Archived' : 'Show Archived'}
      </Button>
    );
  };

  return (
    <header className="sticky top-0 z-50 border-b border-border/50 bg-background/80 backdrop-blur-xl supports-[backdrop-filter]:bg-background/60 overflow-x-hidden">
      <div className="max-w-7xl mx-auto px-4 overflow-hidden">
        <div className="flex items-center justify-between h-16 overflow-hidden">
          {/* Left: Logo + Desktop Nav */}
          <div className="flex items-center gap-4 md:gap-6 min-w-0 flex-shrink overflow-hidden">
            {logo}
            {/* Desktop view toggle */}
            {showViewToggle && (
              <div className="hidden md:block">
                {renderViewToggle()}
              </div>
            )}
          </div>

          {/* Right: Controls */}
          <div className="flex items-center gap-2 md:gap-3 flex-shrink-0">
            {readOnly && (
              <div className="hidden sm:flex items-center gap-1.5 rounded-full border border-border/60 bg-muted/50 px-2.5 py-1 text-[11px] font-medium text-muted-foreground" title="All repository and configuration mutations are disabled">
                <LockKeyhole className="h-3 w-3" />
                Read only
              </div>
            )}
            {/* Show Archived toggle - visible on desktop when in kanban view */}
            <div className="hidden md:block">
              {renderShowArchivedToggle()}
            </div>

            {/* Source select - visible on all sizes */}
            {sourceSelect}

            {/* New Idea button - visible on desktop */}
            <div className="hidden md:block">
              {renderNewIdeaButton()}
            </div>

            {/* Settings button - visible on desktop */}
            <div className="hidden md:block">
              {renderSettingsButton()}
            </div>

            {/* Theme toggle - visible on desktop */}
            <div className="hidden md:block">
              {renderThemeToggle()}
            </div>

            {/* Mobile menu */}
            <div className="md:hidden">
              <Sheet open={mobileMenuOpen} onOpenChange={setMobileMenuOpen}>
                <SheetTrigger asChild>
                  <Button variant="ghost" size="icon" aria-label="Open menu" className="h-9 w-9">
                    <Menu className="h-5 w-5" />
                  </Button>
                </SheetTrigger>
                <SheetContent side="right" className="w-[300px] p-0">
                  <div className="flex flex-col h-full">
                    <SheetHeader className="px-5 py-5 border-b border-border">
                      <SheetTitle className="text-left flex items-center gap-2">
                        <Sparkles className="h-4 w-4 text-primary" />
                        Menu
                      </SheetTitle>
                    </SheetHeader>
                    <nav className="flex-1 px-4 py-5 space-y-6">
                      {showViewToggle && (
                        <div className="space-y-2">
                          <p className="px-3 text-xs font-semibold text-muted-foreground uppercase tracking-wider">View</p>
                          {renderViewToggle(true)}
                        </div>
                      )}
                      {currentView === 'kanban' && onShowArchivedChange && (
                        <div className="space-y-2">
                          <p className="px-3 text-xs font-semibold text-muted-foreground uppercase tracking-wider">Filter</p>
                          {renderShowArchivedToggle(true)}
                        </div>
                      )}
                      <div className="space-y-2">
                        <p className="px-3 text-xs font-semibold text-muted-foreground uppercase tracking-wider">Preferences</p>
                        <div className="space-y-1">
                          {renderNewIdeaButton(true)}
                          {renderSettingsButton(true)}
                          {renderThemeToggle(true)}
                        </div>
                      </div>
                    </nav>
                  </div>
                </SheetContent>
              </Sheet>
            </div>
          </div>
        </div>
        <GithubSyncStatus
          sourceMode={sourceMode}
          githubConfig={githubConfig}
          syncHealth={syncHealth}
        />
      </div>
    </header>
  );
}
