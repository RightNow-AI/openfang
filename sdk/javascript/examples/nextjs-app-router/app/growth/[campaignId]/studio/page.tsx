"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import VideoAdStudio from "../../components/VideoAdStudio";
import type { VideoAdProject } from "../../../../lib/mode-types";

type Props = { params: Promise<{ campaignId: string }> };

function makeBlankProject(campaignId: string): VideoAdProject {
  return {
    id:              `vp_${campaignId}`,
    campaign_id:     campaignId,
    title:           "Video Ad",
    step:            "brief",
    brief:           "",
    hook:            "",
    script:          "",
    shot_list:       "",
    asset_notes:     "",
    approval_status: "none",
    performance:     null,
    created_at:      new Date().toISOString(),
  };
}

export default function VideoAdStudioPage({ params }: Props) {
  const [campaignId, setCampaignId] = useState("");
  const [project, setProject]       = useState<VideoAdProject | null>(null);

  useEffect(() => {
    params.then(({ campaignId: cid }) => {
      setCampaignId(cid);
      // Local state only — persisting to backend can be added later via modes API meta patch
      setProject(makeBlankProject(cid));
    });
  }, [params]);

  if (!project) return <main style={{ padding: 24 }}>Loading…</main>;

  return (
    <main style={{ padding: "24px 32px", maxWidth: 860, margin: "0 auto" }}>
      <div style={{ marginBottom: 20 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 4 }}>
          <Link href={`/growth/${campaignId}`} style={{ fontSize: 13, color: "var(--text-muted, #888)" }}>← Campaign overview</Link>
        </div>
        <h1 style={{ fontSize: 22, fontWeight: 700 }}>🎬 Video Ad Studio</h1>
        <p style={{ color: "var(--text-muted, #888)", fontSize: 14, marginTop: 4 }}>
          Walk each video ad from brief to publish in 9 steps.
        </p>
      </div>
      <VideoAdStudio project={project} onUpdate={setProject} />
    </main>
  );
}
