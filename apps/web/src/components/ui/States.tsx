import type { ReactNode } from "react";
import { errorMessage } from "../../lib/errors";
import { Button } from "./Button";

export function EmptyState({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="rounded-xl border border-dashed border-line bg-surface p-6 text-center">
      <p className="font-semibold">{title}</p>
      {children && <div className="mt-1 text-sm text-muted">{children}</div>}
      {action && <div className="mt-4 flex justify-center">{action}</div>}
    </div>
  );
}

export function LoadingState({ label = "Cargando…", rows = 3 }: { label?: string; rows?: number }) {
  return (
    <div role="status" aria-live="polite" aria-busy="true" className="space-y-3">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: esqueleto estático
        <div key={i} className="h-14 animate-pulse rounded-xl bg-surface-2" />
      ))}
    </div>
  );
}

export function ErrorState({
  error,
  onRetry,
  title = "No se pudo cargar",
}: {
  error: unknown;
  onRetry?: () => void;
  title?: string;
}) {
  return (
    <div role="alert" className="rounded-xl border border-danger/40 bg-surface p-4">
      <p className="font-semibold text-danger">{title}</p>
      <p className="mt-1 text-sm">{errorMessage(error)}</p>
      {onRetry && (
        <Button variant="secondary" size="sm" className="mt-3" onClick={onRetry}>
          Reintentar
        </Button>
      )}
    </div>
  );
}

/** Contenedor para queries de TanStack: carga / error / vacío / datos. */
export function QueryState<T>({
  query,
  empty,
  isEmpty,
  children,
  loadingRows,
}: {
  query: {
    data: T | undefined;
    isPending: boolean;
    isError: boolean;
    error: unknown;
    refetch: () => unknown;
  };
  empty?: ReactNode;
  isEmpty?: (d: T) => boolean;
  children: (d: T) => ReactNode;
  loadingRows?: number;
}) {
  if (query.isPending) return <LoadingState rows={loadingRows ?? 3} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => query.refetch()} />;
  const d = query.data as T;
  if (empty && isEmpty?.(d)) return <>{empty}</>;
  return <>{children(d)}</>;
}
