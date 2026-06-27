"use client";

import { useState } from "react";
import { NodeCard } from "@/components/infrastructure/node-card";
import { SearchBar } from "@/components/search/search-bar";
import { SearchResults } from "@/components/search/search-results";
import { OperationCard } from "@/components/operations/operation-card";
import { Separator } from "@/components/ui/separator";
import { Skeleton } from "@/components/ui/skeleton";
import { useHealth, useOperations, useSemanticSearch } from "@/hooks/use-api";

// ── Infrastructure nodes config ──────────────────────────────

const INFRA_NODES = [
  {
    name: "Fūinjutsu",
    icon: "💾",
    technology: "pgvector/pgvector:pg17",
    port: 5433,
  },
  {
    name: "Fūinjutsu",
    icon: "🔴",
    technology: "Redis 8 Alpine",
    port: 6380,
  },
  {
    name: "Nen",
    icon: "📨",
    technology: "Apache Kafka (KRaft)",
    port: 9092,
  },
  {
    name: "Genjutsu",
    icon: "🐝",
    technology: "IPFS Kubo",
    port: 5001,
  },
];

// ═══════════════════════════════════════════════════════════════
// Dashboard Page — Main
// ═══════════════════════════════════════════════════════════════

export default function DashboardPage() {
  const [searchQuery, setSearchQuery] = useState("");

  const { data: health, loading: healthLoading } = useHealth();
  const { data: opsData, loading: opsLoading } = useOperations(5);
  const { data: searchData, loading: searchLoading } = useSemanticSearch(searchQuery);

  // Derive infrastructure status from health endpoint
  const infraStatus = health?.status === "operational" ? "online" : healthLoading ? "unknown" : "offline";

  return (
    <div className="max-w-5xl mx-auto space-y-8">
      {/* ── Zone 1 : Infrastructure Status ─────────────── */}
      <section>
        <div className="flex items-center gap-2 mb-4">
          <h2 className="text-sm font-semibold tracking-wide uppercase text-muted-foreground">
            Infrastructure
          </h2>
          <div className="flex-1 h-px bg-border" />
        </div>

        <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
          {INFRA_NODES.map((node, i) => (
            <NodeCard
              key={i}
              name={node.name}
              icon={node.icon}
              technology={node.technology}
              port={node.port}
              status={infraStatus as "online" | "offline" | "unknown"}
              className={`animate-fade-in-up stagger-${i + 1}`}
            />
          ))}
        </div>
      </section>

      <Separator />

      {/* ── Zone 2 : Recherche Tensai (RAG) ────────────── */}
      <section>
        <div className="flex items-center gap-2 mb-4">
          <h2 className="text-sm font-semibold tracking-wide uppercase text-muted-foreground">
            🧠 Recherche Sémantique Tensai
          </h2>
          <div className="flex-1 h-px bg-border" />
        </div>

        <SearchBar
          value={searchQuery}
          onChange={setSearchQuery}
          loading={searchLoading}
        />

        <SearchResults
          results={searchData?.chunks ?? null}
          loading={searchLoading && searchQuery.length >= 2}
          query={searchQuery}
        />
      </section>

      <Separator />

      {/* ── Zone 3 : Activité Récente ──────────────────── */}
      <section>
        <div className="flex items-center gap-2 mb-4">
          <h2 className="text-sm font-semibold tracking-wide uppercase text-muted-foreground">
            Activité Récente
          </h2>
          <div className="flex-1 h-px bg-border" />
        </div>

        {opsLoading ? (
          <div className="space-y-3">
            {[...Array(3)].map((_, i) => (
              <Skeleton key={i} className="h-20 w-full rounded-lg" />
            ))}
          </div>
        ) : opsData && opsData.operations.length > 0 ? (
          <div className="space-y-3">
            {opsData.operations.map((op, i) => (
              <OperationCard
                key={op.id}
                operation={op}
                className={`animate-fade-in-up stagger-${Math.min(i + 1, 5)}`}
              />
            ))}
            <p className="text-xs text-muted-foreground text-center mt-2">
              {opsData.count} opération{opsData.count > 1 ? "s" : ""} au total
            </p>
          </div>
        ) : (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
            <span className="text-4xl mb-3 opacity-30">📜</span>
            <p className="text-sm">Aucune opération enregistrée</p>
            <p className="text-xs mt-1 opacity-60">
              Utilisez POST /api/v1/operations pour créer votre première opération
            </p>
          </div>
        )}
      </section>
    </div>
  );
}
