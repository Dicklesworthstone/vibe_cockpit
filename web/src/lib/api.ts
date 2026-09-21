/** API client for vc_web JSON endpoints */

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";

export interface FleetOverview {
  total_machines: number;
  online_machines: number;
  offline_machines: number;
  fleet_health: number;
  active_alerts: number;
  pending_approvals: number;
}

export interface Machine {
  machine_id: string;
  hostname: string;
  status: string;
  ip_address?: string;
  tags?: string;
  enabled?: boolean;
}

export interface HealthScore {
  machine_id: string;
  score: number;
  factors: Record<string, number>;
}

export interface Alert {
  id: string;
  severity: string;
  machine: string;
  message: string;
  acknowledged: boolean;
  created_at: string;
}

export interface GuardianPlaybook {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
}

export interface GuardianRun {
  id: string;
  playbook_id: string;
  status: string;
  started_at: string;
  completed_at?: string;
}

export interface HealthResponse {
  status: string;
  version: string;
  uptime_secs: number;
}

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`);
  if (!res.ok) {
    throw new Error(`API error: ${res.status} ${res.statusText}`);
  }
  return res.json();
}

export const api = {
  health: () => fetchJson<HealthResponse>("/api/health"),
  overview: () => fetchJson<FleetOverview>("/api/overview"),
  fleet: () => fetchJson<FleetOverview>("/api/fleet"),
  machines: (limit = 50, offset = 0) =>
    fetchJson<{ machines: Machine[]; total: number }>(
      `/api/machines?limit=${limit}&offset=${offset}`,
    ),
  machine: (id: string) => fetchJson<Machine>(`/api/machines/${id}`),
  machineHealth: (id: string) => fetchJson<HealthScore>(`/api/machines/${id}/health`),
  alerts: (limit = 50) =>
    fetchJson<{ alerts: Alert[]; limit: number }>(`/api/alerts?limit=${limit}`),
  guardianPlaybooks: () => fetchJson<{ playbooks: GuardianPlaybook[] }>("/api/guardian/playbooks"),
  guardianRuns: () => fetchJson<{ runs: GuardianRun[] }>("/api/guardian/runs"),
};
