"use client";

import { useEffect, useState } from "react";
import { api, type HealthResponse } from "@/lib/api";

export default function SettingsPage() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .health()
      .then(setHealth)
      .catch((e) => setError(e.message));
  }, []);

  return (
    <div>
      <h1>Settings</h1>

      <h2 style={{ fontSize: "16px", marginTop: "24px" }}>Server Info</h2>
      {error && <p style={{ color: "#ef4444" }}>{error}</p>}
      {health && (
        <table
          style={{
            borderCollapse: "collapse",
            fontSize: "13px",
          }}
        >
          <tbody>
            <tr style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px", color: "#9ca3af" }}>Status</td>
              <td style={{ padding: "8px" }}>{health.status}</td>
            </tr>
            <tr style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px", color: "#9ca3af" }}>Version</td>
              <td style={{ padding: "8px" }}>{health.version}</td>
            </tr>
            <tr style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px", color: "#9ca3af" }}>Uptime</td>
              <td style={{ padding: "8px" }}>{health.uptime_secs}s</td>
            </tr>
          </tbody>
        </table>
      )}
    </div>
  );
}
