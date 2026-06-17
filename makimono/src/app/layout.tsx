import type { Metadata } from "next";
import { Inter, JetBrains_Mono } from "next/font/google";
import { AppShell } from "@/components/layout/app-shell";
import "./globals.css";

const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
  display: "swap",
});

const jetbrainsMono = JetBrains_Mono({
  variable: "--font-jetbrains",
  subsets: ["latin"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Shinobi — Dashboard de l'Architecte",
  description:
    "Makimono — Interface d'exploration sémantique du VCS Next-Gen SHINOBI. Recherche RAG, monitoring d'infrastructure, visualisation de code.",
  keywords: ["shinobi", "vcs", "rag", "semantic-search", "makimono"],
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="fr"
      data-theme="ninja"
      className={`${inter.variable} ${jetbrainsMono.variable} h-full antialiased`}
      suppressHydrationWarning
    >
      <body className="min-h-full flex">
        <AppShell>{children}</AppShell>
      </body>
    </html>
  );
}
