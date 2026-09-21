import type { Metadata } from "next";
import { NavBar } from "@/components/NavBar";

export const metadata: Metadata = {
  title: "Vibe Cockpit",
  description: "Agent fleet monitoring and orchestration dashboard",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body
        style={{
          margin: 0,
          fontFamily: "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, monospace",
          backgroundColor: "#030712",
          color: "#e5e7eb",
          minHeight: "100vh",
        }}
      >
        <NavBar />
        <main style={{ padding: "24px", maxWidth: "1200px", margin: "0 auto" }}>{children}</main>
      </body>
    </html>
  );
}
