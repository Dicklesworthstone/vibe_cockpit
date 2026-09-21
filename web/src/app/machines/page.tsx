"use client";

import { useEffect, useState } from "react";
import { StatusBadge } from "@/components/StatusBadge";
import { api, type Machine } from "@/lib/api";

export default function MachinesPage() {
  const [machines, setMachines] = useState<Machine[]>([]);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .machines()
      .then((d) => {
        setMachines(d.machines);
        setTotal(d.total);
      })
      .catch((e) => setError(e.message));
  }, []);

  return (
    <div>
      <h1>Machines ({total})</h1>
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
            <th style={{ padding: "8px" }}>ID</th>
            <th style={{ padding: "8px" }}>Hostname</th>
            <th style={{ padding: "8px" }}>Status</th>
            <th style={{ padding: "8px" }}>IP</th>
            <th style={{ padding: "8px" }}>Tags</th>
          </tr>
        </thead>
        <tbody>
          {machines.map((m) => (
            <tr key={m.machine_id} style={{ borderBottom: "1px solid #1f2937" }}>
              <td style={{ padding: "8px", fontWeight: 600 }}>{m.machine_id}</td>
              <td style={{ padding: "8px" }}>{m.hostname}</td>
              <td style={{ padding: "8px" }}>
                <StatusBadge status={m.status} />
              </td>
              <td style={{ padding: "8px", color: "#9ca3af" }}>{m.ip_address || "-"}</td>
              <td style={{ padding: "8px", color: "#9ca3af" }}>{m.tags || "-"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {machines.length === 0 && !error && (
        <p style={{ color: "#6b7280", textAlign: "center", padding: "24px" }}>No machines found</p>
      )}
    </div>
  );
}
