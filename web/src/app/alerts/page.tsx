"use client";

import { useEffect, useState } from "react";
import { StatusBadge } from "@/components/StatusBadge";
import { type Alert, api } from "@/lib/api";

export default function AlertsPage() {
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .alerts()
      .then((d) => setAlerts(d.alerts))
      .catch((e) => setError(e.message));
  }, []);

  return (
    <div>
      <h1>Alerts</h1>
      {error && <p style={{ color: "#ef4444" }}>{error}</p>}
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          fontSize: "13px",
        }}
      >
        <thead>
          <tr style={{ borderBottom: "1px solid #374151", textAlign: "left" }}>
            <th style={{ padding: "8px" }}>Severity</th>
            <th style={{ padding: "8px" }}>Machine</th>
            <th style={{ padding: "8px" }}>Message</th>
            <th style={{ padding: "8px" }}>Time</th>
            <th style={{ padding: "8px" }}>Acked</th>
          </tr>
        </thead>
        <tbody>
          {alerts.map((a) => (
            <tr key={a.id} style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px" }}>
                <StatusBadge status={a.severity} />
              </td>
              <td style={{ padding: "8px" }}>{a.machine}</td>
              <td style={{ padding: "8px" }}>{a.message}</td>
              <td style={{ padding: "8px", color: "#9ca3af" }}>{a.created_at}</td>
              <td style={{ padding: "8px" }}>{a.acknowledged ? "Yes" : "No"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {alerts.length === 0 && !error && (
        <p style={{ color: "#6b7280", textAlign: "center", padding: "24px" }}>No alerts</p>
      )}
    </div>
  );
}
