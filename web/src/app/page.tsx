"use client";

import { useEffect, useState } from "react";
import { HealthCard } from "@/components/HealthCard";
import { api, type FleetOverview } from "@/lib/api";

export default function OverviewPage() {
  const [data, setData] = useState<FleetOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<string>("");

  useEffect(() => {
    api
      .fleet()
      .then((d) => {
        setData(d);
        setLastUpdated(new Date().toLocaleTimeString());
      })
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div>
        <h1>Overview</h1>
        <p style={{ color: "#ef4444" }}>Failed to load fleet data: {error}</p>
        <p style={{ color: "#6b7280", fontSize: "12px" }}>
          Ensure vc web server is running (vc web)
        </p>
      </div>
    );
  }

  if (!data) {
    return (
      <div>
        <h1>Overview</h1>
        <div style={{ color: "#6b7280" }}>Loading fleet data...</div>
      </div>
    );
  }

  const healthPct = Math.round(data.fleet_health * 100);
  const healthColor = healthPct >= 85 ? "#16a34a" : healthPct >= 70 ? "#d97706" : "#dc2626";

  return (
    <div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
        }}
      >
        <h1>Fleet Overview</h1>
        {lastUpdated && (
          <span style={{ fontSize: "12px", color: "#6b7280" }}>Updated {lastUpdated}</span>
        )}
      </div>
      <div style={{ display: "flex", gap: "16px", flexWrap: "wrap" }}>
        <HealthCard title="Fleet Health" value={`${healthPct}%`} color={healthColor} />
        <HealthCard
          title="Machines"
          value={`${data.online_machines}/${data.total_machines}`}
          subtitle="online"
          color="#3b82f6"
        />
        <HealthCard
          title="Active Alerts"
          value={data.active_alerts}
          color={data.active_alerts > 0 ? "#ea580c" : "#16a34a"}
        />
        <HealthCard
          title="Pending Approvals"
          value={data.pending_approvals}
          color={data.pending_approvals > 0 ? "#d97706" : "#16a34a"}
        />
      </div>
    </div>
  );
}
