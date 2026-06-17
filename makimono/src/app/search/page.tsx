"use client";

import { useState } from "react";
import { SearchBar } from "@/components/search/search-bar";
import { SearchResults } from "@/components/search/search-results";
import { useSemanticSearch } from "@/hooks/use-api";

export default function SearchPage() {
  const [query, setQuery] = useState("");
  const { data, loading } = useSemanticSearch(query);

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <div>
        <h1 className="text-lg font-bold tracking-wide mb-1">
          🔍 Recherche Sémantique
        </h1>
        <p className="text-sm text-muted-foreground">
          Interrogez Tensai en langage naturel pour explorer le codebase via la recherche vectorielle RAG.
        </p>
      </div>

      <SearchBar
        value={query}
        onChange={setQuery}
        loading={loading}
        placeholder="Ex: fonctions qui gèrent l'authentification..."
      />

      <SearchResults
        results={data?.chunks ?? null}
        loading={loading && query.length >= 2}
        query={query}
      />
    </div>
  );
}
