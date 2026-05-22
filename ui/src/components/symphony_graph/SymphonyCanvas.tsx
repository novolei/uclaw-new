import { useState, useEffect } from "react";
import { getStatus } from "./lib/api";
import { RepoSelector } from "./components/RepoSelector";
import { IssueList } from "./components/IssueList";
import { Dashboard } from "./components/Dashboard";
import { ActiveAgents } from "./components/ActiveAgents";
import { Settings } from "./components/Settings";
import { LogViewer } from "./components/LogViewer";
import { PipelineReportView } from "./components/PipelineReportView";
import { FolderOpen, FileText, LayoutDashboard, Activity, Settings as SettingsIcon } from "lucide-react";

export type View = "repos" | "issues" | "dashboard" | "agents" | "settings";

export const SYMPHONY_NEW_TAB_SENTINEL = "__symphony_new__";

export interface SymphonyCanvasProps {
  workflowId?: string;
}

export function SymphonyCanvas({ workflowId }: SymphonyCanvasProps = {}) {
  const [view, setView] = useState<View>("repos");
  const [selectedRepos, setSelectedRepos] = useState<string[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [reportRunId, setReportRunId] = useState<string | null>(null);

  useEffect(() => {
    getStatus()
      .then((status) => {
        if (status.repos && status.repos.length > 0) {
          setSelectedRepos(status.repos);
          setView("dashboard");
        }
      })
      .catch((err) => {
        console.error("Failed to load status:", err);
      });
  }, []);

  return (
    <div className="flex h-full bg-transparent text-foreground gap-3 overflow-hidden">
      {/* Sidebar */}
      <div className="w-56 bg-[#161b22] border border-[#30363d] rounded-2xl flex flex-col shrink-0 shadow-lg overflow-hidden">
        <div className="p-4 border-b border-[#30363d]">
          <h1 className="text-lg font-bold text-foreground flex items-center gap-2">
            <span className="text-xl text-primary">&#9835;</span> Symphony
          </h1>
          <p className="text-xs text-muted-foreground mt-1">Agent Orchestrator</p>
        </div>

        <nav className="flex-1 p-2 space-y-1">
          <NavItem
            label="Repositories"
            active={view === "repos"}
            onClick={() => setView("repos")}
            icon={<FolderOpen size={16} />}
          />
          <NavItem
            label="Issues"
            active={view === "issues"}
            onClick={() => setView("issues")}
            icon={<FileText size={16} />}
            disabled={selectedRepos.length === 0}
          />
          <NavItem
            label="Dashboard"
            active={view === "dashboard"}
            onClick={() => setView("dashboard")}
            icon={<LayoutDashboard size={16} />}
          />
          <NavItem
            label="Active Agents"
            active={view === "agents"}
            onClick={() => setView("agents")}
            icon={<Activity size={16} />}
          />
          <NavItem
            label="Settings"
            active={view === "settings"}
            onClick={() => setView("settings")}
            icon={<SettingsIcon size={16} />}
          />
        </nav>

        {selectedRepos.length > 0 && (
          <div className="p-3 border-t border-[#30363d] bg-muted/5">
            <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
              Active Repos ({selectedRepos.length})
            </p>
            <div className="max-h-32 overflow-y-auto space-y-1">
              {selectedRepos.map((repo) => (
                <p key={repo} className="text-xs text-primary font-medium truncate" title={repo}>
                  {repo}
                </p>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden bg-[#0d1117] border border-[#30363d] rounded-2xl shadow-lg">
        {view === "repos" && (
          <RepoSelector
            selectedRepos={selectedRepos}
            onToggleRepo={(repo) => {
              setSelectedRepos((prev) =>
                prev.includes(repo)
                  ? prev.filter((r) => r !== repo)
                  : [...prev, repo]
              );
            }}
            onConfirm={() => setView("issues")}
          />
        )}
        {view === "issues" && selectedRepos.length > 0 && (
          <IssueList
            repos={selectedRepos}
            onRunStarted={() => setView("dashboard")}
          />
        )}
        {view === "dashboard" && (
          <Dashboard
            onViewLogs={(runId) => {
              setReportRunId(null);
              setSelectedRunId(runId);
            }}
            onViewReport={(runId) => {
              setSelectedRunId(null);
              setReportRunId(runId);
            }}
          />
        )}
        {view === "agents" && (
          <ActiveAgents
            onViewLogs={(runId) => setSelectedRunId(runId)}
          />
        )}
        {view === "settings" && <Settings />}
      </div>

      {/* Report panel (slides in) */}
      {reportRunId && (
        <div className="w-[500px] bg-[#161b22] border border-[#30363d] rounded-2xl flex flex-col shrink-0 shadow-lg overflow-hidden animate-in slide-in-from-right duration-200">
          <PipelineReportView
            runId={reportRunId}
            onClose={() => setReportRunId(null)}
            onViewLogs={(runId) => {
              setReportRunId(null);
              setSelectedRunId(runId);
            }}
          />
        </div>
      )}

      {/* Log panel (slides in) */}
      {selectedRunId && (
        <div className="w-[500px] bg-[#161b22] border border-[#30363d] rounded-2xl flex flex-col shrink-0 shadow-lg overflow-hidden animate-in slide-in-from-right duration-200">
          <LogViewer
            runId={selectedRunId}
            onClose={() => setSelectedRunId(null)}
          />
        </div>
      )}
    </div>
  );
}

interface NavItemProps {
  label: string;
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  disabled?: boolean;
}

function NavItem({ label, active, onClick, icon, disabled }: NavItemProps) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`w-full text-left px-3 py-2 rounded-md text-sm flex items-center gap-2.5 transition-colors
        ${active
          ? "bg-primary/10 text-primary font-medium"
          : disabled
            ? "text-muted-foreground/40 cursor-not-allowed opacity-50"
            : "text-muted-foreground hover:bg-muted hover:text-foreground"
        }`}
    >
      <span className="shrink-0">{icon}</span>
      <span className="truncate">{label}</span>
    </button>
  );
}
