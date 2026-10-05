import { NextRequest, NextResponse } from "next/server";

// ── IPFS Proxy — Anti-CORS Gateway (Phase 28C) ───────────────────
// Proxies requests to the local Kubo IPFS gateway to avoid CORS issues.
// Route: GET /api/ipfs/[cid]

const IPFS_GATEWAY = process.env.IPFS_GATEWAY_URL || "http://127.0.0.1:8080";

export async function GET(
  _request: NextRequest,
  { params }: { params: Promise<{ cid: string }> },
) {
  const { cid } = await params;

  if (!cid || cid.length < 10) {
    return NextResponse.json({ error: "Invalid CID" }, { status: 400 });
  }

  try {
    const ipfsRes = await fetch(`${IPFS_GATEWAY}/ipfs/${cid}`, {
      signal: AbortSignal.timeout(30_000), // 30s timeout
    });

    if (!ipfsRes.ok) {
      return NextResponse.json(
        { error: `IPFS gateway returned ${ipfsRes.status}` },
        { status: ipfsRes.status },
      );
    }

    const contentType = ipfsRes.headers.get("content-type") || "application/octet-stream";
    const body = await ipfsRes.arrayBuffer();

    return new NextResponse(body, {
      status: 200,
      headers: {
        "Content-Type": contentType,
        "Cache-Control": "public, max-age=31536000, immutable", // CIDs are content-addressed
      },
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : "IPFS fetch failed";
    return NextResponse.json({ error: message }, { status: 502 });
  }
}
