import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Proxy API requests to the Taijutsu backend (port 3000)
  // This avoids CORS issues in development
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: "http://localhost:3000/api/:path*",
      },
      {
        source: "/health",
        destination: "http://localhost:3000/health",
      },
      {
        source: "/metrics",
        destination: "http://localhost:3000/metrics",
      },
    ];
  },
};

export default nextConfig;
