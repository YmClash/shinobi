"use client";

import { useMemo, useCallback, useState, useRef, useEffect } from "react";
import { useRouter, useParams } from "next/navigation";
import Link from "next/link";
import {
  ReactFlow,
  Background,
  Controls,
  MarkerType,
  type Node,
  type Edge,
  type NodeTypes,
  useNodesState,
  useEdgesState,
} from "@xyflow/react";
import dagre from "dagre";
import "@xyflow/react/dist/style.css";

import { useOperations, useRepositories } from "@/hooks/use-api";
import { buildRepoPrefix } from "@/lib/api";
import { CommitNode, type CommitNodeData } from "@/components/operations/commit-node";
import { ScoreSparkline } from "@/components/operations/score-sparkline";
import { Skeleton } from "@/components/ui/skeleton";
import type { Operation } from "@/lib/api";

// ── Dagre layout helper ─────────────────────────────

function getLayoutedElements(
  nodes: Node[],
  edges: Edge[],
  direction: "TB" | "LR" = "TB",
) {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({
    rankdir: direction,
    nodesep: 40,
    ranksep: 80,
    marginx: 30,
    marginy: 30,
  });

  for (const node of nodes) {
    g.setNode(node.id, { width: 260, height: 100 });
  }

  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }

  dagre.layout(g);

  const layoutedNodes = nodes.map((node) => {
    const nodeWithPosition = g.node(node.id);
    return {
      ...node,
      position: {
        x: nodeWithPosition.x - 130,
        y: nodeWithPosition.y - 50,
      },
    };
  });

  return { nodes: layoutedNodes, edges };
}

// ── Operations to React Flow conversion ─────────────

function operationsToFlow(operations: Operation[]): {
  nodes: Node[];
  edges: Edge[];
} {
  if (operations.length === 0) return { nodes: [], edges: [] };

  // Find the HEAD (latest by created_at)
  const sorted = [...operations].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );
  const headId = sorted[0]?.id;

  const nodes: Node[] = operations.map((op) => ({
    id: op.id,
    type: "commitNode",
    position: { x: 0, y: 0 }, // dagre will set this
    data: {
      id: op.id,
      description: op.description,
      authorId: op.author_id,
      createdAt: op.created_at,
      contentId: op.content_id,
      ipfsCid: op.ipfs_content_id,
      isHead: op.id === headId,
    } satisfies CommitNodeData,
  }));

  // Edges: each operation links to its parents
  const edges: Edge[] = [];
  for (const op of operations) {
    for (const parentId of op.parent_ids) {
      // Only create edge if parent exists in our data
      if (operations.some((o) => o.id === parentId)) {
        edges.push({
          id: `${parentId}->${op.id}`,
          source: parentId,
          target: op.id,
          type: "smoothstep",
          animated: true,
          markerEnd: {
            type: MarkerType.ArrowClosed,
            width: 20,
            height: 20,
            color: "#10b981",
          },
          style: {
            stroke: "#10b981",
            strokeWidth: 2.5,
          },
        });
      }
    }
  }

  return getLayoutedElements(nodes, edges);
}

// ── Node types registry ─────────────────────────────

const nodeTypes: NodeTypes = {
  commitNode: CommitNode,
};

// ── Repo Selector Component ─────────────────────────

function RepoSelector({ currentOwner, currentRepo }: { currentOwner: string; currentRepo: string }) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const { data: reposData } = useRepositories(currentOwner);
  const repos = reposData?.repositories ?? [];

  // Close on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as globalThis.Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  if (repos.length <= 1) return null;

  return (
    <div className="repo-selector" ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        className="repo-breadcrumb cursor-pointer hover:bg-accent"
      >
        <span>📦</span>
        <span>{currentOwner}/{currentRepo}</span>
        <span className="text-[8px] opacity-50">{open ? "▲" : "▼"}</span>
      </button>

      {open && (
        <div className="repo-selector-menu">
          {repos.map((repo) => {
            const isActive = repo.name === currentRepo;
            return (
              <button
                key={repo.id}
                onClick={() => {
                  setOpen(false);
                  if (!isActive) {
                    router.push(`/${currentOwner}/${repo.name}/operations`);
                  }
                }}
                className={`repo-selector-item w-full text-left ${isActive ? "active" : ""}`}
              >
                <span className="text-sm">📦</span>
                <span className="repo-selector-item-name">{repo.name}</span>
                {repo.description && (
                  <span className="repo-selector-item-desc">{repo.description}</span>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Page Component ──────────────────────────────────

export default function OperationsPage() {
  const params = useParams();
  const owner = params.owner as string;
  const repo = params.repo as string;
  const repoPrefix = buildRepoPrefix(owner, repo);

  const { data, loading, error } = useOperations(repoPrefix, 100);
  const router = useRouter();

  const { nodes: initialNodes, edges: initialEdges } = useMemo(
    () => operationsToFlow(data?.operations ?? []),
    [data],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

  // Sync ReactFlow internal state when data changes (async fetch)
  useEffect(() => {
    setNodes(initialNodes);
  }, [initialNodes, setNodes]);

  useEffect(() => {
    setEdges(initialEdges);
  }, [initialEdges, setEdges]);

  // Navigate to detail page on node click
  const onNodeClick = useCallback(
    (_event: React.MouseEvent, node: Node) => {
      router.push(`/${owner}/${repo}/operations/${node.id}`);
    },
    [router, owner, repo],
  );

  if (error) {
    return (
      <div className="max-w-4xl mx-auto">
        <h1 className="text-lg font-bold tracking-wide mb-4">
          📜 Graphe VCS Jujutsu
        </h1>
        <div className="text-sm text-destructive bg-destructive/10 rounded-lg p-4">
          Erreur de connexion au backend : {error}
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="max-w-4xl mx-auto">
        <h1 className="text-lg font-bold tracking-wide mb-4">
          📜 Graphe VCS Jujutsu
        </h1>
        <Skeleton className="w-full h-[500px] rounded-lg" />
      </div>
    );
  }

  const operations = data?.operations ?? [];

  if (operations.length === 0) {
    return (
      <div className="max-w-4xl mx-auto">
        <h1 className="text-lg font-bold tracking-wide mb-4">
          📜 Graphe VCS Jujutsu
        </h1>
        <div className="flex flex-col items-center justify-center py-20 text-muted-foreground">
          <span className="text-6xl mb-4 opacity-30">🌳</span>
          <p className="text-sm">Aucune opération enregistrée</p>
          <p className="text-xs mt-2 opacity-60">
            Utilisez la <code className="font-mono bg-muted px-1.5 py-0.5 rounded">Command Room</code> ou l&apos;API fédérée pour créer votre première opération
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-lg font-bold tracking-wide">
            🌳 Graphe VCS Jujutsu
          </h1>
          <RepoSelector currentOwner={owner} currentRepo={repo} />
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-muted-foreground">
            {operations.length} opération{operations.length > 1 ? "s" : ""} · Cliquez sur un nœud pour voir les détails
          </span>
          <Link
            href={`/${owner}/${repo}/operations/new`}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg
                       bg-primary text-primary-foreground hover:bg-primary/90
                       transition-all hover:shadow-lg hover:shadow-primary/20 hover:scale-105 active:scale-95"
          >
            <span>⚔️</span>
            <span>Nouvelle Opération</span>
          </Link>
        </div>
      </div>

      {/* ── Sparkline ECG (Phase 9.2) ────────────── */}
      <ScoreSparkline />

      <div
        className="rounded-lg border border-border/50 overflow-hidden glass-card"
        style={{ height: "calc(100vh - 200px)", minHeight: "400px" }}
      >
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          nodeTypes={nodeTypes}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          proOptions={{ hideAttribution: true }}
          className="bg-transparent"
        >
          <Background
            gap={20}
            size={1}
            color="hsl(var(--muted-foreground) / 0.07)"
          />
          <Controls
            className="!bg-card !border-border !shadow-lg"
            showInteractive={false}
          />
        </ReactFlow>
      </div>
    </div>
  );
}
