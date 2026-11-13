// Tabla con filtro de texto, orden por columna y paginación. En pantallas angostas se muestra como tarjetas
// (sin desbordamiento horizontal).
import { type ReactNode, useId, useMemo, useState } from "react";
import { fold } from "../../lib/format";
import { Button } from "./Button";
import { EmptyState } from "./States";

export interface Column<T> {
  key: string;
  header: string;
  render: (row: T) => ReactNode;
  /** Valor para ordenar; si falta la columna no es ordenable. */
  sortValue?: (row: T) => string | number | null | undefined;
  /** Oculta la columna en la vista de tarjetas. */
  hideOnCard?: boolean;
  className?: string;
}

export interface DataTableProps<T> {
  rows: T[];
  columns: Column<T>[];
  rowKey: (row: T) => string;
  /** Texto en el que busca el filtro (sin acentos ni mayúsculas). */
  searchText?: (row: T) => string;
  filters?: ReactNode;
  pageSize?: number;
  caption: string;
  emptyTitle?: string;
  onRowClick?: (row: T) => void;
  initialSort?: { key: string; dir: "asc" | "desc" };
}

export function DataTable<T>({
  rows,
  columns,
  rowKey,
  searchText,
  filters,
  pageSize = 20,
  caption,
  emptyTitle = "Sin resultados",
  onRowClick,
  initialSort,
}: DataTableProps<T>) {
  const [q, setQ] = useState("");
  const [sort, setSort] = useState(initialSort ?? null);
  const [page, setPage] = useState(0);
  const searchId = useId();

  const filtered = useMemo(() => {
    const needle = fold(q.trim());
    let out = needle && searchText ? rows.filter((r) => fold(searchText(r)).includes(needle)) : rows;
    const col = sort && columns.find((c) => c.key === sort.key);
    if (col?.sortValue && sort) {
      const sv = col.sortValue;
      out = [...out].sort((a, b) => {
        const x = sv(a);
        const y = sv(b);
        if (x == null && y == null) return 0;
        if (x == null) return 1;
        if (y == null) return -1;
        const c =
          typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), "es");
        return sort.dir === "asc" ? c : -c;
      });
    }
    return out;
  }, [rows, q, sort, columns, searchText]);

  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const current = Math.min(page, pages - 1);
  const visible = filtered.slice(current * pageSize, current * pageSize + pageSize);

  const toggleSort = (key: string) => {
    setSort((s) => (s?.key === key ? { key, dir: s.dir === "asc" ? "desc" : "asc" } : { key, dir: "asc" }));
    setPage(0);
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-end gap-3">
        {searchText && (
          <div className="min-w-0 flex-1">
            <label htmlFor={searchId} className="mb-1 block text-sm font-medium">
              Filtrar
            </label>
            <input
              id={searchId}
              type="search"
              value={q}
              onChange={(e) => {
                setQ(e.target.value);
                setPage(0);
              }}
              placeholder="Escribe para filtrar…"
              className="w-full rounded-lg border border-line bg-surface px-3 py-2"
            />
          </div>
        )}
        {filters}
      </div>

      {filtered.length === 0 ? (
        <EmptyState title={emptyTitle} />
      ) : (
        <>
          {/* Escritorio / tablet: tabla */}
          <div className="hidden overflow-x-auto rounded-xl border border-line md:block">
            <table className="w-full border-collapse text-left text-sm">
              <caption className="sr-only">{caption}</caption>
              <thead className="bg-surface-2">
                <tr>
                  {columns.map((c) => (
                    <th
                      key={c.key}
                      scope="col"
                      className={`px-3 py-2 font-semibold ${c.className ?? ""}`}
                      aria-sort={
                        sort?.key === c.key ? (sort.dir === "asc" ? "ascending" : "descending") : undefined
                      }
                    >
                      {c.sortValue ? (
                        <button
                          type="button"
                          className="inline-flex items-center gap-1 underline-offset-2 hover:underline"
                          onClick={() => toggleSort(c.key)}
                        >
                          {c.header}
                          <span aria-hidden>
                            {sort?.key === c.key ? (sort.dir === "asc" ? "▲" : "▼") : "↕"}
                          </span>
                        </button>
                      ) : (
                        c.header
                      )}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {visible.map((r) => (
                  <tr
                    key={rowKey(r)}
                    className={`border-t border-line bg-surface ${onRowClick ? "cursor-pointer hover:bg-surface-2" : ""}`}
                    onClick={onRowClick ? () => onRowClick(r) : undefined}
                  >
                    {columns.map((c) => (
                      <td key={c.key} className={`px-3 py-2 align-top ${c.className ?? ""}`}>
                        {c.render(r)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {/* Móvil: tarjetas */}
          <ul className="space-y-2 md:hidden" aria-label={caption}>
            {visible.map((r) => (
              <li key={rowKey(r)}>
                <div className="rounded-xl border border-line bg-surface p-3">
                  <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
                    {columns
                      .filter((c) => !c.hideOnCard)
                      .map((c) => (
                        <div key={c.key} className="contents">
                          <dt className="text-muted">{c.header}</dt>
                          <dd className="min-w-0 break-words">{c.render(r)}</dd>
                        </div>
                      ))}
                  </dl>
                  {onRowClick && (
                    <Button
                      variant="secondary"
                      size="sm"
                      className="mt-2 w-full"
                      onClick={() => onRowClick(r)}
                    >
                      Abrir
                    </Button>
                  )}
                </div>
              </li>
            ))}
          </ul>
          {pages > 1 && (
            <nav className="flex items-center justify-between gap-2" aria-label="Paginación">
              <Button
                variant="secondary"
                size="sm"
                disabled={current === 0}
                onClick={() => setPage(current - 1)}
              >
                Anterior
              </Button>
              <span className="text-sm text-muted" aria-live="polite">
                Página {current + 1} de {pages} · {filtered.length} elementos
              </span>
              <Button
                variant="secondary"
                size="sm"
                disabled={current >= pages - 1}
                onClick={() => setPage(current + 1)}
              >
                Siguiente
              </Button>
            </nav>
          )}
        </>
      )}
    </div>
  );
}
