"use client";

import { OperationCard } from "@/components/operations/operation-card";
import { Skeleton } from "@/components/ui/skeleton";
import { useOperations } from "@/hooks/use-api";

export default function OperationsPage() {
  const { data, loading, error } = useOperations(50);

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <div>
        <h1 className="text-lg font-bold tracking-wide mb-1">
          📜 Opérations VCS
        </h1>
        <p className="text-sm text-muted-foreground">
          Historique des opérations de versioning enregistrées dans Taijutsu.
        </p>
      </div>

      {error && (
        <div className="text-sm text-destructive bg-destructive/10 rounded-lg p-3">
          Erreur de connexion au backend : {error}
        </div>
      )}

      {loading ? (
        <div className="space-y-3">
          {[...Array(5)].map((_, i) => (
            <Skeleton key={i} className="h-24 w-full rounded-lg" />
          ))}
        </div>
      ) : data && data.operations.length > 0 ? (
        <div className="space-y-3">
          <p className="text-xs text-muted-foreground">
            {data.count} opération{data.count > 1 ? "s" : ""}
          </p>
          {data.operations.map((op, i) => (
            <OperationCard
              key={op.id}
              operation={op}
              className={`animate-fade-in-up stagger-${Math.min(i + 1, 5)}`}
            />
          ))}
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
          <span className="text-5xl mb-4 opacity-30">📜</span>
          <p className="text-sm">Aucune opération enregistrée</p>
          <p className="text-xs mt-1 opacity-60">
            Utilisez <code className="font-mono bg-muted px-1 rounded">POST /api/v1/operations</code> pour créer votre première opération
          </p>
        </div>
      )}
    </div>
  );
}
