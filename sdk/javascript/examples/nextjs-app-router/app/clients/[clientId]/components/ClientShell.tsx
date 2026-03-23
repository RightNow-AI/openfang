"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import FounderWorkspacePanel from "./FounderWorkspacePanel";
import { getFounderWorkspaceSnapshotForClient } from "../lib/client-api";
import type { FounderWorkspaceSnapshot, HealthLevel } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

type ClientShellProps = {
  clientId: string;
  clientName: string;
  currentPage: "home" | "pulse" | "plan" | "approvals" | "results" | "founder";
  approvalsWaiting: number;
  tasksDueToday: number;
  lastActivityAt: string | null;
  health: HealthLevel;
  children: React.ReactNode;
};

const pageLabels: Record<ClientShellProps["currentPage"], string> = {
  home: "Client Home",
  pulse: "Client Pulse",
  plan: "Plan and Assign",
  approvals: "Approvals and Execution",
  results: "Results and Review",
  founder: "Founder Start",
};

const navItems = [
  { key: "home", label: "Client Home", buildHref: (clientId: string) => `/clients/${clientId}` },
  { key: "pulse", label: "Client Pulse", buildHref: (clientId: string) => `/clients/${clientId}/pulse` },
  { key: "plan", label: "Plan and Assign", buildHref: (clientId: string) => `/clients/${clientId}/plan` },
  { key: "approvals", label: "Approvals and Execution", buildHref: (clientId: string) => `/clients/${clientId}/approvals` },
  { key: "results", label: "Results and Review", buildHref: (clientId: string) => `/clients/${clientId}/results` },
] as const;

const secondaryItems = ["Files", "Comms", "Finance"] as const;

const healthLabels: Record<HealthLevel, string> = {
  green: "Healthy",
  yellow: "Watch",
  red: "Risk",
};

function formatWhen(value: string | null) {
  if (!value) return "No recent activity";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "No recent activity";
  return date.toLocaleString();
}

function buildFounderWorkspaceHref(
  clientId: string,
  clientName: string,
  workspaceId: string,
  playbookId?: string | null,
) {
  return `/deep-research?${new URLSearchParams({
    clientId,
    clientName,
    workspaceId,
    ...(playbookId ? { playbookId } : {}),
  }).toString()}`;
}

function buildFounderStartHref(clientId: string) {
  return `/clients/${clientId}/founder/start`;
}

export default function ClientShell({
  clientId,
  clientName,
  currentPage,
  approvalsWaiting,
  tasksDueToday,
  lastActivityAt,
  health,
  children,
}: ClientShellProps) {
  const pathname = usePathname();
  const [founderSnapshot, setFounderSnapshot] = useState<FounderWorkspaceSnapshot | null>(null);
  const [founderLoading, setFounderLoading] = useState(true);
  const [founderError, setFounderError] = useState("");

  useEffect(() => {
    let cancelled = false;

    async function loadFounderSnapshot() {
      setFounderLoading(true);
      setFounderError("");

      try {
        const snapshot = await getFounderWorkspaceSnapshotForClient(clientId);
        if (cancelled) return;
        setFounderSnapshot(snapshot);
      } catch (error) {
        if (cancelled) return;
        setFounderError(error instanceof Error ? error.message : "Failed to load founder workspace.");
      } finally {
        if (!cancelled) setFounderLoading(false);
      }
    }

    loadFounderSnapshot().catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [clientId]);

  const founderWorkspaceId = founderSnapshot?.workspace?.workspaceId || `client-${clientId}`;
  const founderStartHref = buildFounderStartHref(clientId);
  const hasFounderRuns = Boolean(founderSnapshot?.runs?.length);
  const hasFounderWorkspace = Boolean(founderSnapshot?.workspace);
  const founderPlaybookId = founderSnapshot?.runs?.[0]?.playbookId
    ?? (typeof founderSnapshot?.workspace?.playbookDefaults?.defaultPlaybookId === "string"
      ? founderSnapshot.workspace.playbookDefaults.defaultPlaybookId
      : null);
  const founderWorkspaceHref = buildFounderWorkspaceHref(clientId, clientName, founderWorkspaceId, founderPlaybookId);
  const founderPrimaryHref = hasFounderRuns ? founderWorkspaceHref : founderStartHref;
  const founderPrimaryLabel = hasFounderRuns
    ? "Resume founder work"
    : hasFounderWorkspace
      ? "Start founder research"
      : "Start founder setup";

  return (
    <main className={styles.shellPage}>
      <div className={styles.shellGrid}>
        <aside className={styles.shellSidebar}>
          <div className={styles.shellSidebarIntro}>
            <div className={styles.eyebrow}>
              Client Workspace
            </div>
            <div className={styles.shellTitle}>{clientName}</div>
            <div className={styles.shellCaption}>{pageLabels[currentPage]}</div>
          </div>

          <nav className={styles.shellNav}>
            {navItems.map((item) => {
              const href = item.buildHref(clientId);
              const active = pathname === href;
              return (
                <Link
                  key={item.key}
                  href={href}
                  className={styles.navLink}
                  data-active={active}
                  aria-current={active ? "page" : undefined}
                >
                  {item.label}
                </Link>
              );
            })}
          </nav>

          <div className={styles.shellSecondary}>
            <div className={styles.eyebrow}>Founder Tools</div>
            <Link href={founderPrimaryHref} className={styles.linkButton}>
              {founderPrimaryLabel}
            </Link>
            {secondaryItems.map((label) => (
              <div key={label} className={styles.shellSecondaryItem}>
                {label}
              </div>
            ))}
          </div>
        </aside>

        <section>
          <header className={styles.shellHeader}>
            <div className={styles.shellHeaderTop}>
              <div>
                <div className={styles.eyebrow}>
                  Client Dashboard
                </div>
                <h1 className={styles.shellHeaderName}>{clientName}</h1>
                <div className={styles.shellHeaderSubtext}>
                  {pageLabels[currentPage]} · Last activity {formatWhen(lastActivityAt)}
                </div>
              </div>

              <div className={styles.shellActions}>
                <span className={styles.healthPill} data-health={health}>
                  {healthLabels[health]}
                </span>
                <Link href={founderPrimaryHref} className={styles.linkButton}>
                  {founderPrimaryLabel}
                </Link>
                <Link href={`/clients/${clientId}/approvals`} className={styles.linkButton}>
                  Approvals {approvalsWaiting > 0 ? `(${approvalsWaiting})` : ""}
                </Link>
                <Link href={`/clients/${clientId}/plan`} className={`${styles.linkButton} ${styles.linkPrimary}`}>
                  Plan work
                </Link>
              </div>
            </div>

            <div className={styles.shellStats}>
              {[
                { label: "Approvals waiting", value: approvalsWaiting.toString() },
                { label: "Tasks due today", value: tasksDueToday.toString() },
                { label: "Current sprint", value: "This cycle" },
                { label: "Quick actions", value: "Draft, approve, run" },
              ].map((stat) => (
                <div key={stat.label} className={styles.statCard}>
                  <div className={styles.statLabel}>{stat.label}</div>
                  <div className={styles.statValue}>{stat.value}</div>
                </div>
              ))}
            </div>
          </header>

          <FounderWorkspacePanel
            clientId={clientId}
            clientName={clientName}
            founderHref={founderWorkspaceHref}
            founderStartHref={founderStartHref}
            loading={founderLoading}
            error={founderError}
            snapshot={founderSnapshot}
          />

          {children}
        </section>
      </div>
    </main>
  );
}