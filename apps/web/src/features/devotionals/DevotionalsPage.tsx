// Devocionales (Devocionales.base: tabla por fecha desc con pasaje) + editor por fecha con plantilla personalizable
// (o meditación). Un devocional con contenido real marca el hábito vinculado; vaciarlo lo desmarca (servidor).
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Button } from "../../components/ui/Button";
import { DatePicker } from "../../components/ui/DatePicker";
import { ConfirmDialog } from "../../components/ui/Dialog";
import { Card, PageHeader } from "../../components/ui/Display";
import { TextArea, TextField } from "../../components/ui/Field";
import { EmptyState, QueryState } from "../../components/ui/States";
import { useToast } from "../../components/ui/Toast";
import { api, type Schemas } from "../../lib/api";
import { ApiError, errorMessage, unwrap } from "../../lib/errors";
import { formatLongDate, todayIso } from "../../lib/format";
import { isValidIsoDate } from "../../lib/telegram";
import { useAuth } from "../auth/AuthProvider";

type Devotional = Schemas["Devotional"];
type Section = Schemas["TemplateSection"];
interface VerseAction {
  verse: string;
  action: string;
}

const FIXED_KEYS = new Set(["passage", "verse_actions", "learned", "teaching", "prayer", "song"]);

export function DevotionalsPage() {
  const { me } = useAuth();
  const today = todayIso(me?.settings.timezone ?? "America/Monterrey");
  const [q, setQ] = useState("");
  const [offset, setOffset] = useState(0);
  const limit = 30;
  const list = useQuery({
    queryKey: ["devotionals", q, offset],
    queryFn: async () =>
      unwrap(await api.GET("/devotionals", { params: { query: { q: q || null, limit, offset } } })),
  });
  const label = me?.settings.devotional_mode === "meditation" ? "Meditaciones" : "Devocionales";
  return (
    <div className="space-y-4">
      <PageHeader
        title={label}
        actions={
          <Link to={`/devocionales/${today}`} className="font-semibold">
            Escribir el de hoy →
          </Link>
        }
      />
      <TextField
        label="Buscar (sin importar acentos)"
        type="search"
        value={q}
        onChange={(e) => {
          setQ(e.target.value);
          setOffset(0);
        }}
        placeholder="Pasaje, aprendizaje, oración…"
      />
      <QueryState
        query={list}
        isEmpty={(d) => d.items.length === 0}
        empty={
          <EmptyState
            title={q ? "Sin resultados" : "Aún no hay registros"}
            action={<Link to={`/devocionales/${today}`}>Escribir el de hoy</Link>}
          />
        }
      >
        {(page) => (
          <Card>
            <ul className="divide-y divide-line">
              {page.items.map((d) => (
                <li key={d.id} className="flex flex-wrap gap-x-3 py-2">
                  <Link to={`/devocionales/${d.on_date}`} className="w-28 shrink-0 tabular-nums">
                    {d.on_date}
                  </Link>
                  <span className="min-w-0 flex-1 break-words">
                    {d.passage || <span className="text-muted">(sin pasaje)</span>}
                  </span>
                  {d.mode === "meditation" && <span className="text-xs text-muted">meditación</span>}
                </li>
              ))}
            </ul>
            <div className="mt-3 flex items-center justify-between text-sm">
              <span className="text-muted">
                {page.offset + 1}–{page.offset + page.items.length} de {page.total}
              </span>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={offset === 0}
                  onClick={() => setOffset(Math.max(0, offset - limit))}
                >
                  Anteriores
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={offset + limit >= page.total}
                  onClick={() => setOffset(offset + limit)}
                >
                  Siguientes
                </Button>
              </div>
            </div>
          </Card>
        )}
      </QueryState>
    </div>
  );
}

export function DevotionalEditorPage() {
  const { date: raw } = useParams();
  const { me } = useAuth();
  const today = todayIso(me?.settings.timezone ?? "America/Monterrey");
  const date = isValidIsoDate(raw ?? null) ? (raw as string) : today;
  const navigate = useNavigate();
  const template = useQuery({
    queryKey: ["devotional-template"],
    queryFn: async () => unwrap(await api.GET("/devotionals/template")),
  });
  const current = useQuery({
    queryKey: ["devotional", date],
    queryFn: async () => {
      const r = await api.GET("/devotionals/{date}", { params: { path: { date } } });
      if (r.response.status === 404) return null;
      return unwrap(r);
    },
  });
  return (
    <div className="space-y-4">
      <p>
        <Link to="/devocionales">← Historial</Link>
      </p>
      <PageHeader
        title={template.data?.mode === "meditation" ? "Meditación" : "Devocional"}
        subtitle={<span className="capitalize">{formatLongDate(date)}</span>}
        actions={<DatePicker value={date} max={today} onChange={(d) => navigate(`/devocionales/${d}`)} />}
      />
      {template.data && !template.data.enabled && (
        <p className="rounded-xl border border-warn p-3 text-sm">
          El módulo está desactivado en <Link to="/ajustes">Ajustes</Link>; aún puedes consultar tu historial.
        </p>
      )}
      {template.isPending || current.isPending ? null : template.isError || current.isError ? (
        <p className="text-danger">No se pudo cargar.</p>
      ) : (
        <Editor
          key={`${date}-${current.data?.version ?? 0}`}
          date={date}
          sections={template.data?.sections ?? []}
          mode={template.data?.mode ?? "devotional"}
          d={current.data ?? null}
        />
      )}
    </div>
  );
}

function Editor({
  date,
  sections,
  mode,
  d,
}: {
  date: string;
  sections: Section[];
  mode: string;
  d: Devotional | null;
}) {
  const qc = useQueryClient();
  const toast = useToast();
  const [passage, setPassage] = useState(d?.passage ?? "");
  const [verses, setVerses] = useState<VerseAction[]>(() => {
    const v = (d?.verse_actions ?? []) as unknown as VerseAction[];
    return v.length
      ? v.map((x) => ({ verse: x.verse ?? "", action: x.action ?? "" }))
      : [{ verse: "", action: "" }];
  });
  const [texts, setTexts] = useState<Record<string, string>>(() => ({
    learned: d?.learned ?? "",
    teaching: d?.teaching ?? "",
    prayer: d?.prayer ?? "",
    song: d?.song ?? "",
    ...((d?.extra ?? {}) as unknown as Record<string, string>),
  }));
  const [confirmDelete, setConfirmDelete] = useState(false);
  const invalidate = () => {
    void qc.invalidateQueries({ queryKey: ["devotional", date] });
    void qc.invalidateQueries({ queryKey: ["devotionals"] });
    void qc.invalidateQueries({ queryKey: ["day", date] });
    void qc.invalidateQueries({ queryKey: ["habits"] });
    void qc.invalidateQueries({ queryKey: ["dashboard"] });
  };
  const save = useMutation({
    mutationFn: async () => {
      const extra: Record<string, string> = {};
      for (const s of sections) if (!FIXED_KEYS.has(s.key)) extra[s.key] = texts[s.key] ?? "";
      return unwrap(
        await api.PUT("/devotionals/{date}", {
          params: { path: { date } },
          body: {
            mode,
            passage,
            verse_actions: verses.filter((v) => v.verse.trim() || v.action.trim()),
            learned: texts.learned ?? "",
            teaching: texts.teaching ?? "",
            prayer: texts.prayer ?? "",
            song: texts.song ?? "",
            extra: extra as unknown as Record<string, never>,
            version: d?.version ?? 0,
          },
        }),
      );
    },
    onSuccess: () => {
      toast.show("Guardado", { tone: "ok" });
      invalidate();
    },
    onError: (e) =>
      toast.show(
        e instanceof ApiError && e.isConflict
          ? "Se editó en otro dispositivo: recarga para ver la versión actual"
          : errorMessage(e),
        { tone: "error" },
      ),
  });
  const remove = useMutation({
    mutationFn: async () => unwrap(await api.DELETE("/devotionals/{date}", { params: { path: { date } } })),
    onSuccess: () => {
      toast.show("Eliminado");
      invalidate();
    },
  });

  const list = sections.length
    ? sections
    : [
        { key: "passage", label: "Pasaje", kind: "text" },
        { key: "verse_actions", label: "¿Cómo puedo agradar a Dios hoy?", kind: "verse_actions" },
        { key: "learned", label: "¿Qué aprendí?", kind: "text" },
        { key: "teaching", label: "¿Qué enseñanza puedo compartir?", kind: "text" },
        { key: "prayer", label: "Oración", kind: "text" },
        { key: "song", label: "Canción", kind: "text" },
      ];

  return (
    <Card>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
      >
        {list.map((s) =>
          s.key === "passage" ? (
            <TextField
              key={s.key}
              label={s.label}
              value={passage}
              onChange={(e) => setPassage(e.target.value)}
              placeholder="Ej. Juan 15:1-8"
            />
          ) : s.kind === "verse_actions" ? (
            <fieldset key={s.key}>
              <legend className="mb-1 text-sm font-medium">{s.label}</legend>
              <div className="space-y-2">
                {verses.map((v, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: filas editables de la tabla versículo/acción
                  <div key={i} className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]">
                    <TextField
                      label={`Versículo ${i + 1}`}
                      value={v.verse}
                      onChange={(e) =>
                        setVerses(verses.map((x, j) => (j === i ? { ...x, verse: e.target.value } : x)))
                      }
                    />
                    <TextField
                      label="Acción"
                      value={v.action}
                      onChange={(e) =>
                        setVerses(verses.map((x, j) => (j === i ? { ...x, action: e.target.value } : x)))
                      }
                    />
                    <Button
                      variant="ghost"
                      size="sm"
                      className="sm:mt-6"
                      aria-label={`Quitar fila ${i + 1}`}
                      onClick={() => setVerses(verses.filter((_, j) => j !== i))}
                    >
                      ✕
                    </Button>
                  </div>
                ))}
              </div>
              <Button
                variant="secondary"
                size="sm"
                className="mt-2"
                onClick={() => setVerses([...verses, { verse: "", action: "" }])}
              >
                Agregar fila
              </Button>
            </fieldset>
          ) : (
            <TextArea
              key={s.key}
              label={s.label}
              rows={3}
              value={texts[s.key] ?? ""}
              onChange={(e) => setTexts({ ...texts, [s.key]: e.target.value })}
            />
          ),
        )}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={save.isPending}>
            Guardar
          </Button>
          {d && (
            <Button variant="danger" onClick={() => setConfirmDelete(true)}>
              Eliminar
            </Button>
          )}
        </div>
        <p className="text-xs text-muted">
          Si está vacío no marca el hábito. Personaliza las secciones en <Link to="/ajustes">Ajustes</Link>.
        </p>
      </form>
      <ConfirmDialog
        open={confirmDelete}
        title="¿Eliminar este registro?"
        confirmLabel="Eliminar"
        danger
        message="Se quitará también la marca automática del hábito de ese día (si la había)."
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => {
          setConfirmDelete(false);
          remove.mutate();
        }}
      />
    </Card>
  );
}
