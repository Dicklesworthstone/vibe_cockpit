"use client";

import { useEffect, useState } from "react";
import { StatusBadge } from "@/components/StatusBadge";
import { api, type GuardianPlaybook, type GuardianRun } from "@/lib/api";

export default function GuardianPage() {
  const [playbooks, setPlaybooks] = useState<GuardianPlaybook[]>([]);
  const [runs, setRuns] = useState<GuardianRun[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([api.guardianPlaybooks(), api.guardianRuns()])
      .then(([pb, r]) => {
        setPlaybooks(pb.playbooks);
        setRuns(r.runs);
      })
      .catch((e) => setError(e.message));
  }, []);

  return (
    <div>
      <h1>Guardian</h1>
      {error && <p style={{ color: "#ef4444" }}>{error}</p>}

      <h2 style={{ fontSize: "16px", marginTop: "24px" }}>Playbooks</h2>
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "13px" }}>
        <thead>
          <tr style={{ borderBottom: "1px solid #374151", textAlign: "left" }}>
            <th style={{ padding: "8px" }}>ID</th>
            <th style={{ padding: "8px" }}>Name</th>
            <th style={{ padding: "8px" }}>Status</th>
          </tr>
        </thead>
        <tbody>
          {playbooks.map((p) => (
            <tr key={p.id} style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px", fontWeight: 600 }}>{p.id}</td>
              <td style={{ padding: "8px" }}>{p.name}</td>
              <td style={{ padding: "8px" }}>
                <StatusBadge
                  status={p.enabled ? "ok" : "offline"}
                  label={p.enabled ? "enabled" : "disabled"}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 style={{ fontSize: "16px", marginTop: "24px" }}>Recent Runs</h2>
      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: "13px" }}>
        <thead>
          <tr style={{ borderBottom: "1px solid #374151", textAlign: "left" }}>
            <th style={{ padding: "8px" }}>Run ID</th>
            <th style={{ padding: "8px" }}>Playbook</th>
            <th style={{ padding: "8px" }}>Status</th>
            <th style={{ padding: "8px" }}>Started</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((r) => (
            <tr key={r.id} style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px", fontWeight: 600 }}>{r.id}</td>
              <td style={{ padding: "8px" }}>{r.playbook_id}</td>
              <td style={{ padding: "8px" }}>
                <StatusBadge status={r.status === "completed" ? "ok" : r.status} label={r.status} />
              </td>
              <td style={{ padding: "8px", color: "#9ca3af" }}>{r.started_at}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
