"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import type { FounderWorkspaceItem } from "../lib/client-types";
import styles from "../../client-dashboard.module.css";

type Playbook = {
  id: string;
  title: string;
  description?: string;
  icon?: string;
  starterQuestions?: string[];
};

type Props = {
  clientId: string;
  clientName: string;
  initialIdea: string;
  founderWorkspace: FounderWorkspaceItem | null;
};

const STAGES = [
  { value: "idea", label: "Idea" },
  { value: "validation", label: "Validation" },
  { value: "launch", label: "Launch" },
  { value: "growth", label: "Growth" },
];

export default function FounderStartWizard({ clientId, clientName, initialIdea, founderWorkspace }: Props) {
  const router = useRouter();
  const [stepIdx, setStepIdx] = useState(0);
  const [playbooks, setPlaybooks] = useState<Playbook[]>([]);
  const [loadingPlaybooks, setLoadingPlaybooks] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [form, setForm] = useState({
    focus: initialIdea || founderWorkspace?.idea || "",
    playbookId: typeof founderWorkspace?.playbookDefaults?.defaultPlaybookId === "string"
      ? founderWorkspace.playbookDefaults.defaultPlaybookId
      : "customer-discovery",
    workspaceName: founderWorkspace?.name || `${clientName} Founder Workspace`,
    companyName: founderWorkspace?.companyName || clientName,
    idea: founderWorkspace?.idea || initialIdea || "",
    stage: founderWorkspace?.stage || "validation",
  });

  useEffect(() => {
    let cancelled = false;

    fetch("/api/playbooks")
      .then((response) => response.ok ? response.json() : { playbooks: [] })
      .then((data) => {
        if (cancelled) return;
        const nextPlaybooks = Array.isArray(data?.playbooks) ? data.playbooks : [];
        setPlaybooks(nextPlaybooks);
        if (!form.playbookId && nextPlaybooks[0]?.id) {
          setForm((current) => ({ ...current, playbookId: nextPlaybooks[0].id }));
        }
      })
      .catch(() => {
        if (cancelled) return;
        setPlaybooks([]);
      })
      .finally(() => {
        if (!cancelled) setLoadingPlaybooks(false);
      });

    return () => {
      cancelled = true;
    };
  }, [form.playbookId]);

  const selectedPlaybook = useMemo(
    () => playbooks.find((playbook) => playbook.id === form.playbookId) ?? null,
    [form.playbookId, playbooks],
  );

  const steps = [
    {
      title: "What are you working on?",
      description: "Describe the founder problem in one sentence so we can point the research in the right direction.",
      content: (
        <label>
          <span className={styles.inputLabel}>What do you need help figuring out?</span>
          <textarea
            className={styles.textarea}
            value={form.focus}
            onChange={(event) => setForm((current) => ({ ...current, focus: event.target.value }))}
            placeholder="Example: I need to validate which buyer problem is urgent enough to build around."
          />
        </label>
      ),
      canContinue: Boolean(form.focus.trim()),
    },
    {
      title: "Pick a founder playbook",
      description: "Choose the type of help you want. Keep it simple and pick the closest fit.",
      content: loadingPlaybooks ? (
        <div className={styles.mutedText}>Loading playbooks...</div>
      ) : (
        <div className={styles.founderWizardChoiceGrid}>
          {playbooks.map((playbook) => {
            const selected = form.playbookId === playbook.id;
            return (
              <button
                key={playbook.id}
                type="button"
                className={`${styles.founderWizardChoice} ${selected ? styles.founderWizardChoiceSelected : ""}`}
                onClick={() => {
                  setForm((current) => ({
                    ...current,
                    playbookId: playbook.id,
                    focus: current.focus || playbook.starterQuestions?.[0] || current.focus,
                  }));
                }}
              >
                <div className={styles.founderWizardChoiceTitle}>{playbook.icon || "🔎"} {playbook.title}</div>
                <div className={styles.itemMeta}>{playbook.description || "No description available."}</div>
              </button>
            );
          })}
        </div>
      ),
      canContinue: Boolean(form.playbookId),
    },
    {
      title: "Tell us about the company",
      description: "Add the minimum context the research needs. Avoid jargon and write it like you’d explain it to a smart friend.",
      content: (
        <div className={styles.stack}>
          <label>
            <span className={styles.inputLabel}>Company name</span>
            <input
              className={styles.input}
              value={form.companyName}
              onChange={(event) => setForm((current) => ({ ...current, companyName: event.target.value }))}
            />
          </label>
          <label>
            <span className={styles.inputLabel}>Founder workspace name</span>
            <input
              className={styles.input}
              value={form.workspaceName}
              onChange={(event) => setForm((current) => ({ ...current, workspaceName: event.target.value }))}
            />
          </label>
          <label>
            <span className={styles.inputLabel}>Company or idea</span>
            <textarea
              className={styles.textarea}
              value={form.idea}
              onChange={(event) => setForm((current) => ({ ...current, idea: event.target.value }))}
              placeholder="What are you building, for whom, and why now?"
            />
          </label>
          <label>
            <span className={styles.inputLabel}>Stage</span>
            <select
              className={styles.select}
              value={form.stage}
              onChange={(event) => setForm((current) => ({ ...current, stage: event.target.value }))}
            >
              {STAGES.map((stage) => (
                <option key={stage.value} value={stage.value}>{stage.label}</option>
              ))}
            </select>
          </label>
        </div>
      ),
      canContinue: Boolean(form.companyName.trim() && form.idea.trim()),
    },
    {
      title: "Review and start",
      description: "This is the handoff into Deep Research. We’ll save the founder workspace and open the result flow with the right context.",
      content: (
        <div className={styles.founderWizardReviewCard}>
          <div className={styles.metricRow}><span>Goal</span><strong>{form.focus}</strong></div>
          <div className={styles.metricRow}><span>Playbook</span><strong>{selectedPlaybook?.title || form.playbookId}</strong></div>
          <div className={styles.metricRow}><span>Company</span><strong>{form.companyName}</strong></div>
          <div className={styles.metricRow}><span>Stage</span><strong>{form.stage}</strong></div>
          <div className={styles.metricRow}><span>Workspace</span><strong>{form.workspaceName}</strong></div>
        </div>
      ),
      canContinue: true,
    },
  ];

  const currentStep = steps[stepIdx];

  async function handlePrimaryAction() {
    if (stepIdx < steps.length - 1) {
      setStepIdx((current) => Math.min(current + 1, steps.length - 1));
      return;
    }

    setSaving(true);
    setError("");
    const workspaceId = founderWorkspace?.workspaceId || `client-${clientId}`;

    try {
      const response = await fetch("/api/founder/workspaces", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          workspaceId,
          clientId,
          name: form.workspaceName,
          companyName: form.companyName,
          idea: form.idea,
          stage: form.stage,
          playbookDefaults: { defaultPlaybookId: form.playbookId },
        }),
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(payload.error || "Could not save the founder workspace.");
      }

      router.push(`/deep-research?${new URLSearchParams({
        clientId,
        clientName,
        workspaceId,
        playbookId: form.playbookId,
        draftQuery: form.focus,
        autoStart: "1",
        from: "founder-wizard",
      }).toString()}`);
    } catch (event) {
      setError(event instanceof Error ? event.message : "Could not start founder research.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className={`${styles.card} ${styles.span12}`}>
      <div className={styles.sectionTitle}>Founder guided start</div>
      <div className={styles.sectionLead}>Follow four short steps to choose the right playbook, add company context, and start research without guessing what comes next.</div>

      <div className={styles.founderWizardProgress}>
        <div className={styles.founderWizardProgressBar}>
          <span
            className={
              stepIdx === 0
                ? styles.founderWizardProgressFillStep1
                : stepIdx === 1
                  ? styles.founderWizardProgressFillStep2
                  : stepIdx === 2
                    ? styles.founderWizardProgressFillStep3
                    : styles.founderWizardProgressFillStep4
            }
          />
        </div>
        <div className={styles.itemMeta}>Step {stepIdx + 1} of {steps.length}</div>
      </div>

      <div className={styles.founderWizardStepCard}>
        <div className={styles.founderWizardStepTitle}>{currentStep.title}</div>
        <div className={styles.sectionLead}>{currentStep.description}</div>
        {currentStep.content}
      </div>

      {error ? <div className={styles.errorBanner}>{error}</div> : null}

      <div className={styles.founderWizardActions}>
        <button
          type="button"
          className={styles.button}
          onClick={() => setStepIdx((current) => Math.max(current - 1, 0))}
          disabled={stepIdx === 0 || saving}
        >
          Back
        </button>
        <button
          type="button"
          className={`${styles.button} ${styles.buttonPrimary}`}
          onClick={handlePrimaryAction}
          disabled={!currentStep.canContinue || saving}
        >
          {stepIdx === steps.length - 1 ? (saving ? "Starting research..." : "Start research") : "Next"}
        </button>
      </div>
    </section>
  );
}