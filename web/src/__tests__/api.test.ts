import { api } from "@/lib/api";

// Mock global fetch
const mockFetch = jest.fn();
global.fetch = mockFetch;

beforeEach(() => {
  mockFetch.mockClear();
});

describe("api client", () => {
  it("fetches health endpoint", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ status: "ok", version: "0.1.0", uptime_secs: 42 }),
    });
    const result = await api.health();
    expect(result.status).toBe("ok");
    expect(result.uptime_secs).toBe(42);
    expect(mockFetch).toHaveBeenCalledWith("/api/health");
  });

  it("fetches fleet overview", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          total_machines: 4,
          online_machines: 3,
          offline_machines: 1,
          fleet_health: 0.85,
          active_alerts: 2,
          pending_approvals: 0,
        }),
    });
    const result = await api.fleet();
    expect(result.total_machines).toBe(4);
    expect(result.fleet_health).toBe(0.85);
  });

  it("fetches machines with pagination", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          machines: [{ machine_id: "m1", hostname: "h1", status: "online" }],
          total: 1,
        }),
    });
    const result = await api.machines(10, 0);
    expect(result.machines).toHaveLength(1);
    expect(mockFetch).toHaveBeenCalledWith("/api/machines?limit=10&offset=0");
  });

  it("throws on API error", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: "Internal Server Error",
    });
    await expect(api.health()).rejects.toThrow("API error: 500");
  });

  it("fetches alerts", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () =>
        Promise.resolve({
          alerts: [
            {
              id: "a1",
              severity: "critical",
              machine: "m1",
              message: "CPU high",
              acknowledged: false,
              created_at: "2026-01-01",
            },
          ],
          limit: 50,
        }),
    });
    const result = await api.alerts();
    expect(result.alerts).toHaveLength(1);
    expect(result.alerts[0].severity).toBe("critical");
  });
});
