import type { ReactNode } from "react";
import "./globals.css";

export const metadata = {
  title: "Public Website",
  description: "Standalone public website connected to CTOX Business OS content."
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="de">
      <body>{children}</body>
    </html>
  );
}
