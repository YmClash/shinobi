import type { NextConfig } from "next";

// Backend URL: configurable via env var pour Docker (http://taijutsu:3000)
// En dev local: http://localhost:3000 (défaut)
const taijutsuUrl = process.env.TAIJUTSU_URL || "http://localhost:3000";

const nextConfig: NextConfig = {
  // Mode standalone : produit un serveur autonome sans node_modules
  // Requis pour le Dockerfile multi-stage (Stage 3 runtime slim)
  output: "standalone",

  // Timeout HTTP étendu pour le streaming SSE (Sensei chat) et
  // le chargement initial du modèle Ollama (~30s cold start).
  httpAgentOptions: {
    keepAlive: true,
  },

  // Proxy API requests to the Taijutsu backend
  // En dev: localhost:3000 | En Docker: http://taijutsu:3000 (réseau shinobi)
  //
  // IMPORTANT: /api/ipfs/* est géré par le route handler Next.js local
  // (app/api/ipfs/[cid]/route.ts — proxy IPFS anti-CORS).
  // On utilise un regex source pour exclure /api/ipfs du proxy Taijutsu.
  async rewrites() {
    return [
      {
        // Match /api/* SAUF /api/ipfs/*
        source: "/api/:path((?!ipfs).*)",
        destination: `${taijutsuUrl}/api/:path*`,
      },
      {
        source: "/health",
        destination: `${taijutsuUrl}/health`,
      },
      {
        source: "/metrics",
        destination: `${taijutsuUrl}/metrics`,
      },
    ];
  },
};

export default nextConfig;
