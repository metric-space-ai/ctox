import "./globals.css";
import { BusinessApiPrefix } from "@/components/business-api-prefix";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <BusinessApiPrefix />
        {children}
      </body>
    </html>
  );
}
