import { Link } from "react-router";
import { GROUP_LABEL, MORE_NAV, visibleItems } from "../../app/nav";
import { PageHeader } from "../../components/ui/Display";
import { useAuth } from "../auth/AuthProvider";

export function MorePage() {
  const { me } = useAuth();
  const items = visibleItems(MORE_NAV, (me?.settings.modules ?? null) as Record<string, unknown> | null);
  const groups = [...new Set(items.map((i) => i.group))];
  return (
    <div>
      <PageHeader title="Menú" subtitle="Todo lo que hay en Dane" />
      <div className="space-y-5">
        {groups.map((g) => (
          <section key={g} aria-labelledby={`g-${g}`}>
            <h2 id={`g-${g}`} className="mb-2 text-sm font-semibold uppercase tracking-wide text-muted">
              {GROUP_LABEL[g]}
            </h2>
            <ul className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
              {items
                .filter((i) => i.group === g)
                .map((i) => (
                  <li key={i.to}>
                    <Link
                      to={i.to}
                      className="flex min-h-16 items-center gap-2 rounded-xl border border-line bg-surface p-3 text-fg no-underline hover:bg-surface-2"
                    >
                      <span aria-hidden className="text-xl">
                        {i.icon}
                      </span>
                      <span className="min-w-0 break-words font-medium">{i.label}</span>
                    </Link>
                  </li>
                ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
