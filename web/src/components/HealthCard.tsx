"use client";

interface HealthCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  color?: string;
}

export function HealthCard({ title, value, subtitle, color }: HealthCardProps) {
  const borderColor = color || "#3b82f6";
  return (
    <div
      style={{
        border: `2px solid ${borderColor}`,
        borderRadius: "8px",
        padding: "16px",
        minWidth: "160px",
      }}
    >
      <div style={{ fontSize: "12px", color: "#9ca3af", textTransform: "uppercase" }}>{title}</div>
      <div style={{ fontSize: "28px", fontWeight: 700, marginTop: "4px" }}>{value}</div>
      {subtitle && (
        <div style={{ fontSize: "12px", color: "#6b7280", marginTop: "2px" }}>{subtitle}</div>
      )}
    </div>
  );
}
