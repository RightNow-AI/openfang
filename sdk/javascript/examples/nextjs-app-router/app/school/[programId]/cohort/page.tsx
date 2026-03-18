"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import StudentHealthDashboard from "../../components/StudentHealthDashboard";
import type { StudentHealthRecord } from "../../../../lib/mode-types";

const MOCK_STUDENTS: StudentHealthRecord[] = [
  {
    student_id: "alice-j",
    program_id: "",
    attendance_pct: 94,
    assignment_completion_pct: 88,
    community_participation_score: 7.2,
    support_requests: 0,
    risk_score: 12,
    upsell_readiness: 85,
    last_seen: "2 days ago",
    flags: [],
  },
  {
    student_id: "brian-t",
    program_id: "",
    attendance_pct: 61,
    assignment_completion_pct: 45,
    community_participation_score: 3.0,
    support_requests: 2,
    risk_score: 74,
    upsell_readiness: 18,
    last_seen: "6 days ago",
    flags: [{ type: "at_risk", note: "Low attendance and late submissions" }],
  },
  {
    student_id: "clara-n",
    program_id: "",
    attendance_pct: 88,
    assignment_completion_pct: 91,
    community_participation_score: 6.6,
    support_requests: 0,
    risk_score: 20,
    upsell_readiness: 73,
    last_seen: "1 day ago",
    flags: [{ type: "upsell_ready", note: "High engagement and completion" }],
  },
  {
    student_id: "david-p",
    program_id: "",
    attendance_pct: 72,
    assignment_completion_pct: 59,
    community_participation_score: 4.4,
    support_requests: 1,
    risk_score: 51,
    upsell_readiness: 40,
    last_seen: "3 days ago",
    flags: [{ type: "at_risk", note: "Borderline completion rate" }],
  },
  {
    student_id: "elena-m",
    program_id: "",
    attendance_pct: 97,
    assignment_completion_pct: 95,
    community_participation_score: 8.8,
    support_requests: 0,
    risk_score: 5,
    upsell_readiness: 92,
    last_seen: "today",
    flags: [{ type: "star_student", note: "Exceptional across all metrics" }],
  },
];

export default function CohortPage() {
  const { programId } = useParams<{ programId: string }>();

  return (
    <div style={{ padding: 32, maxWidth: 900, margin: "0 auto" }}>
      <div style={{ marginBottom: 12 }}>
        <Link href={`/school/${programId}`} style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>
          ← Program Overview
        </Link>
      </div>
      <h1 style={{ fontSize: 22, fontWeight: 700, marginBottom: 4 }}>Cohort Health Dashboard</h1>
      <p style={{ fontSize: 13, color: "var(--text-muted, #888)", marginBottom: 24 }}>
        Real-time learner engagement, risk scoring, and upsell readiness — sorted by risk.
      </p>
      <StudentHealthDashboard students={MOCK_STUDENTS} />
    </div>
  );
}
